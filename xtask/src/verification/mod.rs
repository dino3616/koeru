//! 検証の意味づけ（D02）。 Probe が何を証言できるかを、仮説と要件に結んで、偽の Evidence を防ぐ。
//!
//! - [`capability`] — 観測方法が、その種類の仮説について証言する資格を持つかの表（提案書 10.1.1）
//! - [`facts`] — 仮説・観測・Probe・判断を snapshot から読む
//! - [`canonize`] — Receipt を EVID の候補にしてよいかの境界（関数。 CLI は無い）
//! - [`origin`] — 同じ起源の調査を1つと数える（提案書 10.2 の二つ目）
//! - [`debt`] — Human Verification Debt（提案書 10.3.1）の導出
//!
//! 検査（[`check_verification`]）は `check-meta` に載る。 見るのは、観測方法がその仮説を支持する
//! 資格を持つか、と、語が表にあるかだけ。 **観察の内容の真偽は認定しない。** 仮説の `assessment`
//! も、要件の受入も、人の判断も、ここからは決めない。
//!
//! 資格を判定する対象は、観測の欄（`method`）を書いた調査だけ。 欄が無い今の調査（`EVID-*`）は
//! 判定しない。 落とすと、移行前の記録が全部落ちる。

mod canonize;
mod capability;
mod debt;
mod facts;
mod origin;

#[cfg(test)]
mod tests;

pub(crate) use canonize::{EvidenceCandidate, Refusal, Stance, canonize};
pub(crate) use capability::{
    Capability, CapabilityRegistry, Observer, RegistryIssue, Row, Support,
};
pub(crate) use debt::{
    EvidenceStance, EvidenceStanding, HumanDebtItem, ProbeStanding, Shortfall,
    human_verification_debt,
};
pub(crate) use facts::{DecisionDeps, Facts, Hypothesis, Observation, ProbeSpec};
pub(crate) use origin::{independent_origins, origin_groups};

use std::collections::BTreeSet;

use crate::diagnostic::{Location, Report};
use crate::knowledge::KnowledgeSnapshot;

/// 検査が見つけたこと。 診断（[`check_verification`]）の元。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Finding {
    /// 表そのものの誤り。
    Registry(RegistryIssue),
    /// 仮説の `hypothesis_kind` が表に無い。
    UnknownHypothesisKind {
        hypothesis: String,
        kind: String,
        place: Option<Location>,
    },
    /// 調査または Probe の `method` が表に無い。
    UnknownMethod {
        owner: String,
        method: String,
        place: Option<Location>,
    },
    /// 仮説に種類が無いので、観測の資格を決められない。 観測方法を書いた調査が支持すると言っている。
    MissingHypothesisKind {
        hypothesis: String,
        evidence: String,
        place: Option<Location>,
    },
    /// 仮説の `supporting_evidence` / `contradicting_evidence` が指す調査が無い。
    UnknownEvidence {
        hypothesis: String,
        evidence: String,
        place: Option<Location>,
    },
    /// 調査の `assesses` が指す仮説が無い。
    UnknownHypothesis {
        evidence: String,
        hypothesis: String,
        place: Option<Location>,
    },
    /// 調査が仮説を支持すると言っているが、その観測方法にその種類の仮説を支持する資格が無い。
    BeyondCapability {
        hypothesis: String,
        kind: String,
        evidence: String,
        method: String,
        capability: Capability,
        place: Option<Location>,
    },
}

