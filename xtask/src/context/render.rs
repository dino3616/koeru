//! Context の出力。 人向けの Markdown と、機械向けの JSON。
//!
//! どちらも同じ [`View`] を読む。 項目の集合も、edge も、取得理由も同じで、違うのは形だけ。
//! 機械で決められない項目（今回決めること）は、捏造せず「機械では出せない」と理由を書く。
//!
//! JSON の形は D01 が持つ（`diagnostic` の封筒 `{ command, ok, diagnostics }` に `context` を
//! 足した形）。 `context.version` が形の版で、形を変えるときに上げる。

use std::fmt::Write as _;

use serde_json::{Value, json};

use super::compare::{CLASSES, Change, Changes};
use super::compile::{
    Address, Broken, Context, EdgeInfo, EdgeKey, Item, Origin, RootInput, RootReport, RootState,
    SeedVia, UnmappedReason, key_text,
};
use super::rules::{
    self, ARTIFACT_CLOSURE, BLOCKS_PROFILES, CONSIDERED_EVIDENCE, Dir, PRIOR_DECISION, Tier,
};
use crate::graph::{ChangeKind, NodeKey};
use crate::knowledge::Id;
use crate::knowledge::design::Kind;

/// JSON の形の版。
pub(crate) const FORMAT_VERSION: u32 = 1;

/// 不完全として終わる理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Incomplete {
    /// 指定した ID が、読んだどの版にも無い。
    RootNotFound { root: String },
    /// 指定した path が、読んだどの版にも無い。
    FileMissing { root: String },
    /// 必須 closure だけで予算を超えた。 削らない。
    RequiredOverBudget { required: usize, limit: usize },
}

impl Incomplete {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::RootNotFound { .. } => "context.root-not-found",
            Self::FileMissing { .. } => "context.file-missing",
            Self::RequiredOverBudget { .. } => "context.required-over-budget",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::RootNotFound { root } => {
                format!("`{root}` が、読んだどの版にも無い。 root を直す")
            }
            Self::FileMissing { root } => {
                format!("`{root}` というファイルが、読んだどの版にも無い。 root を直す")
            }
            Self::RequiredOverBudget { required, limit } => format!(
                "必須の closure が {required} 項目で、予算 {limit} を超える。 必須は削らない。 root を狭めるか、分けて呼ぶ"
            ),
        }
    }
}

/// 出力に渡すもの。
pub(crate) struct View<'a> {
    pub(crate) at: &'a Address,
    pub(crate) from: Option<&'a Address>,
    /// 予算を掛けた、読む地点の Context。
    pub(crate) now: &'a Context,
    /// 比較元の Context。 予算は掛けない。
    pub(crate) before: Option<&'a Context>,
    pub(crate) changes: Option<&'a Changes>,
    pub(crate) incomplete: &'a [Incomplete],
}

// ## 共通

fn kind_label(kind: Kind) -> &'static str {
    match kind {
        Kind::Question => "問い",
        Kind::Hypothesis => "仮説",
        Kind::Evidence => "調査",
        Kind::Decision => "判断",
        Kind::Requirement => "要件",
        Kind::FormalContract => "形式 contract",
        Kind::Probe => "Probe",
        Kind::Workload => "workload",
        Kind::Source => "source",
        Kind::Story => "story",
        Kind::Dependency => "部品",
        Kind::Artifact => "artifact",
        Kind::Issue => "Issue",
        Kind::PullRequest => "PR",
        Kind::Commit => "commit",
        Kind::Receipt => "Receipt",
        Kind::UnclassifiedTarget => "目標値（未分類）",
        Kind::ReleaseProfile => "リリース対象",
    }
}

fn tier_label(tier: Tier) -> &'static str {
    match tier {
        Tier::Required => "必須",
        Tier::Peripheral(_) => "周辺",
    }
}

fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn place(origin: Option<&Origin>) -> String {
    match origin {
        Some(Origin {
            path,
            line: Some(line),
        }) => format!("`{path}:{line}`"),
        Some(Origin { path, line: None }) => format!("`{path}`"),
        None => "（出どころ不明）".to_owned(),
    }
}

fn address_text(address: &Address) -> String {
    use crate::repo::RevisionRef;
    let short = |sha: &str| sha.chars().take(12).collect::<String>();
    match (&address.given, &address.identity) {
        (None, Some(RevisionRef::WorkingTree { head, dirty })) => format!(
            "作業ツリー（HEAD `{}`{}）",
            short(head),
            if *dirty {
                "、未コミットの変更あり"
            } else {
                ""
            }
        ),
        (None, _) => "作業ツリー".to_owned(),
        (Some(rev), Some(RevisionRef::Revision { sha })) => format!("`{rev}`（`{}`）", short(sha)),
        (Some(rev), _) => format!("`{rev}`"),
    }
}

