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

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

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
///
/// **`samples` は再生中に差し替えない。** 差し替えたければ止めてから作り直す。
#[derive(Debug)]
struct Shared {
    samples: Vec<f32>,
    /// 次に読む位置。**コールバックだけが進める。**
    cursor: AtomicUsize,
    /// 末尾まで流し終えたか。
    done: AtomicBool,
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
    let built = build(unit, samples, rate_hz);
    match built {
        Ok(p) => Ok(p),
        Err(e) => {
            // SAFETY: 初期化前でも Dispose は安全。
            unsafe { sys::AudioComponentInstanceDispose(unit) };
            Err(e)
        }
    }
}

fn build(unit: sys::AudioUnit, samples: Vec<f32>, rate_hz: u32) -> Result<Playback> {
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
        samples,
        cursor: AtomicUsize::new(0),
        done: AtomicBool::new(false),
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
    let avail = shared.samples.len().saturating_sub(start);
    let n = avail.min(out.len());

    out[..n].copy_from_slice(&shared.samples[start..start + n]);
    // **残りは無音で埋める。** 埋めないと直前のバッファの中身が鳴る。
    out[n..].fill(0.0);

    shared.cursor.store(start + n, Ordering::Release);
    if n < out.len() {
        shared.done.store(true, Ordering::Release);
    }
    0
}
