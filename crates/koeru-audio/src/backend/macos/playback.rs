//! 再生（macOS、`TR-SYN-28`）。試唱を鳴らすためだけの経路。
//!
//! 入力モニタリングは持たない（`TR-REC-22` の「既定で無効」）。
//! 鳴らすのは試唱とガイドだけで、録っている音をそのまま返さない。
//!
//! 収録の入力は `kAudioUnitSubType_HALOutput` でデバイスを名指しする
//! （`DEC-REC-001` の「OS 側の音声加工を無効化する経路へ到達できること」を満たすため）。
//! 再生側にその要求は無いので、`kAudioUnitSubType_DefaultOutput` で
//! OS の既定出力へ流す。名指しの分だけコードが減る。
//!
//! # コールバックの規律（`TR-REC-40`）
//!
//! レンダーコールバックの中で確保も解放もロックもしない。
//! やるのは、リングから書き出す複製だけ。 リングの読み出し側はコールバックだけが持ち、
//! 継ぎ足す側とはロックを挟まずに分かれる。
//!
//! 以前は `RwLock<Vec<f32>>` を継ぎ足しで伸ばし、コールバックが `try_read` していた。
//! 継ぎ足しと重なった周は読めずに無音を出して枯渇と数え、
//! 鳴らし終えたぶんも再生を落とすまで残っていた。
//!
//! # 継ぎ足しは待つ
//!
//! リングは有界（[`STREAM_RING_MS`]）。 満杯なら [`Feed::push`] は空くまで待つ。
//! コールバックは待てないので、待つのは継ぎ足す側。 再生を止めるか落とすと、
//! 待っている継ぎ足しは書き終えずに戻る。

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::sys;
use crate::ring;

/// 継ぎ足す再生のリングが持てる長さ（ミリ秒）。
///
/// 要るのは先行（`TR-SYN-03` の「2秒以上先行」）を保てる長さで、それより長くしても
/// 鳴り方は変わらない。 4倍にしてあるのは、満杯まで書けていれば、次のフレーズの
/// 合成が数秒詰まっても先行を割らないため。 44100 Hz で約 1.4MB。
///
/// 先頭がこれより長ければ、先頭の長さに合わせる（[`play_streaming`]）。
const STREAM_RING_MS: u64 = 8000;

/// 満杯のときに継ぎ足しが空きを見に行く間隔。
///
/// コールバックから起こさない。 起こすのはシステムコールで、`TR-REC-40` の外になる。
/// 止めたあと待ちが抜けるまでの遅れも、これで決まる。
const PUSH_WAIT: Duration = Duration::from_millis(10);

/// 再生の失敗。
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    /// 出力の AudioUnit が見つからない。
    #[error("出力ユニットが見つからない")]
    NoOutputUnit,

    /// CoreAudio の呼び出しが失敗した。
    #[error("再生の CoreAudio の呼び出しが失敗した")]
    CoreAudio {
        /// どの呼び出しか。
        op: &'static str,
        /// `OSStatus`。
        status: i32,
    },
}

