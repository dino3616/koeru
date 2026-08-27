//! KOERU のドメイン層。
//!
//! この層は GUI と OS に依存しない。録音・録音リスト生成・自動原音設定・試唱・
//! 原音設定・パッケージングの規則をここに置き、Tauri コマンドや音声 I/O の
//! 実装はこの層を呼ぶ側に置く。
//!
//! # エラーハンドリング
//!
//! 詳細は `docs/rust-conventions.md`。要点は3つ。
//!
//! - **この層は `thiserror` の列挙体を返す。** 呼び出し側が `match` で網羅的に分岐できる。
//! - **`anyhow` はアプリケーション境界だけで使う。** 畳んだ時点で網羅性は失われる。
//! - **`?` を並べるときは `#[tracing::instrument(err)]` を付ける。** 付けないと
//!   どこで失敗したのかが追えない。
//!
//! # 出力
//!
//! `println!` / `eprintln!` / `dbg!` は lint で禁止している。`tracing` を使う。

pub mod error;
pub mod telemetry;

pub use error::{Error, Result};

/// 音源の方式。
///
/// プロジェクト作成時に選ぶ。**選択肢は方式名ではなく「手作業が必要かどうか」で見せる**が、
/// 内部ではこの列挙体で扱う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// 単独音。自動原音設定が全自動で完結する見込みがある唯一の方式。
    Cv,
    /// 連続音（VCV）。
    Vcv,
    /// CVVC。
    Cvvc,
    /// 多音階連続音。
    MultiPitchVcv,
}

impl Method {
    /// 自動原音設定だけで完結する見込みがあるか。
    ///
    /// **これが選択肢の主軸になる。** 所要時間ではない。単独音と CVVC の録音時間差は
    /// 2分程度しかなく、利用者にとっての実質的な違いは「確認作業をやらされるか」である。
    #[must_use]
    pub fn is_fully_automatic(self) -> bool {
        matches!(self, Self::Cv)
    }

    /// 上位方式から下位方式へ書き出せる関係にあるか。
    ///
    /// 逆方向は成立しない。母音から子音への遷移は連続した発話の中でしか録れないため、
    /// 単独音の素材から連続音は作れない。
    #[must_use]
    pub fn can_export_as(self, target: Self) -> bool {
        self.rank() >= target.rank()
    }

    const fn rank(self) -> u8 {
        match self {
            Self::Cv => 0,
            Self::Vcv => 1,
            Self::Cvvc => 2,
            Self::MultiPitchVcv => 3,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 全自動で完結するのは単独音だけ() {
        assert!(Method::Cv.is_fully_automatic());
        assert!(!Method::Vcv.is_fully_automatic());
        assert!(!Method::Cvvc.is_fully_automatic());
        assert!(!Method::MultiPitchVcv.is_fully_automatic());
    }

    #[test]
    fn 書き出しは下位方式へは可能で上位方式へは不可能() {
        assert!(Method::MultiPitchVcv.can_export_as(Method::Cv));
        assert!(Method::Cvvc.can_export_as(Method::Vcv));
        assert!(!Method::Cv.can_export_as(Method::Vcv));
        assert!(!Method::Vcv.can_export_as(Method::Cvvc));
    }
}
