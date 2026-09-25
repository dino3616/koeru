//! アプリケーション境界の失敗。分類・確定の状態・次の手の決め方は `DEC-PLT-038`。
//!
//! 下の層の列挙体をここで畳む。 畳んでも分類と確定の状態は残す。
//! 画面は `class` と `action` で分岐し、`message` を解析しない。
//!
//! - `code` — 点区切りの固定文字列。送信層へ載せてよいのはこれと分類だけ
//! - `message` — 画面に出す日本語の固定の文。利用者のデータを差し込まない
//! - `source` — 下の層のエラー。同じ端末の診断のためだけに持ち、画面にも送信層にも出さない
//!
//! 移行中の形。 GraphQL の payload（`DEC-PLT-035`）に置き換わる。

use std::sync::Arc;

use koeru_failure::{Action, Class, Failure, Outcome};
use serde::Serialize;

/// 画面へ返す失敗。
#[derive(Debug, Clone, specta::Type)]
pub struct AppError {
    pub code: &'static str,
    pub class: FailureClass,
    /// 操作が確定したかどうか。 分類からの既定を、確定した工程を知る持ち主が上書きする。
    pub outcome: FailureOutcome,
    pub action: FailureAction,
    pub message: String,
    #[specta(skip)]
    source: Option<Arc<dyn std::error::Error + Send + Sync>>,
}

/// 画面へ渡すときに1回だけ記録する（`DEC-PLT-038`）。
///
/// 画面へ出る失敗は、コマンドの戻り値か [`crate::studio::TakeResult::followup`] の
/// どちらかで、どちらも IPC で1回だけ直列化される。 ここが結果の決まった最後の場所で、
/// 確定の状態も持ち主が上書きしたあとの値になっている。 コマンドを1本ずつ包まずに済む。
///
/// **移行中の置き場所。** GraphQL の実行口（`DEC-PLT-035`）ができたら、そこで記録する。
/// 失敗を画面へ渡さずに持ち主がその場で決めるとき（縮退して続けるとき）は、
/// ここを通らないので [`AppError::record`] を呼ぶ。
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        self.record("ipc");
        let mut st = s.serialize_struct("AppError", 5)?;
        st.serialize_field("code", self.code)?;
        st.serialize_field("class", &self.class)?;
        st.serialize_field("outcome", &self.outcome)?;
        st.serialize_field("action", &self.action)?;
        st.serialize_field("message", &self.message)?;
        st.end()
    }
}

