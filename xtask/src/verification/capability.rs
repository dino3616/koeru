//! Evidence capability の表（提案書 10.1.1）。 `meta/verification/capabilities.toml`。
//!
//! 観測方法が、その種類の仮説について証言する資格を持つか、だけを答える。 観察の内容の真偽は
//! 認定しない。 表に無い組は `context-dependent` と同じに扱い、自動では支持にしない。
//! 必要条件を機械が読める形で持たないので、`context-dependent` も昇格させない。

use std::collections::{BTreeMap, BTreeSet};

use crate::diagnostic::Location;
use crate::knowledge::KnowledgeSnapshot;

/// 表の schema 名。
pub(crate) const SCHEMA: &str = "evidence-capability";

/// 表の1行が言うこと。
// 変種の名前は、表の語（`can-support` / `cannot-support`）をそのまま写す。 語から離すと、
// 表と型を突き合わせて読むときに引き直しが要る。
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Support {
    CanSupport,
    CannotSupport,
    HypothesisGenerationOnly,
    ContextDependent,
}

impl Support {
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "can-support" => Self::CanSupport,
            "cannot-support" => Self::CannotSupport,
            "hypothesis-generation-only" => Self::HypothesisGenerationOnly,
            "context-dependent" => Self::ContextDependent,
            _ => return None,
        })
    }
}

/// 観察する主体。 `can-support` の行にだけ付く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Observer {
    Human,
    Automated,
}

impl Observer {
    fn parse(raw: &str) -> Option<Self> {
        match raw {
            "human" => Some(Self::Human),
            "automated" => Some(Self::Automated),
            _ => None,
        }
    }
}

/// 表の1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Row {
    pub(crate) hypothesis_kind: String,
    pub(crate) method: String,
    pub(crate) support: Support,
    pub(crate) observer: Option<Observer>,
    /// 提案書のどのセルから写したか。
    pub(crate) source: String,
    pub(crate) place: Option<Location>,
}

/// ある（仮説の種類、観測方法）の組が、仮説を支持できるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Capability {
    CanSupport,
    CannotSupport,
    HypothesisGenerationOnly,
    /// 必要条件を機械が読めないので、昇格させない。
    ContextDependent,
    /// 表に行が無い。 表に書いていない組を、推測で埋めない。
    Unregistered,
    /// 表に無い仮説の種類。
    UnknownKind,
    /// 表に無い観測方法。
    UnknownMethod,
}

impl Capability {
    /// 仮説を支持できるのは、`can-support` だけ。
    pub(crate) fn supports(self) -> bool {
        self == Self::CanSupport
    }
}

/// 表を読むときに見つかった問題。 表そのものの誤り。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RegistryIssue {
    /// 文字列であるべき欄が、無いか文字列でない。
    MissingField {
        field: &'static str,
        place: Option<Location>,
    },
    /// `support` が4つの語のどれでもない。
    UnknownSupport {
        value: String,
        place: Option<Location>,
    },
    /// `observer` が `human` / `automated` のどちらでもない。
    UnknownObserver {
        value: String,
        place: Option<Location>,
    },
    /// 同じ（仮説の種類、観測方法）の行が2つある。
    DuplicatePair {
        hypothesis_kind: String,
        method: String,
        place: Option<Location>,
    },
    /// 同じ観測方法の `observer` が、行によって違う。
    ObserverConflict {
        method: String,
        place: Option<Location>,
    },
}

/// 読んだ表。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct CapabilityRegistry {
    rows: Vec<Row>,
}

