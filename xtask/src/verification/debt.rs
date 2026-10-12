//! Human Verification Debt（提案書 10.3.1）。 人の観察でしか足りない仮説の、導出した待ち行列。
//!
//! 正本ではない。 仮説・判断・調査・Probe の記録から、いつでも引き直せる。 **優先度を単一の
//! score にしない。** 人と会える一度の機会を、最も情報の多い観察に変えるための planning surface
//! なので、並びは仮説の ID 順で、何を先にするかは持たない。
//!
//! 「人の観察でしか足りない」は、資格の表で、その種類の仮説を支持できる観測方法が、すべて人の
//! 観察であること（[`super::CapabilityRegistry::human_only`]）。 表が観察者を言っていない方法を
//! 含む種類は含めない。
//!
//! 待ち行列から消えるのは、次のどれか。 未検証の数を減らすことを目的にしない。
//! - 人の観察の調査が付いた（支持でも反証でも。 同じ起源は1つと数える）
//! - その仮説に依存する判断が無くなった（置き換えられた判断は数えない）
//! - 仮説の `assessment` が `superseded`（人が書いた値を読むだけ）
//!
//! 入れていないもの: blast radius・reversibility・同じ session で観察できる他の仮説。
//! どれも、記録のどこにも持つ欄が無い。 欄を足す判断は D07c / 後続に残す。
//!
//! CLI（`design-debt --human`）と Receipt との突き合わせは D07c が作る。

use super::capability::CapabilityRegistry;
use super::facts::{Facts, Hypothesis, Observation, ProbeSpec};
use super::origin::independent_origins;

/// 仮説に付いた調査と、その資格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EvidenceStanding {
    pub(crate) id: String,
    /// 観測方法。 欄が無い調査（今の `EVID-*`）は `None`。
    pub(crate) method: Option<String>,
    /// 仮説を支持するとも、反証するとも言っていない調査。
    pub(crate) stance: EvidenceStance,
    /// 人の観察として、その種類の仮説を支持できる方法で得た調査か。
    pub(crate) human_qualified: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EvidenceStance {
    Supporting,
    Contradicting,
    /// `assesses` だけ。 どちらとも言っていない。
    Assessing,
}

/// 何が足りないか。 ここに出るのは待ち行列に残る仮説だけなので、人の観察の調査は無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Shortfall {
    /// 調査が1つも付いていない。
    NoEvidence,
    /// 調査はあるが、どれも人の観察ではない。
    NoHumanObservation,
}

/// 仮説を対象にしている Probe。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeStanding {
    pub(crate) id: String,
    pub(crate) method: Option<String>,
    /// 観測方法が、その種類の仮説を人の観察として支持できる Probe。 走らせれば、足りない
    /// 観察になる。
    pub(crate) can_close_shortfall: bool,
}

/// 待ち行列の1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HumanDebtItem {
    pub(crate) hypothesis: String,
    pub(crate) kind: String,
    /// なぜ人の観察が要るか。 その種類の仮説を支持できる観測方法の、すべて。
    pub(crate) why_human: Vec<String>,
    /// 依存している判断と、その判断が作る・直す要件。
    pub(crate) decisions: Vec<String>,
    pub(crate) requirements: Vec<String>,
    /// 今ある調査。
    pub(crate) evidence: Vec<EvidenceStanding>,
    /// 今ある調査の、独立した起源の数。 同じ起源の調査は1つと数える。 件数を確信度にしない。
    pub(crate) origins: usize,
    pub(crate) shortfall: Shortfall,
    pub(crate) probes: Vec<ProbeStanding>,
}

impl HumanDebtItem {
    /// 足りない観察を取れる Probe があるか。
    pub(crate) fn has_valid_probe(&self) -> bool {
        self.probes.iter().any(|p| p.can_close_shortfall)
    }
}

