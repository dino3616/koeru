//! リングから排出してディスクへ落とす層（`TR-REC-19`, `TR-REC-20`）。
//!
//! ストリームはテイクごとに開閉しない（`REQ-REC-102`）。収録画面に入った時点で
//! 開き、フレーズ間も止めない。ここはその前提の上で、
//! 録音開始の指示より前の音を捨てないために居る。
//!
//! # なぜ遡るのか
//!
//! 人は「録音」を押してから息を吸わない。押した瞬間にはもう発声が始まっている。
//! 指示の時点から書き始めると、語頭の子音が欠ける。子音が欠けた素材は
//! oto の導出も合成も救えないので、構造的に起こらないようにする（`TR-REC-19`）。
//!
//! 常に直近 [`PREROLL_CAPACITY_MS`] を持ち回し、開始の指示で
//! [`PREROLL_MS`] ぶんを先に書き込む。終了の指示のあとも [`TAIL_MS`] ぶん書き続ける。
//!
//! # 遡る起点は指示の時点
//!
//! 起点は [`Pump::position`] で指示を受けた時点に取り、[`Pump::start_take`] へ渡す。
//! 排出スレッドが開始を受け取った時点から遡ると、そのあいだに台帳へ予定を書く時間
//! （`DEC-REC-010`）のぶんだけ起点が後ろへずれ、遡ったはずの 500ms が欠ける。
//! 位置は通算のフレーム数で持つ（`DEC-REC-007`）。
//!
//! # 時計を使わない
//!
//! 末尾の延長はフレーム数で数える。壁時計で測ると、排出が詰まったときに
//! 実際より短く切れる。音の時間軸で数えれば、詰まっても長さは変わらない。
//!
//! # 44100 へ落とすのはここ
//!
//! キャプチャはデバイスのネイティブレートで受ける（`TR-REC-02`、`TR-REC-05`）。
//! リングから出した直後に1回だけ 44100 へ変換し、以降はすべてマスターの時間軸で扱う。
//! プリロールもピークも検査用の収集も、書き出すテイクも、全部 44100。
//!
//! ここより下流でレートを持ち回らない。 持ち回ると、どこかで取り違える——
//! 実際、変換そのものが抜けていて 48000 Hz のマスターが書かれていた（`DEC-REC-006`）。
//! `write_distribution` はヘッダに 44100 と書くだけなので、そのまま配ると
//! 44100 と名乗る 48000 の音になる。

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use koeru_audio::resample::Resampler;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_audio::{ring, wav};

/// 常時保持する長さ（ミリ秒）。`TR-REC-19` の下限は 1000ms。
pub const PREROLL_CAPACITY_MS: u64 = 1500;

/// 開始の指示から遡る長さ（ミリ秒、`TR-REC-19`）。
pub const PREROLL_MS: u64 = 500;

/// 終了の指示から延ばす長さ（ミリ秒、`TR-REC-19`）。
pub const TAIL_MS: u64 = 500;

/// 1回の排出で読む長さ。
const CHUNK: usize = 8192;

/// 波形の1目盛りの長さ（ミリ秒、`TR-REC-43`）。
///
/// 画面の更新間隔より細かくする。 粗いと、目盛りが1つ増えるまで絵が動かない。
const ENVELOPE_STEP_MS: u64 = 5;

/// 保持する目盛りの数。[`PREROLL_CAPACITY_MS`] ぶん。
const ENVELOPE_STEPS: usize = (PREROLL_CAPACITY_MS / ENVELOPE_STEP_MS) as usize;

/// 排出するものが無いときに待つ時間。
const IDLE_SLEEP_MS: u64 = 2;