/// source は比べない。 同じ code・分類・確定の状態・文言なら、画面にとって同じ失敗。
impl PartialEq for AppError {
    fn eq(&self, other: &Self) -> bool {
        self.code == other.code
            && self.class == other.class
            && self.outcome == other.outcome
            && self.action == other.action
            && self.message == other.message
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_deref()
            .map(|e| e as &(dyn std::error::Error + 'static))
    }
}

impl AppError {
    /// 境界で初めて分かる失敗を作る。下の層のエラーを包むなら [`AppError::from_failure`]。
    ///
    /// `message` は固定の文か、件数だけを差し込んだ文にする。
    pub fn new(code: &'static str, class: Class, message: impl Into<String>) -> Self {
        Self {
            code,
            class: class.into(),
            outcome: class.outcome().into(),
            action: class.action().into(),
            message: message.into(),
            source: None,
        }
    }

    /// 下の層のエラーを畳む。 code・分類・次の手・文言はエラー型が持つものを使う。
    pub fn from_failure<E>(e: E) -> Self
    where
        E: Failure + Send + Sync + 'static,
    {
        let class = e.class();
        Self {
            code: e.code(),
            class: class.into(),
            outcome: class.outcome().into(),
            action: e.action().into(),
            message: e.to_string(),
            source: Some(Arc::new(e)),
        }
    }

    /// 文言だけを、境界の文脈に合わせて言い直す。code と分類は変えない。
    #[must_use]
    pub fn saying(mut self, message: &'static str) -> Self {
        message.clone_into(&mut self.message);
        self
    }

    /// 確定の状態を、持ち主が知っているものに置き換える。
    #[must_use]
    pub fn with_outcome(mut self, outcome: Outcome) -> Self {
        self.outcome = outcome.into();
        self
    }

    /// 記録する。 結果を決める持ち主が1回だけ呼ぶ（`DEC-PLT-038`）。
    pub fn record(&self, phase: &'static str) {
        koeru_failure::record(self.code, self.class.into(), self.outcome.into(), phase);
    }
}

/// [`Failure`] を実装した下の層のエラーを `?` で畳めるようにする。
///
/// 総称の `impl<E: Failure> From<E>` は置けない。 `koeru-failure` が将来
/// `std::io::Error` に `Failure` を実装しうるので、下の `From<std::io::Error>` と衝突する。
macro_rules! from_failure {
    ($($t:ty),* $(,)?) => {
        $(
            impl From<$t> for AppError {
                fn from(e: $t) -> Self {
                    Self::from_failure(e)
                }
            }
        )*
    };
}

from_failure!(
    koeru_audio::SessionError,
    koeru_audio::wav::WavError,
    koeru_core::db::LedgerError,
    koeru_core::project::ProjectError,
    koeru_core::handoff::HandoffError,
    koeru_core::text::TextError,
    koeru_core::frq::FrqError,
    koeru_core::frq::FrqWriteError,
    koeru_core::reclist::ReclistError,
    koeru_core::preset::PresetError,
    koeru_core::tone::ToneError,
    koeru_core::ust::UstError,
    koeru_align::review::ReviewError,
    koeru_synth::resampler::RenderError,
    koeru_package::icon::IconError,
    koeru_package::tree::BuildError,
    koeru_package::archive::ArchiveError,
    crate::pump::PumpError,
);

// どの OS でも同じ形で畳む。 書いていない OS では、
// これらは「この OS の音声入出力はまだ書いていない」1つの型に潰れている。
from_failure!(koeru_audio::backend::current::CaptureError);

// macOS では3つが別の型。書いていない OS では同じ型なので、重ねて実装できない。
#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
from_failure!(
    koeru_audio::backend::macos::CoreAudioError,
    koeru_audio::backend::macos::PlaybackError,
);

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        let class = koeru_failure::io_class(&e);
        Self {
            source: Some(Arc::new(e)),
            ..Self::new("app.io", class, "ファイルの読み書きが失敗した")
        }
    }
}

/// コマンドの戻り値。
pub type Result<T> = std::result::Result<T, AppError>;

// ## 画面へ渡す形
//
// `koeru-failure` は依存を持たない葉なので、specta の型をここで写す。
// 変種を足すと `From` の網羅が落ちるので、写し忘れは組み立てで分かる。

/// [`Class`] の写し。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FailureClass {
    InvalidInput,
    Rejected,
    Conflict,
    Cancelled,
    AlreadyCommitted,
    Busy,
    DeviceUnavailable,
    TransientIo,
    Storage,
    Corrupt,
    Unsupported,
    EngineFailed,
    Internal,
}

impl From<Class> for FailureClass {
    fn from(c: Class) -> Self {
        match c {
            Class::InvalidInput => Self::InvalidInput,
            Class::Rejected => Self::Rejected,
            Class::Conflict => Self::Conflict,
            Class::Cancelled => Self::Cancelled,
            Class::AlreadyCommitted => Self::AlreadyCommitted,
            Class::Busy => Self::Busy,
            Class::DeviceUnavailable => Self::DeviceUnavailable,
            Class::TransientIo => Self::TransientIo,
            Class::Storage => Self::Storage,
            Class::Corrupt => Self::Corrupt,
            Class::Unsupported => Self::Unsupported,
            Class::EngineFailed => Self::EngineFailed,
            Class::Internal => Self::Internal,
        }
    }
}

