//! `C(root, revision)`。 ある版で、root の判断に必要な semantic context を作る。
//!
//! 提案書 7.2.1 の「Context は一つの semantic projection」のうち、`C` の側。 比べる側は
//! [`super::compare`]。 入力は D00 の型つきの口（`Design`・graph）だけで、legacy の接頭辞や
//! meta の表の並びは引かない。 どの relation を辿るかは [`super::rules`] の表が決める。
//!
//! 作る順は3つ。
//!
//! 1. root を seed にする（[`resolve`]）。 ID はそのまま、path は引用している ID（`touched` と同じ拾い方）。
//!    引用は論理的な依存ではない（`mentions`）ので、seed が「引用から拾った入口」であることを残す
//! 2. 規則の表を、seed から辿る。 必須は予算で削らず、周辺は seed からの1歩だけ。 循環は
//!    訪問済みで止める
//! 3. 表の外で足すもの——仮説の欄から読む支持・反証、未解決の問いが塞ぐリリース対象、出どころ、
//!    宙に浮いた参照
//!
//! 予算（[`apply_budget`]）は必須の外の周辺だけを削る。 削ったものは数と内訳を残す
//! ——予算は、黙って消す権限ではない。

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use super::rules::{
    ARTIFACT_CLOSURE, BLOCKED_RELEASE, BLOCKS_PROFILES, CONTRADICTION_CLOSURE, Dir,
    EMPIRICAL_SUPPORT, Next, PROVENANCE, ROOT, RULES, Rule, Tier, When,
};
use crate::graph::{GraphIssue, NodeKey, NodeProvenance, SemanticGraph};
use crate::knowledge::design::{Design, DesignIssue, Hypothesis, Kind, relation as relation_spec};
use crate::knowledge::{Id, id_tokens};
use crate::repo::{RepoView, RevisionRef};

/// 未解決の問いの状態の語。 legacy の語彙（`check-profile` も同じ値を見る）。
const OPEN: &str = "open";

/// 仮説の欄の名前。 edge ではなく欄から読む支持と反証を、取得理由と欠けの報告に載せる。
const SUPPORTING: &str = "supporting_evidence";
const CONTRADICTING: &str = "contradicting_evidence";

/// ある版での、仮説の状態。 Context が版を越えて持つので、記録を借りずに値で持つ。
/// 読む口は D00 の [`Hypothesis`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HypothesisState {
    pub(crate) scope: Option<String>,
    pub(crate) assessment: Option<String>,
    /// 支持する調査。 ID の形に読めない値は落とす。 昇順で重複なし。
    pub(crate) supporting: Vec<Id>,
    /// 反証する調査。 ID の形に読めない値は落とす。 昇順で重複なし。
    pub(crate) contradicting: Vec<Id>,
}

impl HypothesisState {
    fn of(h: Hypothesis<'_>) -> Self {
        Self {
            scope: h.scope().map(|s| s.trim().to_owned()),
            assessment: h.assessment().map(str::to_owned),
            supporting: evidence_ids(h.supporting_evidence()),
            contradicting: evidence_ids(h.contradicting_evidence()),
        }
    }
}

fn evidence_ids(raw: Vec<String>) -> Vec<Id> {
    let mut out: Vec<Id> = raw.iter().filter_map(|s| s.parse().ok()).collect();
    out.sort();
    out.dedup();
    out
}

/// 1つの版を読んだもの。
pub(crate) struct World<'v> {
    pub(crate) view: &'v RepoView,
    pub(crate) design: Design,
    pub(crate) graph: SemanticGraph,
    audit: Vec<DesignIssue>,
    pub(crate) identity: Option<RevisionRef>,
}

impl<'v> World<'v> {
    /// 版の meta と FSL を1回だけ読む。 版から読むたびに読む回数が待ち時間になるので、
    /// 読むのは `Design::load` の1回だけにする。
    pub(crate) fn load(view: &'v RepoView) -> Self {
        let design = Design::load(view);
        let graph = design.graph();
        let audit = design.audit();
        Self {
            view,
            graph,
            audit,
            identity: design.revision().cloned(),
            design,
        }
    }

