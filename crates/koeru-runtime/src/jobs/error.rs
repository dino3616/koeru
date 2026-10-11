//! 仕事の操作が失敗した理由と、公開を断った理由（`DEC-PLT-038`）。
//!
//! 予期できる結果は失敗にせず値で返す。 公開を断る理由（[`PublishRefused`]）は
//! 計算の失敗ではない——古い・取り消された・やり直された、という呼び出し側が
//! 分岐する結果で、エンジンの失敗とは別に持つ。

use koeru_failure::{Class, Failure, Outcome};

use super::model::JobFailure;

/// 計算が取り消しの合図で抜けた印。
///
/// [`CancelProbe::check`](super::CancelProbe::check) だけが作る。 計算の closure が
/// 勝手に「取り消された」と名乗って戻ると、状態が「止められた」と偽られる——
/// 外から作れない型にして、probe を通した抜け方だけが
/// `computation_stopped: true` になるようにしてある。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cancelled(());

impl Cancelled {
    pub(crate) const fn new() -> Self {
        Self(())
    }
}

/// 計算の closure が返す失敗。
///
/// 計算は台帳へ書かない（書き込み権限を渡さない）ので、ここで失敗しても
/// 確定したものは無い。 失敗は [`JobFailure`] の code と分類だけで運ぶ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ComputeError {
    /// 取り消しの合図で抜けた（[`Cancelled`] から作る）。
    #[error("計算が取り消された")]
    Cancelled(Cancelled),
    /// 計算が失敗した。
    #[error("計算が失敗した")]
    Failed(JobFailure),
}

impl ComputeError {
    /// [`Failure`] を計算の失敗にする。 確定の状態は常に「確定していない」——
    /// 計算は台帳へ書かない。
    #[must_use]
    pub fn failed<E: Failure + ?Sized>(e: &E) -> Self {
        Self::Failed(JobFailure::of(e, Outcome::NotCommitted))
    }
}

impl From<Cancelled> for ComputeError {
    fn from(c: Cancelled) -> Self {
        Self::Cancelled(c)
    }
}

impl Failure for ComputeError {
    fn code(&self) -> &'static str {
        match self {
            Self::Cancelled(_) => "jobs.compute_cancelled",
            Self::Failed(f) => f.code,
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Cancelled(_) => Class::Cancelled,
            Self::Failed(f) => f.class,
        }
    }
}

/// 仕事の投入・待ち・結果が、成功以外で終わった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JobError {
    /// 上限に当たった（`OverflowPolicy::Reject`、または `Coalesce` で
    /// まとめる相手が無かった）。 待つか、明示して再試行する。
    #[error("背後の仕事が混んでいる")]
    Busy,

    /// その lane は、固まった作業スレッドが多すぎて仕事を受けられない。
    /// 固まったものが戻ってくれば受けられるようになる。
    #[error("この種類の仕事を回す口が使えない")]
    LaneUnavailable,

    /// 止めている最中で、新しい仕事を受けない。
    #[error("背後の仕事を止めている最中")]
    ShuttingDown,

    /// その貸与はもう閉じた（`JobRuntime::cancel_lease`）。
    #[error("project への貸与がもう生きていない")]
    LeaseClosed,

    /// やり直す相手の仕事が無い、または保持の期間を過ぎた。
    #[error("そんな仕事は無い")]
    UnknownJob,

    /// 期限を過ぎた。 **自動でやり直さない。** 走っていたなら、その作業スレッドは
    /// 固まったと見なして lane から外し、遅れて戻った結果は公開しない。
    #[error("仕事が期限までに終わらなかった")]
    DeadlineExceeded,

    /// 同じ仕事の新しい試行に置き換わった。
    #[error("仕事が新しい試行に置き換わった")]
    Superseded,

    /// 取り消した（呼び出し側の取消・まとめられた・止めている最中）。
    #[error("仕事を取り消した")]
    Cancelled,

    /// 計算が失敗した。 code と分類は計算が名乗ったもの。
    #[error("仕事の計算が失敗した")]
    ComputeFailed(JobFailure),

    /// 計算が panic した。 作業スレッドは生きていて、次の仕事を回す。
    #[error("仕事の計算が異常終了した")]
    ComputePanicked,

    /// 公開の途中で、呼び出し側の書き込み（`admit`）が panic した。 台帳へ
    /// 書いたかどうかは分からない。 どの API も返さず、仕事の状態にだけ載る。
    #[error("公開の途中で異常終了した")]
    PublishAborted,
}

