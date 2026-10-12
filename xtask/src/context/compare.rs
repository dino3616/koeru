//! `diff(C(root, from), C(root, at))`。 2つの版の Context を比べる。
//!
//! 比べる範囲は、旧・新の closure の和集合。 head で制約を消しただけで「現在の制約は無くなった」
//! と扱わない（提案書 7.4）——旧い版にだけある項目も edge も、消えたという事実ごと残る。
//!
//! 項目の増減は graph の `diff`（X05）の結果を和集合に絞って使い、closure の出入りと edge の出入りは
//! 2つの Context を突き合わせて出す。 Git の textual diff ではなく、意味の変化を分類する。
//!
//! - 追加・削除: closure に入った・出た。 node が graph から消えたのか、辿れなくなっただけなのかは別に持つ
//! - 変更: 同じ項目の内容・edge が変わった
//! - 置き換え: 後継ができた
//! - 新しい反証: 仮説の `contradicting_evidence` に調査が増えた
//! - 範囲の変更: 仮説の `scope` が変わった
//! - 問いの開閉
//! - 解けない対応: UNMAPPED の変化、宙に浮いた edge

use std::collections::{BTreeMap, BTreeSet};

use super::compile::{Context, EdgeInfo, EdgeKey, RootInput, RootReport, World, key_text};
use super::rules::Tier;
use crate::graph::{ChangeKind, GraphDiff, NodeKey, RelationClass, diff};
use crate::knowledge::Id;
use crate::knowledge::design::Kind;

/// 1件の変化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Change {
    Added {
        key: NodeKey,
        tier: Tier,
        /// node が新しい版の graph に新しく現れた。 `false` なら、すでにあるものが closure に入った。
        new_in_graph: bool,
    },
    Removed {
        key: NodeKey,
        tier: Tier,
        /// node が新しい版の graph から消えた。 `false` なら、あるが closure から外れた。
        gone_from_graph: bool,
    },
    Changed {
        key: NodeKey,
        tier: Tier,
        how: ChangeKind,
    },
    Superseded {
        key: NodeKey,
        tier: Tier,
        /// 新しい版での後継。
        by: Vec<Id>,
    },
    NewlyContradicted {
        hypothesis: Id,
        evidence: Vec<Id>,
    },
    ScopeChanged {
        hypothesis: Id,
    },
    QuestionOpened {
        key: NodeKey,
    },
    QuestionClosed {
        key: NodeKey,
        /// 問いが新しい版の graph から消えた。
        gone_from_graph: bool,
    },
    EdgeAdded {
        edge: EdgeKey,
        info: EdgeInfo,
    },
    EdgeRemoved {
        edge: EdgeKey,
        /// 旧い版での edge の出どころ。
        info: EdgeInfo,
    },
    /// 新しい版で、行き先の node が無い edge。
    UnresolvedEdge {
        from: NodeKey,
        relation: &'static str,
        to: NodeKey,
    },
    /// root が、関係する契約を特定できる・できないが変わった。
    MappingChanged {
        root: RootInput,
        before: &'static str,
        after: &'static str,
    },
}

impl Change {
    /// 出力の節の名前。 提案書 7.2.1 の分類。
    pub(crate) fn class(&self) -> &'static str {
        match self {
            Self::Added { .. } => "added",
            Self::Removed { .. } => "removed",
            Self::Changed { .. } => "changed",
            Self::Superseded { .. } => "superseded",
            Self::NewlyContradicted { .. } => "newly-contradicted",
            Self::ScopeChanged { .. } => "scope-changed",
            Self::QuestionOpened { .. } => "question-opened",
            Self::QuestionClosed { .. } => "question-closed",
            Self::EdgeAdded { .. } => "edge-added",
            Self::EdgeRemoved { .. } => "edge-removed",
            Self::UnresolvedEdge { .. } | Self::MappingChanged { .. } => "unresolved-mapping",
        }
    }
}

/// 2つの Context の比較。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Changes {
    pub(crate) list: Vec<Change>,
}