    /// 仮説のこの版での状態。 仮説でなければ `None`。
    pub(crate) fn hypothesis(&self, id: &Id) -> Option<HypothesisState> {
        self.design
            .object(id)
            .and_then(|o| o.as_hypothesis())
            .map(HypothesisState::of)
    }

    pub(crate) fn kind_of(&self, key: &NodeKey) -> Option<Kind> {
        self.graph.node(key).and_then(Kind::of)
    }

    pub(crate) fn is_open_question(&self, key: &NodeKey) -> bool {
        let NodeKey::Object(id) = key else {
            return false;
        };
        self.design
            .object(id)
            .and_then(|o| o.as_question())
            .is_some_and(|q| q.status() == Some(OPEN))
    }

    /// リポジトリの根からの相対パス（`/` 区切り）。 根の外はそのまま。
    fn relative(&self, path: &Path) -> String {
        let shown = path.strip_prefix(self.view.root()).unwrap_or(path);
        shown.display().to_string().replace('\\', "/")
    }

    fn origin(&self, provenance: &NodeProvenance) -> Option<Origin> {
        provenance.path.as_ref().map(|p| Origin {
            path: self.relative(p),
            line: provenance.line,
        })
    }
}

/// 読んだ地点。 指定された名前と、解決した版。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Address {
    /// `--at` / `--from` に渡された名前。 作業ツリーなら無い。
    pub(crate) given: Option<String>,
    /// 解決した版。 メモリ上の view は持たない。
    pub(crate) identity: Option<RevisionRef>,
}

/// 記録の出どころ。 リポジトリの根からの相対パスと、分かるときだけ行。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Origin {
    pub(crate) path: String,
    pub(crate) line: Option<usize>,
}

/// `--root` に渡されたもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootInput {
    Id(Id),
    Path(String),
}

impl RootInput {
    /// ID の形に読めれば ID、そうでなければ path。 ID は `/` も `.` も含まない。
    pub(crate) fn parse(raw: &str) -> Self {
        match raw.trim().parse::<Id>() {
            Ok(id) => Self::Id(id),
            Err(_) => Self::Path(raw.trim().to_owned()),
        }
    }

    pub(crate) fn text(&self) -> String {
        match self {
            Self::Id(id) => id.as_str().to_owned(),
            Self::Path(p) => p.clone(),
        }
    }
}

/// seed が入口になった理由。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum SeedVia {
    /// ID をそのまま指定された。
    Given,
    /// 指定された path が、この記録の正本（meta の記録、FSL の宣言）。
    Owner,
    /// 指定された path が、この ID を引いている。 引いている行。 **引用であって、満たしている
    /// 証明ではない**（`mentions`）。
    Citation(BTreeSet<usize>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Seed {
    pub(crate) id: Id,
    pub(crate) via: SeedVia,
}

/// path の root が、関係する契約を特定できなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnmappedReason {
    /// この版にファイルが無い。
    FileMissing,
    /// ID を1つも引いていない。
    NoCitations,
    /// ID を引いているが、どれもこの版の node として解決できない。
    NoneResolved,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RootState {
    Mapped(Vec<Seed>),
    /// 関係する設計契約を特定できていない。 関係する契約が無いという意味ではない。
    Unmapped(UnmappedReason),
    /// 指定された ID が、この版に無い。
    NotFound,
}

impl RootState {
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Mapped(_) => "mapped",
            Self::Unmapped(_) => "unmapped",
            Self::NotFound => "not-found",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RootReport {
    pub(crate) input: RootInput,
    pub(crate) state: RootState,
}

/// 項目が入った理由。 どの規則で、どの edge をたどって入ったか。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Reason {
    pub(crate) code: &'static str,
    pub(crate) label: &'static str,
    pub(crate) via: Option<Via>,
}

/// 辿ってきた edge。 `Out` は `focus` から項目へ、`In` は項目から `focus` へ向かう。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Via {
    pub(crate) focus: NodeKey,
    pub(crate) relation: &'static str,
    pub(crate) dir: Dir,
}

