//! Component Inventory（D07b）。 部品台帳の行を、repository・判断・調査に引き直して並べる。
//!
//! 台帳（`meta/evidence/components.toml`）は部品の正本ではない。 `CMP-*` を dependency の
//! 同一性として維持しない（実装計画 9.4）ので、この view は台帳の行ごとに次を値で出す。
//!
//! - package の存在と版 — manifest と lock から（[`manifest`]）。 台帳の記述は当てにしない
//! - 採否の理由 — `decided_by` が指す判断（[`Rationale`]）。 引けなければ戻り先が無いと出す
//! - license と測定 — その部品に触れている調査（[`EvidenceLink`]）
//!
//! 突き合わせの食い違いは [`Discrepancy`] の値で出すだけで、落とす・通すの判定はしない。
//! view は authority を持たない。
//!
//! 台帳の行と package を結ぶ鍵は、今の台帳に無い。 `name` は自由記述（`tracing ＋
//! tracing-subscriber`、`WAV 入出力の自前実装`）なので、名前の語と package の名前の一致だけで
//! 引く（[`name_keys`]）。 当て推量を広げない——一致しなければ見つからないと出す。

// 消費者（D07c・D08・D10）が付くまで、production からは呼ばれない。 再輸出も同じ。
#![allow(dead_code, unused_imports)]

mod manifest;

use std::collections::BTreeSet;

pub(crate) use manifest::{
    Declaration, Ecosystem, IssueReason, ManifestIssue, Manifests, Package, PackageRef, Section,
};

use crate::graph::{NodeKey, SemanticGraph};
use crate::knowledge::Id;
use crate::knowledge::design::{Adoption, Design, Kind};
use crate::repo::RepoView;

/// 台帳の行から見た、package の有無。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Presence {
    /// manifest が宣言している（直接の依存）。
    Declared,
    /// lock にはあるが、宣言していない（推移的な依存）。
    LockedOnly,
    /// どこにも無い。 名前の語に一致する package が無いことも含む。
    Absent,
}

/// 採否の理由の戻り先。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Rationale {
    /// `decided_by` が実在する判断を指す。
    Decision(Id),
    /// 指す先は実在するが、判断ではない。
    NotDecision { id: Id, kind: Kind },
    /// 指す先が実在しない、または ID の形ではない。
    Unresolved(String),
    /// `decided_by` が無い。 戻り先が無い。
    NoneRecorded,
}

/// 調査がこの部品に触れている経路。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum EvidenceVia {
    /// 調査が `affects_components` でこの部品を指している。
    AffectsComponent,
    /// 採否を決めた判断が、この調査を `considers` している。
    ConsideredByDecision(Id),
}

/// この部品の license・測定の戻り先。 どれが license でどれが測定かは `kind` が言う。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct EvidenceLink {
    pub(crate) evidence: Id,
    /// 調査の種類（`license_audit` / `measurement` など）。
    pub(crate) kind: Option<String>,
    pub(crate) via: EvidenceVia,
}

/// 台帳の1行を引き直したもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentEntry {
    pub(crate) id: Id,
    pub(crate) name: Option<String>,
    /// 台帳の採否の語。 `None` は欄が無い。
    pub(crate) status: Option<String>,
    /// `status` が知っている語のとき。 知らない語は `None` のまま（`Design::audit` が出す）。
    pub(crate) adoption: Option<Adoption>,
    /// 名前の語が一致した package。 全 ecosystem を探す。
    pub(crate) packages: Vec<PackageRef>,
    pub(crate) presence: Presence,
    /// 名前に書かれていた版（`v3.0.0`、`1.98`）。 repository の版との突き合わせに使う。
    pub(crate) ledger_versions: Vec<String>,
    pub(crate) rationale: Rationale,
    pub(crate) evidence: Vec<EvidenceLink>,
}

/// 突き合わせの食い違い。 値で出すだけで、合否は持たない。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Discrepancy {
    /// 採用の部品が、manifest にも lock にも見つからない。 外部の package ではない部品
    /// （自前の実装、OS の API、フォント）も、鍵が無いので同じ形で出る。
    AdoptedNotInManifest { component: Id },
    /// manifest が宣言している package を、台帳のどの行も指していない。
    DeclaredNotInLedger { package: PackageRef },
    /// 不適・不採用と台帳が言う部品を、manifest が宣言している。
    RejectedButDeclared { component: Id, package: PackageRef },
    /// 台帳の名前に書かれた版が、repository の版のどれとも合わない。 `repository` が空の
    /// package（submodule）は、比べられないので出さない。
    VersionDiffers {
        component: Id,
        package: PackageRef,
        ledger: Vec<String>,
        repository: Vec<String>,
    },
}

