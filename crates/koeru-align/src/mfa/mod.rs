//! MFA 日本語音響モデルの経路（`TR-ALN-05`, `DEC-ALN-008`）。
//!
//! **Kaldi を組めない OS では、空の実装を置く。** `koeru-audio` の
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
//! **cargo の feature にしない。** feature は加算的であるべきで、
//! `--all-features` で挙動が変わるものを混ぜると macOS の CI がスタブを検査しはじめる。

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
mod kaldi;

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
mod unsupported;

#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
pub use kaldi::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError};

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
pub use unsupported::{FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner, MfaError};
