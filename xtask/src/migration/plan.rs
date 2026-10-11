//! 対応表を実ファイルへ当てる計画。
//!
//! ここで決めるのは「安全に書き換えられるか」だけで、書かない。 衝突と、書き換え方の
//! 分からない参照を理由つきで全部返し、1件でもあれば計画は作らない。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use super::mapping::Mapping;
use super::scan::{self, Edit, EditKind, UnknownInboundRef};
use crate::graph::{GraphIssue, GraphNode, NodeKey, Registry, SemanticGraph};
use crate::knowledge::Id;
use crate::repo::RepoView;

/// 計画を作れなかった理由。 まとめて返すので、直す人は1回で全部を見られる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PlanIssue {
    /// 読み込みの段の失敗（壊れた TOML・知らない schema・重複 ID）。 ID を信用できない。
    Load(String),
    /// 新しい ID が、この対応表で退かない別の記録・宣言とぶつかる。
    NewIdCollision { new: Id, existing_at: String },
    /// 複数の旧 ID が同じ新 ID へ写る。
    MappingCollision { new: Id, olds: Vec<Id> },
    /// 旧 ID がどこにも定義されていない。
    UnknownSource { old: Id },
    /// 書き換え方の分からないファイルに、旧 ID の言及がある。
    UnknownInboundReference { path: String, line: usize, old: Id },
}

impl std::fmt::Display for PlanIssue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PlanIssue::Load(m) => write!(f, "{m}"),
            PlanIssue::NewIdCollision { new, existing_at } => {
                write!(f, "新しい ID `{new}` は {existing_at} がすでに名乗っている")
            }
            PlanIssue::MappingCollision { new, olds } => {
                let olds: Vec<&str> = olds.iter().map(Id::as_str).collect();
                write!(f, "`{new}` へ複数の旧 ID が写る（{}）", olds.join(", "))
            }
            PlanIssue::UnknownSource { old } => {
                write!(f, "旧 ID `{old}` の定義がどこにも無い")
            }
            PlanIssue::UnknownInboundReference { path, line, old } => write!(
                f,
                "{path}:{line}: 書き換え方の分からないファイルに `{old}` の言及がある"
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PlanOptions {
    /// 書き換え方の分からないファイルの言及を、そのまま残して計画を作る。
    /// 残した言及は計画に載る（`MigrationPlan::not_rewritten`）。
    pub(crate) allow_unknown_inbound: bool,
}

/// 記録がファイル名で ID を名乗っている（`<id>.toml`）とき、ID と一緒に付け替える名前。
///
/// `knowledge::load` は1件1ファイルの記録に「ファイル名が id と一致する」を求める。
/// 中身の `id` だけを書き換えると、名前が合わずに読み込みが落ちる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct FileRename {
    pub(crate) from: String,
    pub(crate) to: String,
}

/// 検証済みの計画。 並びは path・行・桁の順で決定的。
#[derive(Debug, Clone)]
pub(crate) struct MigrationPlan {
    pub(crate) edits: Vec<Edit>,
    pub(crate) renames: Vec<FileRename>,
    /// `allow_unknown_inbound` で許した、書き換えない言及。
    pub(crate) not_rewritten: Vec<UnknownInboundRef>,
    /// 計画時点の、触るファイルの全文。 書き換える前に今の中身と突き合わせ、
    /// 計画のあとに変わっていたら止める（古い計画を当てない）。
    pub(crate) snapshots: BTreeMap<String, String>,
    pub(crate) mapping: Mapping,
}

impl MigrationPlan {
    /// dry-run で人に見せる文面。 出力は呼び出し側の仕事。
    pub(crate) fn render(&self) -> String {
        let mut out = String::new();
        let mut by_path: BTreeMap<&str, Vec<&Edit>> = BTreeMap::new();
        for e in &self.edits {
            by_path.entry(e.path.as_str()).or_default().push(e);
        }
        for (path, edits) in &by_path {
            let _ = writeln!(out, "{path}");
            for e in edits {
                let _ = writeln!(
                    out,
                    "  {}:{} {} -> {} ({})",
                    e.line,
                    e.col_start + 1,
                    e.old,
                    e.new,
                    kind_label(e.kind)
                );
            }
        }
        if !self.renames.is_empty() {
            out.push_str("ファイル名の付け替え\n");
            for r in &self.renames {
                let _ = writeln!(out, "  {} -> {}", r.from, r.to);
            }
        }
        if !self.not_rewritten.is_empty() {
            out.push_str("書き換えない言及\n");
            for u in &self.not_rewritten {
                let _ = writeln!(out, "  {}:{} {}", u.path, u.line, u.old);
            }
        }
        let _ = writeln!(
            out,
            "{} 件の書き換え、{} 件のファイル名の付け替え",
            self.edits.len(),
            self.renames.len()
        );
        out
    }
}

