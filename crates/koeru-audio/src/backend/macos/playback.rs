//! 再生（macOS）。**試唱を鳴らすためだけの経路。**
//!
//! 収録の入力は `kAudioUnitSubType_HALOutput` でデバイスを名指しする
//! （`TR-REC-08` の「OS 側の音声加工を無効化する経路」へ到達する必要があるため）。
//! **再生側にその要求は無い**ので、`kAudioUnitSubType_DefaultOutput` で
//! OS の既定出力へ流す。名指しの分だけコードが減る。
//!
//! # コールバックの規律（`TR-REC-40`）
//!
//! **レンダーコールバックの中で確保も解放もロックもしない。**
//! やるのは、あらかじめ置いてある f32 のスライスから書き出す複製だけ。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, RwLock};

use super::sys;

/// 再生の失敗。
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    /// 出力の AudioUnit が見つからない。
    #[error("出力ユニットが見つからない")]
    NoOutputUnit,

    /// CoreAudio の呼び出しが失敗した。
    #[error("CoreAudio の呼び出しが失敗した（{op}、status={status}）")]
    CoreAudio {
        /// どの呼び出しか。
        op: &'static str,
        /// `OSStatus`。
        status: i32,
    },
}

impl PlaybackError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NoOutputUnit => "playback.no_output_unit",
            Self::CoreAudio { .. } => "playback.coreaudio",
        }
    }
}

type Result<T> = std::result::Result<T, PlaybackError>;

fn check(op: &'static str, status: sys::OSStatus) -> Result<()> {
    if status == 0 {
        Ok(())
    } else {
        Err(PlaybackError::CoreAudio { op, status })
    }
}

/// コールバックと呼び出し側で共有する状態。
#[derive(Debug)]
struct Shared {
    /// 流すもの。
    ///
    /// **継ぎ足せる**（`TR-SYN-03`）。先頭フレーズができた時点で鳴らしはじめ、
    /// 残りは並行して作る。`RwLock` の書き側は継ぎ足しのときだけ。
    samples: RwLock<Vec<f32>>,
    /// 次に読む位置。**コールバックだけが進める。**
    cursor: AtomicUsize,
    /// もう継ぎ足さない。
    sealed: AtomicBool,
    /// 末尾まで流し終えたか。
    done: AtomicBool,
    /// 継ぎ足しが間に合わず、無音を出した回数。**枯渇の記録**（`TR-SYN-03`）。
    starved: AtomicUsize,
}

/// 鳴っている最中の再生。**落とすと止まる。**
#[derive(Debug)]
pub struct Playback {
    unit: sys::AudioUnit,
    shared: Arc<Shared>,
    /// `Arc::into_raw` で渡した参照。`Drop` で回収する。
    raw: *const Shared,
}

// **`AudioUnit` は不透明ポインタ。** CoreAudio 側が内部で同期しており、
// 所有権をスレッド間で移すことは許される（同時に触らない限り）。
unsafe impl Send for Playback {}

impl Playback {
    /// 末尾まで流し終えたか。
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.done.load(Ordering::Acquire)
    }

    /// 別のスレッドから継ぎ足すための口（`TR-SYN-03`）。
    ///
    /// **`Playback` そのものはスレッド間で共有しない。**
    /// `AudioUnit` のハンドルを持っているので、共有すると停止と破棄が絡む。
    /// 継ぎ足しに要るのは中の状態だけなので、そこだけ切り出す。
    #[must_use]
    pub fn feed(&self) -> Feed {
        Feed {
            shared: Arc::clone(&self.shared),
        }
    }

    /// 続きを継ぎ足す（`TR-SYN-03`）。
    ///
    /// **鳴らしながら足せる。** 先頭フレーズができた時点で鳴らしはじめ、
    /// 残りは並行して作る。
    pub fn push(&self, more: &[f32]) {
        self.feed().push(more);
    }

    /// もう継ぎ足さないと宣言する。**これを呼ばないと末尾で終われない。**
    pub fn seal(&self) {
        self.feed().seal();
    }

    /// まだ鳴らしていない長さ（サンプル）。
    ///
    /// **これが先行の余裕**（`TR-SYN-03` の「2秒以上先行」）。
    #[must_use]
    pub fn buffered(&self) -> usize {
        let have = self.shared.samples.read().map_or(0, |g| g.len());
        have.saturating_sub(self.shared.cursor.load(Ordering::Acquire))
    }

    /// 継ぎ足しが間に合わず、無音を出した回数。
    #[must_use]
    pub fn starved(&self) -> usize {
        self.shared.starved.load(Ordering::Relaxed)
    }

    /// いま何フレーム目まで流したか。**進捗表示に使う。**
    #[must_use]
    pub fn position(&self) -> usize {
        self.shared.cursor.load(Ordering::Acquire)
    }

    /// 止める。
    pub fn stop(&self) -> Result<()> {
        // SAFETY: `unit` は `start` が作って `Drop` まで生きている。
        check("AudioOutputUnitStop", unsafe {
            sys::AudioOutputUnitStop(self.unit)
        })
    }
}