impl Changes {
    pub(crate) fn of_class(&self, class: &str) -> impl Iterator<Item = &Change> {
        self.list.iter().filter(move |c| c.class() == class)
    }
}

/// 分類の並び。 出力の節はこの順。
pub(crate) const CLASSES: &[&str] = &[
    "newly-contradicted",
    "scope-changed",
    "superseded",
    "question-opened",
    "question-closed",
    "added",
    "removed",
    "changed",
    "edge-added",
    "edge-removed",
    "unresolved-mapping",
];

/// `old`（比較元）と `new`（読む地点）を比べる。 `old_ctx`・`new_ctx` は予算を掛けていないもの。
pub(crate) fn compare(
    old: &World<'_>,
    old_ctx: &Context,
    new: &World<'_>,
    new_ctx: &Context,
) -> Changes {
    let graph_diff = diff(&old.graph, &new.graph);
    let union: BTreeSet<NodeKey> = old_ctx
        .items
        .keys()
        .chain(new_ctx.items.keys())
        .cloned()
        .collect();
    let tier_of = |key: &NodeKey| {
        let old_tier = old_ctx.items.get(key).map(|i| i.tier);
        let new_tier = new_ctx.items.get(key).map(|i| i.tier);
        match (old_tier, new_tier) {
            (Some(a), Some(b)) => a.min(b),
            (Some(t), None) | (None, Some(t)) => t,
            (None, None) => Tier::Peripheral(u8::MAX),
        }
    };

    let mut list = Vec::new();
    membership(old_ctx, new_ctx, &union, &graph_diff, &tier_of, &mut list);
    changed_and_superseded(new, &union, &graph_diff, &tier_of, &mut list);
    hypotheses(old, new, &union, &mut list);
    questions(old, new, &union, &mut list);
    edges(old_ctx, new_ctx, &mut list);
    mapping(&old_ctx.roots, &new_ctx.roots, &mut list);
    for (from, relation, to) in &graph_diff.unresolved {
        if union.contains(from) || union.contains(to) {
            list.push(Change::UnresolvedEdge {
                from: from.clone(),
                relation,
                to: to.clone(),
            });
        }
    }

    list.sort_by_cached_key(sort_key);
    Changes { list }
}

fn membership(
    old_ctx: &Context,
    new_ctx: &Context,
    union: &BTreeSet<NodeKey>,
    graph_diff: &GraphDiff,
    tier_of: &dyn Fn(&NodeKey) -> Tier,
    out: &mut Vec<Change>,
) {
    for key in union {
        match (
            old_ctx.items.contains_key(key),
            new_ctx.items.contains_key(key),
        ) {
            (false, true) => out.push(Change::Added {
                key: key.clone(),
                tier: tier_of(key),
                new_in_graph: graph_diff.added.contains(key),
            }),
            (true, false) => out.push(Change::Removed {
                key: key.clone(),
                tier: tier_of(key),
                gone_from_graph: graph_diff.removed.contains_key(key),
            }),
            _ => {}
        }
    }
}

fn changed_and_superseded(
    new: &World<'_>,
    union: &BTreeSet<NodeKey>,
    graph_diff: &GraphDiff,
    tier_of: &dyn Fn(&NodeKey) -> Tier,
    out: &mut Vec<Change>,
) {
    for (key, how) in &graph_diff.changed {
        if union.contains(key) {
            out.push(Change::Changed {
                key: key.clone(),
                tier: tier_of(key),
                how: *how,
            });
        }
    }
    for key in &graph_diff.superseded {
        if !union.contains(key) {
            continue;
        }
        let mut by: Vec<Id> = new
            .graph
            .incoming(key)
            .iter()
            .filter(|r| r.class == RelationClass::Supersession)
            .filter_map(|r| match &r.from {
                NodeKey::Object(id) => Some(id.clone()),
                NodeKey::Locator { .. } => None,
            })
            .collect();
        by.sort();
        by.dedup();
        out.push(Change::Superseded {
            key: key.clone(),
            tier: tier_of(key),
            by,
        });
    }
}

