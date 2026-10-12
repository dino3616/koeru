//! 目標の形の記録（`design-hypothesis`・`requirement`）の欄を読む。
//!
//! **D00 に口が出るまでの仮の読み。** D00 の `Object` は、仮説の `supporting_evidence` /
//! `contradicting_evidence` / `scope` / `assessment` を読む口も、目標の形の要件の `statement` を
//! 読む口も持たない（欄は `Role::Mirror` で、edge にもならない）。 反証する調査は判断の文脈で
//! 必ず見せるもの（提案書 7.2、14.3）なので、`KnowledgeSnapshot` の `Record` から欄を読む。
//! 欄の名前は提案書 5.5 と `DEC-PLT-048` の6番が決めたもの。 D00 が口を足したら、この module を
//! その口に置き換えて消す（PR 本文の interface change request）。

use std::collections::BTreeMap;

use crate::knowledge::{Id, KnowledgeSnapshot, Record};

/// 仮説。 提案書 5.5。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Hypothesis {
    pub(crate) statement: Option<String>,
    pub(crate) scope: Option<String>,
    pub(crate) assessment: Option<String>,
    /// 支持する調査。 ID の形に読めない値は落とさず `unreadable` に残す。
    pub(crate) supporting: Vec<Id>,
    pub(crate) contradicting: Vec<Id>,
    /// ID の形に読めなかった欄の値。 `(欄の名前, 値)`。
    pub(crate) unreadable: Vec<(&'static str, String)>,
}

/// 目標の形の要件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TargetRequirement {
    pub(crate) statement: Option<String>,
}

/// 目標の形の記録。
#[derive(Debug, Default)]
pub(crate) struct TargetRecords {
    pub(crate) hypotheses: BTreeMap<Id, Hypothesis>,
    pub(crate) requirements: BTreeMap<Id, TargetRequirement>,
}

pub(crate) const SUPPORTING: &str = "supporting_evidence";
pub(crate) const CONTRADICTING: &str = "contradicting_evidence";

impl TargetRecords {
    /// 読んだ記録から取り出す。 `snapshot` は、目標の形（`TARGET_SHAPES`）も読んだもの。
    pub(crate) fn from_snapshot(snapshot: &KnowledgeSnapshot) -> Self {
        let mut out = Self::default();
        for r in snapshot.by_schema("design-hypothesis") {
            if let Some(id) = r.id() {
                out.hypotheses.insert(id.clone(), hypothesis(r));
            }
        }
        for r in snapshot.by_schema("requirement") {
            if let Some(id) = r.id() {
                out.requirements.insert(
                    id.clone(),
                    TargetRequirement {
                        statement: r.str("statement").map(|s| s.trim().to_owned()),
                    },
                );
            }
        }
        out
    }
}

fn hypothesis(r: &Record) -> Hypothesis {
    let mut unreadable = Vec::new();
    let mut ids = |field: &'static str| -> Vec<Id> {
        let mut out = Vec::new();
        for raw in r.strs(field) {
            match raw.parse::<Id>() {
                Ok(id) => out.push(id),
                Err(_) => unreadable.push((field, raw)),
            }
        }
        out.sort();
        out.dedup();
        out
    };
    let supporting = ids(SUPPORTING);
    let contradicting = ids(CONTRADICTING);
    Hypothesis {
        statement: r.str("statement").map(|s| s.trim().to_owned()),
        scope: r.str("scope").map(|s| s.trim().to_owned()),
        assessment: r.str("assessment").map(str::to_owned),
        supporting,
        contradicting,
        unreadable,
    }
}