/// 記録の欄から読んだ、表示に要る事実。 持たない種類の欄は空。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) status: Option<String>,
    pub(crate) statement: Option<String>,
    pub(crate) selected: Option<String>,
    pub(crate) options: Vec<String>,
    /// 判断が自分で持つ制約（見出し、本文）。
    pub(crate) constraint: Option<(String, String)>,
    pub(crate) confidence: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) assessment: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Item {
    pub(crate) key: NodeKey,
    pub(crate) kind: Kind,
    pub(crate) title: Option<String>,
    pub(crate) origin: Option<Origin>,
    /// graph の内容 digest。 版をまたいで同じ記録かを比べる。
    pub(crate) fingerprint: u64,
    pub(crate) tier: Tier,
    pub(crate) reasons: BTreeSet<Reason>,
    pub(crate) facts: Facts,
    /// この記録を置き換えた後継（graph の `supersedes` の行き先が自分である記録）。
    pub(crate) superseded_by: Vec<Id>,
    /// 元の議論・実装 event への locator。 項目として数えない。
    pub(crate) provenance: Vec<String>,
}

/// edge の同一性。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EdgeKey {
    pub(crate) from: NodeKey,
    pub(crate) relation: &'static str,
    pub(crate) to: NodeKey,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EdgeInfo {
    /// 相手の一部だけを表す（`formalizes`）。 完全同値・完全被覆を意味しない。
    pub(crate) partial: bool,
    pub(crate) source: Option<Origin>,
    /// この edge で入った項目。 予算で項目が落ちたとき、edge も一緒に落とす。
    pub(crate) brought: NodeKey,
    pub(crate) code: &'static str,
}

/// 解けない参照。 「無い」と黙らず、欠けとして残す。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Broken {
    /// 記録が持つ edge の相手が、この版に node として無い。
    Dangling {
        from: NodeKey,
        relation: &'static str,
        to: NodeKey,
    },
    /// 関係の欄の値が ID の形に読めない。
    Unparsed {
        from: Id,
        field: &'static str,
        raw: String,
    },
    /// 仮説の欄が挙げた調査が、この版に無い。
    MissingEvidence {
        hypothesis: Id,
        field: &'static str,
        evidence: Id,
    },
    /// path の root が引いている ID が、この版に無い。
    UnresolvedCitation {
        path: String,
        line: usize,
        id: String,
    },
}

impl Broken {
    /// この欠けが属する項目。 項目が予算で落ちたら、欠けも一緒に落とす。
    fn anchors(&self) -> Vec<NodeKey> {
        match self {
            Self::Dangling { from, to, .. } => vec![from.clone(), to.clone()],
            Self::Unparsed { from, .. } => vec![NodeKey::Object(from.clone())],
            Self::MissingEvidence { hypothesis, .. } => vec![NodeKey::Object(hypothesis.clone())],
            Self::UnresolvedCitation { .. } => Vec::new(),
        }
    }
}

/// 予算で削った周辺。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Omitted {
    pub(crate) key: NodeKey,
    pub(crate) tier: Tier,
    pub(crate) codes: BTreeSet<&'static str>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BudgetOutcome {
    pub(crate) limit: usize,
    /// 必須 closure の項目数。 予算では削らない。
    pub(crate) required: usize,
    pub(crate) kept: usize,
    pub(crate) omitted: Vec<Omitted>,
}

impl BudgetOutcome {
    /// 必須 closure だけで予算を超えた。 削らず、不完全として扱う。
    pub(crate) fn exceeded(&self) -> bool {
        self.required > self.limit
    }
}

/// ある版の Context。
#[derive(Debug, Clone, Default)]
pub(crate) struct Context {
    pub(crate) roots: Vec<RootReport>,
    pub(crate) items: BTreeMap<NodeKey, Item>,
    pub(crate) edges: BTreeMap<EdgeKey, EdgeInfo>,
    pub(crate) broken: BTreeSet<Broken>,
    /// 項目になった仮説の状態。 支持と反証を同じ面で出すために持つ。
    pub(crate) hypotheses: BTreeMap<Id, HypothesisState>,
    pub(crate) budget: Option<BudgetOutcome>,
}

