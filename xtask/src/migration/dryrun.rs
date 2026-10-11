//! 書く前に、書いた後を作って確かめる。
//!
//! 書き換えた本文をメモリで作り、それを `knowledge` と `graph` に通す。 ディスクには
//! 触れない。 「書き換えた後の graph が、対応表の rename だけで説明できるか」の判定は
//! [`EquivalenceCheck`] が持つ。 D08 は、より厳しい判定をここへ差し込む。

use std::collections::{BTreeMap, BTreeSet};

use super::mapping::Mapping;
use super::plan::MigrationPlan;
use super::scan::substitute;
use crate::diagnostic::Report;
use crate::graph::{self, ChangeKind, NodeKey, Registry, SemanticGraph};
use crate::knowledge::{self, KnowledgeSnapshot};
use crate::repo::{META_DIR, RepoView, SPEC_DIR};

/// rename だけでは説明できない差。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum EquivalenceIssue {
    /// 対応表に無い node が増えた。
    UnexpectedAddition(NodeKey),
    /// 対応表の新 ID が node にならなかった。
    MissingAddition(NodeKey),
    /// 対応表に無い node が消えた。
    UnexpectedRemoval(NodeKey),
    /// 対応表の旧 ID が node のまま残った。
    MissingRemoval(NodeKey),
    /// 参照の付け替えでは説明できない、node の内容の変化。
    UnexplainedChange(NodeKey),
    /// 行き先の無い edge が（書き換えの後に）できた。
    UnresolvedEdge {
        from: NodeKey,
        relation: &'static str,
        to: NodeKey,
    },
}

/// 書き換えの前後の graph を比べる。
pub(crate) trait EquivalenceCheck {
    fn check(
        &self,
        before: &SemanticGraph,
        after: &SemanticGraph,
        mapping: &Mapping,
    ) -> Vec<EquivalenceIssue>;
}

/// 差が対応表の rename だけで成り立っていることを見る。
///
/// - 消えた node ＝ 旧 ID の node、増えた node ＝ 新 ID の node
/// - ほかの node は、edge の付け替え（relation 欄が新 ID を指す）以外で内容が変わらない
/// - 行き先の無い edge が無い
///
/// 「relation の欄でない場所の言及が書き換わって内容だけが変わった」は説明のつかない
/// 変化として落とす。 どの欄が relation かは registry の持ち物で、ここでは見分けられない。
/// 許す側へ広げるのは D08 の判断。
pub(crate) struct DefaultEquivalence;

impl EquivalenceCheck for DefaultEquivalence {
    fn check(
        &self,
        before: &SemanticGraph,
        after: &SemanticGraph,
        mapping: &Mapping,
    ) -> Vec<EquivalenceIssue> {
        let d = graph::diff(before, after);
        let mut issues = Vec::new();

        // 入れ替え（A→B と B→A）では、A も B も前後に残る。 node の集合の差に出るのは
        // 「旧 ID にだけあるもの」と「新 ID にだけあるもの」。 入れ替わった中身は
        // 下の `content_equivalence` が見る。
        let olds: BTreeSet<NodeKey> = mapping
            .pairs()
            .iter()
            .map(|(old, _)| NodeKey::Object(old.clone()))
            .collect();
        let news: BTreeSet<NodeKey> = mapping
            .pairs()
            .iter()
            .map(|(_, new)| NodeKey::Object(new.clone()))
            .collect();
        let expected_added: BTreeSet<NodeKey> = news.difference(&olds).cloned().collect();
        let expected_removed: BTreeSet<NodeKey> = olds.difference(&news).cloned().collect();
        let removed: BTreeSet<NodeKey> = d.removed.keys().cloned().collect();

        issues.extend(
            d.added
                .difference(&expected_added)
                .cloned()
                .map(EquivalenceIssue::UnexpectedAddition),
        );
        issues.extend(
            expected_added
                .difference(&d.added)
                .cloned()
                .map(EquivalenceIssue::MissingAddition),
        );
        issues.extend(
            removed
                .difference(&expected_removed)
                .cloned()
                .map(EquivalenceIssue::UnexpectedRemoval),
        );
        issues.extend(
            expected_removed
                .difference(&removed)
                .cloned()
                .map(EquivalenceIssue::MissingRemoval),
        );

        for (key, kind) in &d.changed {
            // relation の欄が新 ID を指し直すと、内容と edge が一緒に変わる。 それは rename の結果。
            if matches!(kind, ChangeKind::Edges | ChangeKind::Both) {
                continue;
            }
            issues.push(EquivalenceIssue::UnexplainedChange(key.clone()));
        }

        // 書き換え前から行き先の無かった edge は、この移行の責任ではない。
        for (from, relation, to) in &d.unresolved {
            // 前は行き先があったのに、後で無くなったものは、この移行が作った宙づり。
            let was_dangling = before.node(to).is_none()
                && before
                    .outgoing(from)
                    .iter()
                    .any(|r| r.name == *relation && r.to == *to);
            if !was_dangling {
                issues.push(EquivalenceIssue::UnresolvedEdge {
                    from: from.clone(),
                    relation,
                    to: to.clone(),
                });
            }
        }
        issues
    }
}