impl koeru_failure::Failure for PlaybackError {
    fn code(&self) -> &'static str {
        match self {
            Self::NoOutputUnit => "playback.no_output_unit",
            Self::CoreAudio { .. } => "playback.coreaudio",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::DeviceUnavailable
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
/// アトミックと、継ぎ足す側だけが握るロック。 コールバックはロックに触らない。
#[derive(Debug)]
struct Shared {
    /// 継ぎ足す側（`TR-SYN-03`）。 **コールバックはこのロックに触らない。**
    ///
    /// ロックは `Feed` の複製どうしで書く順序を揃えるためだけにある。
    producer: Mutex<ring::Producer>,
    /// 流し終えたフレーム数。コールバックだけが進める。
    played: AtomicUsize,
    /// リングへ書いたフレーム数。継ぎ足す側だけが進める。
    queued: AtomicUsize,
    /// もう継ぎ足さない。
    sealed: AtomicBool,
    /// 末尾まで流し終えたか。
    done: AtomicBool,
    /// 継ぎ足しが間に合わず、無音を出した回数。枯渇の記録（`TR-SYN-03`）。
    starved: AtomicUsize,
    /// 止めた、または落とした。 満杯で待っている継ぎ足しを抜けさせる。
    closed: AtomicBool,
}

/// レンダーコールバックが持つもの。 コールバックのほかは触らない。
///
/// リングの読み出し側は、読むのに `&mut` が要る（`ring` の「端点は1つずつ」）。
/// 共有する状態の中に置くと、`&Shared` から `&mut` を取り出す口が要り、
/// `Shared` の `Sync` を手で約束することになる。 丸ごとコールバックへ渡せば要らない。
#[derive(Debug)]
struct Render {
    consumer: ring::Consumer,
    shared: Arc<Shared>,
}

impl Render {
    /// コールバックの本体。 `out` を埋め、進み具合を `shared` へ書く。
    ///
    /// 確保も解放もロックもしない（`TR-REC-40`）。
    fn render(&mut self, out: &mut [f32]) {
        // リングより先に読む（`fill` の `sealed`）。
        let sealed = self.shared.sealed.load(Ordering::Acquire);
        let filled = fill(&mut self.consumer, out, sealed);
        self.shared
            .played
            .fetch_add(filled.frames, Ordering::Release);
        match filled.end {
            End::Full => {}
            End::Done => self.shared.done.store(true, Ordering::Release),
            End::Starved => {
                self.shared.starved.fetch_add(1, Ordering::Relaxed);
            }
        }
    }
}

/// 鳴っている最中の再生。落とすと止まる。
#[derive(Debug)]
pub struct Playback {
    unit: sys::AudioUnit,
    shared: Arc<Shared>,
    /// `Box::into_raw` でコールバックへ渡したもの。 `Drop` でユニットを捨てたあとに回収する。
    /// それまでは参照を作らない——作るのはコールバックだけ。
    raw: *mut Render,
}

// SAFETY: `AudioUnit` は不透明ポインタ。 CoreAudio 側が内部で同期しており、
// 所有権をスレッド間で移すことは許される（同時に触らない限り）。
// `raw` は `Drop` で回収するまで参照にしないので、どのスレッドで落としても同じ。
unsafe impl Send for Playback {}

impl Playback {
    /// 末尾まで流し終えたか。
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.shared.done.load(Ordering::Acquire)
    }

    /// 別のスレッドから継ぎ足すための口（`TR-SYN-03`）。
    ///
    /// `Playback` そのものはスレッド間で共有しない。
    /// `AudioUnit` のハンドルを持っているので、共有すると停止と破棄が絡む。
    /// 継ぎ足しに要るのは中の状態だけなので、そこだけ切り出す。
    #[must_use]
    pub fn feed(&self) -> Feed {
        Feed {
            shared: Arc::clone(&self.shared),
        }
    }

    /// 続きを継ぎ足す（`TR-SYN-03`）。 満杯なら空くまで待つ（[`Feed::push`]）。
    ///
    /// 鳴らしながら足せる。 先頭フレーズができた時点で鳴らしはじめ、
    /// 残りは並行して作る。
    pub fn push(&self, more: &[f32]) {
        self.feed().push(more);
    }

    /// もう継ぎ足さないと宣言する。これを呼ばないと末尾で終われない。
    pub fn seal(&self) {
        self.feed().seal();
    }

    /// まだ鳴らしていない長さ（サンプル）。
    ///
    /// これが先行の余裕（`TR-SYN-03` の「2秒以上先行」）。
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.shared.buffered()
    }

    /// 継ぎ足しが間に合わず、無音を出した回数。
    #[must_use]
    pub fn starved(&self) -> usize {
        self.shared.starved.load(Ordering::Relaxed)
    }

    /// いま何フレーム目まで流したか。進捗表示に使う。
    #[must_use]
    pub fn position(&self) -> usize {
        self.shared.played.load(Ordering::Acquire)
    }

    /// 止める。 満杯で待っている継ぎ足しも抜ける。止めたあとの継ぎ足しは捨てる。
    pub fn stop(&self) -> Result<()> {
        self.shared.closed.store(true, Ordering::Release);
        // SAFETY: `unit` は `start` が作って `Drop` まで生きている。
        check("AudioOutputUnitStop", unsafe {
            sys::AudioOutputUnitStop(self.unit)
        })
    }
}

impl Shared {
    /// 目安。 2つを別々に読むので、読む間に流れたぶんだけずれる。
    fn buffered(&self) -> usize {
        let played = self.played.load(Ordering::Acquire);
        self.queued.load(Ordering::Acquire).saturating_sub(played)
    }
}