impl Context {
    pub(crate) fn required(&self) -> impl Iterator<Item = &Item> {
        self.items.values().filter(|i| i.tier == Tier::Required)
    }

    pub(crate) fn of_kind(&self, kind: Kind) -> impl Iterator<Item = &Item> {
        self.items.values().filter(move |i| i.kind == kind)
    }
}

/// 版に出てくる node を、文字列にする。
pub(crate) fn key_text(key: &NodeKey) -> String {
    match key {
        NodeKey::Object(id) => id.as_str().to_owned(),
        NodeKey::Locator { kind, value } => format!("{kind}:{value}"),
    }
}

/// `C(roots, world)`。 予算は掛けない。
pub(crate) fn compile(world: &World<'_>, inputs: &[RootInput]) -> Context {
    let mut roots = Vec::new();
    let mut broken = BTreeSet::new();
    let mut seeds: Vec<Seed> = Vec::new();
    for input in inputs {
        let (state, lost) = resolve(world, input);
        broken.extend(lost);
        if let RootState::Mapped(found) = &state {
            seeds.extend(found.iter().cloned());
        }
        roots.push(RootReport {
            input: input.clone(),
            state,
        });
    }

    let mut walk = Walk::new(world);
    walk.start(&seeds);
    walk.run();
    walk.add_hypothesis_evidence();
    walk.add_blocked_profiles();
    let (items, edges, mut walk_broken) = walk.finish();
    broken.append(&mut walk_broken);

    let mut ctx = Context {
        roots,
        items,
        edges,
        broken,
        hypotheses: BTreeMap::new(),
        budget: None,
    };
    for item in ctx.items.values() {
        if let NodeKey::Object(id) = &item.key
            && let Some(h) = world.hypothesis(id)
        {
            ctx.hypotheses.insert(id.clone(), h);
        }
    }
    attach_broken(world, &mut ctx);
    ctx
}

/// 予算を掛ける。 削るのは必須の外の周辺だけ。 必須だけで超えるなら、削らずに超えたと残す。
pub(crate) fn apply_budget(full: &Context, limit: usize) -> Context {
    let required = full.required().count();
    let room = limit.saturating_sub(required);
    let mut peripheral: Vec<&Item> = full
        .items
        .values()
        .filter(|i| i.tier != Tier::Required)
        .collect();
    // 重さ（数が小さいほど重要）、次に鍵の順。 並びは決定的。
    peripheral.sort_by(|a, b| a.tier.cmp(&b.tier).then_with(|| a.key.cmp(&b.key)));
    let (keep, drop) = peripheral.split_at(room.min(peripheral.len()));
    let dropped: BTreeSet<NodeKey> = drop.iter().map(|i| i.key.clone()).collect();
    let omitted = drop
        .iter()
        .map(|i| Omitted {
            key: i.key.clone(),
            tier: i.tier,
            codes: i.reasons.iter().map(|r| r.code).collect(),
        })
        .collect();

    let items: BTreeMap<NodeKey, Item> = full
        .items
        .iter()
        .filter(|(k, _)| !dropped.contains(*k))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let edges = full
        .edges
        .iter()
        .filter(|(_, e)| items.contains_key(&e.brought))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let broken = full
        .broken
        .iter()
        .filter(|b| {
            let anchors = b.anchors();
            anchors.is_empty() || anchors.iter().any(|a| items.contains_key(a))
        })
        .cloned()
        .collect();
    let hypotheses = full
        .hypotheses
        .iter()
        .filter(|(id, _)| items.contains_key(&NodeKey::Object((*id).clone())))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    Context {
        roots: full.roots.clone(),
        items,
        edges,
        broken,
        hypotheses,
        budget: Some(BudgetOutcome {
            limit,
            required,
            kept: keep.len(),
            omitted,
        }),
    }
}

