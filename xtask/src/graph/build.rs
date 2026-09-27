//! `KnowledgeSnapshot` から `SemanticGraph` を作り、近傍・supersession chain・
//! validate の traversal を持つ（X05）。
//!
//! ここでは node / relation の意味を判断しない。 [`Registry`] が渡した規則を
//! そのまま実行するだけで、`Record::schema()` の具体的な値をこのファイルが
//! 知ることはない。

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::model::{GraphNode, NodeKey, NodeProvenance, Relation, RelationClass, fnv1a_64};
use super::registry::Registry;
use crate::diagnostic::{Report, Severity};
use crate::knowledge::{self, Id, KnowledgeSnapshot, Record};
use crate::repo::{RepoView, RevisionRef};

/// build / traversal / diff の途中で見つかった、落とさない問題。
///
/// 呼び出し側が握り潰さない限り検査を続けられるよう、値として返す
/// ——`rust-conventions` の「予期できる結果はエラーにせず値で返す」を
/// graph の内部にも適用したもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum GraphIssue {
    /// 読み込みの段の診断（`knowledge::load_view` の `Report`・重複 ID）を
    /// そのまま持つ。 知らない schema・壊れた TOML・重複 ID は、graph の
    /// 消費者から見ればどれも「その記録は node にならなかった」でしかないので、
    /// ここでは分類しない。
    Load(String),
    /// edge の相手先が、この graph に node として無い。
    Dangling {
        from: NodeKey,
        relation: &'static str,
        to: NodeKey,
    },
    /// `Supersession` class の edge が循環していた。 先頭から辿った経路を持つ
    /// ——最後の要素が、どこかで既に列に入っていた node。
    SupersessionCycle(Vec<NodeKey>),
}

/// 版ごとの typed graph。
#[derive(Debug, Default)]
pub(crate) struct SemanticGraph {
    nodes: BTreeMap<NodeKey, GraphNode>,
    outgoing: BTreeMap<NodeKey, Vec<Relation>>,
    incoming: BTreeMap<NodeKey, Vec<Relation>>,
    revision: Option<RevisionRef>,
}

impl SemanticGraph {
    /// `view` から読み込み、`registry` の語彙で graph を作る。
    ///
    /// 読み込みの段の診断（知らない schema・壊れた TOML・重複 ID）は積んで
    /// 続ける——`load_view` 自身がすでに「読み飛ばさずに落とす」個々の記録の
    /// 話であって、ここではそれを理由に graph の構築全体を止めない
    /// （旧い版が今の `SHAPES` に無い schema を持っていても、比較対象として
    /// graph にできる必要がある）。
    pub(crate) fn build(view: &RepoView, registry: &Registry) -> (Self, Vec<GraphIssue>) {
        let mut rep = Report::default();
        let entries = knowledge::load_view(view, &mut rep);
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&entries);
        let snapshot = snapshot.with_fsl_view(view);

