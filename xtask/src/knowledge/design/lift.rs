//! legacy の meta と FSL を、target の graph が読める形にそろえる（D00）。
//!
//! やることは2つ。
//!
//! - FSL の contract を記録として加える。 正規の `formalizes` は contract から REQ へ向かう
//!   （提案書 5.2.2）が、今の meta は REQ の側が `formalized_as` で持つ。 graph engine の
//!   relation は記録が持つ欄から前向きにしか引けず、FSL の ID は記録ではないので、
//!   contract の側に `formalizes` を持つ記録を組み立てる。 meta のファイルは書き換えない
//! - 読んだものが target の意味で引けているかを検査する（[`Design::audit`]）
//!
//! engine 側に逆向きの規則（記録が持つ欄を、相手から相手への edge として読む）が入れば、
//! 組み立ては要らなくなる。 PR 本文の interface 変更の要求を見る。

use std::collections::BTreeMap;
use std::path::Path;

use super::legacy::{CONTRACT_SCHEMA, Object, Role, spec};
use super::ontology::{Kind, relation};
use crate::diagnostic::{Report, Severity};
use crate::graph::{GraphIssue, NodeKey, SemanticGraph};
use crate::knowledge::fsl::fsl_sites_view;
use crate::knowledge::model::{Entry, Shape, list_of, str_of};
use crate::knowledge::{Id, KnowledgeSnapshot, load_view};
use crate::repo::RepoView;

/// 組み立てる contract の形。 meta のファイルではないので、読み込みは通らない。
static CONTRACT_SHAPE: Shape = Shape {
    schema: CONTRACT_SCHEMA,
    dir: "specs",
    entity: Some(("", &["id"])),
    collection: None,
    entity_arrays: &[],
};

