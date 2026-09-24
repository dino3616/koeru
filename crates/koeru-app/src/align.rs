//! アライナを用意する（`TR-ALN-03`, `DEC-ALN-008`）。
//!
//! MFA だけ（`DEC-ALN-016`）。 音響モデルを使わない退避経路は持たない。
//!
//! # モデルが無いのはエラー
//!
//! モデルは配布物に同梱している（`DEC-ALN-012`）。 開発でも submodule を
//! 取ってくるのが前提（`setup-koeru`）。**だからモデルが読めないのは
//! ビルドの失敗であって、動かしてよい状態ではない。**
//!
//! 退避で埋めない。 実際に埋めて見えなくなっていた——macOS の配布物で
//! 資源の場所を1箇所しか探しておらず、**全員が黙って退避経路で動いていた。**
//!
//! 起動で落とす。 [`Chosen::detect`] は失敗を返し、`Studio::open` がそれを
//! 上げる。収録の途中で「原音設定だけ出ない」ことに気づく形にしない。
//!
//! # モデルの置き場所
//!
//! リポジトリに submodule で同梱している（`DEC-ALN-012`）。探す順は3つ。
//!
//! 1. 環境変数 `KOERU_MFA_MODEL_DIR`（開発中の差し替え用）
//! 2. 配布物の資源置き場（`tauri.conf.json` の `bundle.resources`）
//! 3. リポジトリの `crates/koeru-align/models/japanese_mfa/acoustic`（`cargo run` のとき）
//!
//! どこにも無ければ退避経路。実行時に取りに行かない
//! （`TR-PLT-19` の「初回起動後の追加ダウンロードをゼロにする」）。

use std::path::PathBuf;

use koeru_align::aligner::Aligner;
use koeru_align::mfa::MfaAligner;
use koeru_failure::Class;

use crate::error::AppError;

/// 配布物の資源置き場（実行ファイルからの相対）。
///
/// **OS で並びが違う。** Tauri が資源を置くのは
/// macOS なら `Contents/Resources/`（実行ファイルは `Contents/MacOS/`）、
/// Windows と Linux なら実行ファイルの隣。両方見る。
///
/// 片方だけ見ていた。 実行ファイルの隣しか探していなかったので、
/// **macOS の配布物では同梱したモデルが見つからず、全員が黙って
/// 退避経路で動くことになる。**
const MODEL_DIRS_RELATIVE: [&str; 2] = ["models/japanese_mfa", "../Resources/models/japanese_mfa"];

/// 用意できたアライナ（`DEC-ALN-016`）。
///
/// **必ず在る。** 無い状態を型で表さない——無いのはビルドの失敗で、
/// そこまで来たら起動が止まっている（[`Chosen::detect`]）。
#[derive(Debug)]
pub struct Chosen {
    inner: Box<MfaAligner>,
}

impl Chosen {
    /// MFA のモデルを読む。
    ///
    /// **読めなければ失敗を返す**（`DEC-ALN-016`）。 同梱されているはずのものが
    /// 無いのはビルドの失敗で、そのまま動かすと退避も無いまま原音設定だけが
    /// 出ないことになる。起動で止める。
    ///
    /// # Errors
    ///
    /// モデルが見つからない、または読めないとき。
    pub fn detect() -> Result<Self, AppError> {
        let Some(dir) = model_dir() else {
            return Err(AppError::new(
                "align.model_not_found",
                Class::Unsupported,
                "MFA のモデルが見つからない。submodule を取り込んでいるか確かめてほしい",
            ));
        };
        // 識別子はモデルに名乗らせる（`TR-ALN-29`）。定数で持つと、
        // submodule を上げたときに指紋だけが古い版を指す。
        let identity = koeru_align::mfa::model_identity(&dir);
        match MfaAligner::open(&dir, &identity) {
            Ok(a) => {
                tracing::info!(dim = a.feature_dim(), "自動原音設定は MFA で動く");
                Ok(Self { inner: Box::new(a) })
            }
            // パスは載せない（AGENTS.md #3）。種別だけ。
            Err(e) => {
                Err(AppError::from_failure(e).saying("MFA のモデルを読めない。同梱物が壊れている"))
            }
        }
    }

    /// 使うアライナ。
    #[must_use]
    pub fn as_aligner(&self) -> &dyn Aligner {
        self.inner.as_ref()
    }

    /// 何で推定したかの札（`TR-ALN-29`）。
    #[must_use]
    pub fn identity(&self) -> &str {
        self.inner.identity()
    }
}

/// モデルの置き場所を探す。先に見つかったものを使う。
fn model_dir() -> Option<PathBuf> {
    // 両端は `koeru-align` が持つ。 モデルを抱えているのはあちらなので、
    // 置き場所の規則もあちらに置く。ここが足すのは配布物の形だけ。
    koeru_align::mfa::env_model_dir()
        .or_else(exe_model_dir)
        .or_else(koeru_align::mfa::repo_model_dir)
}

/// 配布物の中（`tauri.conf.json` の `bundle.resources` が置く場所）。
///
/// 実体の判定は `koeru-align` のものを通す。 ここだけ `is_file` で見ていたので、
/// LFS のポインタのまま同梱された配布物が選ばれ、MFA が黙って退避経路へ落ちていた。
fn exe_model_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    MODEL_DIRS_RELATIVE
        .into_iter()
        .map(|rel| dir.join(rel))
        .find(|p| koeru_align::mfa::has_model(p))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// MFA を組んである OS では、モデルが読めて札を名乗る（`DEC-ALN-016`）。
    ///
    /// submodule は取ってあるのが前提（`setup-koeru`）。 通らない環境は、
    /// まず submodule を取る——**退避で埋めない。**
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn モデルが読めれば札を名乗る() {
        let c = Chosen::detect().expect("submodule を取り込んでいれば読める");
        assert!(!c.identity().is_empty(), "何で推定したかの札を持つ");
    }

    /// MFA を組んでいない OS では、名指しで失敗する（`DEC-ALN-016`）。
    ///
    /// **黙って動かない。** 退避も持たないので、自動原音設定が無いまま
    /// 収録だけ進む形を作らない。`Studio::open` がこれを上げて起動が止まる。
    ///
    /// `TR-PLT-01` は Windows を第一級の対象としているが、Kaldi の移植が
    /// 終わるまでは動かない。**その衝突を隠さないための試験。**
    #[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
    #[test]
    fn 組んでいない_os_では名指しで失敗する() {
        let e = Chosen::detect().expect_err("この OS には Kaldi を組んでいない");
        assert_eq!(e.code, "mfa.unsupported_platform");
    }

    /// submodule を初期化していれば、環境変数なしでモデルが見つかる（`DEC-ALN-012`）。
    ///
    /// 探す経路の試験で、アライナを組むかどうかとは別。 モデルが無ければ落とす
    /// ——戻ると何も見ずに通る（`DEC-PLT-039`）。
    #[test]
    fn リポジトリの中のモデルを見つけられる() {
        assert!(
            koeru_align::mfa::repo_model_dir().is_some(),
            "MFA のモデルが無い。submodule と LFS を取り込む（setup-koeru）"
        );
        // 環境変数を使わずに見つかること。
        assert!(model_dir().is_some());
    }
}
