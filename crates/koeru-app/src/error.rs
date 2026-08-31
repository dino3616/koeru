//! アプリケーション境界の失敗。
//!
//! **ここより先に回復の余地は無い**ので、下の層の列挙体をここで畳む
//! （`rust-conventions`）。ただし畳むときに**2つに分ける。**
//!
//! - `kind` — 送信層へ載せてよい固定文字列。**種別だけ。**
//! - `message` — 画面に出す日本語。**パスも音源名も入りうる。**
//!
//! IPC の相手は同じ端末のウィンドウなので、`message` を渡してよい。
//! **送信層へ渡してよいのは `kind` だけ**（`TR-TEL` 系）。

use serde::Serialize;

/// 画面へ返す失敗。
#[derive(Debug, Clone, Serialize)]
pub struct AppError {
    /// 送信してよい種別文字列。
    pub kind: String,
    /// 画面に出す説明。**送信層へ載せない。**
    pub message: String,
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AppError {}

impl AppError {
    /// 種別と説明から作る。
    pub fn new(kind: impl Into<String>, message: impl std::fmt::Display) -> Self {
        Self {
            kind: kind.into(),
            message: message.to_string(),
        }
    }
}

/// `kind()` を持つ下の層のエラーを畳む。
///
/// **`kind()` を実装していない型は畳めない。** 種別文字列を持たない失敗が
/// 境界を越えると、送信層に何を載せてよいか決められなくなる。
macro_rules! from_domain {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<$t> for AppError {
                fn from(e: $t) -> Self {
                    Self::new(e.kind(), &e)
                }
            }
        )*
    };
}

from_domain!(
    koeru_audio::SessionError,
    koeru_audio::wav::WavError,
    koeru_core::db::LedgerError,
    koeru_core::project::ProjectError,
    koeru_core::handoff::HandoffError,
    koeru_core::text::TextError,
    koeru_core::frq::FrqError,
    koeru_core::reclist::ReclistError,
    koeru_synth::resampler::RenderError,
);

// **どの OS でも同じ形で畳む。** 書いていない OS では、
// これらは「この OS の音声入出力はまだ書いていない」1つの型に潰れている。
from_domain!(koeru_audio::backend::current::CaptureError);

// macOS では3つが別の型。書いていない OS では同じ型なので、重ねて実装できない。
#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
from_domain!(
    koeru_audio::backend::macos::CoreAudioError,
    koeru_audio::backend::macos::PlaybackError,
);

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        Self::new("app.io", e)
    }
}

/// コマンドの戻り値。
pub type Result<T> = std::result::Result<T, AppError>;
