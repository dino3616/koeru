//! Receipt から durable な EVID へ canonize してよいかの境界（提案書 10.0、10.2）。
//!
//! **Probe の定義と Receipt があるだけでは、Evidence を supported にできない。** ここが返すのは
//! EVID の候補で、EVID そのものではない。 条件・限界・provenance を足して記録に起こすのは人で、
//! 仮説の `assessment` はここからは決めない。
//!
//! 毎回の CI の結果を EVID にしない。 断る理由は [`Refusal`] で値として返す。 失敗ではなく、
//! 予期できる結果。

use std::fmt;

use super::capability::{Capability, CapabilityRegistry};
use super::facts::{Hypothesis, ProbeSpec};
use crate::probe::{ExecutionContext, NotRunReason, ProbeDefinition, Receipt, Status};

/// 候補が仮説をどちら向きに評価するか。 Receipt の結果から決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stance {
    /// 通った。 仮説を支持する候補。 観測方法の資格を超えると断られる。
    Supporting,
    /// 落ちた。 仮説を反証する候補。 支持ではないので、資格の表は掛けない。
    Contradicting,
}

/// EVID にしてよい観測。 まだ記録ではない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceCandidate {
    pub(crate) hypothesis: String,
    pub(crate) probe: String,
    pub(crate) method: String,
    pub(crate) stance: Stance,
    /// 走らせた版。 作業ツリーが汚れていれば、再現できない版。
    pub(crate) tested_sha: String,
    pub(crate) dirty: bool,
    pub(crate) platform: String,
    pub(crate) backend: String,
    pub(crate) actual_work: u64,
}

/// 断る理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// Receipt が、この Probe のものではない。
    ReceiptOfAnotherProbe,
    /// 走らせていない（組み立てが通らない・前提が欠けた・知らない runner）。
    NotRun(NotRunReason),
    /// Probe そのものを走らせない選択だった。
    Skipped,
    /// この環境の対象ではなかった。
    NotApplicable,
    /// 受領証と要約に数えない Receipt。
    NotCounted,
    /// 実際に仕事をしていない（0 件を開いて通った、前提を欠いて早期 return した）。
    NoActualWork,
    /// native が要る Probe を、native ではない backend で走らせた。 その結果は、
    /// native が要る主張について何も言わない。
    NativeRequired,
    /// 人の観察が要る Probe。 機械の Receipt は、人の観察の代わりにならない。
    HumanObservationRequired,
    /// この Probe は、その仮説を対象にしていない。
    NotTargeted,
    /// Probe に `method` が無い。 資格を決められない。
    ProbeWithoutMethod,
    /// 仮説に `hypothesis_kind` が無い。 資格を決められない。
    HypothesisWithoutKind,
    /// Probe の観測方法が、その種類の仮説を支持する資格を持たない。
    BeyondCapability(Capability),
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ReceiptOfAnotherProbe => "Receipt が、この Probe のものではない",
            Self::NotRun(_) => "走らせていない Probe は、観測にならない",
            Self::Skipped => "Probe を走らせない選択だった",
            Self::NotApplicable => "この環境の対象ではなかった",
            Self::NotCounted => "数えない Receipt は、観測にならない",
            Self::NoActualWork => "実際に仕事をしていない。 0 件を開いて通ったものは観測にならない",
            Self::NativeRequired => "native が要る Probe を、native ではない backend で走らせた",
            Self::HumanObservationRequired => {
                "人の観察が要る Probe は、機械の Receipt から EVID にしない"
            }
            Self::NotTargeted => "この Probe は、その仮説を対象にしていない",
            Self::ProbeWithoutMethod => "Probe に method が無く、資格を決められない",
            Self::HypothesisWithoutKind => "仮説に hypothesis_kind が無く、資格を決められない",
            Self::BeyondCapability(Capability::CannotSupport) => {
                "その観測方法は、その種類の仮説を支持できない"
            }
            Self::BeyondCapability(Capability::HypothesisGenerationOnly) => {
                "その観測方法は、その種類の仮説について仮説を作る材料にしかならない"
            }
            Self::BeyondCapability(Capability::ContextDependent) => {
                "その観測方法は、必要条件を読めないので自動では支持にならない"
            }
            Self::BeyondCapability(Capability::Unregistered) => {
                "その組は資格の表に無く、自動では支持にならない"
            }
            Self::BeyondCapability(Capability::UnknownKind) => "仮説の種類が資格の表に無い",
            Self::BeyondCapability(Capability::UnknownMethod) => "観測方法が資格の表に無い",
            Self::BeyondCapability(Capability::CanSupport) => {
                "資格はあるのに断った（ここには来ない）"
            }
        })
    }
}

impl std::error::Error for Refusal {}

/// Receipt を、仮説の EVID の候補にしてよいか。
///
/// 断る順は、実行したか、仕事をしたか、native と人の条件を満たしたか、Probe がその仮説を
/// 対象にしているか、観測方法に資格があるか。 通ったときだけ候補を返す。
///
/// 資格を掛けるのは支持する候補（通った Receipt）だけ。 落ちた Receipt は反証の候補で、
/// 表の「支持できるか」は問わない。 仮説を支持できない方法でも、落ちたという観測は残せる。
///
/// # Errors
///
/// 上の理由のどれかに当たったとき、その [`Refusal`]。
pub(crate) fn canonize(
    registry: &CapabilityRegistry,
    hypothesis: &Hypothesis,
    probe: &ProbeSpec,
    definition: &ProbeDefinition,
    receipt: &Receipt,
    context: &ExecutionContext,
) -> Result<EvidenceCandidate, Refusal> {
    if receipt.probe != probe.id || definition.id != probe.id {
        return Err(Refusal::ReceiptOfAnotherProbe);
    }
    match receipt.status {
        Status::NotRun(reason) => return Err(Refusal::NotRun(reason)),
        Status::Skipped => return Err(Refusal::Skipped),
        Status::NotApplicable => return Err(Refusal::NotApplicable),
        Status::Passed | Status::Failed => {}
    }
    if !receipt.applies {
        return Err(Refusal::NotApplicable);
    }
    // 今は `NotRun` だけが数えない。 数えない状態が増えても、通さない。
    if !receipt.countable() {
        return Err(Refusal::NotCounted);
    }
    if receipt.actual_work == 0 {
        return Err(Refusal::NoActualWork);
    }
    if probe.requires_native && context.backend != "native" {
        return Err(Refusal::NativeRequired);
    }
    if probe.requires_human {
        return Err(Refusal::HumanObservationRequired);
    }
    if !probe.targets.iter().any(|t| t == &hypothesis.id) {
        return Err(Refusal::NotTargeted);
    }
    let method = probe.method.as_deref().ok_or(Refusal::ProbeWithoutMethod)?;
    let stance = if receipt.status == Status::Passed {
        Stance::Supporting
    } else {
        Stance::Contradicting
    };
    if stance == Stance::Supporting {
        let kind = hypothesis
            .kind
            .as_deref()
            .ok_or(Refusal::HypothesisWithoutKind)?;
        let capability = registry.assess(kind, method);
        if !capability.supports() {
            return Err(Refusal::BeyondCapability(capability));
        }
    }
    Ok(EvidenceCandidate {
        hypothesis: hypothesis.id.clone(),
        probe: probe.id.clone(),
        method: method.to_owned(),
        stance,
        tested_sha: context.git_sha.clone(),
        dirty: context.dirty,
        platform: context.platform.to_owned(),
        backend: context.backend.to_owned(),
        actual_work: receipt.actual_work,
    })
}
