// JobHandle — submit の返り値、cancel/await の窓口。

use tokio::sync::mpsc;

use crate::{JobId, JobKind, JobState};

/// ジョブハンドル。
///
/// `JobSystem::submit()` の返り値。呼び出し元はこれを通して：
/// - 結果を待つ（`recv()`）
/// - キャンセルを要求する（`cancel()`）
#[derive(Debug)]
pub struct JobHandle {
    pub job_id: JobId,
    pub kind: JobKind,
    pub(crate) rx: mpsc::Receiver<JobState>,
    pub(crate) cancel_tx: mpsc::Sender<()>,
}

impl JobHandle {
    /// 新しい JobHandle を作成する。
    /// 呼び出し元は返された tx と rx を外部で保持する（例: JobSystem の running_tasks に）。
    ///
    /// 返値: (Self, state_tx, cancel_rx)
    /// - `Self`: job_id, kind 取得用
    /// - `state_tx`: ジョブ状態を push する side（JobSystem 側が持つ）
    /// - `cancel_rx`: キャンセル要求を受け取る side（JobHandle 側が持つ）
    pub(crate) fn new(
        job_id: JobId,
        kind: JobKind,
    ) -> (Self, mpsc::Sender<JobState>, mpsc::Receiver<()>) {
        let (state_tx, rx) = mpsc::channel(1);
        let (_, cancel_rx) = mpsc::channel(1);
        (
            Self {
                job_id,
                kind,
                rx,
                cancel_tx: mpsc::channel(1).0,
            },
            state_tx,
            cancel_rx,
        )
    }

    /// ジョブ ID を返す。
    pub fn job_id(&self) -> JobId {
        self.job_id
    }

    /// ジョブ種別を返す。
    pub fn kind(&self) -> JobKind {
        self.kind
    }

    /// キャンセルを要求する。
    ///
    /// エンジン呼び出し中（FFI 実行中）は、engine が終了するまで待機する。
    /// 仕様: "running FFI の強制停止を約束しない"。
    pub async fn cancel(&self) -> Result<(), JobHandleError> {
        self.cancel_tx
            .send(())
            .await
            .map_err(|_| JobHandleError::AlreadyFinished)
    }

    /// 結果を待つ（非同期）。
    ///
    /// - `JobState::Succeeded`: 成功
    /// - `JobState::Failed`: エラー情報付き失敗
    /// - `JobState::Cancelled`: キャンセル済み
    pub async fn await_result(&mut self) -> Result<JobState, JobHandleError> {
        self.rx.recv().await.ok_or(JobHandleError::ChannelClosed)
    }
}

/// JobHandle の操作失敗。
#[derive(thiserror::Error, Debug)]
pub enum JobHandleError {
    #[error("channel closed")]
    ChannelClosed,

    #[error("job already finished")]
    AlreadyFinished,
}