        let mut issues: Vec<GraphIssue> = rep
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| GraphIssue::Load(d.message.clone()))
            .collect();
        issues.extend(dups.into_iter().map(GraphIssue::Load));

        let mut graph = Self::from_snapshot_inner(&snapshot, registry);
        // 版が分からなくても致命的ではない。 `identity()` が失敗するのは
        // 通常の操作の外側（壊れたリポジトリ）でしかない。
        graph.revision = view.identity().ok();
        (graph, issues)
    }

    /// 版を経由せず、すでに手元にある `KnowledgeSnapshot` から作る。
    ///
    /// 単体試験が、一時ディレクトリや git を用意せずに graph を作れるようにする口。
    pub(crate) fn from_snapshot(snapshot: &KnowledgeSnapshot, registry: &Registry) -> Self {
        Self::from_snapshot_inner(snapshot, registry)
    }

    fn from_snapshot_inner(snapshot: &KnowledgeSnapshot, registry: &Registry) -> Self {
        let mut nodes: BTreeMap<NodeKey, GraphNode> = BTreeMap::new();

        for record in snapshot.records() {
            let Some(kind) = registry.kind_for_schema(record.schema()) else {
                continue;
            };
            // id を持たない記録は `load_view` 側の `check_shape` がすでに
            // 診断済み（形が崩れた記録）。 ここでは黙って node にしない。
            let Some(id) = record.id() else {
                continue;
            };
            let key = NodeKey::Object(id.clone());
            nodes.insert(
                key.clone(),
                GraphNode {
                    key,
                    kind,
                    provenance: provenance_of(record),
                    fingerprint: fnv1a_64(&record.canonical_fields()),
                },
            );
        }

        if let Some(fsl_kind) = registry.fsl_node_kind {
            for id in snapshot.fsl_ids() {
                let key = NodeKey::Object(id.clone());
                // meta 側がすでに同じ ID を node にしていたら、そちらを優先する
                // ——meta と FSL が同じ ID を名乗るのは想定していないが、
                // 名乗った場合に上書きで消さない。
                nodes.entry(key.clone()).or_insert_with(|| GraphNode {
                    key: key.clone(),
                    kind: fsl_kind,
                    provenance: fsl_provenance(snapshot, id),
                    fingerprint: fnv1a_64(snapshot.fsl_site(id).unwrap_or_default()),
                });
            }
        }

        let mut outgoing: BTreeMap<NodeKey, Vec<Relation>> = BTreeMap::new();
        let mut incoming: BTreeMap<NodeKey, Vec<Relation>> = BTreeMap::new();

        for record in snapshot.records() {
            let Some(id) = record.id() else { continue };
            let from_key = NodeKey::Object(id.clone());
            // node にならなかった記録（registry が拾わない schema）は edge の
            // 起点にもしない。 起点が node として存在しない edge を許すと、
            // 「起点も無い」場合まで dangling の判定に混ざる。
            if !nodes.contains_key(&from_key) {
                continue;
            }
            let source = provenance_of(record);

            for rule in &registry.relations {
                if rule.from_schema != record.schema() {
                    continue;
                }
                for target in field_targets(record, rule.field) {
                    let Ok(target_id) = target.parse::<Id>() else {
                        continue;
                    };
                    push_edge(
                        &mut outgoing,
                        &mut incoming,
                        Relation {
                            from: from_key.clone(),
                            to: NodeKey::Object(target_id),
                            name: rule.relation,
                            class: rule.class,
                            source: source.clone(),
                        },
                    );
                }
            }

            for rule in &registry.locators {
                if rule.from_schema != record.schema() {
                    continue;
                }
                for value in field_targets(record, rule.field) {
                    let to_key = NodeKey::Locator {
                        kind: rule.locator_kind,
                        value: value.clone(),
                    };
                    nodes.entry(to_key.clone()).or_insert_with(|| GraphNode {
                        key: to_key.clone(),
                        kind: rule.locator_kind,
                        provenance: NodeProvenance::default(),
                        fingerprint: fnv1a_64(&value),
                    });
                    push_edge(
                        &mut outgoing,
                        &mut incoming,
                        Relation {
                            from: from_key.clone(),
                            to: to_key,
                            name: rule.relation,
                            class: RelationClass::Reference,
                            source: source.clone(),
                        },
                    );
                }
            }
        }

        for rels in outgoing.values_mut() {
            rels.sort();
        }
        for rels in incoming.values_mut() {
            rels.sort();
        }

        Self {
            nodes,
            outgoing,
            incoming,
            revision: None,
        }
    }

    pub(crate) fn nodes(&self) -> impl Iterator<Item = &GraphNode> {
        self.nodes.values()
    }

    pub(crate) fn node(&self, key: &NodeKey) -> Option<&GraphNode> {
        self.nodes.get(key)
    }

    pub(crate) fn outgoing(&self, key: &NodeKey) -> &[Relation] {
        self.outgoing.get(key).map_or(&[], Vec::as_slice)
    }

    pub(crate) fn incoming(&self, key: &NodeKey) -> &[Relation] {
        self.incoming.get(key).map_or(&[], Vec::as_slice)
    }

    /// 全 edge。 `diff` が dangling・superseded を見るのに読む。
    pub(crate) fn all_relations(&self) -> impl Iterator<Item = &Relation> {
        self.outgoing.values().flatten()
    }

    pub(crate) fn revision(&self) -> Option<&RevisionRef> {
        self.revision.as_ref()
    }

    /// `start` から `depth` 段までの近傍。 `filter` を通った edge だけを辿る。
    ///
    /// `visited` で止めるので、`Reference` class の循環があっても終わる
    /// ——一般の参照グラフに循環があってよいという前提を、engine 側で保証する。
    pub(crate) fn neighborhood(
        &self,
        start: &NodeKey,
        depth: usize,
        filter: impl Fn(&Relation) -> bool,
    ) -> BTreeSet<NodeKey> {
        let mut visited = BTreeSet::new();
        visited.insert(start.clone());
        let mut frontier = vec![start.clone()];
        for _ in 0..depth {
            let mut next = Vec::new();
            for key in &frontier {
                for rel in self.outgoing(key).iter().filter(|r| filter(r)) {
                    if visited.insert(rel.to.clone()) {
                        next.push(rel.to.clone());
                    }
                }
            }
            if next.is_empty() {
                break;
            }
            frontier = next;
        }
        visited
    }

    /// `Supersession` class の edge だけを辿った後継の連なり。 `start` 自身を含む。
    ///
    /// `GraphIssue` を `Box` に包むのは、`Dangling` の分だけこの `Result` の
    /// `Err` を大きくしないため（`clippy::result_large_err`）。
    ///
    /// # Errors
    ///
    /// 辿った先がすでに列へ入っている（循環している）。
    pub(crate) fn supersession_chain(
        &self,
        start: &NodeKey,
    ) -> Result<Vec<NodeKey>, Box<GraphIssue>> {
        let mut chain = vec![start.clone()];
        let mut seen = BTreeSet::new();
        seen.insert(start.clone());
        let mut current = start.clone();
        loop {
            let next = self
                .outgoing(&current)
                .iter()
                .find(|r| r.class == RelationClass::Supersession)
                .map(|r| r.to.clone());
            let Some(next) = next else { break };
            if !seen.insert(next.clone()) {
                chain.push(next);
                return Err(Box::new(GraphIssue::SupersessionCycle(chain)));
            }
            chain.push(next.clone());
            current = next;
        }
        Ok(chain)
    }

    /// dangling edge と supersession の循環をまとめて見る。
    ///
    /// build 自体はこれらの理由で止まらない（edge は残したまま、値として返す）。
    /// 呼びたいときに呼ぶ、独立した検査。
    pub(crate) fn validate(&self) -> Vec<GraphIssue> {
        let mut issues = Vec::new();
        for rel in self.all_relations() {
            if !self.nodes.contains_key(&rel.to) {
                issues.push(GraphIssue::Dangling {
                    from: rel.from.clone(),
                    relation: rel.name,
                    to: rel.to.clone(),
                });
            }
        }
        let mut checked = BTreeSet::new();
        for key in self.nodes.keys() {
            if checked.contains(key) {
                continue;
            }
            match self.supersession_chain(key) {
                Ok(chain) => checked.extend(chain),
                Err(boxed) => match *boxed {
                    GraphIssue::SupersessionCycle(cycle) => {
                        checked.extend(cycle.iter().cloned());
                        issues.push(GraphIssue::SupersessionCycle(cycle));
                    }
                    other => issues.push(other),
                },
            }
        }
        issues
    }
}