// ## seed

fn resolve(world: &World<'_>, input: &RootInput) -> (RootState, BTreeSet<Broken>) {
    match input {
        RootInput::Id(id) => {
            let key = NodeKey::Object(id.clone());
            if world.graph.node(&key).is_some() {
                let seed = Seed {
                    id: id.clone(),
                    via: SeedVia::Given,
                };
                (RootState::Mapped(vec![seed]), BTreeSet::new())
            } else {
                (RootState::NotFound, BTreeSet::new())
            }
        }
        RootInput::Path(raw) => resolve_path(world, &normalize(world, raw)),
    }
}

/// 区切りを `/` にし、リポジトリの根の下の絶対パスは相対にする。
fn normalize(world: &World<'_>, raw: &str) -> String {
    let unified = raw.replace('\\', "/");
    let relative = Path::new(&unified)
        .strip_prefix(world.view.root())
        .map_or(unified.clone(), |p| {
            p.display().to_string().replace('\\', "/")
        });
    relative.trim_start_matches("./").to_owned()
}

fn resolve_path(world: &World<'_>, rel: &str) -> (RootState, BTreeSet<Broken>) {
    let text = match world.view.read(rel) {
        Ok(Some(text)) => text,
        // 読めないファイル（UTF-8 ではない、など）も、この版では読めないものとして同じに扱う。
        Ok(None) | Err(_) => {
            return (
                RootState::Unmapped(UnmappedReason::FileMissing),
                BTreeSet::new(),
            );
        }
    };

    // そのファイル自身が正本のもの（meta の記録、FSL の宣言）。 本文が引いている ID より
    // 優先する——記録の本文は、ほかの記録を引いているだけで、依っているわけではない。
    let owners: Vec<Id> = world
        .design
        .objects()
        .filter(|o| world.relative(o.origin().path) == rel)
        .map(|o| o.id().clone())
        .collect();
    if !owners.is_empty() {
        let mut seeds: Vec<Seed> = owners
            .into_iter()
            .map(|id| Seed {
                id,
                via: SeedVia::Owner,
            })
            .collect();
        seeds.sort_by(|a, b| a.id.cmp(&b.id));
        seeds.dedup_by(|a, b| a.id == b.id);
        return (RootState::Mapped(seeds), BTreeSet::new());
    }

    // 引用から拾う。 `touched` と同じ ID の拾い方。
    let mut cited: BTreeMap<Id, BTreeSet<usize>> = BTreeMap::new();
    let mut lost = BTreeSet::new();
    let mut any_token = false;
    for (n, line) in text.lines().enumerate() {
        for token in id_tokens(line) {
            any_token = true;
            let resolved = token
                .parse::<Id>()
                .ok()
                .filter(|id| world.graph.node(&NodeKey::Object(id.clone())).is_some());
            match resolved {
                Some(id) => {
                    cited.entry(id).or_default().insert(n + 1);
                }
                None => {
                    lost.insert(Broken::UnresolvedCitation {
                        path: rel.to_owned(),
                        line: n + 1,
                        id: token,
                    });
                }
            }
        }
    }
    if cited.is_empty() {
        let reason = if any_token {
            UnmappedReason::NoneResolved
        } else {
            UnmappedReason::NoCitations
        };
        return (RootState::Unmapped(reason), lost);
    }
    let seeds = cited
        .into_iter()
        .map(|(id, lines)| Seed {
            id,
            via: SeedVia::Citation(lines),
        })
        .collect();
    (RootState::Mapped(seeds), lost)
}

// ## 規則の表を辿る

/// 辿り方。 seed と後継は全部の規則、連なりは同じ規則だけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Mode {
    Full,
    Chain(&'static str),
}

struct Walk<'w, 'v> {
    world: &'w World<'v>,
    items: BTreeMap<NodeKey, Item>,
    edges: BTreeMap<EdgeKey, EdgeInfo>,
    broken: BTreeSet<Broken>,
    visited: BTreeSet<(NodeKey, Mode)>,
    queue: VecDeque<(NodeKey, Mode)>,
}