/// 人の観察でしか足りない仮説の待ち行列。 仮説の ID 順。
pub(crate) fn human_verification_debt(
    facts: &Facts,
    registry: &CapabilityRegistry,
) -> Vec<HumanDebtItem> {
    let mut out: Vec<HumanDebtItem> = facts
        .hypotheses
        .iter()
        .filter_map(|h| item(facts, registry, h))
        .collect();
    out.sort_by(|a, b| a.hypothesis.cmp(&b.hypothesis));
    out
}

fn item(facts: &Facts, registry: &CapabilityRegistry, h: &Hypothesis) -> Option<HumanDebtItem> {
    let kind = h.kind.as_deref()?;
    let why_human: Vec<String> = registry
        .human_only(kind)?
        .into_iter()
        .map(str::to_owned)
        .collect();
    if h.assessment.as_deref() == Some("superseded") {
        return None;
    }
    let dependents: Vec<_> = facts
        .decisions
        .iter()
        .filter(|d| d.is_live() && d.relies_on.contains(&h.id))
        .collect();
    if dependents.is_empty() {
        return None;
    }
    let linked = linked_evidence(facts, h);
    let standings: Vec<EvidenceStanding> = linked
        .iter()
        .map(|(e, stance)| EvidenceStanding {
            id: e.id.clone(),
            method: e.method.clone(),
            stance: *stance,
            human_qualified: e
                .method
                .as_deref()
                .is_some_and(|m| registry.is_human_support(kind, m)),
        })
        .collect();
    // 人の観察の調査が付いたら、解決した。 支持でも反証でも、人が観察した事実は残る。
    if standings.iter().any(|s| s.human_qualified) {
        return None;
    }
    let others: Vec<&Observation> = linked.iter().map(|(e, _)| *e).collect();
    let shortfall = if standings.is_empty() {
        Shortfall::NoEvidence
    } else {
        Shortfall::NoHumanObservation
    };
    let mut requirements: Vec<String> = dependents
        .iter()
        .flat_map(|d| d.requirements.iter().cloned())
        .collect();
    requirements.sort();
    requirements.dedup();
    let mut decisions: Vec<String> = dependents.iter().map(|d| d.id.clone()).collect();
    decisions.sort();
    let probes = facts
        .probes
        .iter()
        .filter(|p| p.targets.contains(&h.id))
        .map(|p| probe_standing(registry, kind, p))
        .collect();
    Some(HumanDebtItem {
        hypothesis: h.id.clone(),
        kind: kind.to_owned(),
        why_human,
        decisions,
        requirements,
        evidence: standings,
        origins: independent_origins(&others),
        shortfall,
        probes,
    })
}

fn probe_standing(registry: &CapabilityRegistry, kind: &str, p: &ProbeSpec) -> ProbeStanding {
    ProbeStanding {
        id: p.id.clone(),
        method: p.method.clone(),
        can_close_shortfall: p
            .method
            .as_deref()
            .is_some_and(|m| registry.is_human_support(kind, m)),
    }
}

/// 仮説に付いた調査。 仮説の `supporting_evidence` / `contradicting_evidence` と、調査の
/// `assesses` の両側から引く。 同じ調査が2つの側から引かれたら、仮説が言う側を優先する。
/// 記録に無い ID は引けないので飛ばす（検査が別に出す）。
pub(crate) fn linked_evidence<'a>(
    facts: &'a Facts,
    h: &Hypothesis,
) -> Vec<(&'a Observation, EvidenceStance)> {
    let mut out: Vec<(&Observation, EvidenceStance)> = Vec::new();
    let mut push = |e: &'a Observation, stance| {
        if !out.iter().any(|(seen, _)| seen.id == e.id) {
            out.push((e, stance));
        }
    };
    for id in &h.supporting {
        if let Some(e) = facts.observation(id) {
            push(e, EvidenceStance::Supporting);
        }
    }
    for id in &h.contradicting {
        if let Some(e) = facts.observation(id) {
            push(e, EvidenceStance::Contradicting);
        }
    }
    for e in &facts.evidence {
        if e.assesses.contains(&h.id) {
            push(e, EvidenceStance::Assessing);
        }
    }
    out
}
