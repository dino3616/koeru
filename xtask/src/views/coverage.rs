//! Requirement Coverage（D07b）。 要件ごとに、何が覆っているかを関係ごとに分けて出す。
//!
//! 要件の網羅を `needs_component` のような真偽1つで表さない（実装計画 8）。 同じ要件を
//! 3つの別の問いが見る。 混ぜると、形式にしただけで実現したことになり、部品を採っただけで
//! 確かめたことになる。
//!
//! - `formalizes` — どの FSL の contract が、要件の一部を形式にしているか。 一部であって
//!   完全な表現ではない（`partial`）
//! - `implements` — どの部品が要件を実現するか。 採用の部品だけが数えられる（`DEC-PLT-048`）。
//!   不適・採用候補は要件と結びついていても数えず、`unadopted` に分けて残す。
//!   今の供給元は台帳の採用行。 台帳を消した後に何が張るかは D08 が決める（`DEC-PLT-049`）
//! - `targets` — どの Probe が要件を確かめるためのものか。 実行したことは意味しない
//!   （Receipt が持つ）
//!
//! 関係の規則は D00 の語彙（`knowledge::design`）が持つ。 ここは graph の edge を読んで
//! 数えるだけで、覆っていない要件の合否は持たない。

// 消費者（D07c・D08・D10）が付くまで、production からは呼ばれない。
#![allow(dead_code)]

use crate::graph::{NodeKey, SemanticGraph};
use crate::knowledge::Id;
use crate::knowledge::design::{Design, Kind, relation};

/// 1本の関係から見た被覆。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Axis {
    /// 数えた関係の名前。
    pub(crate) relation: &'static str,
    /// 要件の一部だけを表す関係か。 そうなら、空でないことは完全な被覆ではない。
    pub(crate) partial: bool,
    /// 要件を覆う側の ID。 重複なし、昇順。
    pub(crate) by: Vec<Id>,
}

impl Axis {
    pub(crate) fn is_covered(&self) -> bool {
        !self.by.is_empty()
    }
}

/// 1つの要件の被覆。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequirementCoverage {
    pub(crate) requirement: Id,
    pub(crate) title: Option<String>,
    /// 形式にしている contract。
    pub(crate) formalizes: Axis,
    /// 実現する採用の部品。
    pub(crate) implements: Axis,
    /// 確かめるための Probe。
    pub(crate) targets: Axis,
    /// 要件と結びついているが、採用ではないために `implements` に数えない部品。
    pub(crate) unadopted: Vec<Id>,
}

impl RequirementCoverage {
    /// 3つの関係のどれも覆っていない。
    pub(crate) fn is_uncovered(&self) -> bool {
        !(self.formalizes.is_covered() || self.implements.is_covered() || self.targets.is_covered())
    }
}

/// 関係ごとの件数。 合否ではない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CoverageSummary {
    pub(crate) requirements: usize,
    pub(crate) formalized: usize,
    pub(crate) implemented: usize,
    pub(crate) targeted: usize,
    /// 3つとも覆っていない。
    pub(crate) uncovered: usize,
}

/// 全要件の被覆。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Coverage {
    /// 要件の ID 順。
    pub(crate) requirements: Vec<RequirementCoverage>,
}

impl Coverage {
    /// `graph` は `design.graph()`。 要件は、種類が要件の node すべて
    /// （技術要件と、定量の要件である上限）。
    pub(crate) fn build(design: &Design, graph: &SemanticGraph) -> Self {
        let mut requirements: Vec<RequirementCoverage> = graph
            .nodes()
            .filter(|n| Kind::of(n) == Some(Kind::Requirement))
            .filter_map(|n| match &n.key {
                NodeKey::Object(id) => Some(id),
                NodeKey::Locator { .. } => None,
            })
            .map(|id| {
                let title = design.object(id).and_then(|o| o.title().map(str::to_owned));
                RequirementCoverage {
                    requirement: id.clone(),
                    title,
                    formalizes: axis(graph, id, "formalizes"),
                    // 採用でない部品は D00 の組み立てが `implements` にしないので、
                    // ここで採否を見直さない。
                    implements: axis(graph, id, "implements"),
                    targets: axis(graph, id, "targets"),
                    // `supports_requirements` は調査（EVID）も張る。 部品だけを取る。
                    unadopted: sources(graph, id, "supports_requirements", |k| {
                        k == Kind::Dependency
                    }),
                }
            })
            .collect();
        requirements.sort_by(|a, b| a.requirement.cmp(&b.requirement));
        Self { requirements }
    }

    pub(crate) fn summary(&self) -> CoverageSummary {
        let count = |pred: fn(&RequirementCoverage) -> bool| {
            self.requirements.iter().filter(|r| pred(r)).count()
        };
        CoverageSummary {
            requirements: self.requirements.len(),
            formalized: count(|r| r.formalizes.is_covered()),
            implemented: count(|r| r.implements.is_covered()),
            targeted: count(|r| r.targets.is_covered()),
            uncovered: count(RequirementCoverage::is_uncovered),
        }
    }
}

/// `name` は D00 の語彙にある関係。 張ってよい側の種類は、その規則が決める。
fn axis(graph: &SemanticGraph, requirement: &Id, name: &'static str) -> Axis {
    let rule = relation(name);
    Axis {
        relation: name,
        partial: rule.is_some_and(|r| r.partial),
        by: sources(graph, requirement, name, |k| {
            rule.is_some_and(|r| r.from.admits(k))
        }),
    }
}

/// `requirement` へ `name` の edge を張っている、種類が `admits` を通る node の ID。
fn sources(
    graph: &SemanticGraph,
    requirement: &Id,
    name: &str,
    admits: impl Fn(Kind) -> bool,
) -> Vec<Id> {
    let mut out: Vec<Id> = graph
        .incoming(&NodeKey::Object(requirement.clone()))
        .iter()
        .filter(|rel| rel.name == name)
        .filter(|rel| {
            graph
                .node(&rel.from)
                .and_then(Kind::of)
                .is_some_and(&admits)
        })
        .filter_map(|rel| match &rel.from {
            NodeKey::Object(id) => Some(id.clone()),
            NodeKey::Locator { .. } => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod tests;
