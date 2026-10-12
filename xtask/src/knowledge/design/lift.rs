//! legacy の meta と FSL を、target の graph が読める形にそろえる（D00）。
//!
//! やることは2つ。
//!
//! - 記録の欄から、正規の relation の edge を作れる形に組み立てる（`DEC-PLT-048`）。
//!   組み立てるのは読んだ記録の写しで、meta のファイルは書き換えない
//!   - FSL の contract を記録として加える（id と宣言の場所だけ。 FSL の ID は meta の記録では
//!     ないが、node と出どころを持たせるため）。 REQ との関係（`formalizes`、contract から REQ。
//!     提案書 5.2.2）は REQ の `formalized_as` を逆向きに読む規則が作る
//!   - 部品の `supports_requirements` を、採用なら `implements_requirements`、それ以外なら
//!     `unadopted_requirements` に振り分ける
//!   - 試験の `contracts` を、要件を指すものは `targets_requirements`、残りは
//!     `other_contracts` に振り分ける
//! - 読んだものが target の意味で引けているかを検査する（[`Design::audit`]）
//!
//! 組み立てた欄は、`lift` が加える欄で、meta のファイルには現れない。 逆向きの規則で読めるものは
//! 欄を足さない——相手の記録の内容の digest が、書かれていない欄で変わらないようにするため。
//!
//! 値からの振り分け（`implements`・`targets`）は、engine が値から relation を決められるように
//! なるまで、ここで組み立てる（X05 への要求。 D00 の後の作業）。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::identity::{IdentityError, TargetId};
use super::legacy::{Adoption, CONTRACT_SCHEMA, Object, Role, spec};
use super::ontology::{Kind, relation};
use super::shapes::{TARGET_SHAPES, is_target_only};
use crate::diagnostic::{Report, Severity};
use crate::graph::{GraphIssue, NodeKey, SemanticGraph};
use crate::knowledge::fsl::fsl_sites_view;
use crate::knowledge::model::{Entry, Shape, list_of, str_of};
use crate::knowledge::{Id, KnowledgeSnapshot, load_view_with};
use crate::repo::{RepoView, RevisionRef};

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
    /// `formalized_as` が指す contract が FSL に無い。 **今は出さない。** 逆向きの規則で読むように
    /// なってから、宙に浮いた edge（`Graph(GraphIssue::Dangling)`）として出る。 互換のために残す。
    MissingContract {
        requirement: String,
        contract: String,
    },
    /// 欄の値が指す記録が無く、組み立てられなかった。 **今は出さない。** `supports_decisions` が
    /// 指す判断が無いときは、`Graph(GraphIssue::Dangling)` として出る。 互換のために残す。
    MissingLiftTarget {
        source: String,
        field: &'static str,
        target: String,
    },
    /// この adapter に写し方が無い schema。 読み込みは通るのに target の意味で引けない。
    UnmappedSchema { schema: &'static str },
    /// 部品の採否の語が、知っている語彙に無い。
    UnknownStatus { id: Id, raw: String },
    /// 採用済みの判断の `selected` が、同じ ID のまま書き換わった。 意味を変えるときは、
    /// 上書きせず新しい判断を作って `supersedes` で繋ぐ。
    DecisionOverwritten { id: Id },
    /// 目標の形の ID（`<接頭辞>-<opaque>`）が、記録の種類と合わない。 接頭辞が別の種類を言って
    /// いる（質問の記録に `DEC-…`）、または目標の形でだけ書く schema なのに ID が目標の形でない。
    TargetIdentity { id: Id, error: IdentityError },
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
    /// 読んだ版。 [`Self::load`] が view から取る。 版を持たない view（メモリ上）や
    /// [`Self::from_entries`] では無く、[`Self::with_revision`] で足せる。
    revision: Option<RevisionRef>,
}

impl Design {
    /// `view` の版を読む。 作業ツリーも、任意の版も同じ。
    pub(crate) fn load(view: &RepoView) -> Self {
        let mut rep = Report::default();
        // 目標の schema（`TARGET_SHAPES`）も読む。 今の meta には無いので、読む結果は変わらない。
        let entries = load_view_with(view, TARGET_SHAPES, &mut rep);
        let issues = rep
            .diagnostics()
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .map(|d| DesignIssue::Load(d.message.clone()))
            .collect();
        let mut design = Self::assemble(view.root(), &entries, &fsl_sites_view(view), issues);
        // 版が分からなくても致命的ではない（`SemanticGraph::build` と同じ扱い）。
        design.revision = view.identity().ok();
        design
    }

    /// 読んだ版を持たせる。 版を持たない view から読んだものに、呼び出し側が分かっている版を足す。
    pub(crate) fn with_revision(mut self, revision: RevisionRef) -> Self {
        self.revision = Some(revision);
        self
    }

