//! リングから排出してディスクへ落とす層（`TR-REC-19`, `TR-REC-20`）。
//!
//! **ストリームはテイクごとに開閉しない**（`REQ-REC-102`）。収録画面に入った時点で
//! 開き、フレーズ間も止めない。ここはその前提の上で、
//! **録音開始の指示より前の音を捨てないため**に居る。
//!
//! # なぜ遡るのか
//!
//! 人は「録音」を押してから息を吸わない。**押した瞬間にはもう発声が始まっている。**
//! 指示の時点から書き始めると、語頭の子音が欠ける。子音が欠けた素材は
//! oto の導出も合成も救えないので、**構造的に起こらないようにする**（`TR-REC-19`）。
//!
//! 常に直近 [`PREROLL_CAPACITY_MS`] を持ち回し、開始の指示で
//! [`PREROLL_MS`] ぶんを先に書き込む。終了の指示のあとも [`TAIL_MS`] ぶん書き続ける。
//!
//! # 時計を使わない
//!
//! 末尾の延長は**フレーム数で数える**。壁時計で測ると、排出が詰まったときに
//! 実際より短く切れる。**音の時間軸で数えれば、詰まっても長さは変わらない。**
//!
//! # 44100 へ落とすのはここ
//!
//! キャプチャはデバイスのネイティブレートで受ける（`TR-REC-02`、`TR-REC-05`）。
//! **リングから出した直後に1回だけ 44100 へ変換し、以降はすべてマスターの時間軸で扱う。**
//! プリロールもピークも検査用の収集も、書き出すテイクも、全部 44100。
//!
//! **ここより下流でレートを持ち回らない。** 持ち回ると、どこかで取り違える——
//! 実際、変換そのものが抜けていて 48000 Hz のマスターが書かれていた（`DEC-REC-006`）。
//! `write_distribution` はヘッダに 44100 と書くだけなので、**そのまま配ると
//! 44100 と名乗る 48000 の音**になる。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use koeru_audio::resample::Resampler;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_audio::{ring, wav};

/// 常時保持する長さ（ミリ秒）。**`TR-REC-19` の下限は 1000ms。**
pub const PREROLL_CAPACITY_MS: u64 = 1500;

/// 開始の指示から遡る長さ（ミリ秒、`TR-REC-19`）。
pub const PREROLL_MS: u64 = 500;

/// 終了の指示から延ばす長さ（ミリ秒、`TR-REC-19`）。
pub const TAIL_MS: u64 = 500;

/// 1回の排出で読む長さ。
const CHUNK: usize = 8192;

/// 波形の1目盛りの長さ（ミリ秒、`TR-REC-43`）。
///
/// **画面の更新間隔より細かくする。** 粗いと、目盛りが1つ増えるまで絵が動かない。
const ENVELOPE_STEP_MS: u64 = 5;

/// 保持する目盛りの数。[`PREROLL_CAPACITY_MS`] ぶん。
const ENVELOPE_STEPS: usize = (PREROLL_CAPACITY_MS / ENVELOPE_STEP_MS) as usize;

/// 排出するものが無いときに待つ時間。
const IDLE_SLEEP_MS: u64 = 2;

/// いま流れている音の包絡（`TR-REC-43`）。
///
/// **波形そのものは持たない。** 目盛りごとの min/max だけを積む。
///
/// # 生の音を共有していた
///
/// 最初はリングを丸ごと写していた。**1.5 秒ぶんで 265 KB、写すのに 461 µs。**
/// 排出は 2ms ごとに回るので、**1秒あたり 230 ms を写すためだけに使っていた。**
/// 排出が実時間から遅れ、画面の波形が速くなったり遅くなったりした。**踏んだ。**
#[derive(Debug, Default)]
pub struct Envelope {
    /// 目盛りごとの min/max。**古いものが先頭。**
    pub steps: VecDeque<(f32, f32)>,
    /// 排出しはじめてからの通算フレーム数。**単調に増える。**
    ///
    /// **画面はこれで古い応答を捨てる。** 問い合わせが重なると
    /// 順序が入れ替わって届くことがあり、そのまま描くと**波形が巻き戻る。**
    pub position: u64,
}