/// 台帳と repository を突き合わせた結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ComponentInventory {
    /// 台帳の行ごと。 ID 順。
    pub(crate) components: Vec<ComponentEntry>,
    /// repository から得た package。 台帳の記述に依らない。
    pub(crate) packages: Vec<Package>,
    pub(crate) discrepancies: Vec<Discrepancy>,
    /// 読んだ manifest・lock。
    pub(crate) sources: Vec<String>,
    /// 読めなかった manifest・lock。 これがあると、`Absent` は「無い」ではなく「読めていない」
    /// かもしれない。
    pub(crate) manifest_issues: Vec<ManifestIssue>,
}

impl ComponentInventory {
    /// `repo` の manifest・lock と、`design` の台帳・判断・調査から作る。
    ///
    /// `graph` は `design.graph()`。 呼び側が1度作って、同じ版の他の view と共有する。
    pub(crate) fn build(design: &Design, graph: &SemanticGraph, repo: &RepoView) -> Self {
        let manifests = Manifests::read(repo);
        let index = PackageIndex::new(&manifests.packages);

        let mut components = Vec::new();
        let mut linked: BTreeSet<&PackageRef> = BTreeSet::new();
        for object in design.objects() {
            let Some(component) = object.as_component() else {
                continue;
            };
            let name = component.name();
            let (keys, ledger_versions) = name_keys(name.unwrap_or_default());
            let found = index.find(&keys);
            linked.extend(found.iter().map(|p| &p.package));
            let presence = if found.iter().any(|p| p.is_declared()) {
                Presence::Declared
            } else if found.is_empty() {
                Presence::Absent
            } else {
                Presence::LockedOnly
            };
            let rationale = rationale_of(design, component.decided_by());
            let evidence = evidence_of(design, graph, object.id(), &rationale);
            components.push(ComponentEntry {
                id: object.id().clone(),
                name: name.map(str::to_owned),
                status: component.status().map(str::to_owned),
                adoption: component.adoption(),
                packages: found.iter().map(|p| p.package.clone()).collect(),
                presence,
                ledger_versions,
                rationale,
                evidence,
            });
        }
        components.sort_by(|a, b| a.id.cmp(&b.id));

        let mut discrepancies = Vec::new();
        for entry in &components {
            discrepancies.extend(entry_discrepancies(entry, &manifests.packages));
        }
        discrepancies.extend(
            manifests
                .packages
                .iter()
                .filter(|p| p.is_declared() && !linked.contains(&p.package))
                .map(|p| Discrepancy::DeclaredNotInLedger {
                    package: p.package.clone(),
                }),
        );
        discrepancies.sort();

        Self {
            components,
            packages: manifests.packages,
            discrepancies,
            sources: manifests.sources,
            manifest_issues: manifests.issues,
        }
    }
}

fn entry_discrepancies(entry: &ComponentEntry, packages: &[Package]) -> Vec<Discrepancy> {
    let mut out = Vec::new();
    match entry.adoption {
        Some(Adoption::Adopted) if entry.presence == Presence::Absent => {
            out.push(Discrepancy::AdoptedNotInManifest {
                component: entry.id.clone(),
            });
        }
        Some(Adoption::Unsuitable | Adoption::Rejected) => {
            for package in entry
                .packages
                .iter()
                .filter(|r| packages.iter().any(|p| &p.package == *r && p.is_declared()))
            {
                out.push(Discrepancy::RejectedButDeclared {
                    component: entry.id.clone(),
                    package: package.clone(),
                });
            }
        }
        _ => {}
    }
    if entry.ledger_versions.is_empty() {
        return out;
    }
    for package in packages
        .iter()
        .filter(|p| entry.packages.contains(&p.package))
    {
        // 版を持たない package（submodule）は比べられない。
        if package.locked_versions.is_empty() {
            continue;
        }
        let agrees = entry
            .ledger_versions
            .iter()
            .any(|l| package.locked_versions.iter().any(|r| version_covers(l, r)));
        if !agrees {
            out.push(Discrepancy::VersionDiffers {
                component: entry.id.clone(),
                package: package.package.clone(),
                ledger: entry.ledger_versions.clone(),
                repository: package.locked_versions.clone(),
            });
        }
    }
    out
}

/// 台帳が書いた版 `ledger` が、repository の版 `repository` と同じか、その上位の書き方
/// （`1.98` と `1.98.1`）か。
fn version_covers(ledger: &str, repository: &str) -> bool {
    repository == ledger
        || repository
            .strip_prefix(ledger)
            .is_some_and(|rest| rest.starts_with('.'))
}

