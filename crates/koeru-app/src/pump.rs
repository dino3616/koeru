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

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

use koeru_audio::{ring, wav};

/// 常時保持する長さ（ミリ秒）。**`TR-REC-19` の下限は 1000ms。**
pub const PREROLL_CAPACITY_MS: u64 = 1500;

/// 開始の指示から遡る長さ（ミリ秒、`TR-REC-19`）。
pub const PREROLL_MS: u64 = 500;

/// 終了の指示から延ばす長さ（ミリ秒、`TR-REC-19`）。
pub const TAIL_MS: u64 = 500;

/// 1回の排出で読む長さ。
const CHUNK: usize = 8192;

/// 排出するものが無いときに待つ時間。
const IDLE_SLEEP_MS: u64 = 2;

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
        rate_hz: u32,
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
    /// 直近に流れてきた音のピーク。**入力が届いているかの判定に使う**（`TR-REC-17`）。
    /// 読むたびに 0 へ戻すので、「前回見てから今までの最大」になる。
    recent_peak: Arc<Mutex<f32>>,
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
    #[must_use]
    pub fn start(consumer: ring::Consumer, rate_hz: u32) -> Self {
        let (cmd_tx, cmd_rx) = channel();
        let stop = Arc::new(AtomicBool::new(false));
        let held = Arc::new(Mutex::new(0_usize));
        let recent_peak = Arc::new(Mutex::new(0.0_f32));

        let handle = std::thread::spawn({
            let stop = Arc::clone(&stop);
            let held = Arc::clone(&held);
            let peak = Arc::clone(&recent_peak);
            move || run(consumer, rate_hz, &cmd_rx, &stop, &held, &peak)
        });

        Self {
            cmd: cmd_tx,
            stop,
            handle: Some(handle),
            held,
            recent_peak,
            rate_hz,
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

    /// テイクを始める。**プリロールぶんを先に書き込む。**
    pub fn start_take(&self, path: PathBuf, rate_hz: u32) -> Result<(), PumpError> {
        let (tx, rx) = channel();
        self.cmd
            .send(Cmd::Start {
                path,
                rate_hz,
                reply: tx,
            })
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

fn run(
    consumer: ring::Consumer,
    rate_hz: u32,
    cmd: &Receiver<Cmd>,
    stop: &AtomicBool,
    held: &Mutex<usize>,
    recent_peak: &Mutex<f32>,
) {
    let cap = (u64::from(rate_hz) * PREROLL_CAPACITY_MS / 1000) as usize;
    let preroll_want = (u64::from(rate_hz) * PREROLL_MS / 1000) as usize;
    let tail_want = (u64::from(rate_hz) * TAIL_MS / 1000) as usize;

    let mut ring_buf: VecDeque<f32> = VecDeque::with_capacity(cap + CHUNK);
    let mut buf = vec![0.0_f32; CHUNK];
    let mut rec: Option<Recording> = None;

    while !stop.load(Ordering::Acquire) {
        // ── 指示 ──
        match cmd.try_recv() {
            Ok(Cmd::Start {
                path,
                rate_hz: r,
                reply,
            }) => {
                match wav::PartialTake::create(&path, r) {
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
        let got = &buf[..n];

        // プリロールは常に回す。**録音中も止めない**（次のテイクが続けて来る）。
        ring_buf.extend(got.iter().copied());
        while ring_buf.len() > cap {
            ring_buf.pop_front();
        }
        if let Ok(mut g) = held.lock() {
            *g = ring_buf.len();
        }
        if let Ok(mut g) = recent_peak.lock() {
            *g = got.iter().fold(*g, |m, v| m.max(v.abs()));
        }

        if let Some(r) = rec.as_mut() {
            // 末尾を延ばしている最中なら、必要なぶんだけ取る。
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
