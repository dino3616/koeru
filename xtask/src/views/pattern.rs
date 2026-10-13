//! Derived Pattern View（D06）。 graph の中で何度も現れる構造を、候補として毎回計算して出す。
//!
//! Pattern は SSOT の object にしない（提案書 5.1.3）。 `PAT-*` も `meta/patterns/` も無く、
//! ここが出す [`Candidate`] はどこにも保存されない。 同じ規則を元の Q / HYP / EVID / DEC / REQ と
//! Pattern の2箇所に持たないための選択で、source が変われば次の計算で候補も変わる。
//!
//! 見つけるのは、明示された relation と typed field から決定的に決まる構造だけ。
//! 「この3件は同じ意味だ」という semantic な抽象は確定しない。 そのため候補が持つのは、
//! 何の規則で、どの object を元に、何を共有しているか、だけ。 名前・説明・反例は持たない
//! （Agent が後から提案する層で、authority を持たない）。
//!
//! **候補に authority は無い。** 候補があることも無いことも、合否や検査の失敗にしない。
//! 規範にしたいなら、新しい DEC が REQ を establish / revise する（提案書 11.4）。
//!
//! 4つの規則。 どの relation を読むかは [`BASES`] の1か所に表にしてある。
//!
//! 1. [`PatternKind::ConvergentRequirement`] — 2つ以上の現在有効な DEC が、同じ現在有効な REQ を
//!    `establishes` / `revises` / `removes` している
//! 2. [`PatternKind::SharedPremise`] — 2つ以上の現在有効な DEC が、同じ HYP に
//!    `relies_on_hypotheses`、または同じ EVID を `considers` している
//! 3. [`PatternKind::RepeatedShape`] — 異なる Q から辿った reasoning subgraph の形が一致する
//! 4. [`PatternKind::RepeatedCondition`] — 2つ以上の現在有効な DEC が、同じ見直し条件を持つ
//!
//! 現在有効でない object は、source にも共有の対象にも数えない。 後継を持つ（`supersedes` の
//! 行き先になっている）もの、DEC なら `superseded` と名乗るもの、が現在有効でない。
//! 後継が同じ構造を引き継いでいれば、後継を source にして候補が組み直される。
//!
//! 入力は D00 の型つきの口（`knowledge::design`）と graph だけ。 legacy の接頭辞や meta の
//! 表の並びは引かない。

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use crate::graph::{NodeKey, Relation, SemanticGraph};
use crate::knowledge::Id;
use crate::knowledge::design::{Decision, Design, Kind, relation};

/// 候補の種類。 提案書 5.1.3 の4つの例に1つずつ対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum PatternKind {
    /// 同じ要件へ、複数の判断が収束している。
    ConvergentRequirement,
    /// 同じ経験的前提（HYP・EVID）に、複数の判断が依っている。
    SharedPremise,
    /// 異なる Q から辿った reasoning subgraph が、同じ形をしている。
    RepeatedShape,
    /// 同じ見直し条件が、複数の判断に反復している。
    RepeatedCondition,
}

impl PatternKind {
    pub(crate) const ALL: [PatternKind; 4] = [
        PatternKind::ConvergentRequirement,
        PatternKind::SharedPremise,
        PatternKind::RepeatedShape,
        PatternKind::RepeatedCondition,
    ];

    pub(crate) const fn name(self) -> &'static str {
        match self {
            PatternKind::ConvergentRequirement => "convergent-requirement",
            PatternKind::SharedPremise => "shared-premise",
            PatternKind::RepeatedShape => "repeated-shape",
            PatternKind::RepeatedCondition => "repeated-condition",
        }
    }
}

// 候補を出した規則の名前。 種類の中でさらに分かれるもの（前提が HYP か EVID か）を区別する。
pub(crate) const RULE_REQUIREMENT: &str = "converge-on-requirement";
pub(crate) const RULE_HYPOTHESIS: &str = "rely-on-hypothesis";
pub(crate) const RULE_EVIDENCE: &str = "consider-evidence";
pub(crate) const RULE_SHAPE: &str = "repeat-reasoning-shape";
pub(crate) const RULE_CONDITION: &str = "repeat-condition";

