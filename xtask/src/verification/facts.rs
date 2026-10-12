//! 検証の意味づけが読む記録（仮説・観測・Probe・判断）を、snapshot から読む。
//!
//! 読むのは欄の値だけで、仮説の真偽（`assessment`）を決める口は無い。 `assessment` は人が
//! 書いたものを読み取り専用で持つ。 legacy の接頭辞や TOML の並びは、ここより外へ出さない。
//!
//! 欄が無いものは `None` / 空のまま持つ。 欄が無い調査（今の `EVID-*`）は、判定の対象にならない
//! ことと、書き忘れたこととを、ここでは区別しない。 区別は検査（[`super::check_verification`]）が、
//! 判定に要る欄がそろった組にだけ規則を掛けることで付ける。

use crate::diagnostic::Location;
use crate::knowledge::{KnowledgeSnapshot, Record};
use crate::probe::{Prerequisites, ProbeDefinition};

/// 仮説。 schema `design-hypothesis`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hypothesis {
    pub(crate) id: String,
    pub(crate) place: Option<Location>,
    pub(crate) kind: Option<String>,
    pub(crate) supporting: Vec<String>,
    pub(crate) contradicting: Vec<String>,
    /// 人が書いた現在の状態（`untested` / `supported` など）。 読むだけで、ここからは書き換えない。
    pub(crate) assessment: Option<String>,
}

/// 観測としての調査。 schema `evidence`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observation {
    pub(crate) id: String,
    pub(crate) place: Option<Location>,
    /// 観測方法。 無い調査は、観測として書かれていない（今の `EVID-*`）。
    pub(crate) method: Option<String>,
    pub(crate) assesses: Vec<String>,
    pub(crate) origin_group: Option<String>,
    pub(crate) derived_from: Vec<String>,
}

/// 繰り返し使う検証方法の定義。 schema `probe`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeSpec {
    pub(crate) id: String,
    pub(crate) place: Option<Location>,
    pub(crate) targets: Vec<String>,
    pub(crate) method: Option<String>,
    pub(crate) requires_human: bool,
    pub(crate) requires_native: bool,
    pub(crate) runner: Option<String>,
    pub(crate) package: Option<String>,
    pub(crate) target: Option<String>,
    pub(crate) platforms: Vec<String>,
    pub(crate) backends: Vec<String>,
    pub(crate) min_cases: Option<u64>,
    pub(crate) fixtures: Vec<String>,
}

impl ProbeSpec {
    /// 実行の契約を、X04 の `ProbeDefinition` として渡す。 契約が書かれていない Probe
    /// （人の観察の手順など）は `None`——走らせる runner が無いので、Receipt も作られない。
    ///
    /// `backends` を書かないときは、今の試験の登録と同じ既定（`native` と `unsupported`）。
    /// 走らせる runner と件数の下限（`runner`・`package`・`target`・`min_cases`）のどれかが
    /// 欠けていれば `None`。 欠けたものを既定で埋めない。
    pub(crate) fn definition(&self) -> Option<ProbeDefinition> {
        Some(ProbeDefinition {
            id: self.id.clone(),
            runner: self.runner.clone()?,
            package: self.package.clone()?,
            target: self.target.clone()?,
            platforms: self.platforms.clone(),
            backends: if self.backends.is_empty() {
                vec!["native".to_owned(), "unsupported".to_owned()]
            } else {
                self.backends.clone()
            },
            requires_native: self.requires_native,
            min_cases: self.min_cases?,
            manual: 0,
            prerequisites: Prerequisites {
                fixtures: self.fixtures.clone(),
                ..Prerequisites::default()
            },
        })
    }
}

/// 判断が仮説に依存していることを引くための、判断の欄。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DecisionDeps {
    pub(crate) id: String,
    pub(crate) status: Option<String>,
    pub(crate) relies_on: Vec<String>,
    /// この判断が作る・直す要件。
    pub(crate) requirements: Vec<String>,
}

impl DecisionDeps {
    /// 置き換えられた判断は、もう仮説に依存していない。
    pub(crate) fn is_live(&self) -> bool {
        self.status.as_deref() != Some("superseded")
    }
}

/// 検証の意味づけが読む記録の全部。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Facts {
    pub(crate) hypotheses: Vec<Hypothesis>,
    pub(crate) evidence: Vec<Observation>,
    pub(crate) probes: Vec<ProbeSpec>,
    pub(crate) decisions: Vec<DecisionDeps>,
}

fn place(r: &Record) -> Option<Location> {
    Some(Location::new(r.provenance().path(), r.provenance().line()))
}

impl Facts {
    pub(crate) fn from_snapshot(snapshot: &KnowledgeSnapshot) -> Self {
        let hypotheses = snapshot
            .by_schema("design-hypothesis")
            .filter_map(|r| {
                Some(Hypothesis {
                    id: r.str("id")?.to_owned(),
                    place: place(r),
                    kind: r.str("hypothesis_kind").map(str::to_owned),
                    supporting: r.strs("supporting_evidence"),
                    contradicting: r.strs("contradicting_evidence"),
                    assessment: r.str("assessment").map(str::to_owned),
                })
            })
            .collect();
        let evidence = snapshot
            .by_schema("evidence")
            .filter_map(|r| {
                Some(Observation {
                    id: r.str("id")?.to_owned(),
                    place: place(r),
                    method: r.str("method").map(str::to_owned),
                    assesses: r.strs("assesses"),
                    origin_group: r.str("origin_group").map(str::to_owned),
                    derived_from: r.strs("derived_from"),
                })
            })
            .collect();
        let probes = snapshot
            .by_schema("probe")
            .filter_map(|r| {
                Some(ProbeSpec {
                    id: r.str("id")?.to_owned(),
                    place: place(r),
                    targets: r.strs("targets"),
                    method: r.str("method").map(str::to_owned),
                    requires_human: r.bool("requires_human").unwrap_or(false),
                    requires_native: r.bool("requires_native").unwrap_or(false),
                    runner: r.str("runner").map(str::to_owned),
                    package: r.str("package").map(str::to_owned),
                    target: r.str("target").map(str::to_owned),
                    platforms: r.strs("platforms"),
                    backends: r.strs("backends"),
                    min_cases: r.int("min_cases").and_then(|n| u64::try_from(n).ok()),
                    fixtures: r.strs("fixtures"),
                })
            })
            .collect();
        let decisions = snapshot
            .by_schema("decision")
            .filter_map(|r| {
                let mut requirements = r.strs("establishes_requirements");
                requirements.extend(r.strs("revises_requirements"));
                Some(DecisionDeps {
                    id: r.str("id")?.to_owned(),
                    status: r.str("status").map(str::to_owned),
                    relies_on: r.strs("relies_on_hypotheses"),
                    requirements,
                })
            })
            .collect();
        Self {
            hypotheses,
            evidence,
            probes,
            decisions,
        }
    }

    pub(crate) fn hypothesis(&self, id: &str) -> Option<&Hypothesis> {
        self.hypotheses.iter().find(|h| h.id == id)
    }

    pub(crate) fn observation(&self, id: &str) -> Option<&Observation> {
        self.evidence.iter().find(|e| e.id == id)
    }
}