fn kind_label(kind: EditKind) -> &'static str {
    match kind {
        EditKind::Definition => "定義",
        EditKind::FslDeclaration => "FSL の宣言",
        EditKind::RelationValue => "TOML の値",
        EditKind::Mention => "言及",
    }
}

/// 計画を作る。 理由が1件でもあれば、計画ごと返さない。
///
/// 存在の確認は `SemanticGraph`（X05）が持つ node で行う。 `registry` が node にしない
/// schema の記録は「存在しない」と見なされるので、旧 ID を持つ schema は `registry` に
/// 登録されている必要がある。
pub(crate) fn plan(
    view: &RepoView,
    mapping: &Mapping,
    registry: &Registry,
    options: PlanOptions,
) -> Result<MigrationPlan, Vec<PlanIssue>> {
    let mut issues = Vec::new();

    let (graph, graph_issues) = SemanticGraph::build(view, registry);
    // dangling や循環は、この移行が作ったものではない。 `check-*` の持ち物で、ここでは止めない。
    issues.extend(graph_issues.into_iter().filter_map(|i| match i {
        GraphIssue::Load(m) => Some(PlanIssue::Load(m)),
        _ => None,
    }));

    let mut by_new: BTreeMap<&Id, Vec<&Id>> = BTreeMap::new();
    for (old, new) in mapping.pairs() {
        by_new.entry(new).or_default().push(old);
    }
    for (new, olds) in by_new {
        if olds.len() > 1 {
            issues.push(PlanIssue::MappingCollision {
                new: new.clone(),
                olds: olds.into_iter().cloned().collect(),
            });
        }
    }

    for (old, new) in mapping.pairs() {
        if graph.node(&NodeKey::Object(old.clone())).is_none() {
            issues.push(PlanIssue::UnknownSource { old: old.clone() });
        }
        // 退く側の ID なら跡地は空くので、ぶつかっているうちに入らない。
        if !mapping.is_old(new)
            && let Some(existing) = graph.node(&NodeKey::Object(new.clone()))
        {
            issues.push(PlanIssue::NewIdCollision {
                new: new.clone(),
                existing_at: describe(existing),
            });
        }
    }

    let scanned = scan::scan(view, mapping);
    let mut not_rewritten = Vec::new();
    if options.allow_unknown_inbound {
        not_rewritten = scanned.unknown;
    } else {
        issues.extend(
            scanned
                .unknown
                .into_iter()
                .map(|u| PlanIssue::UnknownInboundReference {
                    path: u.path,
                    line: u.line,
                    old: u.old,
                }),
        );
    }

    if !issues.is_empty() {
        return Err(issues);
    }

    let renames = renames_for(&scanned.edits, mapping);
    let mut touched: BTreeSet<&str> = scanned.edits.iter().map(|e| e.path.as_str()).collect();
    touched.extend(renames.iter().map(|r| r.from.as_str()));
    let snapshots = touched
        .into_iter()
        .filter_map(|path| match view.read(path) {
            Ok(Some(text)) => Some((path.to_owned(), text)),
            _ => None,
        })
        .collect();

    Ok(MigrationPlan {
        edits: scanned.edits,
        renames,
        not_rewritten,
        snapshots,
        mapping: mapping.clone(),
    })
}

fn describe(node: &GraphNode) -> String {
    match (&node.provenance.path, node.provenance.line) {
        (Some(p), Some(line)) => format!("{}:{line}", p.display()),
        (Some(p), None) => p.display().to_string(),
        (None, _) => "出どころ不明の記録".to_owned(),
    }
}

/// 定義の行が `<旧 id>.toml` に書かれていたら、そのファイルを `<新 id>.toml` へ付け替える。
///
/// 収集ファイル（`requirement-set` など）はファイル名が個々の ID と無関係なので、
/// 名前が旧 ID と一致しない限り付け替えは起きない。
fn renames_for(edits: &[Edit], mapping: &Mapping) -> Vec<FileRename> {
    let mut out = Vec::new();
    for (old, new) in mapping.pairs() {
        let Some(def) = edits
            .iter()
            .find(|e| e.kind == EditKind::Definition && &e.old == old)
        else {
            continue;
        };
        let path = Path::new(&def.path);
        let owns_file = path.extension().is_some_and(|e| e == "toml")
            && path.file_stem().is_some_and(|s| s == old.as_str());
        if !owns_file {
            continue;
        }
        let name = format!("{new}.toml");
        let to = match path.parent().and_then(Path::to_str) {
            Some(dir) if !dir.is_empty() => format!("{}/{name}", dir.replace('\\', "/")),
            _ => name,
        };
        out.push(FileRename {
            from: def.path.clone(),
            to,
        });
    }
    out
}