/// この view が読む relation の表。 どの relation をどの規則に使うかは、ここだけが持つ。
///
/// 名前は D00 の語彙（`knowledge::design::RELATIONS`）にある正規の関係だけ。 端点の種類も
/// 語彙の規則（`RelationSpec::from` / `to`）で見るので、ここには書かない。 `mentions`（引用）、
/// `affects`（論理的含意ではない）、`provenance`・`context_for` は規範的な構造を言わないので、
/// 載せない。 `constrains` は REQ から DEC へ向く関係で、収束の向きと逆なので載せない。
const BASES: &[Basis] = &[
    Basis::reasoning("resolved_by"),
    Basis::reasoning("assesses"),
    Basis::reasoning("relies_on_hypotheses").premise(RULE_HYPOTHESIS),
    Basis::reasoning("considers").premise(RULE_EVIDENCE),
    Basis::reasoning("establishes").changes_requirement(),
    Basis::reasoning("revises").changes_requirement(),
    Basis::reasoning("removes").changes_requirement(),
];

/// 1つの relation が、どの規則の材料になるか。
#[derive(Debug, Clone, Copy)]
struct Basis {
    relation: &'static str,
    /// 規則1。 DEC が REQ を変える。
    changes_requirement: bool,
    /// 規則2。 DEC が依る前提。 値は規則の名前。
    premise: Option<&'static str>,
    /// 規則3。 reasoning subgraph の辺として辿る。
    reasoning: bool,
}

impl Basis {
    const fn reasoning(relation: &'static str) -> Self {
        Self {
            relation,
            changes_requirement: false,
            premise: None,
            reasoning: true,
        }
    }

    const fn premise(mut self, rule: &'static str) -> Self {
        self.premise = Some(rule);
        self
    }

    const fn changes_requirement(mut self) -> Self {
        self.changes_requirement = true;
        self
    }

    fn of(relation: &str) -> Option<&'static Basis> {
        BASES.iter().find(|b| b.relation == relation)
    }
}

/// 規則4が値を比べる、DEC の typed field。 空白を畳んだ文字列が一致するものを同じとみなす。
/// 自由文の意味的な類似は扱わない。
const CONDITIONS: &[Condition] = &[Condition {
    field: "review_triggers",
    read: |d| d.review_triggers(),
}];

struct Condition {
    field: &'static str,
    read: fn(&Decision<'_>) -> Vec<String>,
}

/// reasoning subgraph の下限。 辺がこれより少ない形は、Q から判断へ1本引いただけで、
/// 全部の Q が同じ形になる。 候補にしても何も言っていない。
const MIN_SHAPE_EDGES: usize = 2;

/// reasoning subgraph の形。 ID も本文も含めない。
///
/// Q を根に、[`BASES`] の `reasoning` の関係を現在有効な node だけ辿ったとき、
/// 辿った node の種類の多重集合と、辿った辺の（始点の種類、relation 名、終点の種類）の
/// 多重集合。 辺の端点が同じ node かどうか（同じ EVID を2つの DEC が引くのか、別々の EVID
/// を1つずつ引くのか）は、node の種類の数が違うので区別される。
///
/// 同形の十分条件ではない。 2つの異なる graph が同じ要約を持ちうる。 一致は「同じ形だった」
/// という候補の根拠であって、同形の証明ではない。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Shape {
    pub(crate) nodes: BTreeMap<Kind, usize>,
    pub(crate) edges: BTreeMap<(Kind, &'static str, Kind), usize>,
}

impl Shape {
    /// 辺の本数。
    pub(crate) fn edge_count(&self) -> usize {
        self.edges.values().sum()
    }
}

/// 候補が何を共有しているか。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Shared {
    /// 規則1・2。 source が共通に指している object（REQ・HYP・EVID）。
    Object(Id),
    /// 規則3。 一致した形。
    Shape(Shape),
    /// 規則4。 一致した値（空白を1つに畳んだ文字列）と、それを持つ欄の名前。
    Condition { field: &'static str, text: String },
}

/// 1つの候補。 authority を持たず、合否にも使わない。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Candidate {
    pub(crate) kind: PatternKind,
    /// 出した規則の名前（`RULE_*`）。
    pub(crate) rule: &'static str,
    pub(crate) shared: Shared,
    /// 候補が立っている object の ID。 空にならない。 重複なし、昇順。
    /// source が現在有効でなくなれば、候補も消えるか組み直される。
    pub(crate) sources: Vec<Id>,
}

