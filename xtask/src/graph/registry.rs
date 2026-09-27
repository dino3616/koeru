//! node の種類・relation の規則の登録（X05）。
//!
//! ここに具体語（`"decision"`、`"supersedes"`）を書かない。 実在する登録は
//! D00（`knowledge` への legacy adapter、未着手）の仕事で、ここは呼び出し側が
//! 渡した [`Registry`] をそのまま実行するだけ。単体試験はここで作った fixture
//! の語彙（`widget` schema・`uses` relation のような、実在しない名前）で行う
//! ——本物の語彙が来ても fixture の語彙が来ても、engine の動きが変わらないことを見る。

use super::model::RelationClass;

/// `Record::schema()` が名乗る schema を、node の種類として登録する。
#[derive(Debug, Clone, Copy)]
pub(crate) struct NodeKind {
    pub(crate) schema: &'static str,
    pub(crate) kind: &'static str,
}

/// 1つの欄が edge を生む規則。
///
/// `from_schema` の記録が持つ `field`（文字列、または文字列の配列）の値を ID
/// として解決し、`relation` という名前の edge にする。 行き先が node として
/// 実在するかどうかはここでは見ない——[`super::build::SemanticGraph::validate`] の仕事。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RelationRule {
    pub(crate) from_schema: &'static str,
    pub(crate) field: &'static str,
    pub(crate) relation: &'static str,
    pub(crate) class: RelationClass,
}

/// 欄の値が ID ではなく操作上の locator（path・URL）を持つときの規則。
///
/// [`RelationRule`] と違い、行き先は ID として解決せず、そのまま
/// `NodeKey::Locator` にする——parse に失敗して黙って捨てることが無い。
#[derive(Debug, Clone, Copy)]
pub(crate) struct LocatorRule {
    pub(crate) from_schema: &'static str,
    pub(crate) field: &'static str,
    pub(crate) relation: &'static str,
    /// locator node の種類（`NodeKey::Locator` の `kind`）。
    pub(crate) locator_kind: &'static str,
}

/// graph engine が読む、呼び出し側の語彙。
///
/// 空の `Registry::default()` は、node を1つも作らない graph になる
/// ——それ自体は engine にとって不正ではない。
#[derive(Debug, Clone, Default)]
pub(crate) struct Registry {
    pub(crate) node_kinds: Vec<NodeKind>,
    pub(crate) relations: Vec<RelationRule>,
    pub(crate) locators: Vec<LocatorRule>,
    /// FSL の ID も node にするか、するならその種類。 `None` なら FSL の ID は
    /// node にならない。
    pub(crate) fsl_node_kind: Option<&'static str>,
}

impl Registry {
    /// この schema を node にするなら、その種類。
    pub(crate) fn kind_for_schema(&self, schema: &str) -> Option<&'static str> {
        self.node_kinds
            .iter()
            .find(|k| k.schema == schema)
            .map(|k| k.kind)
    }
}