/// 継ぎ足す口（`TR-SYN-03`）。
///
/// **合成スレッドが持つのはこれだけ。** `AudioUnit` には触らない。
#[derive(Debug, Clone)]
pub struct Feed {
    shared: Arc<Shared>,
}

// SAFETY: `Shared` の中身は `RwLock` とアトミックだけで、内部で同期している。
// `AudioUnit` のハンドルはここに含まれない。
unsafe impl Send for Feed {}
// SAFETY: 同上。
unsafe impl Sync for Feed {}

impl Feed {
    /// 続きを継ぎ足す。
    pub fn push(&self, more: &[f32]) {
        if let Ok(mut g) = self.shared.samples.write() {
            g.extend_from_slice(more);
        }
    }

    /// もう継ぎ足さないと宣言する。
    pub fn seal(&self) {
        self.shared.sealed.store(true, Ordering::Release);
    }

    /// まだ鳴らしていない長さ（サンプル）。
    #[must_use]
    pub fn buffered(&self) -> usize {
        let have = self.shared.samples.read().map_or(0, |g| g.len());
        have.saturating_sub(self.shared.cursor.load(Ordering::Acquire))
    }
}

impl Drop for Playback {
    fn drop(&mut self) {
        // SAFETY: `unit` はここでだけ捨てる。停止 → 解除 → 破棄の順。
        unsafe {
            sys::AudioOutputUnitStop(self.unit);
            sys::AudioUnitUninitialize(self.unit);
            sys::AudioComponentInstanceDispose(self.unit);
        }
        // SAFETY: `start` の `Arc::into_raw` と1対1で対応する。
        drop(unsafe { Arc::from_raw(self.raw) });
    }
}

/// モノラルの f32 を既定の出力デバイスへ流す。
///
/// **返った `Playback` を落とすと止まる。** 最後まで鳴らしたいなら持ち続ける。
#[tracing::instrument(skip(samples), fields(frames = samples.len(), rate_hz), err)]
pub fn play(samples: Vec<f32>, rate_hz: u32) -> Result<Playback> {
    start(samples, rate_hz, true)
}

/// 継ぎ足せる再生を始める（`TR-SYN-03`）。
///
/// **先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。**
/// 足し終わったら [`Playback::seal`] を呼ぶ。
///
/// # Errors
///
/// 出力ユニットを開けないとき。
#[tracing::instrument(skip(head), fields(frames = head.len(), rate_hz), err)]
pub fn play_streaming(head: Vec<f32>, rate_hz: u32) -> Result<Playback> {
    start(head, rate_hz, false)
}

fn start(samples: Vec<f32>, rate_hz: u32, sealed: bool) -> Result<Playback> {
    let desc = sys::AudioComponentDescription {
        componentType: sys::kAudioUnitType_Output,
        componentSubType: sys::kAudioUnitSubType_DefaultOutput,
        componentManufacturer: sys::kAudioUnitManufacturer_Apple,
        componentFlags: 0,
        componentFlagsMask: 0,
    };

    // SAFETY: `desc` は生きているスタック上の値で、C 側は借用しない。
    let component = unsafe { sys::AudioComponentFindNext(std::ptr::null_mut(), &raw const desc) };
    if component.is_null() {
        return Err(PlaybackError::NoOutputUnit);
    }

    let mut unit: sys::AudioComponentInstance = std::ptr::null_mut();
    // SAFETY: `unit` は書き込み先として渡す。
    check("AudioComponentInstanceNew", unsafe {
        sys::AudioComponentInstanceNew(component, &raw mut unit)
    })?;

    // ここから先で失敗したら unit を捨てる。
    let built = build(unit, samples, rate_hz, sealed);
    match built {
        Ok(p) => Ok(p),
        Err(e) => {
            // SAFETY: 初期化前でも Dispose は安全。
            unsafe { sys::AudioComponentInstanceDispose(unit) };
            Err(e)
        }
    }
}