impl Candidate {
    /// `sources` が空の候補は作らない。 作ろうとしたら `None`。
    fn new(
        kind: PatternKind,
        rule: &'static str,
        shared: Shared,
        sources: BTreeSet<Id>,
    ) -> Option<Self> {
        (!sources.is_empty()).then(|| Self {
            kind,
            rule,
            shared,
            sources: sources.into_iter().collect(),
        })
    }
}

/// 種類ごとの件数。 合否ではない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct PatternSummary {
    pub(crate) convergent_requirement: usize,
    pub(crate) shared_premise: usize,
    pub(crate) repeated_shape: usize,
    pub(crate) repeated_condition: usize,
}

/// 全候補。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pattern {
    /// 種類、規則、共有、source の順。
    pub(crate) candidates: Vec<Candidate>,
}

impl Pattern {
    /// `graph` は `design.graph()`。 版ごとに毎回計算し、前の結果を持ち越さない。
    pub(crate) fn build(design: &Design, graph: &SemanticGraph) -> Self {
        let mut candidates = Vec::new();
        candidates.extend(shared_by_target(design, graph));
        candidates.extend(repeated_shapes(design, graph));
        candidates.extend(repeated_conditions(design, graph));
        candidates.sort();
        Self { candidates }
    }

    pub(crate) fn of_kind(&self, kind: PatternKind) -> impl Iterator<Item = &Candidate> {
        self.candidates.iter().filter(move |c| c.kind == kind)
    }

    pub(crate) fn summary(&self) -> PatternSummary {
        let count = |kind| self.of_kind(kind).count();
        PatternSummary {
            convergent_requirement: count(PatternKind::ConvergentRequirement),
            shared_premise: count(PatternKind::SharedPremise),
            repeated_shape: count(PatternKind::RepeatedShape),
            repeated_condition: count(PatternKind::RepeatedCondition),
        }
    }
}

/// edge の両端が node として実在し、D00 の語彙がその向きと種類を許している。
/// 許していなければ（宙に浮いた edge・種類違い）`None`。
fn kinds_of(graph: &SemanticGraph, rel: &Relation) -> Option<(Kind, Kind)> {
    let from = Kind::of(graph.node(&rel.from)?)?;
    let to = Kind::of(graph.node(&rel.to)?)?;
    let spec = relation(rel.name)?;
    (spec.from.admits(from) && spec.to.admits(to)).then_some((from, to))
}

fn object_id(key: &NodeKey) -> Option<&Id> {
    match key {
        NodeKey::Object(id) => Some(id),
        NodeKey::Locator { .. } => None,
    }
}