impl Envelope {
    /// `buckets` 個の min/max へ畳んで、通算フレーム数と一緒に返す。
    #[must_use]
    pub fn sample(&self, buckets: usize) -> (Vec<(f32, f32)>, u64) {
        if buckets == 0 || self.steps.is_empty() {
            return (Vec::new(), self.position);
        }
        let out = (0..buckets)
            .map(|b| {
                let lo = b * self.steps.len() / buckets;
                let hi = ((b + 1) * self.steps.len() / buckets)
                    .max(lo + 1)
                    .min(self.steps.len());
                self.steps
                    .range(lo..hi)
                    .fold((0.0_f32, 0.0_f32), |(mn, mx), (l, h)| {
                        (mn.min(*l), mx.max(*h))
                    })
            })
            .collect();
        (out, self.position)
    }
}

/// 確定したテイク。
#[derive(Debug)]
pub struct Finished {
    /// 確定した WAV のパス。
    pub path: PathBuf,
    /// 書き込んだサンプル（プリロールを含む）。
    pub samples: Vec<f32>,
    /// プリロールから持ってきたフレーム数（`TR-REC-19`）。
    pub preroll_frames: usize,
}

/// 排出スレッドへの指示。
enum Cmd {
    Start {
        path: PathBuf,
        reply: Sender<Result<(), wav::WavError>>,
    },
    Finish {
        reply: Sender<Result<Finished, wav::WavError>>,
    },
}

/// 排出スレッド。**収録画面にいる間ずっと回っている。**
pub struct Pump {
    cmd: Sender<Cmd>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    /// いま何サンプル保持しているか。**待っているだけの状態を見分けるのに使う。**
    held: Arc<Mutex<usize>>,
    /// いま流れている音の包絡（`TR-REC-43`）。
    envelope: Arc<Mutex<Envelope>>,
    /// 直近に流れてきた音のピーク。**入力が届いているかの判定に使う**（`TR-REC-17`）。
    /// 読むたびに 0 へ戻すので、「前回見てから今までの最大」になる。
    recent_peak: Arc<Mutex<f32>>,
    /// 検査のあいだだけ、流れてきたものを丸ごと溜める（`TR-REC-24`）。
    /// **録音とは別の経路。** テイクの中身には混ぜない。
    probe: Arc<Mutex<Option<Vec<f32>>>>,
    rate_hz: u32,
}

impl std::fmt::Debug for Pump {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Pump")
            .field("rate_hz", &self.rate_hz)
            .finish_non_exhaustive()
    }
}

/// 排出に失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum PumpError {
    /// 排出スレッドが応答しない。
    #[error("排出スレッドが応答しない")]
    Gone,

    /// ファイルの書き込みに失敗した。
    #[error("テイクの書き込みに失敗した")]
    Wav(#[from] wav::WavError),
}

impl PumpError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Gone => "pump.gone",
            Self::Wav(e) => e.kind(),
        }
    }
}

impl Pump {
    /// 排出を始める。**この時点からプリロールが溜まりはじめる。**
    ///
    /// `device_rate_hz` はキャプチャが実際に開けたレート。
    /// **ここで 44100 へ落とすので、外へ出るものはすべてマスターの時間軸**
    /// （`TR-REC-02`）。
    #[must_use]
    pub fn start(consumer: ring::Consumer, device_rate_hz: u32) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let held = Arc::new(Mutex::new(0_usize));
        let envelope = Arc::new(Mutex::new(Envelope::default()));
        let recent_peak = Arc::new(Mutex::new(0.0_f32));
        let probe = Arc::new(Mutex::new(None));

        let handle = std::thread::spawn({
            let stop = Arc::clone(&stop);
            let shared = Shared {
                held: Arc::clone(&held),
                envelope: Arc::clone(&envelope),
                peak: Arc::clone(&recent_peak),
                probe: Arc::clone(&probe),
            };
            move || run(consumer, device_rate_hz, &cmd_rx, &stop, &shared)
        });