/// いま流れている音の包絡（`TR-REC-43`）。
///
/// 波形そのものは持たない。 目盛りごとの min/max だけを積む。
///
/// リングを丸ごと写す形をやめた経緯と実測は `DEC-PLT-017`。
#[derive(Debug, Default)]
pub struct Envelope {
    /// 目盛りごとの min/max。古いものが先頭。
    pub steps: VecDeque<(f32, f32)>,
    /// フルスケールに達した回数（`TR-REC-16` の定義）。
    ///
    /// **画面に数えさせない。** 目盛りは 1.5 秒ぶんの窓を丸ごと渡すので、
    /// 1つの割れが 30 回ぶんの通知に残り続ける——画面側で「窓が割れているか」を
    /// 通知ごとに数えると、**割れた回数ではなく更新の回数を数えることになる。**
    /// ここは流れてくるサンプルを直接見ているので、連続長で正しく数えられる。
    ///
    /// 数えはじめはストリームを開いたとき。 テイクごとの数は
    /// `TakeMetrics::full_scale_runs` が持つ（`TR-REC-16` の正本はあちら）。
    pub clipped_runs: u64,
    /// 排出しはじめてからの通算フレーム数。単調に増える。
    ///
    /// 画面はこれで古い応答を捨てる。 問い合わせが重なると
    /// 順序が入れ替わって届くことがあり、そのまま描くと波形が巻き戻る。
    pub position: u64,
}

impl Envelope {
    /// 目盛りをそのまま、通算フレーム数と一緒に返す。
    ///
    /// # 畳まない
    ///
    /// 好きな本数へ畳ませない。 目盛りは 1.5 秒ぶんで 300 本あり、
    /// 50ms ごとに 10 本ずつ入れ替わる。**300 を 240 のような割り切れない数へ畳むと、
    /// 入れ替わるたびに目盛りと枠の対応がずれ、絵が揺れる**
    /// ——「速度が一定じゃない」に見える。
    ///
    /// 画面は1本につき1列を描く。10 本ずれれば 10 列ずれるだけで、
    /// 位置の対応が毎回同じになる。
    #[must_use]
    pub fn sample(&self) -> (Vec<(f32, f32)>, u64, u64) {
        (
            self.steps.iter().copied().collect(),
            self.position,
            self.clipped_runs,
        )
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
        /// 指示を受けた時点のリングの位置（[`Pump::position`]）。 ここから遡る。
        from: u64,
        reply: Sender<Result<(), wav::WavError>>,
    },
    Finish {
        reply: Sender<Result<Finished, PumpError>>,
    },
}

/// 排出スレッド。収録画面にいる間ずっと回っている。
pub struct Pump {
    cmd: Sender<Cmd>,
    stop: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
    /// いま何サンプル保持しているか。待っているだけの状態を見分けるのに使う。
    held: Arc<Mutex<usize>>,
    /// いま流れている音の包絡（`TR-REC-43`）。
    envelope: Arc<Mutex<Envelope>>,
    /// 直近に流れてきた音のピーク。入力が届いているかの判定に使う（`TR-REC-17`）。
    /// 読むたびに 0 へ戻すので、「前回見てから今までの最大」になる。
    recent_peak: Arc<Mutex<f32>>,
    /// 検査のあいだだけ、流れてきたものを丸ごと溜める（`TR-REC-24`）。
    /// 録音とは別の経路。 テイクの中身には混ぜない。
    probe: Arc<Mutex<Option<Vec<f32>>>>,
    /// リングへ入れた通算フレーム数（マスターの時間軸）。
    position: Arc<AtomicU64>,
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

    /// rename は済んだが、置き場所のディレクトリを fsync できなかった。
    ///
    /// WAV は確定した名前でそこにある。 ただ電源を失うと名前ごと消えうるので、
    /// 台帳には載せない（`DEC-REC-004`）。
    #[error("テイクの置き場所を永続化できなかった")]
    SyncDir(#[source] std::io::Error),
}

impl koeru_failure::Failure for PumpError {
    fn code(&self) -> &'static str {
        match self {
            Self::Gone => "pump.gone",
            Self::Wav(e) => e.code(),
            Self::SyncDir(_) => "pump.sync_dir_failed",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            // 排出スレッドは収録の寿命のあいだ生きている前提。
            Self::Gone => koeru_failure::Class::Internal,
            Self::Wav(e) => e.class(),
            Self::SyncDir(e) => koeru_failure::io_class(e),
        }
    }
}