/// 継ぎ足す口（`TR-SYN-03`）。
///
/// 合成スレッドが持つのはこれだけ。 `AudioUnit` には触らない。
#[derive(Debug, Clone)]
pub struct Feed {
    shared: Arc<Shared>,
}

impl Feed {
    /// 続きを継ぎ足す（`TR-SYN-03`）。
    ///
    /// **満杯なら空くまで待つ。** 空くのは鳴らしたぶんだけなので、
    /// リングの長さ（`STREAM_RING_MS`）を超えて先へは書けない。
    /// 再生を止めるか落とすと、書き終えていなくても戻る。
    ///
    /// 待つので、リアルタイムのスレッドからは呼ばない。 画面の操作が通るロックを
    /// 握ったまま呼ぶと、書き終えるまで止める操作も通らない。 呼ぶのは合成のスレッドにする。
    pub fn push(&self, more: &[f32]) {
        let Ok(mut producer) = self.shared.producer.lock() else {
            return;
        };
        push_waiting(
            &mut producer,
            more,
            &self.shared.closed,
            &self.shared.queued,
            PUSH_WAIT,
        );
    }

    pub fn seal(&self) {
        self.shared.sealed.store(true, Ordering::Release);
    }

    /// まだ鳴らしていない長さ（サンプル）。
    #[must_use]
    pub fn buffered(&self) -> usize {
        self.shared.buffered()
    }
}

/// 入りきるまで待って書く。戻り値は書けたフレーム数。
///
/// `closed` が立ったら、書き終えていなくても戻る。 書くたびに `queued` を進めるので、
/// 待っている間もどこまで書けたかが読める。
fn push_waiting(
    producer: &mut ring::Producer,
    more: &[f32],
    closed: &AtomicBool,
    queued: &AtomicUsize,
    wait: Duration,
) -> usize {
    let mut at = 0;
    while at < more.len() && !closed.load(Ordering::Acquire) {
        let n = producer.push(&more[at..]);
        queued.fetch_add(n, Ordering::Release);
        at += n;
        if at < more.len() {
            std::thread::sleep(wait);
        }
    }
    at
}

impl Drop for Playback {
    fn drop(&mut self) {
        // 先に閉じる。 閉じないと、満杯で待っている継ぎ足しが、止まって空かなくなった
        // リングを待ち続ける。
        self.shared.closed.store(true, Ordering::Release);
        // SAFETY: `unit` はここでだけ捨てる。停止 → 解除 → 破棄の順。
        unsafe {
            sys::AudioOutputUnitStop(self.unit);
            sys::AudioUnitUninitialize(self.unit);
            sys::AudioComponentInstanceDispose(self.unit);
        }
        // SAFETY: `build` の `Box::into_raw` と1対1で対応する。 ユニットは直前に捨てたので、
        // もうコールバックは来ず、`Render` を触っている者はいない。
        drop(unsafe { Box::from_raw(self.raw) });
    }
}

/// モノラルの f32 を既定の出力デバイスへ流す。
///
/// 返った `Playback` を落とすと止まる。 最後まで鳴らしたいなら持ち続ける。
///
/// リングは渡したものがちょうど入る長さで作り、鳴らす前に全部書いておく。
/// 継ぎ足す前提ではないので、[`Playback::push`] は鳴らしたぶんが空くまで待つ。
#[tracing::instrument(skip(samples), fields(frames = samples.len(), rate_hz))]
pub fn play(samples: Vec<f32>, rate_hz: u32) -> Result<Playback> {
    // 1枠は満杯と空の区別に使う（`ring::channel`）。
    let capacity = samples.len() + 1;
    start(samples, capacity, rate_hz, true)
}

/// 継ぎ足せる再生を始める（`TR-SYN-03`）。
///
/// 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。
/// 足し終わったら [`Playback::seal`] を呼ぶ。
///
/// リングの長さは `STREAM_RING_MS`。 `head` がそれより長ければ `head` に合わせる
/// ——先頭は鳴らす前に全部書いておきたく、ここで待つと鳴りはじめが遅れる。
///
/// # Errors
///
/// 出力ユニットを開けないとき。
#[tracing::instrument(skip(head), fields(frames = head.len(), rate_hz))]
pub fn play_streaming(head: Vec<f32>, rate_hz: u32) -> Result<Playback> {
    let capacity = stream_capacity(head.len(), rate_hz);
    start(head, capacity, rate_hz, false)
}