        Self {
            cmd: cmd_tx,
            stop,
            handle: Some(handle),
            held,
            envelope,
            recent_peak,
            probe,
            // **保持しているのは変換後のフレーム。** デバイスのレートで割ると狂う。
            rate_hz: MASTER_RATE_HZ,
        }
    }

    /// いま保持しているプリロールの長さ（ミリ秒）。
    ///
    /// **収録を始めてよいかの目安。** [`PREROLL_MS`] に足りていなければ、
    /// 遡れる分がその長さしかない。
    #[must_use]
    pub fn preroll_ms(&self) -> u64 {
        let held = self.held.lock().map(|g| *g).unwrap_or(0);
        held as u64 * 1000 / u64::from(self.rate_hz).max(1)
    }

    /// いま流れている音の包絡（`TR-REC-43`）。
    ///
    /// 直近 [`PREROLL_CAPACITY_MS`] を `buckets` 個の min/max に畳んで返す。
    /// あわせて、**通算フレーム数**を返す——画面はこれで古い応答を捨てる。
    ///
    /// **録音していなくても出る。** 収録画面に入った時点からリングは回っていて
    /// （`TR-REC-19`）、「マイクが拾っているか」は録る前に知りたい。
    ///
    /// **読んでも消えない。** ピーク（`TR-REC-17`）と違って、
    /// これは今の状態であって、区間の集計ではない。
    #[must_use]
    pub fn envelope(&self, buckets: usize) -> (Vec<(f32, f32)>, u64) {
        self.envelope
            .lock()
            .map_or_else(|_| (Vec::new(), 0), |g| g.sample(buckets))
    }

    /// 包絡そのものの持ち手（`TR-REC-43`）。
    ///
    /// **アプリの状態ロックの外から読むために出す。** 収録画面は 50ms ごとに
    /// 波形を引くが、テイクの確定はアライメントを含めて数秒かかる。
    /// **同じロックを通すと、その間ずっと詰まり、解けた瞬間に溜まった応答が
    /// 一気に返る**——順序が入れ替わって波形が巻き戻る。**踏んだ。**
    #[must_use]
    pub fn envelope_handle(&self) -> Arc<Mutex<Envelope>> {
        Arc::clone(&self.envelope)
    }

    /// 前回見てから今までの入力ピーク。**読むと 0 へ戻る**（`TR-REC-17`）。
    ///
    /// **ストリームを止めずに測る。** 止めて測ると、そのぶんプリロールが途切れる。
    #[must_use]
    pub fn take_peak(&self) -> f32 {
        self.recent_peak.lock().map_or(0.0, |mut g| {
            let v = *g;
            *g = 0.0;
            v
        })
    }

    /// 検査のための収集を始める（`TR-REC-24`）。
    ///
    /// **録音とは別の経路。** テイクの中身には混ざらない。
    pub fn begin_probe(&self) {
        if let Ok(mut g) = self.probe.lock() {
            *g = Some(Vec::new());
        }
    }

    /// 集めたものを取り出して、収集を終える。
    #[must_use]
    pub fn end_probe(&self) -> Vec<f32> {
        self.probe
            .lock()
            .map_or_else(|_| Vec::new(), |mut g| g.take().unwrap_or_default())
    }

    /// テイクを始める。**プリロールぶんを先に書き込む。**
    ///
    /// **レートは受け取らない。** マスターは常に 44100（`TR-REC-01`, `TR-REC-02`）で、
    /// **呼び出し側が別の値を渡せると、そこが壊れる口になる。**
    pub fn start_take(&self, path: PathBuf) -> Result<(), PumpError> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Start { path, reply: tx })
            .map_err(|_| PumpError::Gone)?;
        rx.recv().map_err(|_| PumpError::Gone)??;
        Ok(())
    }

    /// テイクを終える。**指示のあと [`TAIL_MS`] ぶん書いてから確定する。**
    pub fn finish_take(&self) -> Result<Finished, PumpError> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Finish { reply: tx })
            .map_err(|_| PumpError::Gone)?;
        Ok(rx.recv().map_err(|_| PumpError::Gone)??)
    }
}