fn rationale_of(design: &Design, raw: Option<&str>) -> Rationale {
    let Some(raw) = raw else {
        return Rationale::NoneRecorded;
    };
    let Ok(id) = raw.parse::<Id>() else {
        return Rationale::Unresolved(raw.to_owned());
    };
    match design.object(&id) {
        None => Rationale::Unresolved(raw.to_owned()),
        Some(object) if object.as_decision().is_some() => Rationale::Decision(id),
        Some(object) => Rationale::NotDecision {
            id,
            kind: object.kind(),
        },
    }
}

fn object_id(key: &NodeKey) -> Option<&Id> {
    match key {
        NodeKey::Object(id) => Some(id),
        NodeKey::Locator { .. } => None,
    }
}

fn kind_at(graph: &SemanticGraph, key: &NodeKey) -> Option<Kind> {
    graph.node(key).and_then(Kind::of)
}

fn evidence_of(
    design: &Design,
    graph: &SemanticGraph,
    component: &Id,
    rationale: &Rationale,
) -> Vec<EvidenceLink> {
    let kind_of = |id: &Id| {
        design
            .object(id)
            .and_then(|o| o.as_evidence())
            .and_then(|e| e.kind().map(str::to_owned))
    };
    let mut out = BTreeSet::new();
    for rel in graph.incoming(&NodeKey::Object(component.clone())) {
        if rel.name == "affects"
            && kind_at(graph, &rel.from) == Some(Kind::Evidence)
            && let Some(evidence) = object_id(&rel.from)
        {
            out.insert(EvidenceLink {
                evidence: evidence.clone(),
                kind: kind_of(evidence),
                via: EvidenceVia::AffectsComponent,
            });
        }
    }
    if let Rationale::Decision(decision) = rationale {
        for rel in graph.outgoing(&NodeKey::Object(decision.clone())) {
            if rel.name == "considers"
                && kind_at(graph, &rel.to) == Some(Kind::Evidence)
                && let Some(evidence) = object_id(&rel.to)
            {
                out.insert(EvidenceLink {
                    evidence: evidence.clone(),
                    kind: kind_of(evidence),
                    via: EvidenceVia::ConsideredByDecision(decision.clone()),
                });
            }
        }
    }
    out.into_iter().collect()
}

/// 照合に使う名前の索引。 全 ecosystem をまたぐ。
struct PackageIndex<'a> {
    packages: &'a [Package],
}

impl<'a> PackageIndex<'a> {
    fn new(packages: &'a [Package]) -> Self {
        Self { packages }
    }

    /// 名前が `keys` のどれかに一致する package。 重複なし。
    fn find(&self, keys: &BTreeSet<String>) -> Vec<&'a Package> {
        self.packages
            .iter()
            .filter(|p| keys.contains(&manifest::normalize(&p.package.name)))
            .collect()
    }
}

/// 台帳の `name` から、package の名前として照合する語と、名前に書かれた版を取り出す。
///
/// 語の切り方は決め打ちで、台帳の欄が package の鍵を持つようになるまでの代わり。 全角の括弧と
/// `＋` で区切り、空白で語に分ける。 スコープ付きの名前（`@shadcn/lint`）は崩さず、それ以外の
/// `/` は区切る。 隣り合う2語は、つなげた形（`Tailwind CSS` → `tailwindcss`）も試す。
/// 版は `3.0.0` のように `.` を含む数字の並び（先頭の `v` は許す）。
pub(super) fn name_keys(name: &str) -> (BTreeSet<String>, Vec<String>) {
    let mut keys = BTreeSet::new();
    let mut versions = Vec::new();
    let separators = ['＋', '+', '（', '）', '(', ')', '、', ',', '・', '「', '」'];
    let words: Vec<String> = name
        .split(|c: char| c.is_whitespace() || separators.contains(&c))
        .flat_map(|w| -> Vec<&str> {
            if w.starts_with('@') {
                vec![w]
            } else {
                w.split('/').collect()
            }
        })
        .map(|w| w.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || c == '@')))
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect();

    let mut add = |word: &str| {
        if !word.bytes().any(|b| b.is_ascii_alphanumeric()) {
            return;
        }
        if is_version(word) {
            let version = word.trim_start_matches(['v', 'V']).to_owned();
            if !versions.contains(&version) {
                versions.push(version);
            }
        } else {
            keys.insert(manifest::normalize(word));
        }
    };
    for word in &words {
        add(word);
    }
    for pair in words.windows(2) {
        add(&format!("{}{}", pair[0], pair[1]));
        add(&format!("{}-{}", pair[0], pair[1]));
    }
    (keys, versions)
}

/// `3.0.0`・`v1.98`。 `.` を1つ以上含む数字の並び。
fn is_version(word: &str) -> bool {
    let body = word.trim_start_matches(['v', 'V']);
    body.contains('.')
        && body
            .split('.')
            .all(|part| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()))
}

#[cfg(test)]
mod tests;
