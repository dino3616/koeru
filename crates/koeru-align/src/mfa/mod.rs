//! MFA 日本語音響モデルの経路（`TR-ALN-05`, `DEC-ALN-008`）。
//!
//! Kaldi を組めない OS では、空の実装を置く。 `koeru-audio` の
//! `backend/unsupported.rs` と同じ規律（`DEC-REC-001` の帰結）。
//!
//! そうしないと `koeru-align` が macOS でしかコンパイルできず、
//! **他 OS の CI が「クレートが組み立たない」ところで止まって、
//! その先にあるドメイン層の回帰にも気づけなくなる。**
//!
//! # 手元で他 OS 向けの組み立てを検査する
//!
//! ```bash
//! RUSTFLAGS='--cfg koeru_force_unsupported_backend' cargo check --workspace --all-targets
//! ```
//!
//! cargo の feature にしない。 feature は加算的であるべきで、
//! `--all-features` で挙動が変わるものを混ぜると macOS の CI がスタブを検査しはじめる。

use std::path::{Path, PathBuf};

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
mod kaldi;

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
mod unsupported;

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
pub use kaldi::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError};

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
pub use unsupported::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError};

/// モデルを差し替える環境変数。開発中に別の版を当てるときに使う。
pub const MODEL_DIR_ENV: &str = "KOERU_MFA_MODEL_DIR";

/// このクレートが submodule で抱えているモデルの位置（`DEC-ALN-012`）。
const MODEL_DIR_IN_REPO: &str = "models/japanese_mfa/acoustic";

/// そこに音響モデルがあるか。 ディレクトリの存在では見ない。
///
/// LFS を入れずに clone すると、`final.mdl` は 130 バイト程度のポインタで置かれる。
/// ファイルはあるので `is_file` は通り、Kaldi が読む段になって初めて落ちる。
fn has_model(dir: &Path) -> bool {
    let p = dir.join("final.mdl");
    std::fs::metadata(&p).is_ok_and(|m| m.is_file() && m.len() > 1_000_000)
}

/// 環境変数が指すモデル。
#[must_use]
pub fn env_model_dir() -> Option<PathBuf> {
    let p = PathBuf::from(std::env::var(MODEL_DIR_ENV).ok()?);
    has_model(&p).then_some(p)
}

/// submodule の中のモデル。 `cargo run` と `cargo test` のときに見つかる。
///
/// submodule を取っていない環境では `None`。 そのときアプリは退避経路へ下がり、
/// 試験は静かに戻る。
#[must_use]
pub fn repo_model_dir() -> Option<PathBuf> {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(MODEL_DIR_IN_REPO);
    has_model(&p).then_some(p)
}

/// 配布物の形を持たない呼び出しが使う探索（環境変数 → submodule）。
///
/// 試験はこれを使う。 **以前は環境変数だけを見ていたので、submodule を
/// 同梱したあとも実モデルの試験が1件も走っていなかった**——立てずに走らせると
/// 10 件が中身を実行せずに `ok` と出て、CI も 135MB のモデルを取得したうえで
/// 素通りしていた。**踏んだ。**
///
/// 実行ファイルの隣（配布物の形）は `koeru-app` が足す。
/// ここが知っているのは、このクレートが抱えているものだけ。
#[must_use]
pub fn model_dir() -> Option<PathBuf> {
    env_model_dir().or_else(repo_model_dir)
}

/// 同梱しているモデルの版（`TR-ALN-29`）。
///
/// **モデル自身が名乗るものを読む。** 版を定数で持つと、submodule を上げたときに
/// 片方だけが古くなる——`DEC-ALN-012` で v3.3.0 へ上げたあとも、指紋には
/// `3.0.0` が書かれていた。そうなると、いまのモデルで作った推定と
/// 前の版で作った推定が見分けられず、指紋を持つ意味が消える。
///
/// 読めなければ `None`。 呼び出し側は「版が分からない」ことを指紋に残す。
#[must_use]
pub fn model_version(dir: &Path) -> Option<String> {
    let text = std::fs::read_to_string(dir.join("meta.json")).ok()?;
    let doc: serde_json::Value = serde_json::from_str(&text).ok()?;
    doc.get("version")?.as_str().map(str::to_owned)
}

/// アライナの識別子（`TR-ALN-29` の指紋に入る）。
///
/// 版が読めなければ `unknown` を入れる。 **今の版を名乗らせない**——
/// 名乗らせると、確かめられないものが確かめたことになる。
#[must_use]
pub fn model_identity(dir: &Path) -> String {
    let v = model_version(dir).unwrap_or_else(|| "unknown".to_owned());
    format!("mfa-japanese@{v}")
}