fn address_json(address: &Address) -> Value {
    use crate::repo::RevisionRef;
    match (&address.given, &address.identity) {
        (None, Some(RevisionRef::WorkingTree { head, dirty })) => {
            json!({ "kind": "working-tree", "head": head, "dirty": dirty })
        }
        (None, _) => json!({ "kind": "working-tree", "head": null, "dirty": null }),
        (Some(rev), Some(RevisionRef::Revision { sha })) => {
            json!({ "kind": "revision", "rev": rev, "sha": sha })
        }
        (Some(rev), _) => json!({ "kind": "revision", "rev": rev, "sha": null }),
    }
}

fn ids_text(ids: &[Id]) -> String {
    ids.iter()
        .map(|i| format!("`{}`", i.as_str()))
        .collect::<Vec<_>>()
        .join("、")
}

fn item_line(item: &Item) -> String {
    let title = item.title.as_deref().map(one_line).unwrap_or_default();
    format!(
        "`{}`（{}）{}",
        key_text(&item.key),
        kind_label(item.kind),
        if title.is_empty() {
            String::new()
        } else {
            format!(" {title}")
        }
    )
}

fn reason_text(item: &Item) -> String {
    item.reasons
        .iter()
        .map(|r| match &r.via {
            None => r.label.to_owned(),
            Some(via) => {
                let arrow = match via.dir {
                    Dir::Out => format!("`{}` →{}→ 自分", key_text(&via.focus), via.relation),
                    Dir::In => format!("自分 →{}→ `{}`", via.relation, key_text(&via.focus)),
                };
                format!("{}（{arrow}）", r.label)
            }
        })
        .collect::<Vec<_>>()
        .join("／")
}

/// 項目が持つ取得理由のうち、この規則の名前で入ったもの。
fn by_code<'a>(ctx: &'a Context, code: &'a str) -> impl Iterator<Item = &'a Item> {
    ctx.items
        .values()
        .filter(move |i| i.reasons.iter().any(|r| r.code == code))
}

fn is_root_item(item: &Item) -> bool {
    item.reasons.iter().any(|r| r.code == rules::ROOT.0)
}

/// 機械では出せない項目。 カード番号、名前、理由。
const NOT_MACHINE: &[(u8, &str, &str)] = &[(
    1,
    "今回決めること",
    "今回何を決めるかは、Issue / PR の文脈を読む人が決める。 graph からは出せない",
)];

// ## Markdown

pub(crate) fn markdown(view: &View<'_>) -> String {
    let mut out = String::new();
    header(&mut out, view);
    cards(&mut out, view);
    details(&mut out, view);
    if let (Some(before), Some(changes)) = (view.before, view.changes) {
        changes_section(&mut out, view, before, changes);
    }
    out
}

fn header(out: &mut String, view: &View<'_>) {
    let _ = writeln!(out, "# Context\n");
    for incomplete in view.incomplete {
        let _ = writeln!(out, "> **不完全**: {}\n", incomplete.message());
    }
    let _ = writeln!(out, "- 読む地点（`--at`）: {}", address_text(view.at));
    if let Some(from) = view.from {
        let _ = writeln!(out, "- 比較元（`--from`）: {}", address_text(from));
    }
    let roots: Vec<String> = view.now.roots.iter().map(root_text).collect();
    let _ = writeln!(out, "- root: {}", roots.join("、"));
    if let Some(b) = &view.now.budget {
        let _ = writeln!(
            out,
            "- 予算: {} 項目（必須 {}、周辺 {}、削った周辺 {}）",
            b.limit,
            b.required,
            b.kept,
            b.omitted.len()
        );
    }
    let _ = writeln!(out);
}

fn root_text(root: &RootReport) -> String {
    let input = root.input.text();
    match (&root.input, &root.state) {
        (RootInput::Id(_), RootState::Mapped(_)) => format!("`{input}`"),
        (RootInput::Path(_), RootState::Mapped(seeds)) => {
            let owner = seeds.iter().any(|s| s.via == SeedVia::Owner);
            if owner {
                format!("`{input}`（ファイルが正本の記録 {} 件）", seeds.len())
            } else {
                format!(
                    "`{input}`（引用している ID {} 件。引用から拾った入口であり、満たしている証明ではない）",
                    seeds.len()
                )
            }
        }
        (_, RootState::Unmapped(_)) => format!("`{input}`（UNMAPPED）"),
        (_, RootState::NotFound) => format!("`{input}`（見つからない）"),
    }
}