impl Pump {
    /// 排出を始める。この時点からプリロールが溜まりはじめる。
    ///
    /// `device_rate_hz` はキャプチャが実際に開けたレート。
    /// ここで 44100 へ落とすので、外へ出るものはすべてマスターの時間軸
    /// （`TR-REC-02`）。
    #[must_use]
    pub fn start(consumer: ring::Consumer, device_rate_hz: u32) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let held = Arc::new(Mutex::new(0_usize));
        let envelope = Arc::new(Mutex::new(Envelope::default()));
        let recent_peak = Arc::new(Mutex::new(0.0_f32));
        let probe = Arc::new(Mutex::new(None));
        let position = Arc::new(AtomicU64::new(0));

        let handle = std::thread::spawn({
            let stop = Arc::clone(&stop);
            let shared = Shared {
                held: Arc::clone(&held),
                envelope: Arc::clone(&envelope),
                peak: Arc::clone(&recent_peak),
                probe: Arc::clone(&probe),
                position: Arc::clone(&position),
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
            position,
            // 保持しているのは変換後のフレーム。 デバイスのレートで割ると狂う。
            rate_hz: MASTER_RATE_HZ,
        }
    }

    /// いまのリングの位置。 リングへ入れた通算フレーム数（マスターの時間軸）。
    ///
    /// 録音の指示を受けたらまずここを読み、[`Self::start_take`] へ渡す（`TR-REC-19`）。
    #[must_use]
    pub fn position(&self) -> u64 {
        self.position.load(Ordering::Acquire)
    }

    /// いま保持しているプリロールの長さ（ミリ秒）。
    ///
    /// 収録を始めてよいかの目安。 [`PREROLL_MS`] に足りていなければ、
    /// 遡れる分がその長さしかない。
    #[must_use]
    pub fn preroll_ms(&self) -> u64 {
        let held = self.held.lock().map(|g| *g).unwrap_or(0);
        held as u64 * 1000 / u64::from(self.rate_hz).max(1)
    }

    /// いま流れている音の包絡（`TR-REC-43`）。
    ///
    /// 直近 [`PREROLL_CAPACITY_MS`] を `buckets` 個の min/max に畳んで返す。
    /// あわせて、通算フレーム数を返す——画面はこれで古い応答を捨てる。
    ///
    /// 録音していなくても出る。 収録画面に入った時点からリングは回っていて
    /// （`TR-REC-19`）、「マイクが拾っているか」は録る前に知りたい。
    ///
    /// 読んでも消えない。 ピーク（`TR-REC-17`）と違って、
    /// これは今の状態であって、区間の集計ではない。
    #[must_use]
    pub fn envelope(&self) -> (Vec<(f32, f32)>, u64, u64) {
        self.envelope
            .lock()
            .map_or_else(|_| (Vec::new(), 0, 0), |g| g.sample())
    }

    /// 包絡そのものの持ち手（`TR-REC-43`）。
    ///
    /// アプリの状態ロックの外から読むために出す。 テイクの確定はアライメントを
    /// 含めて数秒かかるので、同じロックを通すとその間ずっと波形が止まる。
    ///
    /// 止まること自体が問題。 巻き戻りの原因はロックではない
    /// （`DEC-PLT-017` が3つの原因を分けている）。
    #[must_use]
    pub fn envelope_handle(&self) -> Arc<Mutex<Envelope>> {
        Arc::clone(&self.envelope)
    }

    /// 前回見てから今までの入力ピーク。読むと 0 へ戻る（`TR-REC-17`）。
    ///
    /// ストリームを止めずに測る。 止めて測ると、そのぶんプリロールが途切れる。
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
    /// 録音とは別の経路。 テイクの中身には混ざらない。
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

    /// テイクを始める。 `from`（指示を受けた時点の [`Self::position`]）から
    /// [`PREROLL_MS`] 遡った位置を頭にし、そこから今までのぶんを先に書き込む。
    ///
    /// レートは受け取らない。 マスターは常に 44100（`TR-REC-01`, `TR-REC-02`）で、
    /// 呼び出し側が別の値を渡せると、そこが壊れる口になる。
    pub fn start_take(&self, path: PathBuf, from: u64) -> Result<(), PumpError> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Start {
                path,
                from,
                reply: tx,
            })
            .map_err(|_| PumpError::Gone)?;
        rx.recv().map_err(|_| PumpError::Gone)??;
        Ok(())
    }

    /// テイクを終える。指示のあと [`TAIL_MS`] ぶん書いてから確定する。
    ///
    /// 確定は WAV の fsync・rename と、置き場所のディレクトリの fsync まで。
    pub fn finish_take(&self) -> Result<Finished, PumpError> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Finish { reply: tx })
            .map_err(|_| PumpError::Gone)?;
        rx.recv().map_err(|_| PumpError::Gone)?
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
    reply: Option<Sender<Result<Finished, PumpError>>>,
}

