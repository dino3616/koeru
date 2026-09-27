//! `KnowledgeSnapshot` と操作上の locator から、版ごとの typed graph を作り、
//! 比較できるようにする（X05）。
//!
//! node の種類・relation の名前と分類は、呼び出し側が渡す [`Registry`] が
//! 決める——`"decision"` や `"supersedes"` のような具体語をここへ持ち込まない。
//! 実在する登録は D00（`knowledge` への legacy adapter、未着手）の仕事で、
//! ここは fixture の語彙（単体試験を見る）で検査する。
//!
//! - [`model`] — node / edge の型、fingerprint
//! - [`registry`] — 呼び出し側が渡す語彙
//! - [`build`] — `KnowledgeSnapshot` から `SemanticGraph` を作る。近傍・
//!   supersession chain・validate の traversal もここに持つ
//! - [`diff`] — 2つの版の `SemanticGraph` を比べる
//!
//! この crate から呼ぶ経路はまだ無い。 消費するのは D01（Context）・
//! X06（migration の before/after 検証）・X07（既存 check の載せ替え）で、
//! それまでは通常の組み立てでは到達しない——`#[allow(dead_code)]` は
//! `lib.rs` の `mod graph;` に1つだけ置いてある。

mod build;
mod diff;
mod model;
mod registry;

pub(crate) use build::{GraphIssue, SemanticGraph};
pub(crate) use diff::{ChangeKind, GraphDiff, diff};
pub(crate) use model::{GraphNode, NodeKey, NodeProvenance, Relation, RelationClass};
pub(crate) use registry::{LocatorRule, NodeKind, Registry, RelationRule};