impl Finding {
    /// 規則ごとに決まる安定した名前。
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::Registry(RegistryIssue::MissingField { .. }) => {
                "verification.registry-missing-field"
            }
            Self::Registry(RegistryIssue::UnknownSupport { .. }) => {
                "verification.registry-unknown-support"
            }
            Self::Registry(RegistryIssue::UnknownObserver { .. }) => {
                "verification.registry-unknown-observer"
            }
            Self::Registry(RegistryIssue::DuplicatePair { .. }) => {
                "verification.registry-duplicate-pair"
            }
            Self::Registry(RegistryIssue::ObserverConflict { .. }) => {
                "verification.registry-observer-conflict"
            }
            Self::UnknownHypothesisKind { .. } => "verification.unknown-hypothesis-kind",
            Self::UnknownMethod { .. } => "verification.unknown-method",
            Self::MissingHypothesisKind { .. } => "verification.missing-hypothesis-kind",
            Self::UnknownEvidence { .. } => "verification.unknown-evidence",
            Self::UnknownHypothesis { .. } => "verification.unknown-hypothesis",
            Self::BeyondCapability { capability, .. } => match capability {
                Capability::HypothesisGenerationOnly => "verification.generation-only",
                Capability::ContextDependent => "verification.context-dependent",
                Capability::Unregistered => "verification.unregistered-capability",
                // `CanSupport` は出さない。 vocabulary の誤りは別の finding。
                Capability::CannotSupport
                | Capability::CanSupport
                | Capability::UnknownKind
                | Capability::UnknownMethod => "verification.cannot-support",
            },
        }
    }

    pub(crate) fn place(&self) -> Option<Location> {
        match self {
            Self::Registry(
                RegistryIssue::MissingField { place, .. }
                | RegistryIssue::UnknownSupport { place, .. }
                | RegistryIssue::UnknownObserver { place, .. }
                | RegistryIssue::DuplicatePair { place, .. }
                | RegistryIssue::ObserverConflict { place, .. },
            )
            | Self::UnknownHypothesisKind { place, .. }
            | Self::UnknownMethod { place, .. }
            | Self::MissingHypothesisKind { place, .. }
            | Self::UnknownEvidence { place, .. }
            | Self::UnknownHypothesis { place, .. }
            | Self::BeyondCapability { place, .. } => place.clone(),
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::Registry(RegistryIssue::MissingField { field, .. }) => {
                format!("資格の表の行に文字列の `{field}` が無い")
            }
            Self::Registry(RegistryIssue::UnknownSupport { value, .. }) => format!(
                "資格の表の support `{value}` は can-support / cannot-support / \
                 hypothesis-generation-only / context-dependent のいずれでもない"
            ),
            Self::Registry(RegistryIssue::UnknownObserver { value, .. }) => {
                format!("資格の表の observer `{value}` は human / automated のどちらでもない")
            }
            Self::Registry(RegistryIssue::DuplicatePair {
                hypothesis_kind,
                method,
                ..
            }) => format!(
                "資格の表に（{hypothesis_kind}, {method}）の行が2つある。 食い違えば、支持にならない側に倒す"
            ),
            Self::Registry(RegistryIssue::ObserverConflict { method, .. }) => format!(
                "資格の表で、観測方法 {method} の observer が行によって違う。 同じ方法は同じ値にする"
            ),
            Self::UnknownHypothesisKind {
                hypothesis, kind, ..
            } => format!("{hypothesis}: hypothesis_kind `{kind}` は資格の表に無い"),
            Self::UnknownMethod { owner, method, .. } => {
                format!("{owner}: method `{method}` は資格の表に無い")
            }
            Self::MissingHypothesisKind {
                hypothesis,
                evidence,
                ..
            } => format!(
                "{hypothesis} に hypothesis_kind が無く、観測方法を書いた {evidence} が支持するための\
                 資格を決められない"
            ),
            Self::UnknownEvidence {
                hypothesis,
                evidence,
                ..
            } => format!("{hypothesis}: 証拠として挙げた `{evidence}` という調査は存在しない"),
            Self::UnknownHypothesis {
                evidence,
                hypothesis,
                ..
            } => format!("{evidence}: assesses の `{hypothesis}` という仮説は存在しない"),
            Self::BeyondCapability {
                hypothesis,
                kind,
                evidence,
                method,
                capability,
                ..
            } => {
                let why = match capability {
                    Capability::HypothesisGenerationOnly => {
                        "仮説を作る材料にしかならない（hypothesis-generation-only）"
                    }
                    Capability::ContextDependent => {
                        "必要条件を読めないので自動では昇格させない（context-dependent）"
                    }
                    Capability::Unregistered => "資格の表に行が無く、自動では支持にならない",
                    _ => "それだけでは Evidence にならない（cannot-support）",
                };
                format!(
                    "{evidence} は観測方法 {method} で {hypothesis}（{kind}）を支持すると言っているが、\
                     {why}"
                )
            }
        }
    }
}