impl<'w, 'v> Walk<'w, 'v> {
    fn new(world: &'w World<'v>) -> Self {
        Self {
            world,
            items: BTreeMap::new(),
            edges: BTreeMap::new(),
            broken: BTreeSet::new(),
            visited: BTreeSet::new(),
            queue: VecDeque::new(),
        }
    }

    fn start(&mut self, seeds: &[Seed]) {
        for seed in seeds {
            let key = NodeKey::Object(seed.id.clone());
            let reason = Reason {
                code: ROOT.0,
                label: ROOT.1,
                via: None,
            };
            if self.add(&key, Tier::Required, reason) {
                self.queue.push_back((key, Mode::Full));
            }
        }
    }

    fn run(&mut self) {
        while let Some((key, mode)) = self.queue.pop_front() {
            if !self.visited.insert((key.clone(), mode)) {
                continue;
            }
            // 全部の規則で辿った記録は、同じ記録の連なりを辿り直さない。
            if matches!(mode, Mode::Chain(_)) && self.visited.contains(&(key.clone(), Mode::Full)) {
                continue;
            }
            let Some(kind) = self.world.kind_of(&key) else {
                continue;
            };
            for rule in RULES.iter().filter(|r| applies(r, kind, mode)) {
                self.apply(&key, rule);
            }
        }
    }

    fn apply(&mut self, focus: &NodeKey, rule: &Rule) {
        let graph = &self.world.graph;
        let relations = match rule.dir {
            Dir::Out => graph.outgoing(focus),
            Dir::In => graph.incoming(focus),
        };
        for rel in relations.iter().filter(|r| r.name == rule.relation) {
            let other = match rule.dir {
                Dir::Out => &rel.to,
                Dir::In => &rel.from,
            };
            // 相手が無い edge は、`attach_broken` が欠けとして出す。
            let Some(kind) = self.world.kind_of(other) else {
                continue;
            };
            if other == focus || !(rule.to.is_empty() || rule.to.contains(&kind)) {
                continue;
            }
            let open = self.world.is_open_question(other);
            match rule.when {
                When::Always => {}
                When::OpenQuestion if !open => continue,
                When::NotOpenQuestion if open => continue,
                When::OpenQuestion | When::NotOpenQuestion => {}
            }
            let reason = Reason {
                code: rule.code,
                label: rule.label,
                via: Some(Via {
                    focus: focus.clone(),
                    relation: rule.relation,
                    dir: rule.dir,
                }),
            };
            self.add(other, rule.tier, reason);
            self.add_edge(
                rel.from.clone(),
                rel.name,
                rel.to.clone(),
                &rel.source,
                other,
                rule.code,
            );
            // すでに項目でも辿る。 訪問済みの組は `run` が止める。
            match rule.next {
                Next::Stop => {}
                Next::SameRule => self
                    .queue
                    .push_back((other.clone(), Mode::Chain(rule.code))),
                Next::Full => self.queue.push_back((other.clone(), Mode::Full)),
            }
        }
    }

    fn add_edge(
        &mut self,
        from: NodeKey,
        relation: &'static str,
        to: NodeKey,
        source: &NodeProvenance,
        brought: &NodeKey,
        code: &'static str,
    ) {
        let info = EdgeInfo {
            partial: relation_spec(relation).is_some_and(|r| r.partial),
            source: self.world.origin(source),
            brought: brought.clone(),
            code,
        };
        self.edges
            .entry(EdgeKey { from, relation, to })
            .or_insert(info);
    }