/// 読み込みと、target の意味で引く段で見つかった問題。 落とさず値で持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DesignIssue {
    /// 読み込みの診断（知らない schema・壊れた TOML・必須項目の欠け・ファイル名と ID の不一致）
    /// と、重複した ID。 `GraphIssue::Load` と同じ。
    Load(String),
    /// 記録が `id` を持つのに ID の形に読めない。 node にならず、graph からは消える。
    UnparsedId { schema: &'static str, raw: String },
    /// 関係の欄の値が ID の形に読めない。 edge にならず、graph からは消える。
    UnparsedReference {
        from: Id,
        field: &'static str,
        raw: String,
    },
    /// FSL の ID が ID の形に読めない。 contract の node にならない。
    UnparsedContract { raw: String },
    /// `formalized_as` が指す contract が FSL に無い。 `formalizes` の edge を作れない。
    MissingContract {
        requirement: String,
        contract: String,
    },
    /// この adapter に写し方が無い schema。 読み込みは通るのに target の意味で引けない。
    UnmappedSchema { schema: &'static str },
    /// 部品の採否の語が、知っている語彙に無い。
    UnknownStatus { id: Id, raw: String },
    /// 採用済みの判断の `selected` が、同じ ID のまま書き換わった。 意味を変えるときは、
    /// 上書きせず新しい判断を作って `supersedes` で繋ぐ。
    DecisionOverwritten { id: Id },
    /// 登録していない relation が graph にある。
    UnregisteredRelation { relation: &'static str },
    /// relation の両端の種類が、登録した組に入らない。
    EndpointKind {
        from: NodeKey,
        to: NodeKey,
        relation: &'static str,
        from_kind: Kind,
        to_kind: Kind,
    },
    /// graph の検査（宙に浮いた edge・後継の循環）。
    Graph(GraphIssue),
}

/// legacy の meta と FSL を、target の意味で読んだもの。
#[derive(Debug)]
pub(crate) struct Design {
    snapshot: KnowledgeSnapshot,
    issues: Vec<DesignIssue>,
}

impl Design {
    /// `view` の版を読む。 作業ツリーも、任意の版も同じ。
    pub(crate) fn load(view: &RepoView) -> Self {
        let mut rep = Report::default();
        let entries = load_view(view, &mut rep);
        let issues = rep
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| DesignIssue::Load(d.message.clone()))
            .collect();
        Self::assemble(view.root(), &entries, &fsl_sites_view(view), issues)
    }

    /// すでに読んだ記録と FSL の索引から作る。 読み込みの診断は呼び側が持っているので、
    /// ここでは足さない。
    pub(crate) fn from_entries(
        root: &Path,
        entries: &[Entry],
        fsl_sites: &BTreeMap<String, String>,
    ) -> Self {
        Self::assemble(root, entries, fsl_sites, Vec::new())
    }

    fn assemble(
        root: &Path,
        entries: &[Entry],
        fsl_sites: &BTreeMap<String, String>,
        mut issues: Vec<DesignIssue>,
    ) -> Self {
        let mut all: Vec<Entry> = entries.iter().map(duplicate).collect();
        all.extend(contract_entries(root, entries, fsl_sites, &mut issues));
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&all);
        issues.extend(dups.into_iter().map(DesignIssue::Load));
        Self { snapshot, issues }
    }

    /// target の語彙の graph。 node は [`Kind`] の名前、edge は [`super::RELATIONS`] の名前を持つ。
    ///
    /// 版の識別は持たない（`SemanticGraph::from_snapshot` が `revision` を埋めない）。
    pub(crate) fn graph(&self) -> SemanticGraph {
        SemanticGraph::from_snapshot(&self.snapshot, &super::registry())
    }

    /// target の意味で見た全件。 知らない schema・ID の形に読めない記録は含まない
    /// ——それは [`Self::audit`] が出す。
    pub(crate) fn objects(&self) -> impl Iterator<Item = Object<'_>> {
        self.snapshot.records().filter_map(Object::new)
    }

    pub(crate) fn object(&self, id: &Id) -> Option<Object<'_>> {
        self.snapshot.get(id).and_then(Object::new)
    }

    /// 読み込みと組み立てで出た問題だけ。 graph は作らない。
    pub(crate) fn issues(&self) -> &[DesignIssue] {
        &self.issues
    }

    /// すべての問題。 読み込み・組み立ての問題に、target の意味で引けているかの検査を足す。
    ///
    /// 黙って消えるものを数える——ID の形に読めない記録と参照は、graph には現れない。
    pub(crate) fn audit(&self) -> Vec<DesignIssue> {
        let mut out = self.issues.clone();
        self.audit_records(&mut out);
        let graph = self.graph();
        audit_relations(&graph, &mut out);
        out.extend(graph.validate().into_iter().map(DesignIssue::Graph));
        out
    }

    /// `self` を古い版、`new` を新しい版として、採用済みの判断が上書きされていないかを見る。
    ///
    /// 見るのは `selected` の文字列が変わったかだけで、意味が変わったかは判定できない。
    /// 誤字の修正も出る。 出たものを、人が新しい判断にするか直すかを決める。
    pub(crate) fn overwritten_decisions(&self, new: &Design) -> Vec<DesignIssue> {
        let mut out = Vec::new();
        for old in self
            .objects()
            .filter_map(|o| o.as_decision().map(|d| (o, d)))
        {
            let (object, decision) = old;
            if decision.status() != Some("accepted") {
                continue;
            }
            let Some(next) = new.object(object.id()).and_then(|o| o.as_decision()) else {
                continue;
            };
            if next.selected() != decision.selected() {
                out.push(DesignIssue::DecisionOverwritten {
                    id: object.id().clone(),
                });
            }
        }
        out
    }

    fn audit_records(&self, out: &mut Vec<DesignIssue>) {
        for r in self.snapshot.records() {
            let Some(spec) = spec(r.schema()) else {
                out.push(DesignIssue::UnmappedSchema { schema: r.schema() });
                continue;
            };
            if let (Some(raw), None) = (r.str("id"), r.id()) {
                out.push(DesignIssue::UnparsedId {
                    schema: r.schema(),
                    raw: raw.to_owned(),
                });
                continue;
            }
            let Some(id) = r.id() else { continue };
            for f in spec.fields {
                if !matches!(f.role, Role::Edge(_)) {
                    continue;
                }
                for raw in references(r, f.name) {
                    if raw.parse::<Id>().is_err() {
                        out.push(DesignIssue::UnparsedReference {
                            from: id.clone(),
                            field: f.name,
                            raw,
                        });
                    }
                }
            }
            if let Some(c) = Object::new(r).and_then(|o| o.as_component())
                && let Some(raw) = c.status()
                && c.adoption().is_none()
            {
                out.push(DesignIssue::UnknownStatus {
                    id: id.clone(),
                    raw: raw.to_owned(),
                });
            }
        }
    }
}

