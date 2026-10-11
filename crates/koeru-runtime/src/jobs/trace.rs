//! 仕事の状態遷移の記録。 遷移ごとに1行。
//!
//! 載せるのは固定の語彙か数値だけ。 **入力の印（`BasedOn`）・まとめる鍵・音源名・
//! パス・歌詞は載せない**（`AGENTS.md` の禁止事項3）。 載せてよいフィールド名は
//! `crates/koeru-app/tests/offline.rs` の `TRACE_FIELDS_ALLOWED` が持つ。
//!
//! 記録は表のロックの外で出す（[`Outbox`]）。 記録の先（端末・ファイル）が遅くても、
//! 待ち数を読む側や公開の線形化点が止まらない。

use std::sync::Arc;

use koeru_failure::Class;

use super::error::JobError;
use super::ids::{Attempt, JobId};
use super::model::{JobFailure, JobKind, Priority};
use super::spec::Resolve;

/// 遷移1つ。
#[derive(Debug, Clone, Copy)]
pub(crate) struct TraceEvent {
    pub job: JobId,
    pub kind: JobKind,
    pub attempt: Attempt,
    pub priority: Priority,
    pub lane: &'static str,
    pub queue_wait_ms: u64,
    pub inflight_bytes: usize,
    /// 受理の判断。 `accepted` / `deferred` / `coalesced` / `promoted` /
    /// `rejected_busy` / `rejected_lane` / `rejected_shutdown` / `rejected_lease` /
    /// `admitted` / `refused_stale` / `refused_cancelled` / `refused_superseded` /
    /// `refused_lease_closed`、判断を伴わない遷移は `none`。
    pub admission: &'static str,
    /// 遷移の結果。 `queued` / `running` / `computed` / `publishing` /
    /// `succeeded` / `failed` / `cancel_requested` / `cancelled` / `refused`。
    pub outcome: &'static str,
    pub failure: Option<JobFailure>,
}

impl TraceEvent {
    pub(crate) fn emit(&self) {
        let job_id = self.job;
        match self.failure {
            // 内部の破れと、走らせたものの失敗だけを警告にする。 古い・取り消した・
            // 混んでいる、は予期できる結果で、普段の運転で起きる。
            Some(f) if matches!(f.class, Class::EngineFailed | Class::Internal) => {
                tracing::warn!(
                    job_id = %job_id,
                    kind = self.kind.as_str(),
                    attempt = self.attempt.get(),
                    priority = self.priority.as_str(),
                    lane = self.lane,
                    queue_wait_ms = self.queue_wait_ms,
                    inflight_bytes = self.inflight_bytes,
                    admission = self.admission,
                    outcome = self.outcome,
                    code = f.code,
                    class = f.class.as_str(),
                    "job"
                );
            }
            Some(f) => {
                tracing::debug!(
                    job_id = %job_id,
                    kind = self.kind.as_str(),
                    attempt = self.attempt.get(),
                    priority = self.priority.as_str(),
                    lane = self.lane,
                    queue_wait_ms = self.queue_wait_ms,
                    inflight_bytes = self.inflight_bytes,
                    admission = self.admission,
                    outcome = self.outcome,
                    code = f.code,
                    class = f.class.as_str(),
                    "job"
                );
            }
            None => {
                tracing::debug!(
                    job_id = %job_id,
                    kind = self.kind.as_str(),
                    attempt = self.attempt.get(),
                    priority = self.priority.as_str(),
                    lane = self.lane,
                    queue_wait_ms = self.queue_wait_ms,
                    inflight_bytes = self.inflight_bytes,
                    admission = self.admission,
                    outcome = self.outcome,
                    "job"
                );
            }
        }
    }
}

/// 表のロックの中で溜めて、外で出すもの。 記録と、待っている側への失敗の通知。
#[derive(Default)]
pub(crate) struct Outbox {
    events: Vec<TraceEvent>,
    resolutions: Vec<(Arc<dyn Resolve>, JobError)>,
}

impl Outbox {
    pub(crate) fn trace(&mut self, event: Option<TraceEvent>) {
        self.events.extend(event);
    }

    pub(crate) fn resolve(&mut self, slot: Arc<dyn Resolve>, error: JobError) {
        self.resolutions.push((slot, error));
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty() && self.resolutions.is_empty()
    }

    /// 記録を出し、待っている側へ失敗を渡す。 **ロックを握っていないこと。**
    /// 渡した先が callback（利用者の closure）かもしれない。
    pub(crate) fn flush(self) {
        for event in &self.events {
            event.emit();
        }
        for (slot, error) in self.resolutions {
            slot.resolve_err(error);
        }
    }
}