    /// 項目を足す。 すでにあれば理由を足し、重さは重いほう（必須が先）に寄せる。
    /// 新しく入ったときだけ `true`。
    fn add(&mut self, key: &NodeKey, tier: Tier, reason: Reason) -> bool {
        if let Some(item) = self.items.get_mut(key) {
            item.tier = item.tier.min(tier);
            item.reasons.insert(reason);
            return false;
        }
        let Some(node) = self.world.graph.node(key) else {
            return false;
        };
        let Some(kind) = Kind::of(node) else {
            return false;
        };
        let id = match key {
            NodeKey::Object(id) => Some(id),
            NodeKey::Locator { .. } => None,
        };
        let item = Item {
            key: key.clone(),
            kind,
            title: id.and_then(|id| display_title(self.world, id)),
            origin: self.world.origin(&node.provenance),
            fingerprint: node.fingerprint,
            tier,
            reasons: BTreeSet::from([reason]),
            facts: id.map(|id| facts_of(self.world, id)).unwrap_or_default(),
            superseded_by: self.world.design.successors(&self.world.graph, key),
            provenance: Vec::new(),
        };
        self.items.insert(key.clone(), item);
        true
    }

    /// 項目になった仮説の、支持と反証。 支持と反証は同じ面に出す（提案書 14.3）。
    fn add_hypothesis_evidence(&mut self) {
        let hypotheses: Vec<(Id, Tier)> = self
            .items
            .values()
            .filter(|i| i.kind == Kind::Hypothesis)
            .filter_map(|i| match &i.key {
                NodeKey::Object(id) => Some((id.clone(), i.tier)),
                NodeKey::Locator { .. } => None,
            })
            .collect();
        for (id, tier) in hypotheses {
            let Some(h) = self.world.hypothesis(&id) else {
                continue;
            };
            let focus = NodeKey::Object(id.clone());
            // 仮説が必須なら、その証拠も必須。 周辺の仮説の証拠は周辺。
            let tier = if tier == Tier::Required {
                Tier::Required
            } else {
                Tier::Peripheral(2)
            };
            let groups = [
                (CONTRADICTING, &h.contradicting, CONTRADICTION_CLOSURE),
                (SUPPORTING, &h.supporting, EMPIRICAL_SUPPORT),
            ];
            for (field, evidence, (code, label)) in groups {
                for ev in evidence {
                    let other = NodeKey::Object(ev.clone());
                    if self.world.kind_of(&other) != Some(Kind::Evidence) {
                        self.broken.insert(Broken::MissingEvidence {
                            hypothesis: id.clone(),
                            field,
                            evidence: ev.clone(),
                        });
                        continue;
                    }
                    let reason = Reason {
                        code,
                        label,
                        via: Some(Via {
                            focus: focus.clone(),
                            relation: field,
                            dir: Dir::Out,
                        }),
                    };
                    self.add(&other, tier, reason);
                    let info = EdgeInfo {
                        partial: false,
                        source: self.items[&focus].origin.clone(),
                        brought: other.clone(),
                        code,
                    };
                    self.edges
                        .entry(EdgeKey {
                            from: focus.clone(),
                            relation: field,
                            to: other,
                        })
                        .or_insert(info);
                }
            }
        }
    }

    /// 未解決の問いが塞いでいるリリース対象。 問いが解決していれば何も塞がない。
    fn add_blocked_profiles(&mut self) {
        let open: Vec<NodeKey> = self
            .items
            .values()
            .filter(|i| i.kind == Kind::Question && self.world.is_open_question(&i.key))
            .map(|i| i.key.clone())
            .collect();
        for q in open {
            let blocks: Vec<_> = self
                .world
                .graph
                .outgoing(&q)
                .iter()
                .filter(|r| r.name == BLOCKS_PROFILES)
                .cloned()
                .collect();
            for rel in blocks {
                if self.world.kind_of(&rel.to) != Some(Kind::ReleaseProfile) {
                    continue;
                }
                let reason = Reason {
                    code: BLOCKED_RELEASE.0,
                    label: BLOCKED_RELEASE.1,
                    via: Some(Via {
                        focus: q.clone(),
                        relation: BLOCKS_PROFILES,
                        dir: Dir::Out,
                    }),
                };
                self.add(&rel.to, Tier::Required, reason);
                self.add_edge(
                    rel.from.clone(),
                    BLOCKS_PROFILES,
                    rel.to.clone(),
                    &rel.source,
                    &rel.to,
                    BLOCKED_RELEASE.0,
                );
            }
        }
    }

