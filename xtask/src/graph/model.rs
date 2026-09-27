//! node / edge の型（X05）。
//!
//! ここは同一性と分類だけを持つ。「DEC とは何か」のような具体語は持ち込み側の
//! [`super::registry::Registry`] が決める——このモジュールが知っているのは、
//! node が [`NodeKey`] で区別されることと、edge が [`RelationClass`] で
//! 循環検査の対象かどうかが決まることだけ。

use std::path::PathBuf;

use crate::knowledge::Id;

/// node の同一性。 中身の文字列がいくら同じでも、`key` が違えば別の node
/// ——「同じ本文でも ID が違えば別 node」という前提を型で保つ。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NodeKey {
    /// meta / FSL が持つ、型つき ID を持つ object。
    Object(Id),
    /// 操作上の locator（path・URL）。ID を持たない。
    Locator { kind: &'static str, value: String },
}

/// node / edge がどこから来たかの、graph 独自の持ち方。
///
/// `knowledge::Provenance` をそのまま抱えない。 FSL 由来の node は
/// `specs/foo.fsl:12` という文字列でしか出どころを持たず、meta の記録が持つ
/// 構造化された `Provenance`（`path` と `line` に分かれている）とは形が違う。
/// 両方をここへ写して、消費者（D01 など）は種類を問わず同じ形で読めるようにする。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Default)]
pub(crate) struct NodeProvenance {
    pub(crate) path: Option<PathBuf>,
    pub(crate) line: Option<usize>,
}

/// 1つの node。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GraphNode {
    pub(crate) key: NodeKey,
    /// [`super::registry::Registry`] が名付けた種類。 `"decision"` のような
    /// 具体語はここでは決めない——registry が渡した文字列をそのまま持つだけ。
    pub(crate) kind: &'static str,
    pub(crate) provenance: NodeProvenance,
    /// 記録の欄を決定的に写した内容 digest（[`fnv1a_64`]）。 `diff` の
    /// `changed` 判定に使う。 本文そのものは持たない——`toml::Table` を
    /// graph の外へ漏らさないという `knowledge` の境界を、graph の中でも保つ。
    pub(crate) fingerprint: u64,
}

/// edge の分類。 `supersedes` の循環検査・diff の `superseded` はこれで決める。
/// 具体的な relation 名（`"supersedes"` のような文字列）では決めない
/// ——`Registry` が同じ意味の relation に別の名前を付けても、ここは揺れない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum RelationClass {
    /// 通常の参照。 循環があってよい。
    Reference,
    /// 後継を作る関係。 循環は不正
    /// ——「意味を上書きせず後継を作る」という前提が壊れている合図。
    Supersession,
}

/// 1本の edge。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Relation {
    pub(crate) from: NodeKey,
    pub(crate) to: NodeKey,
    pub(crate) name: &'static str,
    pub(crate) class: RelationClass,
    /// この edge を引いている記録の出どころ。
    pub(crate) source: NodeProvenance,
}

/// FNV-1a、64 bit。
///
/// `std::hash::DefaultHasher` は Rust の版をまたいで安定するとは決められていない
/// ——同じ内容の記録でも、コンパイラの版が変わっただけで `changed` になりかねない。
/// diff は複数の版・複数の実行環境をまたいで比べるものなので、版を選ばない
/// 実装を手で持つ。
pub(crate) fn fnv1a_64(s: &str) -> u64 {
    const OFFSET_BASIS: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let mut hash = OFFSET_BASIS;
    for byte in s.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同じ内容なら同じ digest、1 byte でも違えば別の digest になる。
    #[test]
    fn fnv1a_は内容が同じなら同じ値になる() {
        assert_eq!(fnv1a_64("abc"), fnv1a_64("abc"));
        assert_ne!(fnv1a_64("abc"), fnv1a_64("abd"));
        assert_ne!(fnv1a_64(""), fnv1a_64("a"));
    }
}