/// `snapshot` の資格の表・仮説・調査・Probe を検査する。 仮説の `assessment` は読まず、
/// 書き換えない。
pub(crate) fn findings(snapshot: &KnowledgeSnapshot) -> Vec<Finding> {
    let (registry, issues) = CapabilityRegistry::from_snapshot(snapshot);
    let facts = Facts::from_snapshot(snapshot);
    let mut out: Vec<Finding> = issues.into_iter().map(Finding::Registry).collect();

    // 語が表にあるか。 欄を書いたものだけ見る。 書かない記録を、落とさない。
    for h in &facts.hypotheses {
        if let Some(kind) = &h.kind
            && !registry.has_kind(kind)
        {
            out.push(Finding::UnknownHypothesisKind {
                hypothesis: h.id.clone(),
                kind: kind.clone(),
                place: h.place.clone(),
            });
        }
    }
    let owners = facts
        .evidence
        .iter()
        .map(|e| (&e.id, &e.method, &e.place))
        .chain(facts.probes.iter().map(|p| (&p.id, &p.method, &p.place)));
    for (id, method, place) in owners {
        if let Some(method) = method
            && !registry.has_method(method)
        {
            out.push(Finding::UnknownMethod {
                owner: id.clone(),
                method: method.clone(),
                place: place.clone(),
            });
        }
    }

    // 支持すると言っている組が、資格を持つか。
    let mut seen: BTreeSet<(&str, &str)> = BTreeSet::new();
    for h in &facts.hypotheses {
        let mut claims: Vec<(&Observation, Option<Location>)> = Vec::new();
        for id in &h.supporting {
            match facts.observation(id) {
                Some(e) => claims.push((e, h.place.clone())),
                None => out.push(Finding::UnknownEvidence {
                    hypothesis: h.id.clone(),
                    evidence: id.clone(),
                    place: h.place.clone(),
                }),
            }
        }
        // 調査の側から評価すると言っているもの。 仮説が反証側に挙げた調査は、支持ではない。
        for e in &facts.evidence {
            if e.assesses.contains(&h.id) && !h.contradicting.contains(&e.id) {
                claims.push((e, e.place.clone()));
            }
        }
        for (e, place) in claims {
            if !seen.insert((h.id.as_str(), e.id.as_str())) {
                continue;
            }
            // 欄が無い調査は判定しない。
            let Some(method) = &e.method else { continue };
            let Some(kind) = &h.kind else {
                out.push(Finding::MissingHypothesisKind {
                    hypothesis: h.id.clone(),
                    evidence: e.id.clone(),
                    place,
                });
                continue;
            };
            let capability = registry.assess(kind, method);
            // 語の誤りは、上で別に出した。
            if capability.supports()
                || matches!(
                    capability,
                    Capability::UnknownKind | Capability::UnknownMethod
                )
            {
                continue;
            }
            out.push(Finding::BeyondCapability {
                hypothesis: h.id.clone(),
                kind: kind.clone(),
                evidence: e.id.clone(),
                method: method.clone(),
                capability,
                place,
            });
        }
    }
    // 評価すると言った仮説が無い。
    for e in &facts.evidence {
        for id in &e.assesses {
            if facts.hypothesis(id).is_none() {
                out.push(Finding::UnknownHypothesis {
                    evidence: e.id.clone(),
                    hypothesis: id.clone(),
                    place: e.place.clone(),
                });
            }
        }
    }
    out
}

/// `check-meta` が呼ぶ入口。 見つけたことを診断に積む。
pub(crate) fn check_verification(snapshot: &KnowledgeSnapshot, rep: &mut Report) {
    for f in findings(snapshot) {
        rep.error_at(f.code(), f.place(), f.message());
    }
}