impl CapabilityRegistry {
    /// snapshot の `evidence-capability` の行から読む。 読めない行は表に入れず、問題として返す
    /// ——黙って減らさない。
    pub(crate) fn from_snapshot(snapshot: &KnowledgeSnapshot) -> (Self, Vec<RegistryIssue>) {
        let mut rows: Vec<Row> = Vec::new();
        let mut issues = Vec::new();
        for r in snapshot.by_schema(SCHEMA) {
            let place = Some(Location::new(r.provenance().path(), r.provenance().line()));
            let text = |field: &'static str, issues: &mut Vec<RegistryIssue>| {
                let value = r.str(field).map(str::to_owned);
                if value.is_none() {
                    issues.push(RegistryIssue::MissingField {
                        field,
                        place: place.clone(),
                    });
                }
                value
            };
            let kind = text("hypothesis_kind", &mut issues);
            let method = text("method", &mut issues);
            let support = text("support", &mut issues);
            let source = text("source", &mut issues);
            let (Some(hypothesis_kind), Some(method), Some(support), Some(source)) =
                (kind, method, support, source)
            else {
                continue;
            };
            let Some(support) = Support::parse(&support) else {
                issues.push(RegistryIssue::UnknownSupport {
                    value: support,
                    place,
                });
                continue;
            };
            let observer = match r.str("observer") {
                None => None,
                Some(raw) => {
                    let parsed = Observer::parse(raw);
                    if parsed.is_none() {
                        issues.push(RegistryIssue::UnknownObserver {
                            value: raw.to_owned(),
                            place: place.clone(),
                        });
                    }
                    parsed
                }
            };
            if rows
                .iter()
                .any(|p| p.hypothesis_kind == hypothesis_kind && p.method == method)
            {
                issues.push(RegistryIssue::DuplicatePair {
                    hypothesis_kind: hypothesis_kind.clone(),
                    method: method.clone(),
                    place: place.clone(),
                });
            }
            rows.push(Row {
                hypothesis_kind,
                method,
                support,
                observer,
                source,
                place,
            });
        }
        issues.extend(observer_conflicts(&rows));
        (Self { rows }, issues)
    }

    pub(crate) fn rows(&self) -> &[Row] {
        &self.rows
    }

    pub(crate) fn kinds(&self) -> BTreeSet<&str> {
        self.rows
            .iter()
            .map(|r| r.hypothesis_kind.as_str())
            .collect()
    }

    pub(crate) fn methods(&self) -> BTreeSet<&str> {
        self.rows.iter().map(|r| r.method.as_str()).collect()
    }

    pub(crate) fn has_kind(&self, kind: &str) -> bool {
        self.rows.iter().any(|r| r.hypothesis_kind == kind)
    }

    pub(crate) fn has_method(&self, method: &str) -> bool {
        self.rows.iter().any(|r| r.method == method)
    }

    /// `method` が `kind` の仮説を支持できるか。
    ///
    /// 同じ組の行が複数あって食い違うとき（表の誤りとして別に出る）は、支持にならない側に倒す。
    pub(crate) fn assess(&self, kind: &str, method: &str) -> Capability {
        if !self.has_kind(kind) {
            return Capability::UnknownKind;
        }
        if !self.has_method(method) {
            return Capability::UnknownMethod;
        }
        let supports: Vec<Support> = self
            .rows
            .iter()
            .filter(|r| r.hypothesis_kind == kind && r.method == method)
            .map(|r| r.support)
            .collect();
        match supports
            .iter()
            .find(|s| **s != Support::CanSupport)
            .or_else(|| supports.first())
        {
            None => Capability::Unregistered,
            Some(Support::CanSupport) => Capability::CanSupport,
            Some(Support::CannotSupport) => Capability::CannotSupport,
            Some(Support::HypothesisGenerationOnly) => Capability::HypothesisGenerationOnly,
            Some(Support::ContextDependent) => Capability::ContextDependent,
        }
    }

    /// `kind` の仮説を支持できる観測方法。 表の並びのまま。
    pub(crate) fn supporting_methods(&self, kind: &str) -> Vec<&Row> {
        self.rows
            .iter()
            .filter(|r| r.hypothesis_kind == kind && r.support == Support::CanSupport)
            .collect()
    }

    /// `method` が `kind` の仮説を支持でき、しかも人の観察である。
    pub(crate) fn is_human_support(&self, kind: &str, method: &str) -> bool {
        self.supporting_methods(kind)
            .iter()
            .any(|r| r.method == method && r.observer == Some(Observer::Human))
    }

    /// 支持できる観測方法が、すべて人の観察である。 そうなら、その方法の一覧。
    ///
    /// 支持できる方法が1つも無い種類と、`observer` を付けていない方法を含む種類は `None`
    /// ——人の観察だけとは言えない。 表が言っていないことを、推測で人の観察だけにしない。
    pub(crate) fn human_only(&self, kind: &str) -> Option<Vec<&str>> {
        let methods = self.supporting_methods(kind);
        if methods.is_empty() || methods.iter().any(|r| r.observer != Some(Observer::Human)) {
            return None;
        }
        Some(methods.iter().map(|r| r.method.as_str()).collect())
    }
}

/// 同じ観測方法の `observer` が行によって違うもの。 表の並びで最初に食い違った行を指す。
fn observer_conflicts(rows: &[Row]) -> Vec<RegistryIssue> {
    let mut seen: BTreeMap<&str, Observer> = BTreeMap::new();
    let mut reported: BTreeSet<&str> = BTreeSet::new();
    let mut out = Vec::new();
    for row in rows {
        let Some(observer) = row.observer else {
            continue;
        };
        match seen.get(row.method.as_str()) {
            None => {
                seen.insert(&row.method, observer);
            }
            Some(first) if *first != observer && reported.insert(&row.method) => {
                out.push(RegistryIssue::ObserverConflict {
                    method: row.method.clone(),
                    place: row.place.clone(),
                });
            }
            Some(_) => {}
        }
    }
    out
}