impl Drop for Pump {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

/// 録音中の状態。
struct Recording {
    part: wav::PartialTake,
    path: PathBuf,
    samples: Vec<f32>,
    preroll_frames: usize,
    /// 終了の指示を受けたあと、あと何フレーム書くか。
    tail_left: Option<usize>,
    reply: Option<Sender<Result<Finished, wav::WavError>>>,
}

/// 排出スレッドと外側で分け合うもの。
///
/// **引数を並べると取り違える。** まとめて渡す。
struct Shared {
    /// 保持しているプリロールのフレーム数。
    held: Arc<Mutex<usize>>,
    /// いま流れている音の包絡（`TR-REC-43`）。
    envelope: Arc<Mutex<Envelope>>,
    /// 前回見てから今までの入力ピーク（`TR-REC-17`）。
    peak: Arc<Mutex<f32>>,
    /// 検査のための収集（`TR-REC-24`）。
    probe: Arc<Mutex<Option<Vec<f32>>>>,
}

fn run(
    consumer: ring::Consumer,
    device_rate_hz: u32,
    cmd: &Receiver<Cmd>,
    stop: &AtomicBool,
    shared: &Shared,
) {
    // **長さはすべてマスターの時間軸で数える。**
    let cap = (u64::from(MASTER_RATE_HZ) * PREROLL_CAPACITY_MS / 1000) as usize;
    let preroll_want = (u64::from(MASTER_RATE_HZ) * PREROLL_MS / 1000) as usize;
    let tail_want = (u64::from(MASTER_RATE_HZ) * TAIL_MS / 1000) as usize;

    // **キャプチャからマスターまでの、ただ1回の変換**（`TR-REC-02`）。
    // **テイクごとに作り直さない。** 収録中ストリームは開きっぱなしなので
    // （`REQ-REC-102`）、位相を持ち回さないとテイクの継ぎ目に段差が出る。
    let mut conv = match Resampler::to_master(device_rate_hz) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!(kind = e.kind(), "変換器を作れないので排出を始めない");
            return;
        }
    };
    tracing::info!(
        device_rate_hz,
        master_rate_hz = MASTER_RATE_HZ,
        resampler = koeru_audio::resample::IDENTIFIER,
        converting = !conv.is_passthrough(),
        "排出を始める"
    );

    let mut ring_buf: VecDeque<f32> = VecDeque::with_capacity(cap + CHUNK);
    let mut buf = vec![0.0_f32; CHUNK];
    let mut converted: Vec<f32> = Vec::with_capacity(CHUNK);
    // 波形の目盛り（`TR-REC-43`）。**積み上げ中のものと、まとまったもの。**
    let step_samples = (u64::from(MASTER_RATE_HZ) * ENVELOPE_STEP_MS / 1000).max(1) as usize;
    let mut step = (0.0_f32, 0.0_f32);
    let mut step_filled = 0_usize;
    let mut done_steps: Vec<(f32, f32)> = Vec::new();
    let mut rec: Option<Recording> = None;

    while !stop.load(Ordering::Acquire) {
        // ── 指示 ──
        match cmd.try_recv() {
            Ok(Cmd::Start { path, reply }) => {
                match wav::PartialTake::create(&path, MASTER_RATE_HZ) {
                    Ok(mut part) => {
                        // **押した瞬間より前の音を先に書く**（TR-REC-19）。
                        let n = preroll_want.min(ring_buf.len());
                        let head: Vec<f32> = ring_buf.iter().rev().take(n).rev().copied().collect();
                        let written = part.write(&head);
                        if let Err(e) = written {
                            let _ = reply.send(Err(e));
                        } else {
                            rec = Some(Recording {
                                part,
                                path,
                                samples: head,
                                preroll_frames: n,
                                tail_left: None,
                                reply: None,
                            });
                            let _ = reply.send(Ok(()));
                        }
                    }
                    Err(e) => {
                        let _ = reply.send(Err(e));
                    }
                }
            }
            Ok(Cmd::Finish { reply }) => match rec.as_mut() {
                Some(r) => {
                    // **フレームで数える。** 壁時計だと詰まったときに短く切れる。
                    r.tail_left = Some(tail_want);
                    r.reply = Some(reply);
                }
                None => {
                    // 録音していないのに終了を求められた。**握り潰さず切る。**
                    drop(reply);
                }
            },
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }

        // ── 排出 ──
        let n = consumer.pop(&mut buf);
        if n == 0 {
            // 末尾を待っている最中に何も来ないなら、そのぶんは無いものとして確定させる。
            if let Some(r) = rec.as_ref()
                && r.tail_left == Some(0)
            {
                finalize(&mut rec);
                continue;
            }
            std::thread::sleep(std::time::Duration::from_millis(IDLE_SLEEP_MS));
            continue;
        }
        // **ここで 1 回だけ 44100 へ落とす**（`TR-REC-02`）。
        // 以降はすべてマスターの時間軸。**塊の切れ目で段差は出ない。**
        converted.clear();
        conv.push(&buf[..n], &mut converted);
        if converted.is_empty() {
            // 変換の窓に足りなかった。**次の塊で出る。**
            continue;
        }
        let got: &[f32] = &converted;

        // プリロールは常に回す。**録音中も止めない**（次のテイクが続けて来る）。
        ring_buf.extend(got.iter().copied());
        while ring_buf.len() > cap {
            ring_buf.pop_front();
        }
        if let Ok(mut g) = shared.held.lock() {
            *g = ring_buf.len();
        }
        // **波形の目盛りを積む**（`TR-REC-43`）。
        // **写すのは目盛りだけ。** 生の音を写すと、排出が実時間に追いつかない。
        for v in got {
            step.0 = step.0.min(*v);
            step.1 = step.1.max(*v);
            step_filled += 1;
            if step_filled >= step_samples {
                done_steps.push(step);
                step = (0.0, 0.0);
                step_filled = 0;
            }
        }
        if !done_steps.is_empty()
            && let Ok(mut g) = shared.envelope.lock()
        {
            for st in done_steps.drain(..) {
                g.steps.push_back(st);
            }
            while g.steps.len() > ENVELOPE_STEPS {
                g.steps.pop_front();
            }
            // **数えるのは変換後のフレーム。** `n` は入力のぶんで、
            // 48000 から落とすと 8.8% 多い（`TR-REC-02`）。
            g.position += got.len() as u64;
        }
        if let Ok(mut g) = shared.peak.lock() {
            *g = got.iter().fold(*g, |m, v| m.max(v.abs()));
        }
        if let Ok(mut g) = shared.probe.lock()
            && let Some(buf) = g.as_mut()
        {
            buf.extend_from_slice(got);
        }

        if let Some(r) = rec.as_mut() {
            // 末尾を延ばしている最中なら、必要なぶんだけ取る。
            let n = got.len();
            let take_n = r.tail_left.map_or(n, |left| left.min(n));
            if take_n > 0 {
                if let Err(e) = r.part.write(&got[..take_n]) {
                    if let Some(reply) = r.reply.take() {
                        let _ = reply.send(Err(e));
                    }
                    rec = None;
                    continue;
                }
                r.samples.extend_from_slice(&got[..take_n]);
            }
            if let Some(left) = r.tail_left.as_mut() {
                *left = left.saturating_sub(take_n);
                if *left == 0 {
                    finalize(&mut rec);
                }
            }
        }
    }

    // 止められたときに録音が残っていたら、そこまでを確定させる。
    // **書きかけを捨てない。** 押した本人にとっては録れたはずのもの。
    if rec.is_some() {
        finalize(&mut rec);
    }
}

/// 確定させて、待っている呼び出し元へ返す。
fn finalize(rec: &mut Option<Recording>) {
    let Some(r) = rec.take() else { return };
    let Recording {
        part,
        path,
        samples,
        preroll_frames,
        reply,
        ..
    } = r;
    let result = part.finalize().map(|p| Finished {
        path: if p.as_os_str().is_empty() { path } else { p },
        samples,
        preroll_frames,
    });
    if let Some(reply) = reply {
        let _ = reply.send(result);
    }
}
