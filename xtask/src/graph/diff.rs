//! 2つの版の `SemanticGraph` を比べる（X05）。
//!
//! 比べるのは、両方の node key の合併——旧い版にしか無い node も、消えた
//! という事実ごと見えるようにする（`old revision にしか node が無い` の
//! acceptance、`docs/reports/design-system/implementation-plan.md` の
//! acceptance matrix）。

use std::collections::{BTreeMap, BTreeSet};

use super::build::SemanticGraph;
use super::model::{GraphNode, NodeKey, RelationClass};

/// 変わった内容の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ChangeKind {
    /// 記録の欄（fingerprint）が変わった。
    Content,
    /// outgoing edge の集合が変わった。
    Edges,
    /// 両方変わった。
    Both,
}

/// 2つの版の比較結果。 出力はすべて `BTreeMap` / `BTreeSet` で持ち、
/// 並びを決定的にする。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct GraphDiff {
    pub(crate) added: BTreeSet<NodeKey>,
    /// 消えた node。 旧い版の provenance ごと残す
    /// ——「old revision にしか node が無い」を、消して終わらせない。
    pub(crate) removed: BTreeMap<NodeKey, GraphNode>,
    pub(crate) changed: BTreeMap<NodeKey, ChangeKind>,
    /// 新しく `Supersession` class の incoming edge を得た node。
    /// 消えた node が supersede されている場合も含む（置き換えの連鎖）。
    pub(crate) superseded: BTreeSet<NodeKey>,
    /// `new` 側で行き先が node として無い edge。 消えた node への edge も含む
    /// ——消えた node は `new` の node 集合に無いので、この判定だけで両方拾える。
    pub(crate) unresolved: BTreeSet<(NodeKey, &'static str, NodeKey)>,
}

