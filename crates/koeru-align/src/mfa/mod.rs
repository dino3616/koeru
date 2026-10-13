//! MFA 日本語音響モデルの経路（`TR-ALN-05`, `DEC-ALN-008`）。
//!
//! Kaldi を組めない OS では、空の実装を置く。 `koeru-audio` の
//! `backend/unsupported.rs` と同じ規律（`DEC-REC-001` の帰結）。
//!
//! そうしないと `koeru-align` が macOS でしかコンパイルできず、
//! **他 OS の CI が「クレートが組み立たない」ところで止まって、
//! その先にあるドメイン層の回帰にも気づけなくなる。**
//!
//! # モデルの場所は探さない
//!
//! 環境変数・実行ファイルの隣・リポジトリの中を探すのは呼び出し側（`koeru-app` の
//! `align.rs`）で、ここが受け取るのは見つかったディレクトリ（[`MfaAligner::open`]）と、
//! `meta.json` の中身の文字列（[`model_version`]）だけ。 エンジンがファイルシステムと
//! 環境変数に触ると、同じ入力から同じ答えを返す計算の外に、置き場所という入力が混ざる
//! （`DEC-PLT-034`、`crates/koeru-align/tests/purity.rs`）。
//!
//! # 手元で他 OS 向けの組み立てを検査する
//!
//! ```bash
//! RUSTFLAGS='--cfg koeru_force_unsupported_backend' cargo check --workspace --all-targets
//! ```
//!
//! cargo の feature にしない。 feature は加算的であるべきで、
//! `--all-features` で挙動が変わるものを混ぜると macOS の CI がスタブを検査しはじめる。

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
mod kaldi;

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
mod unsupported;

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
pub use kaldi::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError, availability};

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
pub use unsupported::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError, availability};

/// `meta.json` の中身から、同梱しているモデルの版を読む（`TR-ALN-29`）。
///
/// **モデル自身が名乗るものを読む。** 版を定数で持つと、submodule を上げたときに
/// 片方だけが古くなる——`DEC-ALN-012` で v3.3.0 へ上げたあとも、指紋には
/// `3.0.0` が書かれていた。そうなると、いまのモデルで作った推定と
/// 前の版で作った推定が見分けられず、指紋を持つ意味が消える。
///
/// ファイルを読むのは呼び出し側。 読めない、JSON でない、`version` が無いときは `None`
/// ——呼び出し側は「版が分からない」ことを指紋に残す（[`model_identity`]）。
#[must_use]
pub fn model_version(meta_json: &str) -> Option<String> {
    let doc: serde_json::Value = serde_json::from_str(meta_json).ok()?;
    doc.get("version")?.as_str().map(str::to_owned)
}

/// アライナの識別子（`TR-ALN-29` の指紋に入る）。
///
/// 版が読めなければ `unknown` を入れる。 **今の版を名乗らせない**——
/// 名乗らせると、確かめられないものが確かめたことになる。
#[must_use]
pub fn model_identity(version: Option<&str>) -> String {
    format!("mfa-japanese@{}", version.unwrap_or("unknown"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn モデルの版を_meta_json_の中身から読む() {
        assert_eq!(
            model_version(r#"{"version": "3.3.0", "language": "japanese"}"#).as_deref(),
            Some("3.3.0")
        );
    }

    /// 読めないものを今の版として名乗らない。
    #[test]
    fn 読めない_meta_json_は版なし() {
        assert_eq!(model_version("not json"), None);
        assert_eq!(model_version("{}"), None);
        assert_eq!(model_version(r#"{"version": 3}"#), None);
        assert_eq!(model_identity(None), "mfa-japanese@unknown");
    }

    /// 版が変われば識別子が変わる。 再推定の鍵（`TR-ALN-29`）。
    #[test]
    fn 識別子は版を含む() {
        assert_eq!(model_identity(Some("3.3.0")), "mfa-japanese@3.3.0");
        assert_ne!(model_identity(Some("3.3.0")), model_identity(Some("3.4.0")));
    }
}
