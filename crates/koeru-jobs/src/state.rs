// JobState — 状態機械（jobs.graphql の `JobStatus` union）。
// Attempt — 試行回数。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// 試行回数（1 始まりの整数）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Attempt(pub(crate) u32);

impl Attempt {
    /// 最初の試行。
    pub const INITIAL: Self = Self(1);

    /// 次回の試行数を返す。
    pub fn next(&self) -> Self {
        Self(self.0 + 1)
    }

    /// 整数値で返す。
    pub fn value(&self) -> u32 {
        self.0
    }
}

impl Default for Attempt {
    fn default() -> Self {
        Self::INITIAL
    }
}

/// ジョブの状態。
///
/// GraphQL union `JobStatus` の Rust 実装。
/// 状態遷移: Queued → Running → Succeeded | Failed | Cancelled
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum JobState {
    Queued {
        since: DateTime<Utc>,
    },
    Running {
        started_at: DateTime<Utc>,
        progress: Option<f64>,
    },
    Succeeded {
        finished_at: DateTime<Utc>,
    },
    Failed {
        code: String,
        class: FailureClass,
        commit: CommitState,
        actions: Vec<CallerAction>,
        message: String,
        finished_at: DateTime<Utc>,
    },
    Cancelled {
        finished_at: DateTime<Utc>,
        computation_stopped: bool,
    },
}

/// 失敗のクラス（SDL の `FailureClass`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FailureClass {
    /// 一時的な問題（再接試で解決する可能性）
    Transient,
    /// 永続的な問題（同じ入力で再試行しても失敗する）
    Permanent,
    /// システムエラー
    System,
}

/// コミット状態（SDL の `CommitState`）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitState {
    Pending,
    Committed,
    Discarded,
}

/// 呼び出し側のアクション（SDL の `CallerAction`）。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CallerAction {
    Retry,
    Cancel,
    Discard,
    Ignore,
}

impl JobState {
    /// Queued 状態を返す（現在時刻を使用）。
    pub fn queued() -> Self {
        Self::Queued { since: Utc::now() }
    }

    /// 実行中状態を返す（現在時刻 + progress=0.0）。
    pub fn running(progress: f64) -> Self {
        Self::Running {
            started_at: Utc::now(),
            progress: Some(progress),
        }
    }

    /// 成功状態を返す（現在時刻）。
    pub fn succeeded() -> Self {
        Self::Succeeded {
            finished_at: Utc::now(),
        }
    }

    /// キャンセル状態を返す。
    pub fn cancelled(computation_stopped: bool) -> Self {
        Self::Cancelled {
            finished_at: Utc::now(),
            computation_stopped,
        }
    }

    /// 失敗状態を返す。
    pub fn failed(
        code: String,
        class: FailureClass,
        commit: CommitState,
        actions: Vec<CallerAction>,
        message: String,
    ) -> Self {
        Self::Failed {
            code,
            class,
            commit,
            actions,
            message,
            finished_at: Utc::now(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attempt_initial_is_one() {
        assert_eq!(Attempt::INITIAL.value(), 1);
    }

    #[test]
    fn attempt_next_increments() {
        let a = Attempt(3);
        assert_eq!(a.next().value(), 4);
    }

    #[test]
    fn queued_state() {
        let state = JobState::queued();
        match state {
            JobState::Queued { .. } => {}
            _ => panic!("expected Queued"),
        }
    }

    #[test]
    fn running_state() {
        let state = JobState::running(0.5);
        match state {
            JobState::Running {
                progress: Some(p), ..
            } => {
                assert!((p - 0.5).abs() < 0.001);
            }
            _ => panic!("expected Running"),
        }
    }

    #[test]
    fn succeeded_state() {
        let state = JobState::succeeded();
        match state {
            JobState::Succeeded { .. } => {}
            _ => panic!("expected Succeeded"),
        }
    }
}