/// dry-run の結果。 `rewritten` は元のパスをキーに持つ（付け替えるファイルも元の名前で）。
#[derive(Debug, Clone)]
pub(crate) struct DryRun {
    pub(crate) rewritten: BTreeMap<String, String>,
    pub(crate) issues: Vec<EquivalenceIssue>,
}

impl DryRun {
    pub(crate) fn is_ok(&self) -> bool {
        self.issues.is_empty()
    }
}

/// 書き換えをメモリで行い、前後の graph を `check` に通す。 ディスクは書かない。
pub(crate) fn dry_run(
    view: &RepoView,
    plan: &MigrationPlan,
    registry: &Registry,
    check: &dyn EquivalenceCheck,
) -> DryRun {
    let mut rewritten = BTreeMap::new();
    for path in plan.snapshots.keys() {
        let Ok(Some(text)) = view.read(path) else {
            continue;
        };
        rewritten.insert(path.clone(), substitute(&text, &plan.mapping));
    }
    let after = after_view(view, &rewritten, plan);
    let issues = equivalence(view, &after, plan, registry, check);
    DryRun { rewritten, issues }
}

/// `meta/**/*.toml` と `specs/**/*.fsl` の全部を持つ、書き換え後のメモリ上の view。
///
/// `knowledge` と `graph` が読むのはこの2つだけ。 触っていないファイルも、
/// 元の本文のまま入れる——入れないと、触っていない記録が「消えた」ように見える。
fn after_view(
    view: &RepoView,
    rewritten: &BTreeMap<String, String>,
    plan: &MigrationPlan,
) -> RepoView {
    let mut files = knowledge_files(view);
    for (path, text) in rewritten {
        files.insert(path.clone(), text.clone());
    }
    // 入れ替えで `to` が別の付け替えの `from` になるので、全部外してから置く。
    let moved: Vec<(String, String)> = plan
        .renames
        .iter()
        .filter_map(|r| files.remove(&r.from).map(|text| (r.to.clone(), text)))
        .collect();
    files.extend(moved);
    RepoView::in_memory(view.root(), files)
}

fn knowledge_files(view: &RepoView) -> BTreeMap<String, String> {
    let metas = view
        .walk(META_DIR)
        .into_iter()
        .filter(|r| r.ends_with(".toml"));
    let specs = view
        .walk(SPEC_DIR)
        .into_iter()
        .filter(|r| r.ends_with(".fsl"));
    metas
        .chain(specs)
        .filter_map(|rel| match view.read(&rel) {
            Ok(Some(text)) => Some((rel, text)),
            _ => None,
        })
        .collect()
}

/// 今の本文をメモリへ写して固定した view。
///
/// 作業ツリーの view は読むたびにディスクを見る。 書く前の姿を、書いた後も
/// 比べられるよう手元に残すのに使う。
pub(crate) fn frozen(view: &RepoView) -> RepoView {
    RepoView::in_memory(view.root(), knowledge_files(view))
}

/// `before` と `after` を graph にして `check` に通し、内容の突き合わせも足す。
///
/// apply の「書いた後に読み直して確かめる」も、これを呼ぶ。
pub(crate) fn equivalence(
    before: &RepoView,
    after: &RepoView,
    plan: &MigrationPlan,
    registry: &Registry,
    check: &dyn EquivalenceCheck,
) -> Vec<EquivalenceIssue> {
    let (before_graph, _) = SemanticGraph::build(before, registry);
    let (after_graph, _) = SemanticGraph::build(after, registry);
    let mut issues = check.check(&before_graph, &after_graph, &plan.mapping);
    issues.extend(content_equivalence(before, after, &plan.mapping));
    issues
}

/// 旧 ID の記録に対応表を当てたものが、新 ID の記録と一致するか。
///
/// graph の fingerprint は id の欄も含めて作るので、rename した node は前後で一致しない。
/// 欄の文字列に同じ置き換えを当ててから比べる。 FSL の node は出どころ（`path:行`）だけで
/// 作る fingerprint で、rename で動かないので、この比較は要らない。
fn content_equivalence(
    before: &RepoView,
    after: &RepoView,
    mapping: &Mapping,
) -> Vec<EquivalenceIssue> {
    let before = snapshot_of(before);
    let after = snapshot_of(after);
    let mut issues = Vec::new();
    for (old, new) in mapping.pairs() {
        let Some(old_record) = before.get(old) else {
            continue;
        };
        let expected = substitute(&old_record.canonical_fields(), mapping);
        let same = after
            .get(new)
            .is_some_and(|r| r.canonical_fields() == expected);
        if !same {
            issues.push(EquivalenceIssue::UnexplainedChange(NodeKey::Object(
                new.clone(),
            )));
        }
    }
    issues
}

fn snapshot_of(view: &RepoView) -> KnowledgeSnapshot {
    // 読み込みの診断は `plan` と `SemanticGraph::build` がすでに見ている。
    let mut rep = Report::default();
    let entries = knowledge::load_view(view, &mut rep);
    KnowledgeSnapshot::from_entries(&entries).0
}