fn cards(out: &mut String, view: &View<'_>) {
    let now = view.now;
    let _ = writeln!(out, "## 要約\n");

    let _ = writeln!(out, "### 1. 今回決めること\n");
    let (_, _, reason) = NOT_MACHINE[0];
    let _ = writeln!(out, "機械では出せない。 {reason}。\n");
    let open_roots: Vec<&Item> = now
        .items
        .values()
        .filter(|i| is_root_item(i) && i.kind == Kind::Question && is_open(i))
        .collect();
    if !open_roots.is_empty() {
        let _ = writeln!(
            out,
            "root にした未解決の問い（候補であって、決めることそのものではない）:\n"
        );
        for q in open_roots {
            let _ = writeln!(out, "- {}", item_line(q));
        }
        let _ = writeln!(out);
    }

    let _ = writeln!(out, "### 2. 現在守る制約\n");
    let requirements: Vec<&Item> = now
        .required()
        .filter(|i| i.kind == Kind::Requirement)
        .collect();
    let constraints: Vec<(&Item, &(String, String))> = now
        .required()
        .filter(|i| i.kind == Kind::Decision)
        .filter_map(|i| i.facts.constraint.as_ref().map(|c| (i, c)))
        .collect();
    if requirements.is_empty() && constraints.is_empty() {
        let _ = writeln!(
            out,
            "機械では特定できなかった。 root に結びつく要件・制約が graph に無い。"
        );
    }
    for r in &requirements {
        let _ = writeln!(out, "- {}", item_line(r));
    }
    for (decision, (label, _)) in &constraints {
        let _ = writeln!(
            out,
            "- `{}` が持つ制約: {}",
            key_text(&decision.key),
            one_line(label)
        );
    }
    let partial: Vec<&EdgeKey> = now
        .edges
        .iter()
        .filter(|(_, e)| e.partial)
        .map(|(k, _)| k)
        .collect();
    if !partial.is_empty() {
        let _ = writeln!(
            out,
            "\n形式 contract は要件の一部だけを形式にしている。 完全な被覆ではない: {}",
            partial
                .iter()
                .map(|k| format!("`{}` → `{}`", key_text(&k.from), key_text(&k.to)))
                .collect::<Vec<_>>()
                .join("、")
        );
    }
    // legacy の meta は、判断から要件への結びつきを `affects` でしか持たない。 論理的含意では
    // ないので制約には数えず、参考として別に出す。
    let affected: Vec<String> = now
        .of_kind(Kind::Requirement)
        .filter(|i| i.tier != Tier::Required)
        .map(|i| format!("`{}`", key_text(&i.key)))
        .collect();
    if !affected.is_empty() {
        let _ = writeln!(
            out,
            "\n参考（周辺）: `affects` で結ばれた要件 {}。 変更時に確かめる対象で、現在守る制約とは限らない",
            affected.join("、")
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### 3. 現在案を選んだ理由\n");
    let decisions: Vec<&Item> = now
        .required()
        .filter(|i| i.kind == Kind::Decision)
        .collect();
    if decisions.is_empty() {
        let _ = writeln!(out, "root に結びつく判断が graph に無い。\n");
    }
    for d in &decisions {
        let status = d.facts.status.as_deref().unwrap_or("状態不明");
        let _ = writeln!(out, "- {}（{status}）", item_line(d));
        if !d.superseded_by.is_empty() {
            let _ = writeln!(
                out,
                "  - 後継に置き換えられている。 現在有効なのは {}",
                ids_text(&d.superseded_by)
            );
        }
        let considered: Vec<String> = by_code(now, CONSIDERED_EVIDENCE)
            .filter(|e| {
                e.reasons.iter().any(|r| {
                    r.code == CONSIDERED_EVIDENCE
                        && r.via.as_ref().is_some_and(|v| v.focus == d.key)
                })
            })
            .map(|e| format!("`{}`", key_text(&e.key)))
            .collect();
        let _ = writeln!(
            out,
            "  - 理由の本文は {} の `rationale`。 参照した調査: {}",
            place(d.origin.as_ref()),
            if considered.is_empty() {
                "（この Context に無い）".to_owned()
            } else {
                considered.join("、")
            }
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### 4. 別案と、以前採らなかった理由\n");
    let mut any = false;
    for d in &decisions {
        let selected = d.facts.selected.as_deref().map(one_line);
        let options: Vec<String> = d.facts.options.iter().map(|o| one_line(o)).collect();
        if options.is_empty() {
            continue;
        }
        any = true;
        let matched = selected.as_ref().is_some_and(|s| options.contains(s));
        let _ = writeln!(
            out,
            "- `{}` の案{}:",
            key_text(&d.key),
            if matched {
                "（選んだ案を除く）"
            } else {
                "（選んだ案と一致する記述が無いので、全部出す）"
            }
        );
        for option in options
            .iter()
            .filter(|o| !matched || selected.as_ref() != Some(*o))
        {
            let _ = writeln!(out, "  - {option}");
        }
    }
    let prior: Vec<&Item> = by_code(now, PRIOR_DECISION).collect();
    if !prior.is_empty() {
        any = true;
        let _ = writeln!(out, "- 置き換えた以前の判断（周辺）:");
        for p in prior {
            let _ = writeln!(out, "  - {}", item_line(p));
        }
    }
    if any {
        let _ = writeln!(
            out,
            "\n採らなかった理由は、各判断の `rationale` を読む。 機械では案ごとに分けられない。\n"
        );
    } else {
        let _ = writeln!(out, "この Context の判断は、別案を記録していない。\n");
    }

    let _ = writeln!(out, "### 5. 新しく弱まった前提\n");
    match view.changes {
        Some(changes) => {
            let contradicted: Vec<&Change> = changes.of_class("newly-contradicted").collect();
            let scoped: Vec<&Change> = changes.of_class("scope-changed").collect();
            if contradicted.is_empty() && scoped.is_empty() {
                let _ = writeln!(
                    out,
                    "比較元からの間に、この Context の仮説へ新しい反証も範囲の変更も付いていない。\n"
                );
            }
            for c in contradicted {
                if let Change::NewlyContradicted {
                    hypothesis,
                    evidence,
                } = c
                {
                    let _ = writeln!(
                        out,
                        "- `{}` に新しい反証: {}",
                        hypothesis.as_str(),
                        ids_text(evidence)
                    );
                }
            }
            for c in scoped {
                if let Change::ScopeChanged { hypothesis } = c {
                    let _ = writeln!(
                        out,
                        "- `{}` の範囲（`scope`）が変わった",
                        hypothesis.as_str()
                    );
                }
            }
            let _ = writeln!(out);
        }
        None => {
            let _ = writeln!(
                out,
                "比較元（`--from`）が無いので、「新しく」は判定できない。 現在、反証が付いている仮説:\n"
            );
            let mut any = false;
            for (id, h) in &now.hypotheses {
                if !h.contradicting.is_empty() {
                    any = true;
                    let _ = writeln!(out, "- `{}` ← {}", id.as_str(), ids_text(&h.contradicting));
                }
            }
            if !any {
                let _ = writeln!(out, "- （この Context に、反証が付いた仮説は無い）");
            }
            let _ = writeln!(out);
        }
    }

    let _ = writeln!(out, "### 6. 未解決の問い\n");
    let open: Vec<&Item> = now.of_kind(Kind::Question).filter(|i| is_open(i)).collect();
    if open.is_empty() {
        let _ = writeln!(out, "この Context に、未解決の問いは無い。\n");
    }
    for q in open {
        let blocked: Vec<String> = now
            .edges
            .keys()
            .filter(|e| e.relation == BLOCKS_PROFILES && e.from == q.key)
            .map(|e| format!("`{}`", key_text(&e.to)))
            .collect();
        let _ = writeln!(
            out,
            "- {}{}",
            item_line(q),
            if blocked.is_empty() {
                String::new()
            } else {
                format!(" — 塞いでいるリリース対象: {}", blocked.join("、"))
            }
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### 7. 見るべき artifact\n");
    let artifacts: Vec<&Item> = by_code(now, ARTIFACT_CLOSURE).collect();
    if artifacts.is_empty() {
        let _ = writeln!(
            out,
            "この Context に、形式 contract・実装・入口として結びついているものは無い。"
        );
    }
    for a in &artifacts {
        let _ = writeln!(
            out,
            "- {} {} — 実現・形式化していることの証明ではない",
            item_line(a),
            place(a.origin.as_ref())
        );
    }
    let _ = writeln!(out);

    let _ = writeln!(out, "### 8. 欠けている Context\n");
    gaps(out, view);
    let _ = writeln!(out);
}

fn is_open(item: &Item) -> bool {
    item.facts.status.as_deref() == Some("open")
}

fn gaps(out: &mut String, view: &View<'_>) {
    let now = view.now;
    for root in &now.roots {
        match &root.state {
            RootState::Unmapped(_) => {
                let _ = writeln!(out, "- UNMAPPED: `{}`", root.input.text());
            }
            RootState::NotFound => {
                let _ = writeln!(out, "- 見つからない root: `{}`", root.input.text());
            }
            RootState::Mapped(_) => {}
        }
    }
    let citations = now
        .broken
        .iter()
        .filter(|b| matches!(b, Broken::UnresolvedCitation { .. }))
        .count();
    if citations > 0 {
        let _ = writeln!(
            out,
            "- 引用している ID のうち、この版に実体が無いもの: {citations} 件（詳細を参照）"
        );
    }
    let edges = now.broken.len() - citations;
    if edges > 0 {
        let _ = writeln!(out, "- 解けない参照・欠け: {edges} 件（詳細を参照）");
    }
    if let Some(b) = &now.budget {
        if b.exceeded() {
            let _ = writeln!(
                out,
                "- **必須の closure {} 項目が予算 {} を超える。 削らずに不完全として終える。 root を狭めるか、分ける**",
                b.required, b.limit
            );
        }
        if !b.omitted.is_empty() {
            let _ = writeln!(
                out,
                "- 予算で削った周辺 {} 件（詳細を参照。 削ったものは消えたのではなく、この出力に含めていない）",
                b.omitted.len()
            );
        }
    }
    let _ = writeln!(
        out,
        "- 読んでいないもの: Issue / PR の会話（出どころの locator だけを出す）、`design-links.toml`（UI の path から契約への対応）。 グラフの外の現実は、無いのではなく、この出力が知らない"
    );
}

fn details(out: &mut String, view: &View<'_>) {
    let now = view.now;
    let _ = writeln!(out, "## 詳細\n");

    let unmapped: Vec<&RootReport> = now
        .roots
        .iter()
        .filter(|r| matches!(r.state, RootState::Unmapped(_)))
        .collect();
    if !unmapped.is_empty() {
        let _ = writeln!(out, "### UNMAPPED\n");
        for root in unmapped {
            let RootState::Unmapped(reason) = &root.state else {
                continue;
            };
            let why = match reason {
                UnmappedReason::FileMissing => "この版にファイルが無い",
                UnmappedReason::NoCitations => "ID を1つも引いていない",
                UnmappedReason::NoneResolved => "引いている ID が、どれもこの版に実体を持たない",
            };
            let _ = writeln!(out, "UNMAPPED:\n  {}\n", root.input.text());
            let _ = writeln!(
                out,
                "この変更に関係する設計契約を特定できていません。\n「関係する契約がない」という意味ではありません。（{why}）\n"
            );
        }
    }

    let seeds: Vec<&RootReport> = now
        .roots
        .iter()
        .filter(|r| {
            matches!(r.state, RootState::Mapped(_)) && matches!(r.input, RootInput::Path(_))
        })
        .collect();
    if !seeds.is_empty() {
        let _ = writeln!(out, "### path から拾った入口\n");
        let _ = writeln!(
            out,
            "引用から拾った入口は、そのファイルがその契約を満たしている証明ではない。\n"
        );
        for root in seeds {
            let RootState::Mapped(found) = &root.state else {
                continue;
            };
            for seed in found {
                let how = match &seed.via {
                    SeedVia::Given => "指定".to_owned(),
                    SeedVia::Owner => "このファイルが正本".to_owned(),
                    SeedVia::Citation(lines) => format!(
                        "引用（{}行目）",
                        lines
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join("、")
                    ),
                };
                let _ = writeln!(
                    out,
                    "- `{}` ← `{}` {how}",
                    seed.id.as_str(),
                    root.input.text()
                );
            }
        }
        let _ = writeln!(out);
    }

    let required: Vec<&Item> = now.required().collect();
    let _ = writeln!(out, "### 取得した項目\n");
    let _ = writeln!(out, "#### 必須 closure（{} 件）\n", required.len());
    for item in required {
        item_detail(out, item);
    }
    let peripheral: Vec<&Item> = now
        .items
        .values()
        .filter(|i| i.tier != Tier::Required)
        .collect();
    let _ = writeln!(out, "\n#### 周辺（{} 件）\n", peripheral.len());
    for item in peripheral {
        item_detail(out, item);
    }
    let _ = writeln!(out);

    if !now.hypotheses.is_empty() {
        let _ = writeln!(out, "### 仮説の支持と反証\n");
        for (id, h) in &now.hypotheses {
            let _ = writeln!(
                out,
                "- `{}` 評価: {}",
                id.as_str(),
                h.assessment.as_deref().unwrap_or("（未記入）")
            );
            let _ = writeln!(
                out,
                "  - 支持: {}",
                if h.supporting.is_empty() {
                    "なし".to_owned()
                } else {
                    ids_text(&h.supporting)
                }
            );
            let _ = writeln!(
                out,
                "  - 反証: {}",
                if h.contradicting.is_empty() {
                    "なし".to_owned()
                } else {
                    ids_text(&h.contradicting)
                }
            );
            if let Some(scope) = &h.scope {
                let _ = writeln!(out, "  - 範囲: {}", one_line(scope));
            }
        }
        let _ = writeln!(out);
    }

    let with_provenance: Vec<&Item> = now
        .items
        .values()
        .filter(|i| !i.provenance.is_empty())
        .collect();
    if !with_provenance.is_empty() {
        let _ = writeln!(out, "### 出どころ\n");
        let _ = writeln!(
            out,
            "Issue / PR の会話は取りに行かない。 locator だけを出す。\n"
        );
        for item in with_provenance {
            let _ = writeln!(
                out,
                "- `{}`: {}",
                key_text(&item.key),
                item.provenance
                    .iter()
                    .map(|p| format!("`{p}`"))
                    .collect::<Vec<_>>()
                    .join("、")
            );
        }
        let _ = writeln!(out);
    }

    if !now.broken.is_empty() {
        let _ = writeln!(out, "### 解けない参照\n");
        for b in &now.broken {
            let _ = writeln!(out, "- {}", broken_text(b));
        }
        let _ = writeln!(out);
    }

    if let Some(b) = &now.budget
        && !b.omitted.is_empty()
    {
        let _ = writeln!(out, "### 予算で削った周辺\n");
        for o in &b.omitted {
            let _ = writeln!(
                out,
                "- `{}`（{}）{}",
                key_text(&o.key),
                tier_label(o.tier),
                o.codes.iter().copied().collect::<Vec<_>>().join("、")
            );
        }
        let _ = writeln!(out);
    }
}

fn item_detail(out: &mut String, item: &Item) {
    let _ = writeln!(
        out,
        "- {} ／ {} ／ 入った理由: {}",
        item_line(item),
        place(item.origin.as_ref()),
        reason_text(item)
    );
    if !item.superseded_by.is_empty() {
        let _ = writeln!(
            out,
            "  - 後継に置き換えられている: {}",
            ids_text(&item.superseded_by)
        );
    }
}

fn broken_text(b: &Broken) -> String {
    match b {
        Broken::Dangling { from, relation, to } => format!(
            "`{}` →{relation}→ `{}`: 相手がこの版に無い",
            key_text(from),
            key_text(to)
        ),
        Broken::Unparsed { from, field, .. } => format!(
            "`{}` の `{field}` が、ID の形に読めない値を持つ",
            from.as_str()
        ),
        Broken::MissingEvidence {
            hypothesis,
            field,
            evidence,
        } => format!(
            "`{}` の `{field}` が挙げる `{}` が、この版に無い",
            hypothesis.as_str(),
            evidence.as_str()
        ),
        Broken::UnresolvedCitation { path, line, id } => {
            format!("`{path}:{line}` が引く `{id}` が、この版に実体を持たない")
        }
    }
}

fn changes_section(out: &mut String, view: &View<'_>, before: &Context, changes: &Changes) {
    let _ = writeln!(out, "## 変化\n");
    let _ = writeln!(
        out,
        "比較元 {} から、読む地点 {} まで。 範囲は、旧・新の closure の和集合。\n",
        view.from.map_or_else(String::new, address_text),
        address_text(view.at)
    );
    if changes.list.is_empty() {
        let _ = writeln!(out, "この Context に、意味の変化は無い。\n");
        return;
    }
    for class in CLASSES {
        let rows: Vec<&Change> = changes.of_class(class).collect();
        if rows.is_empty() {
            continue;
        }
        let _ = writeln!(out, "### {}（{} 件）\n", class_label(class), rows.len());
        for change in rows {
            let _ = writeln!(out, "- {}", change_text(change, view.now, before));
        }
        let _ = writeln!(out);
    }
}

fn class_label(class: &str) -> &'static str {
    match class {
        "added" => "追加（closure に入った）",
        "removed" => "削除（closure から出た）",
        "changed" => "変更",
        "superseded" => "置き換え",
        "newly-contradicted" => "新しい反証",
        "scope-changed" => "範囲の変更",
        "question-opened" => "問いが開いた",
        "question-closed" => "問いが閉じた",
        "edge-added" => "edge の追加",
        "edge-removed" => "edge の削除",
        "unresolved-mapping" => "解けない対応",
        _ => "（不明）",
    }
}

fn title_in(key: &NodeKey, now: &Context, before: &Context) -> String {
    let item = now.items.get(key).or_else(|| before.items.get(key));
    match item {
        Some(item) => item_line(item),
        None => format!("`{}`", key_text(key)),
    }
}

fn change_text(change: &Change, now: &Context, before: &Context) -> String {
    let tier = |t: &Tier| tier_label(*t);
    match change {
        Change::Added {
            key,
            tier: t,
            new_in_graph,
        } => format!(
            "{}（{}）{}",
            title_in(key, now, before),
            tier(t),
            if *new_in_graph {
                " — 記録そのものが新しい"
            } else {
                " — 記録はすでにあり、closure に入った"
            }
        ),
        Change::Removed {
            key,
            tier: t,
            gone_from_graph,
        } => {
            format!(
                "{}（{}）{}",
                title_in(key, now, before),
                tier(t),
                if *gone_from_graph {
                    " — 記録そのものが無くなった。 旧い版の出どころ: "
                } else {
                    " — 記録は残っているが、closure から外れた。 旧い版の出どころ: "
                }
            ) + &place(before.items.get(key).and_then(|i| i.origin.as_ref()))
        }
        Change::Changed { key, tier: t, how } => format!(
            "{}（{}）{}",
            title_in(key, now, before),
            tier(t),
            match how {
                ChangeKind::Content => " — 内容が変わった",
                ChangeKind::Edges => " — 出る edge が変わった",
                ChangeKind::Both => " — 内容も出る edge も変わった",
            }
        ),
        Change::Superseded { key, tier: t, by } => format!(
            "{}（{}） — 後継: {}",
            title_in(key, now, before),
            tier(t),
            if by.is_empty() {
                "（この版では後継が見つからない）".to_owned()
            } else {
                ids_text(by)
            }
        ),
        Change::NewlyContradicted {
            hypothesis,
            evidence,
        } => format!(
            "`{}` に新しい反証: {}",
            hypothesis.as_str(),
            ids_text(evidence)
        ),
        Change::ScopeChanged { hypothesis } => {
            format!("`{}` の範囲（`scope`）が変わった", hypothesis.as_str())
        }
        Change::QuestionOpened { key } => {
            format!("{} が未解決になった", title_in(key, now, before))
        }
        Change::QuestionClosed {
            key,
            gone_from_graph,
        } => format!(
            "{} が未解決でなくなった{}",
            title_in(key, now, before),
            if *gone_from_graph {
                "（記録そのものが無くなった）"
            } else {
                ""
            }
        ),
        Change::EdgeAdded { edge, info } => edge_text(edge, info, "新しい版にだけある"),
        Change::EdgeRemoved { edge, info } => edge_text(edge, info, "旧い版にだけある"),
        Change::UnresolvedEdge { from, relation, to } => format!(
            "`{}` →{relation}→ `{}`: 相手が新しい版に無い",
            key_text(from),
            key_text(to)
        ),
        Change::MappingChanged {
            root,
            before: b,
            after: a,
        } => format!("`{}` の対応が {b} から {a} へ変わった", root.text()),
    }
}

fn edge_text(edge: &EdgeKey, info: &EdgeInfo, which: &str) -> String {
    format!(
        "`{}` →{}→ `{}`{} — {which}。 出どころ: {}",
        key_text(&edge.from),
        edge.relation,
        key_text(&edge.to),
        if info.partial {
            "（一部だけ）"
        } else {
            ""
        },
        place(info.source.as_ref())
    )
}

// ## JSON

/// `context` の中身。 封筒（`command`・`ok`・`diagnostics`）は呼び出し側が付ける。
pub(crate) fn json_payload(view: &View<'_>) -> Value {
    let now = view.now;
    json!({
        "version": FORMAT_VERSION,
        "complete": view.incomplete.is_empty(),
        "at": address_json(view.at),
        "from": view.from.map(address_json),
        "roots": now.roots.iter().map(root_json).collect::<Vec<_>>(),
        "budget": now.budget.as_ref().map(|b| json!({
            "limit": b.limit,
            "required": b.required,
            "kept": b.kept,
            "exceeded": b.exceeded(),
            "omitted": b.omitted.iter().map(|o| json!({
                "id": key_text(&o.key),
                "tier": tier_json(o.tier),
                "rules": o.codes.iter().collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })),
        "items": now.items.values().map(item_json).collect::<Vec<_>>(),
        "edges": now.edges.iter().map(|(k, v)| edge_json(k, v)).collect::<Vec<_>>(),
        "hypotheses": now.hypotheses.iter().map(|(id, h)| json!({
            "id": id.as_str(),
            "assessment": h.assessment,
            "scope": h.scope,
            "supporting": h.supporting.iter().map(Id::as_str).collect::<Vec<_>>(),
            "contradicting": h.contradicting.iter().map(Id::as_str).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "gaps": {
            "unmapped": now.roots.iter().filter_map(|r| match &r.state {
                RootState::Unmapped(reason) => Some(json!({
                    "root": r.input.text(),
                    "reason": match reason {
                        UnmappedReason::FileMissing => "file-missing",
                        UnmappedReason::NoCitations => "no-citations",
                        UnmappedReason::NoneResolved => "none-resolved",
                    },
                })),
                _ => None,
            }).collect::<Vec<_>>(),
            "not_found": now.roots.iter().filter(|r| r.state == RootState::NotFound)
                .map(|r| r.input.text()).collect::<Vec<_>>(),
            "broken": now.broken.iter().map(broken_json).collect::<Vec<_>>(),
        },
        "not_machine_derivable": NOT_MACHINE.iter().map(|(n, name, reason)| json!({
            "card": n, "name": name, "reason": reason,
        })).collect::<Vec<_>>(),
        "changes": view.changes.map(changes_json),
    })
}

fn tier_json(tier: Tier) -> Value {
    match tier {
        Tier::Required => json!({ "tier": "required", "priority": null }),
        Tier::Peripheral(p) => json!({ "tier": "peripheral", "priority": p }),
    }
}

fn origin_json(origin: Option<&Origin>) -> Value {
    origin.map_or(Value::Null, |o| json!({ "path": o.path, "line": o.line }))
}

fn root_json(root: &RootReport) -> Value {
    let (kind, input) = match &root.input {
        RootInput::Id(id) => ("id", id.as_str().to_owned()),
        RootInput::Path(p) => ("path", p.clone()),
    };
    let seeds: Vec<Value> = match &root.state {
        RootState::Mapped(seeds) => seeds
            .iter()
            .map(|s| match &s.via {
                SeedVia::Given => json!({ "id": s.id.as_str(), "via": "given" }),
                SeedVia::Owner => json!({ "id": s.id.as_str(), "via": "owner" }),
                // 引用であって、満たしている証明ではない。
                SeedVia::Citation(lines) => json!({
                    "id": s.id.as_str(),
                    "via": "citation",
                    "lines": lines.iter().collect::<Vec<_>>(),
                }),
            })
            .collect(),
        _ => Vec::new(),
    };
    json!({ "input": input, "kind": kind, "state": root.state.name(), "seeds": seeds })
}

fn item_json(item: &Item) -> Value {
    let tier = tier_json(item.tier);
    json!({
        "id": key_text(&item.key),
        "kind": item.kind.name(),
        "title": item.title,
        "tier": tier["tier"],
        "priority": tier["priority"],
        "origin": origin_json(item.origin.as_ref()),
        "fingerprint": format!("fnv1a64:{:016x}", item.fingerprint),
        "reasons": item.reasons.iter().map(|r| json!({
            "rule": r.code,
            "label": r.label,
            "via": r.via.as_ref().map(|v| json!({
                "focus": key_text(&v.focus),
                "relation": v.relation,
                "direction": match v.dir { Dir::Out => "out", Dir::In => "in" },
            })),
        })).collect::<Vec<_>>(),
        "facts": {
            "status": item.facts.status,
            "statement": item.facts.statement,
            "selected": item.facts.selected,
            "options": item.facts.options,
            "constraint": item.facts.constraint.as_ref().map(|(l, t)| json!({ "label": l, "text": t })),
            "confidence": item.facts.confidence,
            "scope": item.facts.scope,
            "assessment": item.facts.assessment,
        },
        "superseded_by": item.superseded_by.iter().map(Id::as_str).collect::<Vec<_>>(),
        "provenance": item.provenance,
    })
}

fn edge_json(edge: &EdgeKey, info: &EdgeInfo) -> Value {
    json!({
        "from": key_text(&edge.from),
        "relation": edge.relation,
        "to": key_text(&edge.to),
        "partial": info.partial,
        "source": origin_json(info.source.as_ref()),
        "rule": info.code,
    })
}

fn broken_json(b: &Broken) -> Value {
    match b {
        Broken::Dangling { from, relation, to } => json!({
            "kind": "dangling", "from": key_text(from), "relation": relation, "to": key_text(to),
        }),
        Broken::Unparsed { from, field, raw } => json!({
            "kind": "unparsed", "from": from.as_str(), "field": field, "value": raw,
        }),
        Broken::MissingEvidence {
            hypothesis,
            field,
            evidence,
        } => json!({
            "kind": "missing-evidence", "hypothesis": hypothesis.as_str(),
            "field": field, "evidence": evidence.as_str(),
        }),
        Broken::UnresolvedCitation { path, line, id } => json!({
            "kind": "unresolved-citation", "path": path, "line": line, "id": id,
        }),
    }
}

fn changes_json(changes: &Changes) -> Value {
    let mut by_class = serde_json::Map::new();
    for class in CLASSES {
        by_class.insert(
            (*class).to_owned(),
            Value::Array(changes.of_class(class).map(change_json).collect()),
        );
    }
    Value::Object(by_class)
}

fn change_json(change: &Change) -> Value {
    match change {
        Change::Added {
            key,
            tier,
            new_in_graph,
        } => json!({
            "id": key_text(key), "tier": tier_json(*tier)["tier"], "new_in_graph": new_in_graph,
        }),
        Change::Removed {
            key,
            tier,
            gone_from_graph,
        } => json!({
            "id": key_text(key), "tier": tier_json(*tier)["tier"], "gone_from_graph": gone_from_graph,
        }),
        Change::Changed { key, tier, how } => json!({
            "id": key_text(key),
            "tier": tier_json(*tier)["tier"],
            "how": match how {
                ChangeKind::Content => "content",
                ChangeKind::Edges => "edges",
                ChangeKind::Both => "both",
            },
        }),
        Change::Superseded { key, tier, by } => json!({
            "id": key_text(key),
            "tier": tier_json(*tier)["tier"],
            "by": by.iter().map(Id::as_str).collect::<Vec<_>>(),
        }),
        Change::NewlyContradicted {
            hypothesis,
            evidence,
        } => json!({
            "hypothesis": hypothesis.as_str(),
            "evidence": evidence.iter().map(Id::as_str).collect::<Vec<_>>(),
        }),
        Change::ScopeChanged { hypothesis } => json!({ "hypothesis": hypothesis.as_str() }),
        Change::QuestionOpened { key } => json!({ "id": key_text(key) }),
        Change::QuestionClosed {
            key,
            gone_from_graph,
        } => json!({ "id": key_text(key), "gone_from_graph": gone_from_graph }),
        Change::EdgeAdded { edge, info } | Change::EdgeRemoved { edge, info } => {
            edge_json(edge, info)
        }
        Change::UnresolvedEdge { from, relation, to } => json!({
            "kind": "unresolved-edge", "from": key_text(from), "relation": relation, "to": key_text(to),
        }),
        Change::MappingChanged {
            root,
            before,
            after,
        } => json!({
            "kind": "mapping-changed", "root": root.text(), "before": before, "after": after,
        }),
    }
}