/// 規則1・2。 同じ対象へ edge を張る、現在有効な DEC を数える。
fn shared_by_target(design: &Design, graph: &SemanticGraph) -> Vec<Candidate> {
    let mut groups: BTreeMap<(PatternKind, &'static str, Id), BTreeSet<Id>> = BTreeMap::new();
    for rel in graph.all_relations() {
        let Some(basis) = Basis::of(rel.name) else {
            continue;
        };
        let (kind, rule) = if basis.changes_requirement {
            (PatternKind::ConvergentRequirement, RULE_REQUIREMENT)
        } else if let Some(rule) = basis.premise {
            (PatternKind::SharedPremise, rule)
        } else {
            continue;
        };
        if kinds_of(graph, rel).is_none()
            || !design.is_current(graph, &rel.from)
            || !design.is_current(graph, &rel.to)
        {
            continue;
        }
        let (Some(from), Some(target)) = (object_id(&rel.from), object_id(&rel.to)) else {
            continue;
        };
        groups
            .entry((kind, rule, target.clone()))
            .or_default()
            .insert(from.clone());
    }
    groups
        .into_iter()
        .filter(|(_, deciders)| deciders.len() >= 2)
        .filter_map(|((kind, rule, target), mut sources)| {
            // 共有の対象が消えても候補が立ったままにならないよう、source に含める。
            sources.insert(target.clone());
            Candidate::new(kind, rule, Shared::Object(target), sources)
        })
        .collect()
}

/// 規則3。 Q ごとの reasoning subgraph を作り、形が一致する Q が2つ以上あるものを出す。
///
/// 部分が重なる Q（同じ EVID を引く判断を持つ2つの Q）も、形が一致すれば数える。
/// 重なった部分は規則2が別に出す。
fn repeated_shapes(design: &Design, graph: &SemanticGraph) -> Vec<Candidate> {
    let mut by_shape: BTreeMap<Shape, Vec<BTreeSet<Id>>> = BTreeMap::new();
    for node in graph.nodes() {
        if Kind::of(node) != Some(Kind::Question) || !design.is_current(graph, &node.key) {
            continue;
        }
        let (shape, members) = reasoning_subgraph(design, graph, &node.key);
        if shape.edge_count() >= MIN_SHAPE_EDGES {
            by_shape.entry(shape).or_default().push(members);
        }
    }
    by_shape
        .into_iter()
        .filter(|(_, subgraphs)| subgraphs.len() >= 2)
        .filter_map(|(shape, subgraphs)| {
            let sources = subgraphs.into_iter().flatten().collect();
            Candidate::new(
                PatternKind::RepeatedShape,
                RULE_SHAPE,
                Shared::Shape(shape),
                sources,
            )
        })
        .collect()
}

/// `root` から [`BASES`] の `reasoning` の関係を辿った、形と node の ID。
/// 辿るのは、種類が語彙に合い、現在有効な node だけ。
fn reasoning_subgraph(
    design: &Design,
    graph: &SemanticGraph,
    root: &NodeKey,
) -> (Shape, BTreeSet<Id>) {
    let mut visited: BTreeSet<NodeKey> = BTreeSet::from([root.clone()]);
    let mut queue: VecDeque<NodeKey> = VecDeque::from([root.clone()]);
    let mut edges: BTreeMap<(Kind, &'static str, Kind), usize> = BTreeMap::new();
    while let Some(key) = queue.pop_front() {
        for rel in graph.outgoing(&key) {
            if !Basis::of(rel.name).is_some_and(|b| b.reasoning) {
                continue;
            }
            let Some((from, to)) = kinds_of(graph, rel) else {
                continue;
            };
            if !design.is_current(graph, &rel.to) {
                continue;
            }
            *edges.entry((from, rel.name, to)).or_default() += 1;
            if visited.insert(rel.to.clone()) {
                queue.push_back(rel.to.clone());
            }
        }
    }
    let mut nodes: BTreeMap<Kind, usize> = BTreeMap::new();
    for key in &visited {
        if let Some(kind) = graph.node(key).and_then(Kind::of) {
            *nodes.entry(kind).or_default() += 1;
        }
    }
    let members = visited.iter().filter_map(object_id).cloned().collect();
    (Shape { nodes, edges }, members)
}

/// 規則4。 現在有効な DEC の typed field の値を、空白を畳んで比べる。
fn repeated_conditions(design: &Design, graph: &SemanticGraph) -> Vec<Candidate> {
    let mut groups: BTreeMap<(&'static str, String), BTreeSet<Id>> = BTreeMap::new();
    for object in design.objects() {
        let Some(decision) = object.as_decision() else {
            continue;
        };
        if !design.is_current(graph, &NodeKey::Object(object.id().clone())) {
            continue;
        }
        for condition in CONDITIONS {
            for value in (condition.read)(&decision) {
                let text = squash_whitespace(&value);
                if !text.is_empty() {
                    groups
                        .entry((condition.field, text))
                        .or_default()
                        .insert(object.id().clone());
                }
            }
        }
    }
    groups
        .into_iter()
        .filter(|(_, sources)| sources.len() >= 2)
        .filter_map(|((field, text), sources)| {
            Candidate::new(
                PatternKind::RepeatedCondition,
                RULE_CONDITION,
                Shared::Condition { field, text },
                sources,
            )
        })
        .collect()
}

fn squash_whitespace(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests;