/// 仮説の状態は、closure に入っていたかではなく、それぞれの版の記録から読む。
fn hypotheses(old: &World<'_>, new: &World<'_>, union: &BTreeSet<NodeKey>, out: &mut Vec<Change>) {
    for key in union {
        let NodeKey::Object(id) = key else { continue };
        let Some(now) = new.targets.hypotheses.get(id) else {
            continue;
        };
        let before = old.targets.hypotheses.get(id);
        let known: BTreeSet<&Id> =
            before.map_or_else(BTreeSet::new, |h| h.contradicting.iter().collect());
        let fresh: Vec<Id> = now
            .contradicting
            .iter()
            .filter(|e| !known.contains(e))
            .cloned()
            .collect();
        if !fresh.is_empty() {
            out.push(Change::NewlyContradicted {
                hypothesis: id.clone(),
                evidence: fresh,
            });
        }
        if before.is_some_and(|b| b.scope != now.scope) {
            out.push(Change::ScopeChanged {
                hypothesis: id.clone(),
            });
        }
    }
}

fn questions(old: &World<'_>, new: &World<'_>, union: &BTreeSet<NodeKey>, out: &mut Vec<Change>) {
    for key in union {
        if old.kind_of(key) != Some(Kind::Question) && new.kind_of(key) != Some(Kind::Question) {
            continue;
        }
        let (was, is) = (old.is_open_question(key), new.is_open_question(key));
        if !was && is {
            out.push(Change::QuestionOpened { key: key.clone() });
        } else if was && !is {
            out.push(Change::QuestionClosed {
                key: key.clone(),
                gone_from_graph: new.graph.node(key).is_none(),
            });
        }
    }
}

fn edges(old_ctx: &Context, new_ctx: &Context, out: &mut Vec<Change>) {
    let old_edges: &BTreeMap<EdgeKey, EdgeInfo> = &old_ctx.edges;
    let new_edges: &BTreeMap<EdgeKey, EdgeInfo> = &new_ctx.edges;
    for (edge, info) in new_edges {
        if !old_edges.contains_key(edge) {
            out.push(Change::EdgeAdded {
                edge: edge.clone(),
                info: info.clone(),
            });
        }
    }
    for (edge, info) in old_edges {
        if !new_edges.contains_key(edge) {
            out.push(Change::EdgeRemoved {
                edge: edge.clone(),
                info: info.clone(),
            });
        }
    }
}

fn mapping(old: &[RootReport], new: &[RootReport], out: &mut Vec<Change>) {
    for (before, after) in old.iter().zip(new) {
        if before.state.name() != after.state.name() {
            out.push(Change::MappingChanged {
                root: after.input.clone(),
                before: before.state.name(),
                after: after.state.name(),
            });
        }
    }
}

/// 節の順、次に鍵の順。
fn sort_key(change: &Change) -> (usize, String) {
    let class = CLASSES
        .iter()
        .position(|c| *c == change.class())
        .unwrap_or(CLASSES.len());
    let key = match change {
        Change::Added { key, .. }
        | Change::Removed { key, .. }
        | Change::Changed { key, .. }
        | Change::Superseded { key, .. }
        | Change::QuestionOpened { key }
        | Change::QuestionClosed { key, .. } => key_text(key),
        Change::NewlyContradicted { hypothesis, .. } | Change::ScopeChanged { hypothesis } => {
            hypothesis.as_str().to_owned()
        }
        Change::EdgeAdded { edge, .. } | Change::EdgeRemoved { edge, .. } => format!(
            "{}\u{0}{}\u{0}{}",
            key_text(&edge.from),
            edge.relation,
            key_text(&edge.to)
        ),
        Change::UnresolvedEdge { from, relation, to } => {
            format!("{}\u{0}{relation}\u{0}{}", key_text(from), key_text(to))
        }
        Change::MappingChanged { root, .. } => root.text(),
    };
    (class, key)
}
