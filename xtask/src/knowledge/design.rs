//! Design System の ontology と、legacy の meta を target の意味で読む adapter（D00）。
//!
//! 下流（D01 の Context、D02、D06、D07）が引く入口はここ。 `TR-` のような legacy の
//! 接頭辞や、`[[allocations]]` のような TOML の並びを知らずに済むことが、この層の目的。
//!
//! - [`ontology`] — node の種類（[`Kind`]）と relation の規則（[`RELATIONS`]）。 提案書 5.1 と 7.1 の写し
//! - [`legacy`] — 今の meta の schema ごとに、target では何かと、欄が何の関係かを持つ表。
//!   型つきの口（[`Object`] ほか）は、その表を通して記録を読む
//! - [`lift`] — FSL の contract を記録として加え（[`Design`]）、target の意味で引けているかを検査する
//! - [`identity`] — 目標の ID の形と、書式が決まっている locator の形
//!
//! graph engine（`crate::graph`）は具体語を持たない。 [`registry`] がこの語彙を `Registry`
//! にして渡す。 meta のファイルは書き換えない。 書き換えは D08 の cutover で、それまでは
//! legacy と target の両方の欄名を同じ registry が読む。
//!
//! 今の `check-meta` が使う語彙は `checks::legacy_registry` にあり、これとは別。 D08 の
//! 切り替えで、あちらがこの登録に置き換わって消える。 それまで重複している語彙表は、
//! `checks::legacy_registry` の `NODE_KINDS`（schema から種類への表）と、ここの
//! [`SCHEMAS`] の2つ。

// 消費者（D01 以降・D08）が付くまで、production からは呼ばれない。
// `lib.rs` の `mod graph;` と同じ事情で、許す範囲はこの module に閉じる。
#![allow(dead_code, unused_imports)]

mod identity;
mod legacy;
mod lift;
mod ontology;
mod shapes;

pub(crate) use identity::{IdentityError, Locator, LocatorError, TargetId, check_identity};
pub(crate) use legacy::{
    Adoption, Allocation, Calculation, Component, Contract, ContractKind, Decision, Evidence,
    LegacyOrigin, Meaning, Object, Part, Probe, Quantity, Question, ReleaseProfile, Requirement,
    SCHEMAS, UnclassifiedTarget, Workload,
};
pub(crate) use lift::{Design, DesignIssue};
pub(crate) use ontology::{
    Ends, Kind, Layer, RELATIONS, RelationSpec, SEMANTIC, Standing, relation,
};
pub(crate) use shapes::{TARGET_SHAPES, is_target_only};

use crate::graph::{Direction, NodeKind, Registry, RelationRule};
use legacy::Role;

/// graph engine に渡す語彙。
///
/// 今の meta の schema と、目標の schema の両方を node の種類にし、欄を [`RELATIONS`] の名前の
/// edge にする。 `affects_*` の族は `affects` に、`closed_by` は `resolved_by` にまとまる。
/// `formalized_as` と `supports_decisions` は、関係の行き先の側に書かれた欄なので
/// 逆向きの規則（`Direction::Reverse`）で読む。
/// FSL の ID は記録としては持たない（`Design` が contract の記録を加える）ので
/// `fsl_node_kind` は使わない。 locator の欄を持つ schema がまだ無いので、`locators` は空。
pub(crate) fn registry() -> Registry {
    let node_kinds = SCHEMAS
        .iter()
        .map(|s| NodeKind {
            schema: s.schema,
            kind: s.kind.name(),
        })
        .collect();
    let mut relations = Vec::new();
    for schema in SCHEMAS {
        for field in schema.fields {
            let (name, direction) = match field.role {
                Role::Edge(name) => (name, Direction::Forward),
                Role::Inverse(name) => (name, Direction::Reverse),
                _ => continue,
            };
            let Some(rule) = relation(name) else {
                continue;
            };
            relations.push(RelationRule {
                from_schema: schema.schema,
                field: field.name,
                relation: rule.name,
                class: rule.class,
                direction,
            });
        }
    }
    Registry {
        node_kinds,
        relations,
        locators: Vec::new(),
        fsl_node_kind: None,
    }
}

#[cfg(test)]
mod tests;
