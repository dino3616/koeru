// JobSystem — 非同期ジョブ実行システム（Phase 2）。
//
// 外部エンジン（MFA、synthesis）を detached task として実行し、
// 結果を mpsc channel 経由で JobHandle へ返す。
//
// T07 spec:
// - "bounded items+bytes" を約束する
// - detached task で engine 呼び出し（FFI 強制停止を約束しない）
// - mpsc channel で subscriber へ結果を push

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::{
    BackgroundJob, CancelResult, JobHandle, JobId, JobKind, JobQueue, JobQueueInfo, JobRequest,
    JobState, JobTarget, QueueError, QueueResult, RecordedTake,
};

/// ジョブ実行中のタスク内エンティティ。
#[derive(Debug)]
struct RunningTask {
    /// ジョブリクエスト。
    pub request: JobRequest,
    /// ジョブ状態を JobHandle へ通知する channel sender。
    pub state_tx: mpsc::Sender<JobState>,
    /// このタスクの非同期 task handle（Phase 4 で接続）。
    #[allow(dead_code)]
    task_handle: Option<JoinHandle<()>>,
}

/// 非同期ジョブ実行システム。
///
/// T07 spec: "bounded items+bytes" を約束する。
#[derive(Debug)]
pub struct JobSystem {
    queue: JobQueue,
    /// 実行中のジョブ ID 一覧。
    running: Arc<std::sync::Mutex<HashSet<JobId>>>,
    /// 実行中のジョブ情報（map: JobId → RunningTask）。
    running_tasks: Arc<std::sync::Mutex<HashMap<JobId, RunningTask>>>,
}

impl JobSystem {
    /// 新しい JobSystem を生成する。
    ///
    /// `max_items`: キュー内の最大ジョブ数。
    /// `max_bytes`: 全ジョブの payload の合計最大バイト数。
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        Self {
            queue: JobQueue::new(max_items, max_bytes),
            running: Arc::new(std::sync::Mutex::new(HashSet::new())),
            running_tasks: Arc::new(std::sync::Mutex::new(HashMap::new())),
        }
    }

    /// ジョブリクエストをキューに追加し、JobHandle を返す。
    ///
    /// 上限に達している場合は `Err(QueueError::QueueFull)` を返す。
    pub fn submit(&mut self, request: JobRequest) -> Result<JobHandle, QueueError> {
        let job_id = request.id;
        let job_kind = request.kind;
        let request_clone = request.clone();

        match self.queue.enqueue(request) {
            QueueResult::Enqueued(_bg) => {
                // Create handle and channel endpoints.
                // `new` returns (Self, state_tx, cancel_rx):
                // - Self: with rx (receive state) and cancel_tx (send cancel)
                // - state_tx: to push state updates (stored in RunningTask)
                // - cancel_rx: to receive cancel signals (stored in Self)
                let (handle, state_tx, _cancel_rx) = JobHandle::new(job_id, job_kind);

                // Add to running set (JobId set)
                {
                    let mut running = self.running.lock().unwrap();
                    running.insert(job_id);
                }

                // Add to running_tasks map (with state_tx for pushing state updates)
                {
                    let mut tasks = self.running_tasks.lock().unwrap();
                    tasks.insert(
                        job_id,
                        RunningTask {
                            task_handle: None,
                            state_tx,
                            request: request_clone,
                        },
                    );
                }

                Ok(handle)
            }
            QueueResult::QueueFull {
                pending,
                waiting_bytes,
            } => Err(QueueError::QueueFull {
                pending,
                waiting_bytes,
            }),
        }
    }

    /// キューからジョブを pop して実行 task を起動する。
    ///
    /// Phase 4 でエンジン呼び出しを接続する。
    pub fn pop_and_run(&mut self) -> Option<JobId> {
        // Get job ID from running_tasks (not from queue)
        let job_id = {
            let tasks = self.running_tasks.lock().unwrap();
            tasks.keys().next().copied()
        }?;

        // Get the state_tx from running_tasks and spawn a task
        let running_tasks = self.running_tasks.clone();

        {
            let mut tasks = running_tasks.lock().unwrap();
            if let Some(task) = tasks.get_mut(&job_id) {
                task.task_handle =
                    Some(self.spawn_job_task(task.request.clone(), mpsc::channel(1).0));
            }
        }

        Some(job_id)
    }

    /// 非同期ジョブ実行 task を spawn する。
    ///
    /// Phase 4 でエンジン呼び出しを接続する。
    fn spawn_job_task(
        &self,
        _request: JobRequest,
        _state_tx: mpsc::Sender<JobState>,
    ) -> JoinHandle<()> {
        let running_tasks = self.running_tasks.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            // Phase 4: ここでエンジン呼び出しを接続する
            drop(running_tasks);
            drop(running);
        })
    }

    /// ジョブをキャンセルする。
    pub fn cancel(&mut self, job_id: JobId) -> CancelResult {
        let mut running = self.running.lock().unwrap();
        let mut tasks = self.running_tasks.lock().unwrap();

        if tasks.remove(&job_id).is_some() {
            running.remove(&job_id);
            CancelResult::CancelRequested(BackgroundJob {
                id: job_id,
                kind: JobKind::Analysis, // Placeholder
                attempt: crate::Attempt::INITIAL,
                status: JobState::cancelled(false),
                target: JobTarget::Take(RecordedTake::new(0)), // Placeholder
            })
        } else {
            CancelResult::ReferenceNotFound
        }
    }

    /// 実行中のジョブ ID を返す。
    pub fn running_jobs(&self) -> Vec<JobId> {
        let running = self.running.lock().unwrap();
        running.iter().copied().collect()
    }

    /// キューの情報を返す。
    pub fn info(&self) -> JobQueueInfo {
        self.queue.info()
    }

    /// キュー内の全ジョブを返す（drain せずに）。
    pub fn drain(&self) -> Vec<BackgroundJob> {
        self.queue.drain()
    }
}
