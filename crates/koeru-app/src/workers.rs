//! 背後で回す仕事（`TR-SYN-04`, `TR-SYN-34`）。
//!
//! **録音入力より低い優先度で動かす**（`TR-SYN-34`）。
//! 録音のオーディオコールバックを妨げない。
//!
//! # 優先順位
//!
//! **録音直後の前処理 > 試唱の要求 > 全件再推定 > チャート事前計算**（`TR-SYN-34`）。
//!
//! この順にする理由は、**待っている人がいるかどうか**。
//! 録音直後の前処理は「次に試唱を押す人」を待たせる。試唱の要求は「いま押した人」。
//! 全件再推定とチャートは誰も待っていない。
//!
//! # 完了期限
//!
//! **「次の録音項目まで」ではなく「試唱押下まで」**（`TR-SYN-34`）。
//! 3時間の収録の途中で、次のフレーズを出すのを待たせない。
//! キューに積んで、録音セッション全体または休止中に消化する。

use std::collections::BinaryHeap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;

/// 仕事の優先度（`TR-SYN-34`）。
///
/// **数が大きいほど先に回る。**
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    /// チャートの事前計算。**誰も待っていない。**
    ChartPrecompute = 0,
    /// 全件再推定。**誰も待っていない。**
    Reestimate = 1,
    /// 試唱の要求。**いま押した人が待っている。**
    PreviewRequest = 2,
    /// 録音直後の前処理。**次に試唱を押す人を待たせる。**
    PostRecording = 3,
}

/// 積んだ仕事。
struct Job {
    priority: Priority,
    /// 積んだ順。**同じ優先度なら先に積んだものから。**
    seq: u64,
    run: Box<dyn FnOnce() + Send>,
}

impl PartialEq for Job {
    fn eq(&self, other: &Self) -> bool {
        self.priority == other.priority && self.seq == other.seq
    }
}
impl Eq for Job {}
impl Ord for Job {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // 優先度は大きいほど先。同じなら seq が小さいほど先。
        self.priority
            .cmp(&other.priority)
            .then_with(|| other.seq.cmp(&self.seq))
    }
}
impl PartialOrd for Job {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// 待っている仕事の列の持ち手（`TR-SYN-33`）。
pub type PendingHandle = Arc<(Mutex<Queue>, Condvar)>;

/// 待っている仕事の数を、持ち手から読む（`TR-SYN-33`）。
#[must_use]
pub fn pending_of(queue: &(Mutex<Queue>, Condvar)) -> usize {
    let (lock, _) = queue;
    lock.lock().map_or(0, |q| q.jobs.len())
}

#[derive(Default)]
pub struct Queue {
    jobs: BinaryHeap<Job>,
    stopped: bool,
}

// `Job` は閉包を持つので Debug を実装しない。**中身は出さず、数だけ出す。**
impl std::fmt::Debug for Queue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Queue")
            .field("jobs", &self.jobs.len())
            .field("stopped", &self.stopped)
            .finish()
    }
}

/// 背後で仕事を回す（`TR-SYN-34`）。
///
/// **録音入力とは別のスレッドで動かす。** Rust には GC が無いので、
/// 停止要因は確保・解放とロックに限られる——それを録音の側から遠ざける。
pub struct Workers {
    queue: Arc<(Mutex<Queue>, Condvar)>,
    seq: AtomicUsize,
    /// いま走っている仕事を止める合図。
    cancel: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for Workers {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Workers")
            .field("pending", &self.pending())
            .finish_non_exhaustive()
    }
}

impl Workers {
    /// 回しはじめる。
    #[must_use]
    pub fn start() -> Self {
        let queue = Arc::new((Mutex::new(Queue::default()), Condvar::new()));
        let cancel = Arc::new(AtomicBool::new(false));

        let handle = std::thread::spawn({
            let queue = Arc::clone(&queue);
            move || {
                loop {
                    let job = {
                        let (lock, cv) = &*queue;
                        let Ok(mut q) = lock.lock() else { break };
                        while q.jobs.is_empty() && !q.stopped {
                            let Ok(next) = cv.wait(q) else { return };
                            q = next;
                        }
                        if q.stopped && q.jobs.is_empty() {
                            break;
                        }
                        q.jobs.pop()
                    };
                    if let Some(job) = job {
                        (job.run)();
                    }
                }
            }
        });

        Self {
            queue,
            seq: AtomicUsize::new(0),
            cancel,
            handle: Some(handle),
        }
    }