/// 欄の値を、単数・複数のどちらの形でも一覧として読む。 graph engine の読み方と同じ。
fn references(r: &crate::knowledge::Record, field: &str) -> Vec<String> {
    let many = r.strs(field);
    if many.is_empty() {
        r.str(field).map(|s| vec![s.to_owned()]).unwrap_or_default()
    } else {
        many
    }
}

fn audit_relations(graph: &SemanticGraph, out: &mut Vec<DesignIssue>) {
    for rel in graph.all_relations() {
        let Some(rule) = relation(rel.name) else {
            out.push(DesignIssue::UnregisteredRelation { relation: rel.name });
            continue;
        };
        let from = graph.node(&rel.from).and_then(Kind::of);
        // 行き先が無いのは宙に浮いた edge で、`graph.validate()` が出す。
        let to = graph.node(&rel.to).and_then(Kind::of);
        let (Some(from_kind), Some(to_kind)) = (from, to) else {
            continue;
        };
        if !rule.from.admits(from_kind) || !rule.to.admits(to_kind) {
            out.push(DesignIssue::EndpointKind {
                from: rel.from.clone(),
                to: rel.to.clone(),
                relation: rel.name,
                from_kind,
                to_kind,
            });
        }
    }
}

fn duplicate(e: &Entry) -> Entry {
    Entry {
        path: e.path.clone(),
        shape: e.shape,
        table: e.table.clone(),
        text: e.text.clone(),
    }
}

/// FSL の ID ごとに contract の記録を作る。
///
/// `formalizes` は、その contract を `formalized_as` で指している REQ の ID の一覧。
fn contract_entries(
    root: &Path,
    entries: &[Entry],
    fsl_sites: &BTreeMap<String, String>,
    issues: &mut Vec<DesignIssue>,
) -> Vec<Entry> {
    let mut formalized: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in entries
        .iter()
        .filter(|e| e.shape.schema == "requirement-set")
    {
        for item in e.items() {
            let Some(req) = str_of(&item, "id") else {
                continue;
            };
            for contract in list_of(&item, "formalized_as") {
                if !fsl_sites.contains_key(&contract) {
                    issues.push(DesignIssue::MissingContract {
                        requirement: req.to_owned(),
                        contract: contract.clone(),
                    });
                }
                formalized.entry(contract).or_default().push(req.to_owned());
            }
        }
    }

    let mut out = Vec::new();
    for (id, site) in fsl_sites {
        if id.parse::<Id>().is_err() {
            issues.push(DesignIssue::UnparsedContract { raw: id.clone() });
            continue;
        }
        let (rel, line) = match site.rsplit_once(':') {
            Some((rel, n)) => (rel, n.parse::<usize>().ok()),
            None => (site.as_str(), None),
        };
        let mut table = toml::Table::new();
        table.insert("id".to_owned(), toml::Value::String(id.clone()));
        table.insert("site".to_owned(), toml::Value::String(site.clone()));
        let reqs = formalized.remove(id).unwrap_or_default();
        table.insert(
            "formalizes".to_owned(),
            toml::Value::Array(reqs.into_iter().map(toml::Value::String).collect()),
        );
        // `id = '…'` の行番号を、FSL の宣言の行にそろえる。 記録の出どころが行まで引けるように。
        let pad = "\n".repeat(line.unwrap_or(1).saturating_sub(1));
        out.push(Entry {
            path: root.join(rel),
            shape: &CONTRACT_SHAPE,
            table,
            text: format!("{pad}id = '{id}'\n"),
        });
    }
    out
}