impl Failure for JobError {
    fn code(&self) -> &'static str {
        match self {
            Self::Busy => "jobs.busy",
            Self::LaneUnavailable => "jobs.lane_unavailable",
            Self::ShuttingDown => "jobs.shutting_down",
            Self::LeaseClosed => "jobs.lease_closed",
            Self::UnknownJob => "jobs.unknown_job",
            Self::DeadlineExceeded => "jobs.deadline_exceeded",
            Self::Superseded => "jobs.superseded",
            Self::Cancelled => "jobs.cancelled",
            Self::ComputeFailed(f) => f.code,
            Self::ComputePanicked => "jobs.compute_panicked",
            Self::PublishAborted => "jobs.publish_aborted",
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Busy => Class::Busy,
            // 画面では `CapabilityUnavailable`（この環境では今使えない）に写る。
            Self::LaneUnavailable => Class::Unsupported,
            Self::ShuttingDown => Class::Rejected,
            // 貸与が閉じた・やり直す相手が無い・新しい試行に置き換わった、は
            // どれも「読んだものが古い、または対象がもう無い」（`Class::Conflict`）。
            Self::LeaseClosed | Self::UnknownJob | Self::Superseded => Class::Conflict,
            // 走っていた計算が期限までに戻らなかった。 呼び出し側の手は
            // 「派生物を作り直すか縮退する。元のデータは残す」で、外部エンジンの
            // 失敗と同じ（`DEC-PLT-038` の分類表）。 待ちで期限が切れた場合も、
            // 回す口が間に合わなかったことに変わりがない。
            Self::DeadlineExceeded => Class::EngineFailed,
            Self::Cancelled => Class::Cancelled,
            Self::ComputeFailed(f) => f.class,
            Self::ComputePanicked | Self::PublishAborted => Class::Internal,
        }
    }
}

impl JobError {
    /// 状態に載せる失敗にする。 計算は台帳へ書かないので、確定の状態は
    /// 「確定していない」。 例外は公開の途中の異常終了で、書いたかは分からない。
    #[must_use]
    pub fn as_failure(&self) -> JobFailure {
        match self {
            Self::ComputeFailed(f) => *f,
            Self::PublishAborted => JobFailure::of(self, Outcome::Unknown),
            other => JobFailure::of(other, Outcome::NotCommitted),
        }
    }
}

/// 計算は終わったが、公開を断った理由。
///
/// **エンジンの失敗ではない。** 計算が正しくても、入力が変わった・取り消した・
/// やり直した・project を閉じた、のどれかなら結果は使えない。 呼び出し側は
/// 最新を読み直して、必要なら新しい試行を投入する（自動でやり直さない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum PublishRefused {
    /// 計算が読んだ入力が、今の値と違う（[`BasedOn`](super::BasedOn)）。
    /// 仕事は `Failed`（`jobs.publish_stale`、`Conflict`）になる。
    #[error("計算したあとで入力が変わっていた")]
    Stale,

    /// 公開より先に取り消しが確定していた。
    #[error("公開より先に取り消されていた")]
    Cancelled,

    /// 同じ仕事の新しい試行が出ていて、この試行は最新ではない。
    #[error("新しい試行に置き換わっていた")]
    Superseded,

    /// project への貸与が閉じていた。 閉じて開き直した別の貸与へは書き戻さない。
    #[error("project への貸与がもう生きていない")]
    LeaseClosed,

    /// 表に無い（保持の期間を過ぎた）。 公開してよいと言える根拠が無い。
    #[error("仕事が表に無い")]
    Gone,
}

impl Failure for PublishRefused {
    fn code(&self) -> &'static str {
        match self {
            Self::Stale => "jobs.publish_stale",
            Self::Cancelled => "jobs.publish_cancelled",
            Self::Superseded => "jobs.publish_superseded",
            Self::LeaseClosed => "jobs.publish_lease_closed",
            Self::Gone => "jobs.publish_gone",
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Stale | Self::Superseded | Self::LeaseClosed | Self::Gone => Class::Conflict,
            Self::Cancelled => Class::Cancelled,
        }
    }
}