fn build(unit: sys::AudioUnit, samples: Vec<f32>, rate_hz: u32, sealed: bool) -> Result<Playback> {
    // **モノラル・非インタリーブの f32。** 変換は WORLD 側で済んでいる。
    let format = sys::AudioStreamBasicDescription {
        mSampleRate: f64::from(rate_hz),
        mFormatID: sys::kAudioFormatLinearPCM,
        mFormatFlags: sys::kAudioFormatFlagIsFloat
            | sys::kAudioFormatFlagIsPacked
            | sys::kAudioFormatFlagIsNonInterleaved,
        mBytesPerPacket: 4,
        mFramesPerPacket: 1,
        mBytesPerFrame: 4,
        mChannelsPerFrame: 1,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    // SAFETY: `format` はこの呼び出しの間だけ読まれる。
    check("SetProperty(StreamFormat)", unsafe {
        sys::AudioUnitSetProperty(
            unit,
            sys::kAudioUnitProperty_StreamFormat,
            sys::kAudioUnitScope_Input,
            sys::OUTPUT_ELEMENT,
            (&raw const format).cast(),
            u32::try_from(size_of::<sys::AudioStreamBasicDescription>()).unwrap_or(0),
        )
    })?;

    let shared = Arc::new(Shared {
        samples: RwLock::new(samples),
        cursor: AtomicUsize::new(0),
        sealed: AtomicBool::new(sealed),
        done: AtomicBool::new(false),
        starved: AtomicUsize::new(0),
    });
    // **コールバックへ渡す参照を、`Drop` まで生かす。**
    let raw = Arc::into_raw(Arc::clone(&shared));

    let cb = sys::AURenderCallbackStruct {
        inputProc: Some(render),
        inputProcRefCon: raw.cast::<std::ffi::c_void>().cast_mut(),
    };
    // SAFETY: `cb` はこの呼び出しの間だけ読まれ、中の `raw` は `Drop` まで生きる。
    let set = unsafe {
        sys::AudioUnitSetProperty(
            unit,
            sys::kAudioUnitProperty_SetRenderCallback,
            sys::kAudioUnitScope_Input,
            sys::OUTPUT_ELEMENT,
            (&raw const cb).cast(),
            u32::try_from(size_of::<sys::AURenderCallbackStruct>()).unwrap_or(0),
        )
    };
    if let Err(e) = check("SetProperty(SetRenderCallback)", set) {
        // SAFETY: 上の `into_raw` と1対1。
        drop(unsafe { Arc::from_raw(raw) });
        return Err(e);
    }

    // SAFETY: プロパティを設定し終えてから初期化する。
    if let Err(e) = check("AudioUnitInitialize", unsafe {
        sys::AudioUnitInitialize(unit)
    }) {
        // SAFETY: 上の `into_raw` と1対1。
        drop(unsafe { Arc::from_raw(raw) });
        return Err(e);
    }

    // SAFETY: 初期化済みのユニットを開始する。
    if let Err(e) = check("AudioOutputUnitStart", unsafe {
        sys::AudioOutputUnitStart(unit)
    }) {
        // SAFETY: 初期化は済んでいるので、解除してから捨てる。
        unsafe { sys::AudioUnitUninitialize(unit) };
        // SAFETY: 上の `into_raw` と1対1。
        drop(unsafe { Arc::from_raw(raw) });
        return Err(e);
    }

    Ok(Playback { unit, shared, raw })
}

/// レンダーコールバック。
///
/// **確保も解放もロックもしない**（`TR-REC-40`）。置いてあるスライスから複製するだけ。
unsafe extern "C" fn render(
    in_ref_con: *mut std::ffi::c_void,
    _flags: *mut sys::AudioUnitRenderActionFlags,
    _ts: *const sys::AudioTimeStamp,
    _bus: u32,
    frames: u32,
    io_data: *mut std::ffi::c_void,
) -> sys::OSStatus {
    if in_ref_con.is_null() || io_data.is_null() {
        return 0;
    }
    // SAFETY: `build` が `Arc::into_raw` で渡した参照。`Playback` が生きている間だけ呼ばれる。
    let shared = unsafe { &*in_ref_con.cast::<Shared>() };

    // **`AudioBuffer` はポインタを含むので8バイト境界に揃う。**
    // ヘッダの直後に詰め物が入る（capture 側と同じ落とし穴）。
    let base =
        size_of::<sys::AudioBufferListHeader>().next_multiple_of(align_of::<sys::AudioBuffer>());
    // SAFETY: CoreAudio が渡す `AudioBufferList` の先頭バッファ。
    let buffer = unsafe { &*io_data.cast::<u8>().add(base).cast::<sys::AudioBuffer>() };
    if buffer.mData.is_null() {
        return 0;
    }

    let want = frames as usize;
    // SAFETY: `mDataByteSize` バイトぶんの f32 が書ける、と CoreAudio が保証する。
    let out = unsafe {
        std::slice::from_raw_parts_mut(
            buffer.mData.cast::<f32>(),
            (buffer.mDataByteSize as usize / 4).min(want),
        )
    };

    let start = shared.cursor.load(Ordering::Relaxed);

    // **コールバックの中でロックを待たない**（TR-REC-40 と同じ規律）。
    // 取れなければ無音を出して次の周に回す。継ぎ足し側は一瞬しか握らない。
    let Ok(buf) = shared.samples.try_read() else {
        out.fill(0.0);
        shared.starved.fetch_add(1, Ordering::Relaxed);
        return 0;
    };

    let avail = buf.len().saturating_sub(start);
    let n = avail.min(out.len());

    out[..n].copy_from_slice(&buf[start..start + n]);
    // **残りは無音で埋める。** 埋めないと直前のバッファの中身が鳴る。
    out[n..].fill(0.0);

    shared.cursor.store(start + n, Ordering::Release);
    if n < out.len() {
        if shared.sealed.load(Ordering::Acquire) {
            shared.done.store(true, Ordering::Release);
        } else {
            // **まだ続きが来る予定なのに足りなかった。** 枯渇として数える。
            shared.starved.fetch_add(1, Ordering::Relaxed);
        }
    }
    0
}
