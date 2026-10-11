//! old → new の ID 対応表。
//!
//! 何を何へ写すかは、この engine が決めることではない。 old→new の対応は
//! cutover の D08 が持ち主で、ここが持つのは渡された対応表を安全に当てる手順だけ
//! （`docs/reports/design-system/implementation-plan.md` の 3.2 節）。

use crate::knowledge::Id;

/// 文字列が ID の形をしていなかった。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MappingError(String);

impl std::fmt::Display for MappingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "`{}` は ID の形をしていない", self.0)
    }
}

impl std::error::Error for MappingError {}

/// old ID → new ID の対応。 渡された順を保つ。
///
/// 鍵の重複・循環・連鎖は構築では弾かない。 連鎖（A→B と B→C）も入れ替え
/// （A→B と B→A）も、計画と書き換えが原文だけを見て同時に置き換えるので正しく扱える。
/// 弾くのは、複数の旧 ID が同じ新 ID へ写ることなど、計画（`plan`）が理由を付けて返す。
#[derive(Debug, Clone, Default)]
pub(crate) struct Mapping {
    pairs: Vec<(Id, Id)>,
}

impl Mapping {
    /// 文字列の対を ID として読む。 形の崩れた文字列があればそこで止める。
    pub(crate) fn parse<I, A, B>(pairs: I) -> Result<Self, MappingError>
    where
        I: IntoIterator<Item = (A, B)>,
        A: AsRef<str>,
        B: AsRef<str>,
    {
        let mut out = Vec::new();
        for (old, new) in pairs {
            let old_id: Id = old
                .as_ref()
                .parse()
                .map_err(|_| MappingError(old.as_ref().to_owned()))?;
            let new_id: Id = new
                .as_ref()
                .parse()
                .map_err(|_| MappingError(new.as_ref().to_owned()))?;
            out.push((old_id, new_id));
        }
        Ok(Self { pairs: out })
    }

    pub(crate) fn pairs(&self) -> &[(Id, Id)] {
        &self.pairs
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    /// `id` がこの対応表の旧 ID か。
    ///
    /// 新 ID が既存の別の記録とぶつかっても、その記録自身が同じ対応表でどこかへ
    /// 移るなら跡地は空くので、衝突ではない。 見分けるのに使う。
    pub(crate) fn is_old(&self, id: &Id) -> bool {
        self.pairs.iter().any(|(o, _)| o == id)
    }

    /// すべての旧 ID。
    pub(crate) fn olds(&self) -> impl Iterator<Item = &Id> {
        self.pairs.iter().map(|(o, _)| o)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 形の崩れた_id_は構築で止まる() {
        assert!(Mapping::parse([("DEC-fix-001", "DEC-fix-002")]).is_ok());
        let err = Mapping::parse([("DEC-fix-001", "not an id")]).unwrap_err();
        assert_eq!(err, MappingError("not an id".to_owned()));
        assert!(err.to_string().contains("not an id"));
    }

    #[test]
    fn 旧_id_かどうかを引ける() {
        let m = Mapping::parse([("DEC-fix-001", "DEC-fix-002")]).unwrap();
        assert!(m.is_old(&"DEC-fix-001".parse().unwrap()));
        assert!(!m.is_old(&"DEC-fix-002".parse().unwrap()));
        assert_eq!(m.olds().count(), 1);
    }
}
