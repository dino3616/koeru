// エラー型（T07 spec: "thiserror の列挙体"）。

use thiserror::Error;

/// キュー操作のエラー。
#[derive(Error, Debug)]
pub enum QueueError {
    #[error("queue is full: {pending} pending, {waiting_bytes} bytes requested")]
    QueueFull {
        pending: usize,
        waiting_bytes: usize,
    },

    #[error("job not found: {job_id}")]
    JobNotFound { job_id: String },

    #[error("job already finished")]
    AlreadyFinished,

    #[error("lease expired")]
    LeaseExpired,

    #[error("capability unavailable")]
    CapabilityUnavailable,
}

/// ジョブ実行のエラー。
#[derive(Error, Debug)]
pub enum JobRunError {
    #[error("engine returned error: {code}")]
    EngineError { code: String },

    #[error("engine timeout")]
    Timeout,

    #[error("engine unavailable: {reason}")]
    Unavailable { reason: String },

    #[error("cancellation requested")]
    Cancelled,

    #[error("invalid input: {reason}")]
    InvalidInput { reason: String },
}