    /// 仕事を積む。
    pub fn submit(&self, priority: Priority, run: impl FnOnce() + Send + 'static) {
        let seq = self.seq.fetch_add(1, Ordering::Relaxed) as u64;
        let (lock, cv) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.jobs.push(Job {
                priority,
                seq,
                run: Box::new(run),
            });
            cv.notify_one();
        }
    }

    /// まだ回していない仕事の数。
    #[must_use]
    pub fn pending(&self) -> usize {
        pending_of(&self.queue)
    }

    /// 待ち数だけを読むための持ち手（`TR-SYN-33`）。
    ///
    /// **アプリの状態ロックの外から読むために出す。** 画面は「背後で待っている仕事」を
    /// 定期的に出すが、テイクの確定はアライメントを含めて数秒かかる。
    /// **同じロックを通すと、待ち数がいちばん動くはずの時間に止まる。**
    #[must_use]
    pub fn pending_handle(&self) -> PendingHandle {
        Arc::clone(&self.queue)
    }

    /// 積んである仕事を捨てる（`TR-SYN-27`）。
    ///
    /// **走っている仕事は止めない。** 止めるのはそれぞれの仕事の中の合図で行う。
    pub fn clear(&self) {
        let (lock, _) = &*self.queue;
        if let Ok(mut q) = lock.lock() {
            q.jobs.clear();
        }
    }

    /// 走っている仕事へ「やめて」と伝える合図。
    #[must_use]
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancel)
    }
}

impl Drop for Workers {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Release);
        {
            let (lock, cv) = &*self.queue;
            if let Ok(mut q) = lock.lock() {
                q.stopped = true;
                q.jobs.clear();
            }
            cv.notify_all();
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;

    /// **優先度の高いものから回る**（TR-SYN-34）。
    #[test]
    fn 優先度の高い順に回る() {
        let (tx, rx) = channel();
        let w = Workers::start();

        // **先に低いものを積む。** 順序が優先度で決まることを見る。
        // 最初の1本が走り出す前に全部積みたいので、詰まらせておく。
        let (gate_tx, gate_rx) = channel::<()>();
        w.submit(Priority::PostRecording, move || {
            let _ = gate_rx.recv();
        });

        for (p, name) in [
            (Priority::ChartPrecompute, "chart"),
            (Priority::Reestimate, "reestimate"),
            (Priority::PreviewRequest, "preview"),
            (Priority::PostRecording, "post"),
        ] {
            let tx = tx.clone();
            w.submit(p, move || {
                let _ = tx.send(name);
            });
        }

        // 詰まりを解く。
        let _ = gate_tx.send(());

        let mut got = Vec::new();
        for _ in 0..4 {
            got.push(rx.recv().expect("回ること"));
        }
        assert_eq!(got, ["post", "preview", "reestimate", "chart"]);
    }

    /// **同じ優先度なら積んだ順。**
    #[test]
    fn 同じ優先度なら先に積んだ順() {
        let (tx, rx) = channel();
        let w = Workers::start();
        let (gate_tx, gate_rx) = channel::<()>();
        w.submit(Priority::PostRecording, move || {
            let _ = gate_rx.recv();
        });

        for i in 0..5 {
            let tx = tx.clone();
            w.submit(Priority::Reestimate, move || {
                let _ = tx.send(i);
            });
        }
        let _ = gate_tx.send(());

        let got: Vec<i32> = (0..5).map(|_| rx.recv().expect("回ること")).collect();
        assert_eq!(got, [0, 1, 2, 3, 4]);
    }

    #[test]
    fn 積んだものを捨てられる() {
        let w = Workers::start();
        let (gate_tx, gate_rx) = channel::<()>();
        w.submit(Priority::PostRecording, move || {
            let _ = gate_rx.recv();
        });
        for _ in 0..10 {
            w.submit(Priority::Reestimate, || {});
        }
        assert!(w.pending() > 0);
        w.clear();
        assert_eq!(w.pending(), 0);
        let _ = gate_tx.send(());
    }

    /// **落とすと止まる。** 積み残しがあっても抜ける。
    #[test]
    fn 落とすと止まる() {
        let w = Workers::start();
        for _ in 0..100 {
            w.submit(Priority::ChartPrecompute, || {
                std::thread::sleep(std::time::Duration::from_millis(1));
            });
        }
        let t = std::time::Instant::now();
        drop(w);
        assert!(
            t.elapsed() < std::time::Duration::from_millis(500),
            "積み残しを待たずに抜けること: {:?}",
            t.elapsed()
        );
    }

    /// **優先度は要件どおりの並び**（TR-SYN-34）。
    #[test]
    fn 優先度の並びが要件どおり() {
        assert!(Priority::PostRecording > Priority::PreviewRequest);
        assert!(Priority::PreviewRequest > Priority::Reestimate);
        assert!(Priority::Reestimate > Priority::ChartPrecompute);
    }
}