impl From<FailureClass> for Class {
    fn from(c: FailureClass) -> Self {
        match c {
            FailureClass::InvalidInput => Self::InvalidInput,
            FailureClass::Rejected => Self::Rejected,
            FailureClass::Conflict => Self::Conflict,
            FailureClass::Cancelled => Self::Cancelled,
            FailureClass::AlreadyCommitted => Self::AlreadyCommitted,
            FailureClass::Busy => Self::Busy,
            FailureClass::DeviceUnavailable => Self::DeviceUnavailable,
            FailureClass::TransientIo => Self::TransientIo,
            FailureClass::Storage => Self::Storage,
            FailureClass::Corrupt => Self::Corrupt,
            FailureClass::Unsupported => Self::Unsupported,
            FailureClass::EngineFailed => Self::EngineFailed,
            FailureClass::Internal => Self::Internal,
        }
    }
}

/// [`Outcome`] の写し。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FailureOutcome {
    NotStarted,
    NotCommitted,
    Committed,
    Unknown,
}

impl From<Outcome> for FailureOutcome {
    fn from(o: Outcome) -> Self {
        match o {
            Outcome::NotStarted => Self::NotStarted,
            Outcome::NotCommitted => Self::NotCommitted,
            Outcome::Committed => Self::Committed,
            Outcome::Unknown => Self::Unknown,
        }
    }
}

impl From<FailureOutcome> for Outcome {
    fn from(o: FailureOutcome) -> Self {
        match o {
            FailureOutcome::NotStarted => Self::NotStarted,
            FailureOutcome::NotCommitted => Self::NotCommitted,
            FailureOutcome::Committed => Self::Committed,
            FailureOutcome::Unknown => Self::Unknown,
        }
    }
}

/// [`Action`] の写し。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum FailureAction {
    FixInput,
    MeetCondition,
    Refresh,
    Nothing,
    ShowReceipt,
    Wait,
    Reconnect,
    Grant,
    RetrySame,
    FreeSpace,
    Recover,
    UseAvailable,
    Rebuild,
    Report,
}

impl From<Action> for FailureAction {
    fn from(a: Action) -> Self {
        match a {
            Action::FixInput => Self::FixInput,
            Action::MeetCondition => Self::MeetCondition,
            Action::Refresh => Self::Refresh,
            Action::Nothing => Self::Nothing,
            Action::ShowReceipt => Self::ShowReceipt,
            Action::Wait => Self::Wait,
            Action::Reconnect => Self::Reconnect,
            Action::Grant => Self::Grant,
            Action::RetrySame => Self::RetrySame,
            Action::FreeSpace => Self::FreeSpace,
            Action::Recover => Self::Recover,
            Action::UseAvailable => Self::UseAvailable,
            Action::Rebuild => Self::Rebuild,
            Action::Report => Self::Report,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 畳んでも分類と原因を失わない() {
        let e = AppError::from(koeru_core::db::LedgerError::UnknownTake);
        assert_eq!(e.code, "ledger.unknown_take");
        assert_eq!(e.class, FailureClass::Conflict);
        assert_eq!(e.action, FailureAction::Refresh);
        assert!(std::error::Error::source(&e).is_some(), "source を捨てない");
    }

    #[test]
    fn 画面へは原因を送らない() {
        let e = AppError::from(std::io::Error::other("/Users/someone/秘密.wav"));
        let json = serde_json::to_string(&e).expect("書ける");
        assert!(!json.contains("someone"), "{json}");
        assert!(!json.contains("source"), "{json}");
    }

    #[test]
    fn 確定したあとの失敗は確定済みとして返せる() {
        let e = AppError::from(koeru_core::frq::FrqError::LengthMismatch)
            .with_outcome(Outcome::Committed);
        assert_eq!(e.outcome, FailureOutcome::Committed);
    }
}