/// [`JobRuntime::publish`](super::JobRuntime::publish) の失敗。
#[derive(Debug, thiserror::Error)]
pub enum PublishError<E: std::error::Error + 'static> {
    /// 受理の条件を満たさず、公開を始めなかった。
    #[error("公開を断った")]
    Refused(#[source] PublishRefused),
    /// 受理したが、呼び出し側の書き込み（`admit`）が失敗した。 仕事は `Failed`。
    #[error(transparent)]
    Admit(E),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    const JOB_ERRORS: [JobError; 10] = [
        JobError::Busy,
        JobError::LaneUnavailable,
        JobError::ShuttingDown,
        JobError::LeaseClosed,
        JobError::UnknownJob,
        JobError::DeadlineExceeded,
        JobError::Superseded,
        JobError::Cancelled,
        JobError::ComputePanicked,
        JobError::PublishAborted,
    ];

    const REFUSALS: [PublishRefused; 5] = [
        PublishRefused::Stale,
        PublishRefused::Cancelled,
        PublishRefused::Superseded,
        PublishRefused::LeaseClosed,
        PublishRefused::Gone,
    ];

    #[test]
    fn code_は型をまたいで一意で_jobs_で始まる() {
        let mut seen = HashSet::new();
        for code in JOB_ERRORS
            .iter()
            .map(JobError::code)
            .chain(REFUSALS.iter().map(PublishRefused::code))
            .chain([ComputeError::Cancelled(Cancelled::new()).code()])
        {
            assert!(code.starts_with("jobs."), "{code}");
            assert!(seen.insert(code), "{code} が重複している");
        }
    }

    #[test]
    fn 混んでいるは_busy_で断るのは_conflict_と_cancelled() {
        assert_eq!(JobError::Busy.class(), Class::Busy);
        assert_eq!(JobError::Cancelled.class(), Class::Cancelled);
        assert_eq!(JobError::Superseded.class(), Class::Conflict);
        assert_eq!(PublishRefused::Stale.class(), Class::Conflict);
        assert_eq!(PublishRefused::Cancelled.class(), Class::Cancelled);
    }

    #[test]
    fn 公開を断る理由はエンジンの失敗に分類しない() {
        for r in REFUSALS {
            assert_ne!(r.class(), Class::EngineFailed, "{r:?}");
            assert_ne!(r.class(), Class::Internal, "{r:?}");
        }
    }

    #[test]
    fn 期限切れは外部エンジンの失敗と同じ手で次を選ばせる() {
        assert_eq!(JobError::DeadlineExceeded.class(), Class::EngineFailed);
        assert_eq!(
            JobError::DeadlineExceeded.action(),
            koeru_failure::Action::Rebuild
        );
    }

    #[test]
    fn 計算の失敗は名乗った_code_と分類を保つ() {
        let inner = JobFailure {
            code: "align.model_missing",
            class: Class::Unsupported,
            outcome: Outcome::NotCommitted,
        };
        let e = JobError::ComputeFailed(inner);
        assert_eq!(e.code(), "align.model_missing");
        assert_eq!(e.class(), Class::Unsupported);
        assert_eq!(e.as_failure(), inner);
    }

    #[test]
    fn 計算の失敗は確定していないと名乗る() {
        let e = ComputeError::failed(&JobError::Busy);
        assert_eq!(
            e,
            ComputeError::Failed(JobFailure {
                code: "jobs.busy",
                class: Class::Busy,
                outcome: Outcome::NotCommitted
            })
        );
    }

    #[test]
    fn 取り消しの印は_probe_からしか作れない型で運ぶ() {
        let e: ComputeError = Cancelled::new().into();
        assert_eq!(e.class(), Class::Cancelled);
    }

    #[test]
    fn 状態に載せる失敗は確定していないが公開の途中の異常終了だけは不明() {
        for e in JOB_ERRORS {
            let want = if e == JobError::PublishAborted {
                Outcome::Unknown
            } else {
                Outcome::NotCommitted
            };
            assert_eq!(e.as_failure().outcome, want, "{e:?}");
        }
    }
}