    pub(crate) fn revision(&self) -> Option<&RevisionRef> {
        self.revision.as_ref()
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
        lift_components(&mut all);
        lift_suites(&mut all);
        all.extend(contract_entries(root, fsl_sites, &mut issues));
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&all);
        issues.extend(dups.into_iter().map(DesignIssue::Load));
        Self {
            snapshot,
            issues,
            revision: None,
        }
    }

    /// target の語彙の graph。 node は [`Kind`] の名前、edge は [`super::RELATIONS`] の名前を持つ。
    ///
    /// 版（[`Self::revision`]）を持つ。 版を持たない view から読んだものは `None`。
    pub(crate) fn graph(&self) -> SemanticGraph {
        SemanticGraph::from_snapshot_at(&self.snapshot, &super::registry(), self.revision.clone())
    }

    /// target の意味で見た全件。 知らない schema・ID の形に読めない記録は含まない
    /// ——それは [`Self::audit`] が出す。
    pub(crate) fn objects(&self) -> impl Iterator<Item = Object<'_>> {
        self.snapshot
            .records()
            .filter_map(|r| Object::in_snapshot(r, &self.snapshot))
    }

    pub(crate) fn object(&self, id: &Id) -> Option<Object<'_>> {
        self.snapshot
            .get(id)
            .and_then(|r| Object::in_snapshot(r, &self.snapshot))
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
    /// 誤字の修正も出る。 「意味を変えない修正」と明示した変更を通すには、判断に編集の記録の欄が
    /// 要る。 欄の追加は X03 の `SHAPES` に触るので、D00 の後の作業で行う（`DEC-PLT-048`）。
    /// それまでは、出たものを人が新しい判断にするか直すかを決める。
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
            if id.is_target() || is_target_only(r.schema()) {
                match id.as_str().parse::<TargetId>() {
                    Ok(t) if t.kind() == spec.kind => {}
                    Ok(_) => out.push(DesignIssue::TargetIdentity {
                        id: id.clone(),
                        error: IdentityError::PrefixMismatch,
                    }),
                    Err(error) => out.push(DesignIssue::TargetIdentity {
                        id: id.clone(),
                        error,
                    }),
                }
            }
            for f in spec.fields {
                // 組み立てる欄と逆向きに読む欄も、値が ID の形に読めないと edge にならない。
                if !matches!(f.role, Role::Edge(_) | Role::Lifted(_) | Role::Inverse(_)) {
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

/// 収集ファイルの項目の表を、書き換えられる形で順に渡す。
fn for_each_item(e: &mut Entry, mut f: impl FnMut(&mut toml::Table)) {
    let Some((key, _, _)) = e.shape.collection else {
        return;
    };
    let Some(items) = e.table.get_mut(key).and_then(toml::Value::as_array_mut) else {
        return;
    };
    for item in items.iter_mut().filter_map(toml::Value::as_table_mut) {
        f(item);
    }
}

fn string_array(values: Vec<String>) -> toml::Value {
    toml::Value::Array(values.into_iter().map(toml::Value::String).collect())
}

/// 部品の `supports_requirements` を、採用なら `implements_requirements`、それ以外
/// （知らない採否の語を含む）なら `unadopted_requirements` に振り分ける（`DEC-PLT-048`）。
fn lift_components(all: &mut [Entry]) {
    for e in all
        .iter_mut()
        .filter(|e| e.shape.schema == "component-ledger")
    {
        for_each_item(e, |item| {
            let reqs = list_of(item, "supports_requirements");
            if reqs.is_empty() {
                return;
            }
            let adopted =
                str_of(item, "status").and_then(Adoption::parse) == Some(Adoption::Adopted);
            let field = if adopted {
                "implements_requirements"
            } else {
                "unadopted_requirements"
            };
            item.insert(field.to_owned(), string_array(reqs));
        });
    }
}

/// 試験の `contracts` を、要件を指すものは `targets_requirements`、残りは `other_contracts` に
/// 振り分ける（`DEC-PLT-048`）。 要件は、技術要件と、定量の要件（上限）の両方。
fn lift_suites(all: &mut [Entry]) {
    let mut requirements: BTreeSet<String> = BTreeSet::new();
    for e in all.iter() {
        match e.shape.schema {
            "requirement-set" => requirements.extend(
                e.items()
                    .iter()
                    .filter_map(|i| str_of(i, "id").map(str::to_owned)),
            ),
            "budget" | "requirement" => {
                requirements.extend(str_of(&e.table, "id").map(str::to_owned));
            }
            _ => {}
        }
    }
    for e in all
        .iter_mut()
        .filter(|e| e.shape.schema == "test-portfolio")
    {
        for_each_item(e, |item| {
            let (targets, others): (Vec<String>, Vec<String>) = list_of(item, "contracts")
                .into_iter()
                .partition(|c| requirements.contains(c));
            if !targets.is_empty() {
                item.insert("targets_requirements".to_owned(), string_array(targets));
            }
            if !others.is_empty() {
                item.insert("other_contracts".to_owned(), string_array(others));
            }
        });
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

/// FSL の ID ごとに contract の記録を作る。 持つのは id と宣言の場所だけ。
///
/// REQ との関係（`formalizes`）は記録に持たせない。 REQ の `formalized_as` を逆向きの規則で読む。
fn contract_entries(
    root: &Path,
    fsl_sites: &BTreeMap<String, String>,
    issues: &mut Vec<DesignIssue>,
) -> Vec<Entry> {
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
