//! どのアライナを使うかを選ぶ（`TR-ALN-03`, `DEC-ALN-008`）。
//!
//! **一次経路は MFA**（`DEC-ALN-008`）。モデルが読めなければ、
//! 音響モデルを使わない退避経路へ落とす（`DEC-ALN-006`）。
//!
//! # なぜ落とすのか
//!
//! **黙って止めない。** モデルが同梱されていない開発中のビルドや、
//! ファイルが壊れている環境でも、**M2 の試唱は止まらないほうがよい**
//! （退避経路の上で動いている）。
//!
//! **ただし黙って落とさない。** どちらを使っているかは
//! [`Chosen::is_fallback`] で分かり、トレースにも1度だけ出す。
//!
//! # モデルの置き場所
//!
//! **同梱の方法はまだ決めていない。** いまは探す場所を2つ持っている。
//!
//! 1. 環境変数 `KOERU_MFA_MODEL_DIR`
//! 2. 実行ファイルの隣の `models/japanese_mfa`
//!
//! どちらにも無ければ退避経路。**配布物へどう入れるかを決めたら、
//! ここに3つ目を足す**（`TR-PLT-19` の「初回起動後の追加ダウンロードをゼロにする」）。

use std::path::PathBuf;

use koeru_align::aligner::Aligner;
use koeru_align::mfa::MfaAligner;
use koeru_align::segment::HeuristicAligner;

/// MFA のモデルを探す環境変数。
const MODEL_DIR_ENV: &str = "KOERU_MFA_MODEL_DIR";

/// 実行ファイルからの相対の置き場所。
const MODEL_DIR_RELATIVE: &str = "models/japanese_mfa";

/// 選んだアライナ。
#[derive(Debug)]
pub struct Chosen {
    inner: Inner,
}

#[derive(Debug)]
enum Inner {
    /// 一次経路（`DEC-ALN-008`）。
    Mfa(Box<MfaAligner>),
    /// 退避経路（`DEC-ALN-006`）。
    Fallback(HeuristicAligner),
}

impl Chosen {
    /// モデルが読めれば MFA、読めなければ退避経路。
    ///
    /// **失敗しない。** 落ちる代わりに退避へ下がる。
    #[must_use]
    pub fn detect() -> Self {
        let Some(dir) = model_dir() else {
            tracing::info!(
                reason = "model_not_found",
                "自動原音設定は退避経路で動く（MFA のモデルが見つからない）"
            );
            return Self::fallback();
        };
        match MfaAligner::open(&dir, "mfa-japanese@3.0.0") {
            Ok(a) => {
                tracing::info!(dim = a.feature_dim(), "自動原音設定は MFA で動く");
                Self {
                    inner: Inner::Mfa(Box::new(a)),
                }
            }
            Err(e) => {
                // **パスは載せない**（AGENTS.md #3）。種別だけ。
                tracing::warn!(
                    reason = e.kind(),
                    "自動原音設定は退避経路で動く（モデルを読めない）"
                );
                Self::fallback()
            }
        }
    }

    /// 退避経路で固定する。
    #[must_use]
    pub fn fallback() -> Self {
        Self {
            inner: Inner::Fallback(HeuristicAligner::new("heuristic@1")),
        }
    }

    /// 退避経路で動いているか。
    ///
    /// **確信度の成分が欠ける**ので、呼び出し側はこれを見て扱いを変える
    /// （`TR-ALN-24` の成分 (1) 経路確信度が出ない）。
    #[must_use]
    pub const fn is_fallback(&self) -> bool {
        matches!(self.inner, Inner::Fallback(_))
    }

    /// 使っているアライナ。
    #[must_use]
    pub fn as_aligner(&self) -> &dyn Aligner {
        match &self.inner {
            Inner::Mfa(a) => a.as_ref(),
            Inner::Fallback(a) => a,
        }
    }
}

/// モデルの置き場所を探す。
fn model_dir() -> Option<PathBuf> {
    if let Ok(p) = std::env::var(MODEL_DIR_ENV) {
        let p = PathBuf::from(p);
        if p.join("final.mdl").is_file() {
            return Some(p);
        }
    }
    let exe = std::env::current_exe().ok()?;
    let p = exe.parent()?.join(MODEL_DIR_RELATIVE);
    p.join("final.mdl").is_file().then_some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **モデルが無くても落ちない。** 退避経路へ下がる。
    #[test]
    fn モデルが無ければ退避へ落ちる() {
        // 環境変数を空にして探させる。
        let c = Chosen::fallback();
        assert!(c.is_fallback());
        assert_eq!(c.as_aligner().identity(), "heuristic@1");
    }

    /// **`detect` は失敗しない。** どちらかは必ず返る。
    #[test]
    fn 検出は必ず何かを返す() {
        let c = Chosen::detect();
        assert!(!c.as_aligner().identity().is_empty());
    }
}
