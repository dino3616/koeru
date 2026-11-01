//! 同意の口。 SDL の `Query.privacy` と `Mutation.choosePrivacyConsent` を、
//! 刷新のあいだ tauri-specta で運ぶ（`DEC-PLT-061`）。
//!
//! 型と欄の名前は SDL（`specs/application/schema/privacy.graphql`）に合わせる。
//! 画面の型名が SDL の型名になり、GraphQL へ移すときに画面の側を書き換えずに済む。
//! 入力の `operationId` と payload の `receipt`、`OperationIdReused` の枝は運ばない
//! （`specs/application/README.md` の移行表）。
//!
//! 状態は `AppState` の外に置く（Tauri の別の managed state）。 `studio` のロックを
//! 取らないので、テイクの確定中でも設定の面が待たされない。

use serde::{Deserialize, Serialize};
use tauri::State;

use koeru_model::consent::Consent;
use koeru_runtime::privacy::PrivacyStore;

use crate::error::{AppError, FailureAction, FailureClass, FailureOutcome, Result};

/// 同意と送った記録（`PrivacySettings`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PrivacySettings {
    pub usage: ConsentState,
    pub crash_reports: ConsentState,
    /// RFC 3339。 `null` なら一度も送っていない。 送る経路がまだ無いので、今は常に `null`。
    pub last_sent_at: Option<String>,
}

impl PrivacySettings {
    fn of(p: &koeru_runtime::privacy::PrivacySettings) -> Self {
        Self {
            usage: p.consents.usage.into(),
            crash_reports: p.consents.crash_reports.into(),
            last_sent_at: p.last_sent_at.clone(),
        }
    }
}

/// 同意の状態（`ConsentState`）。 送るのは `GRANTED` のときだけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsentState {
    NotAsked,
    Granted,
    Revoked,
}

impl From<Consent> for ConsentState {
    fn from(c: Consent) -> Self {
        match c {
            Consent::NotAsked => Self::NotAsked,
            Consent::Granted => Self::Granted,
            Consent::Revoked => Self::Revoked,
        }
    }
}

/// 同意の単位（`ConsentSubject`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConsentSubject {
    Usage,
    CrashReports,
}

impl From<ConsentSubject> for koeru_model::consent::ConsentSubject {
    fn from(s: ConsentSubject) -> Self {
        match s {
            ConsentSubject::Usage => Self::Usage,
            ConsentSubject::CrashReports => Self::CrashReports,
        }
    }
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct ChoosePrivacyConsentInput {
    pub subject: ConsentSubject,
    pub granted: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ChoosePrivacyConsentPayload {
    pub outcome: ChoosePrivacyConsentOutcome,
}

/// `ChoosePrivacyConsentOutcome` の union。 枝の名前は `__typename` に載る。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum ChoosePrivacyConsentOutcome {
    PrivacyConsentChosen { privacy: PrivacySettings },
    StorageProblem(StorageProblem),
}

/// 保存できなかった（`StorageProblem`）。 書けなかったとき、同意は前のまま。
///
/// 欄の名前は SDL の `Problem` に合わせる。 分類・確定の状態・次の手の値は
/// [`AppError`] と同じ小文字の語で運ぶ。 SDL の大文字の値へ揃えるのは GraphQL へ移すとき。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct StorageProblem {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
}

impl StorageProblem {
    /// 画面へ値として返すので、`AppError` の直列化を通らない。 ここで1回記録する
    /// （`DEC-PLT-038`）。
    fn of(e: koeru_runtime::privacy::PrivacyError) -> Self {
        let e = AppError::from_failure(e);
        e.record("privacy.choose");
        Self {
            code: e.code,
            class: e.class,
            commit: e.outcome,
            actions: vec![e.action],
            message: e.message,
        }
    }
}

/// 同意と送った記録（`Query.privacy`）。 project を開かずに読める。
#[tauri::command(async)]
#[specta::specta]
pub fn privacy(store: State<'_, PrivacyStore>) -> Result<PrivacySettings> {
    let p = store.load().map_err(AppError::from_failure)?;
    Ok(PrivacySettings::of(&p))
}

/// 1つの同意を本人が選ぶ（`Mutation.choosePrivacyConsent`）。 もう片方は動かない。
#[tauri::command(async)]
#[specta::specta]
pub fn choose_privacy_consent(
    store: State<'_, PrivacyStore>,
    input: ChoosePrivacyConsentInput,
) -> ChoosePrivacyConsentPayload {
    let outcome = match store.choose(input.subject.into(), input.granted) {
        Ok(p) => ChoosePrivacyConsentOutcome::PrivacyConsentChosen {
            privacy: PrivacySettings::of(&p),
        },
        Err(e) => ChoosePrivacyConsentOutcome::StorageProblem(StorageProblem::of(e)),
    };
    ChoosePrivacyConsentPayload { outcome }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定は_sdl_の形でどちらも訊いておらず_送った記録も無い() {
        let p = PrivacySettings::of(&koeru_runtime::privacy::PrivacySettings::default());
        let json = serde_json::to_value(&p).expect("書ける");
        assert_eq!(
            json,
            serde_json::json!({
                "usage": "NOT_ASKED",
                "crashReports": "NOT_ASKED",
                "lastSentAt": null,
            })
        );
    }

    #[test]
    fn 結果の枝は_typename_で分かれる() {
        let chosen = ChoosePrivacyConsentPayload {
            outcome: ChoosePrivacyConsentOutcome::PrivacyConsentChosen {
                privacy: PrivacySettings {
                    usage: ConsentState::Granted,
                    crash_reports: ConsentState::NotAsked,
                    last_sent_at: None,
                },
            },
        };
        let json = serde_json::to_value(&chosen).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "PrivacyConsentChosen");
        assert_eq!(json["outcome"]["privacy"]["usage"], "GRANTED");

        let failed = ChoosePrivacyConsentPayload {
            outcome: ChoosePrivacyConsentOutcome::StorageProblem(StorageProblem::of(
                koeru_runtime::privacy::PrivacyError::Write(std::io::Error::from(
                    std::io::ErrorKind::PermissionDenied,
                )),
            )),
        };
        let json = serde_json::to_value(&failed).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "StorageProblem");
        assert_eq!(json["outcome"]["code"], "privacy.unwritable");
    }

    #[test]
    fn 画面から来る単位は_sdl_の値で読む() {
        let input: ChoosePrivacyConsentInput =
            serde_json::from_str(r#"{"subject":"CRASH_REPORTS","granted":true}"#).expect("読める");
        assert_eq!(input.subject, ConsentSubject::CrashReports);
    }
}