pub(crate) fn diff(old: &SemanticGraph, new: &SemanticGraph) -> GraphDiff {
    let mut out = GraphDiff::default();

    let old_keys: BTreeSet<NodeKey> = old.nodes().map(|n| n.key.clone()).collect();
    let new_keys: BTreeSet<NodeKey> = new.nodes().map(|n| n.key.clone()).collect();

    out.added = new_keys.difference(&old_keys).cloned().collect();
    for key in old_keys.difference(&new_keys) {
        if let Some(node) = old.node(key) {
            out.removed.insert(key.clone(), node.clone());
        }
    }

    for key in old_keys.intersection(&new_keys) {
        let (Some(o), Some(n)) = (old.node(key), new.node(key)) else {
            continue;
        };
        let content_changed = o.fingerprint != n.fingerprint;
        let old_edges: BTreeSet<(&'static str, NodeKey)> = old
            .outgoing(key)
            .iter()
            .map(|r| (r.name, r.to.clone()))
            .collect();
        let new_edges: BTreeSet<(&'static str, NodeKey)> = new
            .outgoing(key)
            .iter()
            .map(|r| (r.name, r.to.clone()))
            .collect();
        let edges_changed = old_edges != new_edges;
        let kind = match (content_changed, edges_changed) {
            (true, true) => Some(ChangeKind::Both),
            (true, false) => Some(ChangeKind::Content),
            (false, true) => Some(ChangeKind::Edges),
            (false, false) => None,
        };
        if let Some(kind) = kind {
            out.changed.insert(key.clone(), kind);
        }
    }

    // superseded: `new` にある Supersession edge の行き先。
    // 行き先がまだ `new` に生きているなら「新しく得たか」を old と突き合わせる。
    // 行き先が消えている（`old` にはあった）なら、それだけで
    // 「置き換えの連鎖」として superseded とする。
    let mut supersession_targets: BTreeSet<NodeKey> = BTreeSet::new();
    for rel in new.all_relations() {
        if rel.class == RelationClass::Supersession {
            supersession_targets.insert(rel.to.clone());
        }
    }
    for target in supersession_targets {
        if new_keys.contains(&target) {
            let had_before = old
                .incoming(&target)
                .iter()
                .any(|r| r.class == RelationClass::Supersession);
            if !had_before {
                out.superseded.insert(target);
            }
        } else if old_keys.contains(&target) {
            out.superseded.insert(target);
        }
    }

    for rel in new.all_relations() {
        if new.node(&rel.to).is_none() {
            out.unresolved
                .insert((rel.from.clone(), rel.name, rel.to.clone()));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::graph::registry::{NodeKind, Registry, RelationRule};
    use crate::repo::RepoView;

    struct TempRoot(std::path::PathBuf);

    impl Drop for TempRoot {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn temp_root(name: &str) -> TempRoot {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "koeru-xtask-graph-diff-{name}-{}-{n}",
            std::process::id()
        ));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join("meta/questions")).expect("meta を作れる");
        TempRoot(root)
    }

    fn write(root: &TempRoot, rel: &str, body: &str) {
        let p = root.0.join(rel);
        fs::create_dir_all(p.parent().expect("親がある")).expect("作れる");
        fs::write(p, body).expect("書ける");
    }

    fn widget(id: &str, extra: &str) -> String {
        format!(
            "schema = 'question'\nid = '{id}'\ntitle = '{id}'\nstatus = 'open'\n\
             owner = 'fixture'\nwhy_it_matters = '…'\nhow_to_close = '…'\n{extra}\n"
        )
    }

    fn write_widget(root: &TempRoot, id: &str, extra: &str) {
        write(
            root,
            &format!("meta/questions/{id}.toml"),
            &widget(id, extra),
        );
    }

    fn key(id: &str) -> NodeKey {
        NodeKey::Object(id.parse().expect("fixture の ID は形が正しい"))
    }

    fn registry() -> Registry {
        Registry {
            node_kinds: vec![NodeKind {
                schema: "question",
                kind: "widget",
            }],
            relations: vec![
                RelationRule {
                    from_schema: "question",
                    field: "uses",
                    relation: "uses",
                    class: RelationClass::Reference,
                },
                RelationRule {
                    from_schema: "question",
                    field: "replaces",
                    relation: "replaces",
                    class: RelationClass::Supersession,
                },
            ],
            ..Registry::default()
        }
    }

    fn build(root: &TempRoot) -> SemanticGraph {
        let (graph, issues) = SemanticGraph::build(&RepoView::working_tree(&root.0), &registry());
        assert!(issues.is_empty(), "{issues:?}");
        graph
    }

    /// 追加・削除・変更をそれぞれ検出する。 削除された node は旧い provenance ごと残る。
    #[test]
    fn 追加_削除_変更を検出する() {
        let old_root = temp_root("basic-old");
        write_widget(&old_root, "Q-fix-01", "");
        write_widget(&old_root, "Q-fix-02", "");
        let old = build(&old_root);

        let new_root = temp_root("basic-new");
        // Q-fix-01 は本文を変える（changed）。Q-fix-02 は消す（removed）。
        // Q-fix-03 を足す（added）。
        write_widget(&new_root, "Q-fix-01", "note = '本文が変わった'");
        write_widget(&new_root, "Q-fix-03", "");
        let new = build(&new_root);

        let d = diff(&old, &new);
        assert_eq!(d.added, BTreeSet::from([key("Q-fix-03")]));
        assert!(d.removed.contains_key(&key("Q-fix-02")));
        assert_eq!(
            d.removed[&key("Q-fix-02")].provenance.path,
            old.node(&key("Q-fix-02")).unwrap().provenance.path
        );
        assert_eq!(d.changed.get(&key("Q-fix-01")), Some(&ChangeKind::Content));
    }

    /// `new` の supersession edge が、消えた node を指していれば「置き換えの
    /// 連鎖」として superseded に入る。 A は消え、B が A を supersede し、
    /// C が B を supersede する——両方の経路（既存 node が新しく得た／消えた
    /// node が supersede されている）を1つの diff で試す。
    #[test]
    fn 置き換えの連鎖を_superseded_が拾う() {
        let old_root = temp_root("chain-old");
        write_widget(&old_root, "Q-fix-01", "");
        let old = build(&old_root);

        let new_root = temp_root("chain-new");
        // Q-fix-01 は消え、Q-fix-02 が Q-fix-01 を、Q-fix-03 が Q-fix-02 を置き換える。
        write_widget(&new_root, "Q-fix-02", "replaces = ['Q-fix-01']");
        write_widget(&new_root, "Q-fix-03", "replaces = ['Q-fix-02']");
        let new = build(&new_root);

        let d = diff(&old, &new);
        assert!(d.removed.contains_key(&key("Q-fix-01")));
        assert!(d.added.contains(&key("Q-fix-02")));
        assert!(d.added.contains(&key("Q-fix-03")));
        // Q-fix-01（消えた node）と Q-fix-02（new で新しく存在するようになった node）の
        // 両方が superseded に入る。
        assert!(d.superseded.contains(&key("Q-fix-01")));
        assert!(d.superseded.contains(&key("Q-fix-02")));
    }

    /// `new` にしか無い dangling edge と、消えた node への edge は、
    /// どちらも `unresolved` にまとまる。
    #[test]
    fn unresolved_は_new_の_dangling_と消えた_node_への_edge_をまとめる() {
        let old_root = temp_root("unresolved-old");
        write_widget(&old_root, "Q-fix-01", "");
        let old = build(&old_root);

        let new_root = temp_root("unresolved-new");
        // Q-fix-01 は消え、Q-fix-02 が Q-fix-01（消えた）と Q-fix-99（存在しない）を指す。
        write_widget(&new_root, "Q-fix-02", "uses = ['Q-fix-01', 'Q-fix-99']");
        let new = build(&new_root);

        let d = diff(&old, &new);
        assert!(
            d.unresolved
                .contains(&(key("Q-fix-02"), "uses", key("Q-fix-01")))
        );
        assert!(
            d.unresolved
                .contains(&(key("Q-fix-02"), "uses", key("Q-fix-99")))
        );
    }

    /// 同じ内容・同じ edge の node は `changed` に入らない。
    #[test]
    fn 変わっていない_node_は_changed_に入らない() {
        let root_old = temp_root("unchanged-old");
        write_widget(&root_old, "Q-fix-01", "uses = ['Q-fix-02']");
        write_widget(&root_old, "Q-fix-02", "");
        let old = build(&root_old);

        let root_new = temp_root("unchanged-new");
        write_widget(&root_new, "Q-fix-01", "uses = ['Q-fix-02']");
        write_widget(&root_new, "Q-fix-02", "");
        let new = build(&root_new);

        let d = diff(&old, &new);
        assert!(d.added.is_empty());
        assert!(d.removed.is_empty());
        assert!(d.changed.is_empty());
        assert!(d.superseded.is_empty());
        assert!(d.unresolved.is_empty());
    }

    /// 2つの実 git commit（`RepoView::revision`）から作った graph を diff する。
    /// fixture ディレクトリを直接 in-memory で作るのではなく、実際に commit を
    /// 積んだリポジトリを使う——`SemanticGraph::build` が版を選んで読めることを
    /// 通しで確かめる。
    #[test]
    fn 実際の_git_の2つの版から_graph_を作って比べる() {
        let root = temp_root("real-git");
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .current_dir(&root.0)
                .args(args)
                .output()
                .expect("git を起動できる");
            assert!(
                out.status.success(),
                "git {args:?} が失敗した: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout)
                .expect("UTF-8")
                .trim()
                .to_owned()
        };
        git(&["init", "-q"]);
        for (k, v) in [
            ("user.name", "graph-diff-test"),
            ("user.email", "graph-diff-test@example.invalid"),
            ("commit.gpgsign", "false"),
        ] {
            git(&["config", k, v]);
        }
        write_widget(&root, "Q-fix-01", "");
        write_widget(&root, "Q-fix-02", "");
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "base"]);
        let base = git(&["rev-parse", "HEAD"]);

        // Q-fix-02 を消し、Q-fix-03 を Q-fix-01 の後継として足す。
        fs::remove_file(root.0.join("meta/questions/Q-fix-02.toml")).expect("消せる");
        write_widget(&root, "Q-fix-03", "replaces = ['Q-fix-01']");
        git(&["add", "-A"]);
        git(&["commit", "-q", "-m", "supersede"]);
        let head = git(&["rev-parse", "HEAD"]);

        let old_view = RepoView::revision(&root.0, &base).expect("base を解決できる");
        let new_view = RepoView::revision(&root.0, &head).expect("head を解決できる");
        let reg = registry();
        let (old, old_issues) = SemanticGraph::build(&old_view, &reg);
        let (new, new_issues) = SemanticGraph::build(&new_view, &reg);
        assert!(old_issues.is_empty(), "{old_issues:?}");
        assert!(new_issues.is_empty(), "{new_issues:?}");
        assert_eq!(
            old.revision(),
            Some(&crate::repo::RevisionRef::Revision { sha: base })
        );

        let d = diff(&old, &new);
        assert!(d.added.contains(&key("Q-fix-03")));
        assert!(d.removed.contains_key(&key("Q-fix-02")));
        assert!(d.superseded.contains(&key("Q-fix-01")));
    }
}
