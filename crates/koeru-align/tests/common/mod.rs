//! 実モデルの置き場所（試験だけが使う）。
//!
//! 製品のコードは置き場所を探さない（`mfa/mod.rs`）。 探すのは呼び出し側（`koeru-app`）で、
//! 試験は試験用の探し方を持つ。 `mfa/kaldi.rs` の単体試験も、この1つを `#[path]` で読む。
//!
//! 探す順は環境変数 → このクレートが submodule で抱えているもの（`DEC-ALN-012`）。
//! **以前は環境変数だけを見ていたので、submodule を同梱したあとも実モデルの試験が
//! 1件も走っていなかった**——立てずに走らせると中身を実行せずに `ok` と出て、CI も
//! 135MB のモデルを取得したうえで素通りしていた。**踏んだ。**

use std::path::{Path, PathBuf};

/// モデルを差し替える環境変数。 開発中に別の版を当てるときに使う。
const MODEL_DIR_ENV: &str = "KOERU_MFA_MODEL_DIR";

/// このクレートが submodule で抱えているモデルの位置。
const MODEL_DIR_IN_REPO: &str = "models/japanese_mfa/acoustic";

/// そこに音響モデルがあるか。 ディレクトリの存在では見ない。
///
/// LFS を入れずに clone すると、`final.mdl` は 130 バイト程度のポインタで置かれる。
/// ファイルはあるので `is_file` は通り、Kaldi が読む段になって初めて落ちる。
fn has_model(dir: &Path) -> bool {
    std::fs::metadata(dir.join("final.mdl")).is_ok_and(|m| m.is_file() && m.len() > 1_000_000)
}

/// 実モデルの置き場所。 見つからなければ `None`。
#[must_use]
pub(crate) fn model_dir() -> Option<PathBuf> {
    let from_env = std::env::var(MODEL_DIR_ENV).ok().map(PathBuf::from);
    let in_repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MODEL_DIR_IN_REPO);
    from_env.into_iter().chain([in_repo]).find(|p| has_model(p))
}
