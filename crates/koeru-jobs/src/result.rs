// 結果型（T07 spec の payload union）。

use serde::Serialize;

use crate::BackgroundJob;

/// キューへの追加結果。
#[derive(Debug)]
pub enum EnqueueResult {
    /// キューへ追加された。
    Enqueued(BackgroundJob),
    /// キューが満杯。
    QueueFull,
    /// リースが期限切れ。
    LeaseExpired(Problem),
}

/// 問題情報（SDL の `Problem` union）。
#[derive(Debug, Serialize)]
pub struct Problem {
    pub code: String,
    pub class: crate::FailureClass,
    pub commit: crate::CommitState,
    pub actions: Vec<crate::CallerAction>,
    pub message: String,
}