fn push_edge(
    outgoing: &mut BTreeMap<NodeKey, Vec<Relation>>,
    incoming: &mut BTreeMap<NodeKey, Vec<Relation>>,
    rel: Relation,
) {
    outgoing
        .entry(rel.from.clone())
        .or_default()
        .push(rel.clone());
    incoming.entry(rel.to.clone()).or_default().push(rel);
}

/// `field` の値を、単数・複数のどちらの形でも同じ `Vec<String>` として読む。
///
/// meta の欄は名前空間によって形が割れている（`supersedes` は配列、
/// `superseded_by` は単数の文字列）。 [`super::registry::RelationRule`] /
/// [`super::registry::LocatorRule`] はどちらの形も同じ規則で書けるようにする。
fn field_targets(record: &Record, field: &str) -> Vec<String> {
    let many = record.strs(field);
    if !many.is_empty() {
        return many;
    }
    record
        .str(field)
        .map(|s| vec![s.to_owned()])
        .unwrap_or_default()
}

fn provenance_of(record: &Record) -> NodeProvenance {
    let p = record.provenance();
    NodeProvenance {
        path: Some(p.path().to_path_buf()),
        line: p.line(),
    }
}

fn fsl_provenance(snapshot: &KnowledgeSnapshot, id: &Id) -> NodeProvenance {
    let Some(site) = snapshot.fsl_site(id) else {
        return NodeProvenance::default();
    };
    match site.rsplit_once(':') {
        Some((path, line)) => NodeProvenance {
            path: Some(PathBuf::from(path)),
            line: line.parse().ok(),
        },
        None => NodeProvenance {
            path: Some(PathBuf::from(site)),
            line: None,
        },
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::graph::registry::{LocatorRule, NodeKind, RelationRule};

    /// 試験ごとに使い捨てる一時ディレクトリ。 `tempfile` は引かない
    /// ——`repo::view` の試験と同じ形（`std::env::temp_dir()` の下）にする。
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
            "koeru-xtask-graph-{name}-{}-{n}",
            std::process::id()
        ));
        fs::remove_dir_all(&root).ok();
        fs::create_dir_all(root.join("meta")).expect("meta を作れる");
        TempRoot(root)
    }

    fn write(root: &TempRoot, rel: &str, body: &str) {
        let p = root.0.join(rel);
        fs::create_dir_all(p.parent().expect("親がある")).expect("作れる");
        fs::write(p, body).expect("書ける");
    }

    /// `question` という実在する schema を器にしつつ、`widget` という
    /// registry 独自の kind 名と、`uses` / `replaces` / `replaced_by` /
    /// `story` という実在しない欄・relation 名で engine を試す。
    ///
    /// schema 文字列そのものは `knowledge::load_view` の `SHAPES`（閉じた集合）
    /// を通す必要があるので実在するものを使うが、それを node の何にするか
    /// （`kind`）・欄を何の edge にするか（`relation` / `class`）は、すべて
    /// ここで決めた registry の値であって、engine 側は一切知らない
    /// ——具体語をハードコードしていないなら、この語彙でも本物の語彙と
    /// 同じように動くはず。
    fn fixture_registry() -> Registry {
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
                RelationRule {
                    from_schema: "question",
                    field: "replaced_by",
                    relation: "replaces",
                    class: RelationClass::Supersession,
                },
            ],
            locators: vec![LocatorRule {
                from_schema: "question",
                field: "story",
                relation: "context_for",
                locator_kind: "story",
            }],
            fsl_node_kind: Some("fsl-requirement"),
        }
    }

    /// `question` schema の必須項目を埋めた、fixture の1件。 `extra` に
    /// registry 独自の欄（`uses` など）を足す。
    fn widget(id: &str, extra: &str) -> String {
        format!(
            "schema = 'question'\nid = '{id}'\ntitle = '{id}'\nstatus = 'open'\n\
             owner = 'fixture'\nwhy_it_matters = '…'\nhow_to_close = '…'\n{extra}\n"
        )
    }

    /// `meta/questions/` の下に、`id` をファイル名として書く。
    /// `load_view` はファイル名と `id` の一致を見るので、呼び出し側で
    /// パスを決め打ちしない。
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

    /// registry が渡した、見たこともない relation 名でも node と edge を作る。
    /// 具体語をハードコードしていないことの、いちばん基本の確認。
    #[test]
    fn registry_が渡した語彙だけで_node_と_edge_を作る() {
        let root = temp_root("registry-driven");
        write_widget(&root, "Q-fix-01", "uses = ['Q-fix-02']");
        write_widget(&root, "Q-fix-02", "");
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let a = key("Q-fix-01");
        let b = key("Q-fix-02");
        assert_eq!(graph.nodes().count(), 2);
        assert_eq!(graph.node(&a).map(|n| n.kind), Some("widget"));
        let out = graph.outgoing(&a);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].to, b);
        assert_eq!(out[0].name, "uses");
        assert_eq!(out[0].class, RelationClass::Reference);
    }

    /// dangling edge は消さずに持ったまま、`validate` が値として報告する。
    #[test]
    fn dangling_edge_は消さずに残り_validate_が報告する() {
        let root = temp_root("dangling");
        write_widget(&root, "Q-fix-01", "uses = ['Q-fix-99']");
        let view = RepoView::working_tree(&root.0);
        let (graph, _issues) = SemanticGraph::build(&view, &fixture_registry());

        let a = key("Q-fix-01");
        let missing = key("Q-fix-99");
        // 消さずに残る。
        assert_eq!(graph.outgoing(&a).len(), 1);
        assert_eq!(graph.outgoing(&a)[0].to, missing);
        assert!(graph.node(&missing).is_none());

        let issues = graph.validate();
        assert!(issues.contains(&GraphIssue::Dangling {
            from: a,
            relation: "uses",
            to: missing,
        }));
    }

    /// `Reference` class の循環は `neighborhood` が `visited` で止めて終わる。
    #[test]
    fn reference_の循環は_neighborhood_が終端する() {
        let root = temp_root("reference-cycle");
        write_widget(&root, "Q-fix-01", "uses = ['Q-fix-02']");
        write_widget(&root, "Q-fix-02", "uses = ['Q-fix-01']");
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let a = key("Q-fix-01");
        let b = key("Q-fix-02");
        let seen = graph.neighborhood(&a, 10, |_| true);
        assert_eq!(seen, BTreeSet::from([a, b]));
    }

    /// `Supersession` class の循環は不正として、`validate` が報告する。
    #[test]
    fn supersession_の循環は不正として報告される() {
        let root = temp_root("supersession-cycle");
        write_widget(&root, "Q-fix-01", "replaces = ['Q-fix-02']");
        write_widget(&root, "Q-fix-02", "replaces = ['Q-fix-01']");
        let view = RepoView::working_tree(&root.0);
        let (graph, _issues) = SemanticGraph::build(&view, &fixture_registry());

        let a = key("Q-fix-01");
        let chain_err = graph.supersession_chain(&a).expect_err("循環している");
        assert!(matches!(*chain_err, GraphIssue::SupersessionCycle(_)));

        let issues = graph.validate();
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, GraphIssue::SupersessionCycle(_))),
            "{issues:?}"
        );
    }

    /// 循環していない supersession chain は、後継の順で並ぶ。
    #[test]
    fn supersession_chain_は後継の順で並ぶ() {
        let root = temp_root("supersession-chain");
        write_widget(&root, "Q-fix-01", "replaced_by = 'Q-fix-02'");
        write_widget(&root, "Q-fix-02", "replaced_by = 'Q-fix-03'");
        write_widget(&root, "Q-fix-03", "");
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let chain = graph
            .supersession_chain(&key("Q-fix-01"))
            .expect("循環していない");
        assert_eq!(
            chain,
            vec![key("Q-fix-01"), key("Q-fix-02"), key("Q-fix-03")]
        );
    }

    /// 本文が同じでも ID が違えば別の node になる。 identity は `key` だけが持つ。
    #[test]
    fn 同じ本文でも_id_が違えば別の_node_になる() {
        let root = temp_root("same-text-different-identity");
        write_widget(&root, "Q-fix-01", "");
        write(
            &root,
            "meta/questions/Q-fix-02.toml",
            &widget("Q-fix-01", "").replace("Q-fix-01", "Q-fix-02"),
        );
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let a = graph.node(&key("Q-fix-01")).expect("引ける");
        let b = graph.node(&key("Q-fix-02")).expect("引ける");
        assert_ne!(a.key, b.key);
        // 本文はほぼ同じ（id の差し替えだけ）でも、fingerprint は欄の値そのもの
        // （id・title を含む）から作るので、これも一致しない。
        assert_ne!(a.fingerprint, b.fingerprint);
    }

    /// 今の `SHAPES` に無い schema（旧い版の廃止済み schema）があっても、
    /// build 全体は止まらない。 診断へ積んで続ける。
    #[test]
    fn 知らない_schema_があっても_build_は続く() {
        let root = temp_root("old-schema");
        write_widget(&root, "Q-fix-01", "");
        write(
            &root,
            "meta/gone.toml",
            "schema = 'no-longer-registered'\nid = 'Q-fix-99'\n",
        );
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());

        assert_eq!(
            graph.nodes().count(),
            1,
            "廃止済み schema は node にならない"
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, GraphIssue::Load(m) if m.contains("知らない schema"))),
            "{issues:?}"
        );
    }

    /// 重複した ID も、diagnostic に積んで build は続く。
    #[test]
    fn 重複した_id_も診断に積んで続く() {
        let root = temp_root("duplicate-id");
        write(
            &root,
            "meta/requirements/dup.toml",
            "schema = 'requirement-set'\n\n[[requirement]]\nid = 'TR-fix-01'\ntitle = '1件目'\nconfidence = 'Fact'\nstatement = '…'\n\n[[requirement]]\nid = 'TR-fix-01'\ntitle = '重複'\nconfidence = 'Fact'\nstatement = '…'\n",
        );
        let view = RepoView::working_tree(&root.0);
        let registry = Registry {
            node_kinds: vec![NodeKind {
                schema: "requirement-set",
                kind: "requirement",
            }],
            ..Registry::default()
        };
        let (_graph, issues) = SemanticGraph::build(&view, &registry);
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, GraphIssue::Load(m) if m.contains("重複している"))),
            "{issues:?}"
        );
    }

    /// FSL の ID も、registry が望めば node になる。
    #[test]
    fn registry_が望めば_fsl_の_id_も_node_になる() {
        let root = temp_root("fsl-node");
        write_widget(&root, "Q-fix-01", "");
        fs::create_dir_all(root.0.join("specs")).expect("作れる");
        fs::write(
            root.0.join("specs/example.fsl"),
            "  acceptance AC-fix-001 \"見本\" {\n  }\n",
        )
        .expect("書ける");
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let fsl_key = key("AC-fix-001");
        let node = graph.node(&fsl_key).expect("FSL の node がある");
        assert_eq!(node.kind, "fsl-requirement");
        assert_eq!(
            node.provenance.path,
            Some(std::path::PathBuf::from("specs/example.fsl"))
        );
        assert_eq!(node.provenance.line, Some(1));
    }

    /// locator の欄は、ID として解決せずそのまま `Locator` node にする。
    #[test]
    fn locator_の欄は_id_解決せず_locator_node_になる() {
        let root = temp_root("locator");
        write_widget(
            &root,
            "Q-fix-01",
            "story = 'crates/koeru-app/ui/src/widget.story.tsx'",
        );
        let view = RepoView::working_tree(&root.0);
        let (graph, issues) = SemanticGraph::build(&view, &fixture_registry());
        assert!(issues.is_empty(), "{issues:?}");

        let locator = NodeKey::Locator {
            kind: "story",
            value: "crates/koeru-app/ui/src/widget.story.tsx".to_owned(),
        };
        let node = graph.node(&locator).expect("locator node がある");
        assert_eq!(node.kind, "story");
        let out = graph.outgoing(&key("Q-fix-01"));
        assert!(
            out.iter()
                .any(|r| r.to == locator && r.name == "context_for")
        );
    }

    /// node / edge の並びは、記録を読み込んだ順に依らず決定的になる。
    #[test]
    fn node_と_edge_の並びは決定的になる() {
        let root = temp_root("deterministic-a");
        write_widget(&root, "Q-fix-02", "uses = ['Q-fix-01', 'Q-fix-03']");
        write_widget(&root, "Q-fix-01", "");
        write_widget(&root, "Q-fix-03", "");
        let view = RepoView::working_tree(&root.0);
        let (g1, _) = SemanticGraph::build(&view, &fixture_registry());
        let (g2, _) = SemanticGraph::build(&view, &fixture_registry());

        let keys1: Vec<&NodeKey> = g1.nodes().map(|n| &n.key).collect();
        let keys2: Vec<&NodeKey> = g2.nodes().map(|n| &n.key).collect();
        assert_eq!(keys1, keys2);
        assert_eq!(
            keys1,
            vec![&key("Q-fix-01"), &key("Q-fix-02"), &key("Q-fix-03")]
        );
        let mid = key("Q-fix-02");
        let edges1: Vec<&NodeKey> = g1.outgoing(&mid).iter().map(|r| &r.to).collect();
        assert_eq!(edges1, vec![&key("Q-fix-01"), &key("Q-fix-03")]);
    }

    /// `load_view(working_tree(...))` と `load(...)` は、同じ fixture で
    /// 同じ `Entry`（path・schema・本文）を返す。
    ///
    /// 既存コマンドは `load` を呼ぶ経路のまま——版を選べる `load_view` に
    /// 載せ替えても、作業ツリーを渡したときの挙動が変わっていないことを
    /// ここで固定する。
    #[test]
    fn load_view_working_tree_は_load_と一致する() {
        let root = temp_root("load-parity");
        write_widget(&root, "Q-fix-01", "uses = ['Q-fix-02']");
        write_widget(&root, "Q-fix-02", "");

        let mut rep_a = Report::default();
        let via_load = knowledge::load(&root.0, &mut rep_a);
        let mut rep_b = Report::default();
        let via_view = knowledge::load_view(&RepoView::working_tree(&root.0), &mut rep_b);

        assert_eq!(rep_a.diagnostics(), rep_b.diagnostics());
        let simplify = |entries: &[knowledge::Entry]| -> Vec<(PathBuf, &'static str, String)> {
            entries
                .iter()
                .map(|e| (e.path.clone(), e.shape.schema, e.text.clone()))
                .collect()
        };
        assert_eq!(simplify(&via_load), simplify(&via_view));
    }
}