/// 継ぎ足す再生のリングの容量（フレーム）。1枠は満杯と空の区別に使う。
fn stream_capacity(head: usize, rate_hz: u32) -> usize {
    let lead = u64::from(rate_hz) * STREAM_RING_MS / 1000;
    head.max(usize::try_from(lead).unwrap_or(usize::MAX))
        .saturating_add(1)
}

/// リングを作って `samples` を先に書き、共有する状態とコールバックの持ち物に分ける。
///
/// 確保はここで済ませる。 コールバックの中ではしない。
fn prepare(samples: &[f32], capacity: usize, sealed: bool) -> (Arc<Shared>, Render) {
    let (mut producer, consumer) = ring::channel(capacity);
    let queued = producer.push(samples);
    let shared = Arc::new(Shared {
        producer: Mutex::new(producer),
        played: AtomicUsize::new(0),
        queued: AtomicUsize::new(queued),
        sealed: AtomicBool::new(sealed),
        done: AtomicBool::new(false),
        starved: AtomicUsize::new(0),
        closed: AtomicBool::new(false),
    });
    let render = Render {
        consumer,
        shared: Arc::clone(&shared),
    };
    (shared, render)
}

fn start(samples: Vec<f32>, capacity: usize, rate_hz: u32, sealed: bool) -> Result<Playback> {
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
    let built = build(unit, samples, capacity, rate_hz, sealed);
    match built {
        Ok(p) => Ok(p),
        Err(e) => {
            // SAFETY: 初期化前でも Dispose は安全。
            unsafe { sys::AudioComponentInstanceDispose(unit) };
            Err(e)
        }
    }
}

fn build(
    unit: sys::AudioUnit,
    samples: Vec<f32>,
    capacity: usize,
    rate_hz: u32,
    sealed: bool,
) -> Result<Playback> {
    // モノラル・非インタリーブの f32。 変換は WORLD 側で済んでいる。
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

    let (shared, state) = prepare(&samples, capacity, sealed);
    drop(samples);
    // コールバックへ丸ごと渡す。 `Drop` でユニットを捨てるまで生かす。
    let raw = Box::into_raw(Box::new(state));

    let cb = sys::AURenderCallbackStruct {
        inputProc: Some(render),
        inputProcRefCon: raw.cast::<std::ffi::c_void>(),
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
        // SAFETY: 上の `into_raw` と1対1。 まだ開始していないので、コールバックは来ていない。
        drop(unsafe { Box::from_raw(raw) });
        return Err(e);
    }

    // SAFETY: プロパティを設定し終えてから初期化する。
    if let Err(e) = check("AudioUnitInitialize", unsafe {
        sys::AudioUnitInitialize(unit)
    }) {
        // SAFETY: 同上。
        drop(unsafe { Box::from_raw(raw) });
        return Err(e);
    }

    // SAFETY: 初期化済みのユニットを開始する。
    if let Err(e) = check("AudioOutputUnitStart", unsafe {
        sys::AudioOutputUnitStart(unit)
    }) {
        // SAFETY: 初期化は済んでいるので、解除してから捨てる。
        unsafe { sys::AudioUnitUninitialize(unit) };
        // SAFETY: 同上。 開始に失敗したので、コールバックは来ていない。
        drop(unsafe { Box::from_raw(raw) });
        return Err(e);
    }

    Ok(Playback { unit, shared, raw })
}

/// レンダーコールバック。
///
/// 確保も解放もロックもしない（`TR-REC-40`）。リングから複製するだけ（[`fill`]）。
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
    // SAFETY: `build` が `Box::into_raw` で渡した `Render`。 `Playback` がユニットを捨てるまで
    // 生きていて、その間これを参照にするのはこのコールバックだけ。 CoreAudio は
    // 1つのユニットのレンダーコールバックを重ねて呼ばないので、`&mut` は重ならない。
    let state = unsafe { &mut *in_ref_con.cast::<Render>() };

    // `AudioBuffer` はポインタを含むので8バイト境界に揃う。
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

    state.render(out);
    0
}

