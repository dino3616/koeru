// JobQueue — メモリ内キュー本体（bounded items + bytes）。
//
// 永続化は行わない（Phase 5 で必要に応じて db 層を追加）。
// T07 spec: "bounded items+bytes" に従う。

use std::collections::VecDeque;

use crate::{BackgroundJob, JobId, JobRequest, JobState};

/// キューへの追加結果。
pub enum QueueResult {
    Enqueued(BackgroundJob),
    QueueFull {
        pending: usize,
        waiting_bytes: usize,
    },
}

/// ジョブキュー。
///
/// T07 spec: "bounded items+bytes" を約束する。
/// 上限に達すると、`QueueResult::QueueFull` を返す。
#[derive(Debug)]
pub struct JobQueue {
    max_items: usize,
    max_bytes: usize,
    current_items: usize,
    current_bytes: usize,
    queue: VecDeque<EnqueuedJob>,
}

/// キュー内のエンティティ。
#[derive(Debug)]
pub struct EnqueuedJob {
    pub request: JobRequest,
}

/// キューの情報（`JobQueue.pending / running / active`）。
#[derive(Debug)]
pub struct JobQueueInfo {
    pub pending: usize,
    pub running: usize,
    pub active: Vec<BackgroundJob>,
}

/// キューからの中止結果。
#[derive(Debug)]
pub enum CancelResult {
    /// キャンセル要求を受け付けた。
    CancelRequested(BackgroundJob),
    /// すでに完了。
    AlreadyFinished { job: BackgroundJob },
    /// ジョブが見つからなかった。
    ReferenceNotFound,
}

impl JobQueue {
    /// 新しいジョブキューを生成する。
    ///
    /// `max_items`: キュー内の最大ジョブ数。
    /// `max_bytes`: 全ジョブの payload の合計最大バイト数。
    pub fn new(max_items: usize, max_bytes: usize) -> Self {
        Self {
            max_items,
            max_bytes,
            current_items: 0,
            current_bytes: 0,
            queue: VecDeque::new(),
        }
    }

    /// ジョブリクエストをキューに追加する。
    ///
    /// 上限に達している場合は `QueueResult::QueueFull` を返す。
    pub fn enqueue(&mut self, request: JobRequest) -> QueueResult {
        let payload_bytes = self.calculate_request_bytes(&request);

        // Check bounds
        if self.current_items + 1 > self.max_items
            || self.current_bytes + payload_bytes > self.max_bytes
        {
            return QueueResult::QueueFull {
                pending: self.current_items,
                waiting_bytes: payload_bytes,
            };
        }

        let job = request.as_background_job(JobState::queued());

        self.queue.push_back(EnqueuedJob { request });
        self.current_items += 1;
        self.current_bytes += payload_bytes;

        QueueResult::Enqueued(job)
    }

    /// キューからジョブを pop する。
    ///
    /// キューが空の場合は `None` を返す。
    pub fn pop(&mut self) -> Option<EnqueuedJob> {
        let job = self.queue.pop_front();
        if let Some(ref job) = job {
            self.current_items -= 1;
            self.current_bytes -= self.calculate_request_bytes(&job.request);
        }
        job
    }

    /// キューの情報を返す。
    pub fn info(&self) -> JobQueueInfo {
        JobQueueInfo {
            pending: self.current_items,
            running: 0,
            active: self
                .queue
                .iter()
                .map(|job| job.request.as_background_job(JobState::queued()))
                .collect(),
        }
    }

    /// ジョブをキャンセルする。
    pub fn cancel(&mut self, job_id: JobId) -> CancelResult {
        if let Some(pos) = self.queue.iter().position(|job| job.request.id == job_id) {
            let job = self.queue.remove(pos).unwrap();
            self.current_items -= 1;
            self.current_bytes -= self.calculate_request_bytes(&job.request);

            CancelResult::CancelRequested(job.request.as_background_job(JobState::cancelled(false)))
        } else {
            CancelResult::ReferenceNotFound
        }
    }

    /// キュー内の全ジョブを返す（drain せずに）。
    pub fn drain(&self) -> Vec<BackgroundJob> {
        self.queue
            .iter()
            .map(|job| job.request.as_background_job(JobState::queued()))
            .collect()
    }

    /// payload のバイト数を計算する（簡易）。
    fn calculate_request_bytes(&self, request: &JobRequest) -> usize {
        serde_json::to_string(request).map(|s| s.len()).unwrap_or(0)
    }
}