/// 排出スレッドと外側で分け合うもの。
///
/// 引数を並べると取り違える。 まとめて渡す。
struct Shared {
    /// 保持しているプリロールのフレーム数。
    held: Arc<Mutex<usize>>,
    /// いま流れている音の包絡（`TR-REC-43`）。
    envelope: Arc<Mutex<Envelope>>,
    /// 前回見てから今までの入力ピーク（`TR-REC-17`）。
    peak: Arc<Mutex<f32>>,
    /// 検査のための収集（`TR-REC-24`）。
    probe: Arc<Mutex<Option<Vec<f32>>>>,
    /// リングへ入れた通算フレーム数（[`Pump::position`]）。
    position: Arc<AtomicU64>,
}

fn run(
    mut consumer: ring::Consumer,
    device_rate_hz: u32,
    cmd: &Receiver<Cmd>,
    stop: &AtomicBool,
    shared: &Shared,
) {
    // 長さはすべてマスターの時間軸で数える。
    let cap = (u64::from(MASTER_RATE_HZ) * PREROLL_CAPACITY_MS / 1000) as usize;
    let preroll_want = (u64::from(MASTER_RATE_HZ) * PREROLL_MS / 1000) as usize;
    let tail_want = (u64::from(MASTER_RATE_HZ) * TAIL_MS / 1000) as usize;

    // キャプチャからマスターまでの、ただ1回の変換（`TR-REC-02`）。
    // テイクごとに作り直さない。 収録中ストリームは開きっぱなしなので
    // （`REQ-REC-102`）、位相を持ち回さないとテイクの継ぎ目に段差が出る。
    let mut conv = match Resampler::to_master(device_rate_hz) {
        Ok(c) => c,
        // 変換器を作れないので排出を始めない。書きかけも作らない。
        Err(e) => {
            koeru_failure::record_failure(&e, koeru_failure::Outcome::NotStarted, "pump.start");
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
    // 波形の目盛り（`TR-REC-43`）。積み上げ中のものと、まとまったもの。
    let step_samples = (u64::from(MASTER_RATE_HZ) * ENVELOPE_STEP_MS / 1000).max(1) as usize;
    let mut step = (0.0_f32, 0.0_f32);
    let mut step_filled = 0_usize;
    let mut done_steps: Vec<(f32, f32)> = Vec::new();
    // まだ公開していないフレーム数。 目盛りが揃った回にまとめて足す。
    // 揃わなかった回のぶんを落とすと、通算が実時間から少しずつずれる。
    let mut carried = 0_u64;
    // 割れた回数（`TR-REC-16`）。塊の切れ目で連続を切らないよう持ち越す。
    let mut clip = koeru_core::analysis::FullScaleCounter::default();
    let mut rec: Option<Recording> = None;
    // リングへ入れた通算フレーム数。 `ring_buf` が持つのは `[total - len, total)`。
    let mut total = 0_u64;

    while !stop.load(Ordering::Acquire) {
        // ## 指示
        match cmd.try_recv() {
            Ok(Cmd::Start { path, from, reply }) => {
                match wav::PartialTake::create(&path, MASTER_RATE_HZ) {
                    Ok(mut part) => {
                        // 押した瞬間より前の音を先に書く（`TR-REC-19`）。 指示のあとに
                        // 流れてきたぶんも、もう録音の中身なので続けて書く。
                        let head = head_from(&ring_buf, total, from, preroll_want as u64);
                        let written = part.write(&head.samples);
                        if let Err(e) = written {
                            let _ = reply.send(Err(e));
                        } else {
                            rec = Some(Recording {
                                part,
                                path,
                                samples: head.samples,
                                preroll_frames: head.preroll_frames,
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
                    // フレームで数える。 壁時計だと詰まったときに短く切れる。
                    r.tail_left = Some(tail_want);
                    r.reply = Some(reply);
                }
                None => {
                    // 録音していないのに終了を求められた。握り潰さず切る。
                    drop(reply);
                }
            },
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => break,
        }

        // ## 排出
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
        // ここで 1 回だけ 44100 へ落とす（`TR-REC-02`）。
        // 以降はすべてマスターの時間軸。塊の切れ目で段差は出ない。
        converted.clear();
        conv.push(&buf[..n], &mut converted);
        if converted.is_empty() {
            // 変換の窓に足りなかった。次の塊で出る。
            continue;
        }
        let got: &[f32] = &converted;

        // プリロールは常に回す。録音中も止めない（次のテイクが続けて来る）。
        ring_buf.extend(got.iter().copied());
        while ring_buf.len() > cap {
            ring_buf.pop_front();
        }
        total += got.len() as u64;
        shared.position.store(total, Ordering::Release);
        if let Ok(mut g) = shared.held.lock() {
            *g = ring_buf.len();
        }
        // 波形の目盛りを積む（`TR-REC-43`）。
        // 写すのは目盛りだけ。 生の音を写すと、排出が実時間に追いつかない。
        carried += got.len() as u64;
        for v in got {
            // 割れた回数は連続長で数える（`TR-REC-16`）。塊をまたいで持ち越す。
            clip.push(*v);
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
            // 数えるのは変換後のフレーム（`n` は入力のぶんで 8.8% 多い、`TR-REC-02`）。
            // 持ち越したぶんも足す。 目盛りが揃わなかった回を落とさない。
            g.position += carried;
            carried = 0;
            g.clipped_runs = clip.runs();
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
                        let _ = reply.send(Err(e.into()));
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
    // 書きかけを捨てない。 押した本人にとっては録れたはずのもの。
    if rec.is_some() {
        finalize(&mut rec);
    }
}

/// 録音の頭。
struct Head {
    samples: Vec<f32>,
    /// そのうち指示より前のフレーム数（`TR-REC-19` のプリロール）。
    preroll_frames: usize,
}

/// リングから録音の頭を切り出す。 `ring` は通算で `[total - len, total)` を持ち、
/// `from` は指示を受けた時点の位置。 `from - preroll` から今までを返す。
///
/// 遡る先がもうリングに無ければ、残っている最も古いところから。 指示から排出スレッドが
/// 開始を受け取るまでがリングの長さを超えたときだけ起き、そのときは記録して進む。
fn head_from(ring: &VecDeque<f32>, total: u64, from: u64, preroll: u64) -> Head {
    let from = from.min(total);
    let oldest = total - ring.len() as u64;
    let begin = from.saturating_sub(preroll).max(oldest);
    if from.saturating_sub(preroll) < oldest {
        tracing::warn!(
            held = ring.len(),
            want = preroll + (total - from),
            "指示の時点まで遡れない"
        );
    }
    let skip = usize::try_from(begin - oldest).unwrap_or(usize::MAX);
    Head {
        samples: ring.iter().skip(skip).copied().collect(),
        preroll_frames: usize::try_from(from.saturating_sub(begin)).unwrap_or(0),
    }
}

/// 確定させて、待っている呼び出し元へ返す。
///
/// 確定は WAV の fsync と rename、そのあと置き場所のディレクトリの fsync。
/// ディレクトリを fsync しないと、電源を失ったときに rename ごと消えうる。
/// 止められたときの確定（台帳には載らず孤児になる）も同じ手順を通す——孤児も
/// 本人が採るまで残す録音（`REQ-REC-006`）。
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
    let result = part.finalize().map_err(PumpError::from).and_then(|p| {
        let p = if p.as_os_str().is_empty() { path } else { p };
        if let Some(dir) = p.parent() {
            koeru_core::project::sync_dir(dir).map_err(PumpError::SyncDir)?;
        }
        Ok(Finished {
            path: p,
            samples,
            preroll_frames,
        })
    });
    if let Some(reply) = reply {
        let _ = reply.send(result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ring(total: u64, len: u64) -> VecDeque<f32> {
        #[allow(clippy::cast_precision_loss, reason = "試験の値は小さい")]
        (total - len..total).map(|i| i as f32).collect()
    }

    #[test]
    fn 指示の位置から遡り_そのあとのぶんも続けて書く() {
        // 通算 1000 フレームまで入っていて、リングは直近 600 を持つ。 指示は 900 の時点。
        let head = head_from(&ring(1000, 600), 1000, 900, 200);
        assert_eq!(head.preroll_frames, 200);
        assert_eq!(
            head.samples.first().copied(),
            Some(700.0),
            "900 から 200 遡る"
        );
        assert_eq!(
            head.samples.last().copied(),
            Some(999.0),
            "指示のあとも欠けない"
        );
        assert_eq!(head.samples.len(), 300);
    }

    /// 排出スレッドが開始を受け取った時点から遡ると、指示から受け取るまでのぶん
    /// （台帳へ予定を書く時間）だけ頭がずれる。 起点は渡された位置で決まる。
    #[test]
    fn 遅れて受け取っても頭は動かない() {
        let now = head_from(&ring(900, 900), 900, 900, 200);
        let late = head_from(&ring(1400, 1000), 1400, 900, 200);
        assert_eq!(now.samples.first().copied(), Some(700.0));
        assert_eq!(late.samples.first().copied(), Some(700.0));
        assert_eq!(now.preroll_frames, late.preroll_frames);
    }

    #[test]
    fn リングの長さを超えて遅れたら残っているところから() {
        let head = head_from(&ring(2000, 600), 2000, 900, 200);
        assert_eq!(head.samples.first().copied(), Some(1400.0));
        assert_eq!(head.preroll_frames, 0);
    }

    #[test]
    fn 溜まりきる前は溜まったぶんだけ遡る() {
        let head = head_from(&ring(100, 100), 100, 100, 200);
        assert_eq!(head.samples.first().copied(), Some(0.0));
        assert_eq!(head.preroll_frames, 100);
    }

    /// 本物の排出スレッドで、指示の位置を渡してから遅れて始めても頭が動かない。
    ///
    /// 流す値は通算の位置そのもの。 テイクの先頭の値を見れば、どこから遡ったかが分かる。
    #[test]
    fn 排出スレッドは渡された位置から遡る() {
        use std::time::{Duration, Instant};

        let (mut tx, rx) = koeru_audio::ring::channel(1 << 16);
        let pump = Pump::start(rx, MASTER_RATE_HZ);
        // マイクと同じく、止めるまで流し続ける。 末尾の延長（`TAIL_MS`）はフレームで数えるので、
        // 流れが止まると確定しない。
        let feeding = Arc::new(AtomicBool::new(true));
        let feeder = std::thread::spawn({
            let feeding = Arc::clone(&feeding);
            move || {
                let mut next = 0_u32;
                while feeding.load(Ordering::Acquire) {
                    #[allow(clippy::cast_precision_loss, reason = "2^24 までは正確")]
                    let v: Vec<f32> = (next..next + 512).map(|i| i as f32).collect();
                    next += u32::try_from(tx.push(&v)).unwrap_or(0);
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        });
        let wait_until = |pos: u64| {
            let deadline = Instant::now() + Duration::from_secs(10);
            while pump.position() < pos {
                assert!(Instant::now() < deadline, "排出が進まない");
                std::thread::sleep(Duration::from_millis(1));
            }
        };

        wait_until(30_000);
        let from = pump.position();
        // 予定を書いているあいだにも音は流れてくる。
        wait_until(from + 10_000);

        let dir = std::env::temp_dir().join(format!("koeru-pump-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("作れる");
        let path = dir.join("head_1.wav");
        pump.start_take(path.clone(), from).expect("始められる");
        let finished = pump.finish_take().expect("確定できる");
        feeding.store(false, Ordering::Release);
        feeder.join().expect("止まる");

        let preroll = u64::from(MASTER_RATE_HZ) * PREROLL_MS / 1000;
        assert_eq!(finished.preroll_frames as u64, preroll);
        #[allow(clippy::cast_precision_loss, reason = "2^24 までは正確")]
        let want = (from - preroll) as f32;
        assert_eq!(finished.samples.first().copied(), Some(want));
        assert!(path.exists(), "確定した名前で置かれる");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