/// 1回のレンダーで、リングから写したもの。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Filled {
    /// リングから写したフレーム数。残りは無音で埋めてある。
    frames: usize,
    end: End,
}

/// 埋めきれたか。 埋めきれなかったなら、なぜか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum End {
    /// 全部リングから埋まった。
    Full,
    /// もう継ぎ足さないと宣言されていて、末尾まで流し終えた。
    Done,
    /// まだ続きが来る予定なのに足りなかった。 枯渇として数える（`TR-SYN-03`）。
    Starved,
}

/// リングから `out` を埋める。 足りないぶんは無音にする。
///
/// 確保も解放もロックもしない（`TR-REC-40`）。 コールバックの本体で、
/// CoreAudio なしで試せるように切り出してある。
///
/// `sealed` は**リングを読む前に**読んだ値を渡す。 継ぎ足しは書いてから閉じるので、
/// 閉じたのを見てから読めば、書いたものは全部見える。 読んだあとで見ると、
/// その間に書き足して閉じたぶんを残したまま、流し終えたことになる。
fn fill(consumer: &mut ring::Consumer, out: &mut [f32], sealed: bool) -> Filled {
    let frames = consumer.pop(out);
    // 埋めないと直前のバッファの中身が鳴る。
    out[frames..].fill(0.0);
    let end = if frames == out.len() {
        End::Full
    } else if sealed {
        End::Done
    } else {
        End::Starved
    };
    Filled { frames, end }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 通し番号の音。 どこまで流れたかを値で見分ける。
    fn ramp(from: usize, n: usize) -> Vec<f32> {
        (from..from + n).map(|i| i as f32).collect()
    }

    #[test]
    fn 足りないぶんは無音で埋める() {
        let (mut p, mut c) = ring::channel(16);
        p.push(&[1.0, 2.0, 3.0]);
        let mut out = [9.0_f32; 5];
        let got = fill(&mut c, &mut out, true);
        assert_eq!(out, [1.0, 2.0, 3.0, 0.0, 0.0], "直前の中身を残さない");
        assert_eq!(got.frames, 3);
    }

    #[test]
    fn 埋めきれれば終わりでも枯渇でもない() {
        let (mut p, mut c) = ring::channel(16);
        p.push(&[1.0, 2.0, 3.0]);
        let mut out = [0.0_f32; 3];
        assert_eq!(
            fill(&mut c, &mut out, false),
            Filled {
                frames: 3,
                end: End::Full
            }
        );
    }

    /// 継ぎ足しが来る予定なら枯渇、もう来ないなら流し終えた（`TR-SYN-03`）。
    #[test]
    fn 足りないときは閉じていれば終わり閉じていなければ枯渇() {
        let (mut p, mut c) = ring::channel(16);
        let mut out = [0.0_f32; 4];

        p.push(&[1.0]);
        assert_eq!(fill(&mut c, &mut out, false).end, End::Starved);

        p.push(&[2.0]);
        assert_eq!(fill(&mut c, &mut out, true).end, End::Done);
    }

    /// 容量が2の冪でなくても、環をまたいで順に流れる（`DEC-REC-007`）。
    #[test]
    fn 容量が2の冪でなくても環をまたいで順に流れる() {
        let (mut p, mut c) = ring::channel(301); // **2の冪ではない。** 実効容量 300
        let mut out = vec![0.0_f32; 128];
        let mut written = 0;
        let mut read = 0;
        while read < 301 * 5 {
            written += p.push(&ramp(written, 97));
            let got = fill(&mut c, &mut out, false);
            for v in &out[..got.frames] {
                assert!(
                    (*v - read as f32).abs() < f32::EPSILON,
                    "{read} フレーム目で順序が壊れた: {v}"
                );
                read += 1;
            }
            assert!(out[got.frames..].iter().all(|v| *v == 0.0), "残りは無音");
        }
    }

    /// 流したフレーム数だけ位置が進み、まだ鳴らしていない長さが減る。
    #[test]
    fn 流したぶんだけ位置が進む() {
        let (shared, mut render) = prepare(&ramp(0, 10), 11, true);
        assert_eq!(shared.buffered(), 10, "先に全部書いてある");

        let mut out = [0.0_f32; 4];
        render.render(&mut out);
        assert_eq!(shared.played.load(Ordering::Acquire), 4);
        assert_eq!(shared.buffered(), 6);
        assert!(!shared.done.load(Ordering::Acquire));

        render.render(&mut out);
        render.render(&mut out); // 残り2つで足りない。閉じているので終わり
        assert_eq!(
            shared.played.load(Ordering::Acquire),
            10,
            "足りなかった周も数えすぎない"
        );
        assert_eq!(shared.buffered(), 0);
        assert!(shared.done.load(Ordering::Acquire));
        assert_eq!(shared.starved.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn 継ぎ足す再生が足りなければ枯渇を数える() {
        let (shared, mut render) = prepare(&[], stream_capacity(0, 100), false);
        let mut out = [0.0_f32; 4];
        render.render(&mut out);
        assert_eq!(shared.starved.load(Ordering::Relaxed), 1);
        assert!(
            !shared.done.load(Ordering::Acquire),
            "閉じていないので終わらない"
        );
    }

    /// 最後まで知っている再生は、渡したものがちょうど入る（`play`）。
    #[test]
    fn 渡したものは鳴らす前に全部入る() {
        let samples = ramp(0, 1234);
        let (shared, _render) = prepare(&samples, samples.len() + 1, true);
        assert_eq!(shared.queued.load(Ordering::Acquire), samples.len());
    }

    /// 継ぎ足す再生のリングは、先行より長く、先頭より短くならない。
    #[test]
    fn 継ぎ足す再生のリングの長さ() {
        let rate = 44_100;
        let lead = rate as usize * STREAM_RING_MS as usize / 1000;
        assert_eq!(stream_capacity(0, rate), lead + 1);
        assert_eq!(
            stream_capacity(lead * 2, rate),
            lead * 2 + 1,
            "先頭は全部入る"
        );
    }

    /// 満杯なら、鳴らして空くまで待ってから続きを書く。
    #[test]
    fn 満杯なら空くまで待って全部書く() {
        let (shared, mut render) = prepare(&[], 101, false); // 実効容量 100
        let feed = Feed {
            shared: Arc::clone(&shared),
        };
        let more = ramp(0, 450);
        let writer = std::thread::spawn({
            let more = more.clone();
            move || feed.push(&more)
        });

        let mut heard = Vec::new();
        let mut out = [0.0_f32; 37];
        while heard.len() < more.len() {
            render.render(&mut out);
            let n = shared.played.load(Ordering::Acquire) - heard.len();
            heard.extend_from_slice(&out[..n]);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        writer.join().expect("書き手が終わる");
        assert_eq!(heard, more, "待って書いたぶんも順に全部鳴る");
        assert_eq!(shared.queued.load(Ordering::Acquire), more.len());
    }

    /// 止めた・落とした再生へ継ぎ足しても、待ち続けない。
    #[test]
    fn 閉じたら満杯でも待たずに戻る() {
        let (shared, _render) = prepare(&ramp(0, 100), 101, false); // 満杯
        let feed = Feed {
            shared: Arc::clone(&shared),
        };
        let writer = std::thread::spawn(move || feed.push(&ramp(100, 50)));
        std::thread::sleep(PUSH_WAIT * 3);
        assert!(!writer.is_finished(), "満杯なので待っている");

        // `Playback::stop` と `Drop` が立てるもの。
        shared.closed.store(true, Ordering::Release);
        writer.join().expect("待ちを抜けて戻る");
        assert_eq!(
            shared.queued.load(Ordering::Acquire),
            100,
            "閉じたあとは書かない"
        );
    }

    #[test]
    fn 閉じたあとの継ぎ足しは捨てる() {
        let (mut producer, _consumer) = ring::channel(8);
        let closed = AtomicBool::new(true);
        let queued = AtomicUsize::new(0);
        let wrote = push_waiting(&mut producer, &[1.0, 2.0], &closed, &queued, PUSH_WAIT);
        assert_eq!(wrote, 0);
        assert_eq!(queued.load(Ordering::Acquire), 0);
    }

    /// 合成スレッドへ渡して複製できる口のまま（`TR-SYN-03`）。
    #[test]
    fn 継ぎ足す口はスレッドをまたいで複製できる() {
        fn shareable<T: Clone + Send + Sync>() {}
        shareable::<Feed>();
    }
}