    fn finish(
        mut self,
    ) -> (
        BTreeMap<NodeKey, Item>,
        BTreeMap<EdgeKey, EdgeInfo>,
        BTreeSet<Broken>,
    ) {
        // 出どころ（provenance の locator）。 項目として数えない。
        for item in self.items.values_mut() {
            let mut locators: Vec<String> = self
                .world
                .graph
                .outgoing(&item.key)
                .iter()
                .filter(|r| r.name == PROVENANCE)
                .filter_map(|r| match &r.to {
                    NodeKey::Locator { value, .. } => Some(value.clone()),
                    NodeKey::Object(_) => None,
                })
                .collect();
            locators.sort();
            locators.dedup();
            item.provenance = locators;
        }
        (self.items, self.edges, self.broken)
    }
}

/// この規則を、この種類の記録に、この辿り方で当てるか。
fn applies(rule: &Rule, kind: Kind, mode: Mode) -> bool {
    if !(rule.from.is_empty() || rule.from.contains(&kind)) {
        return false;
    }
    match mode {
        Mode::Full => true,
        // 連なりで入った記録（依って立つ要件）は、連なりの規則と、読むべき artifact だけを辿る。
        // 縛る要件を形式にした contract や、実現する資源は、判断そのものの文脈に要る。
        Mode::Chain(code) => {
            (rule.code == code && rule.next == Next::SameRule) || rule.code == ARTIFACT_CLOSURE
        }
    }
}

/// graph の宙に浮いた edge と、読めなかった参照のうち、項目に属するもの。
fn attach_broken(world: &World<'_>, ctx: &mut Context) {
    for issue in &world.audit {
        match issue {
            DesignIssue::Graph(GraphIssue::Dangling { from, relation, to })
                if ctx.items.contains_key(from) || ctx.items.contains_key(to) =>
            {
                ctx.broken.insert(Broken::Dangling {
                    from: from.clone(),
                    relation,
                    to: to.clone(),
                });
            }
            DesignIssue::UnparsedReference { from, field, raw }
                if ctx.items.contains_key(&NodeKey::Object(from.clone())) =>
            {
                ctx.broken.insert(Broken::Unparsed {
                    from: from.clone(),
                    field,
                    raw: raw.clone(),
                });
            }
            _ => {}
        }
    }
}

// ## 記録の欄

/// 表題。 持たない種類（仮説、目標の形の要件）は、本文の最初の行。
fn display_title(world: &World<'_>, id: &Id) -> Option<String> {
    let typed = world
        .design
        .object(id)
        .and_then(|o| o.title().map(str::to_owned));
    typed.or_else(|| {
        let object = world.design.object(id)?;
        let statement = object
            .as_hypothesis()
            .and_then(|h| h.statement())
            .or_else(|| object.as_requirement().and_then(|r| r.statement()))?;
        statement.trim().lines().next().map(|l| l.trim().to_owned())
    })
}

fn facts_of(world: &World<'_>, id: &Id) -> Facts {
    let mut facts = Facts::default();
    if let Some(object) = world.design.object(id) {
        if let Some(d) = object.as_decision() {
            facts.status = d.status().map(str::to_owned);
            facts.selected = d.selected().map(|s| s.trim().to_owned());
            facts.options = d.options();
            facts.constraint = d
                .constraint()
                .map(|(label, text)| (label.to_owned(), text.to_owned()));
        } else if let Some(q) = object.as_question() {
            facts.status = q.status().map(str::to_owned);
        } else if let Some(r) = object.as_requirement() {
            facts.statement = r.statement().map(|s| s.trim().to_owned());
            facts.confidence = r.confidence().map(str::to_owned);
        } else if let Some(p) = object.as_profile() {
            facts.status = p.status().map(str::to_owned);
        } else if let Some(h) = object.as_hypothesis() {
            facts.statement = h.statement().map(|s| s.trim().to_owned());
            facts.scope = h.scope().map(|s| s.trim().to_owned());
            facts.assessment = h.assessment().map(str::to_owned);
        }
    }
    facts
}
