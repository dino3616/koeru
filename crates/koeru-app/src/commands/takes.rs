//! テイクの名前と音高の記録の口。 SDL の `Mutation.renameTake` と `Take.pitch` を、
//! 刷新のあいだ tauri-specta で運ぶ（`DEC-PLT-061`）。
//!
//! 型と欄の名前は SDL（`specs/application/schema/recording.graphql`）に合わせる。
//! `lease`・`operationId` は取らない。 落とした入力と枝は `specs/application/README.md` の
//! 移行の表にある。

use serde::{Deserialize, Serialize};
use tauri::State;

use super::{AppState, Finite, TakeSummaryView, lock};
use crate::error::{AppError, FailureAction, FailureClass, FailureOutcome, Result};

/// [`rename_take`] の入力（`RenameTakeInput`）。
#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct RenameTakeInput {
    pub take: i32,
    /// 省くか、前後の空白を落として空なら、名前を消す。
    pub label: Option<String>,
}

/// [`rename_take`] の応答（`RenameTakePayload`）。 `operationId` を取らないので受領証は無い。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RenameTakePayload {
    pub outcome: RenameTakeOutcome,
}

/// `RenameTakeOutcome` の union。 枝の名前は `__typename` に載る。
///
/// `lease`・`operationId` を取らないので、それが古いときの枝
/// （`ProjectLeaseExpired`・`OperationIdReused`）は無い。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum RenameTakeOutcome {
    TakeRenamed { take: TakeSummaryView },
    ReferenceNotFound(ReferenceNotFound),
}

/// 指したテイクが台帳に無い（`ReferenceNotFound`）。
///
/// 欄の名前は SDL の `Problem` に合わせる。 分類・確定の状態・次の手の値は
/// [`AppError`] と同じ小文字の語で運ぶ。 SDL の大文字の値へ揃えるのは GraphQL へ移すとき。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ReferenceNotFound {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
}

impl ReferenceNotFound {
    /// 画面へ値として返すので、`AppError` の直列化を通らない。 ここで1回記録する
    /// （`DEC-PLT-038`）。
    fn of(e: AppError, phase: &'static str) -> Self {
        e.record(phase);
        Self {
            code: e.code,
            class: e.class,
            commit: e.outcome,
            actions: vec![e.action],
            message: e.message,
        }
    }
}

/// テイクに名前を付ける・変える・消す（`Mutation.renameTake`）。
///
/// `label` を `null` か空にすると消え、画面は既定の表示へ戻る。
#[tauri::command(async)]
#[specta::specta]
pub fn rename_take(
    state: State<'_, AppState>,
    input: RenameTakeInput,
) -> Result<RenameTakePayload> {
    let outcome = match lock(&state)?.rename_take(input.take, input.label.as_deref())? {
        Some(t) => RenameTakeOutcome::TakeRenamed { take: t.into() },
        None => RenameTakeOutcome::ReferenceNotFound(ReferenceNotFound::of(
            koeru_core::db::LedgerError::UnknownTake.into(),
            "take.rename",
        )),
    };
    Ok(RenameTakePayload { outcome })
}

/// 確定したテイクの音高の記録（`PitchTrack`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PitchTrack {
    /// 格子の間隔（マスターの標本数）。 `hz` の `i` 番目はテイクの先頭から `hop` × `i` 標本。
    pub hop: u32,
    /// 格子ごとの F0（Hz）。 無声・無音は `null`。
    pub hz: Vec<Option<Finite>>,
}

impl From<crate::studio::TakePitch> for PitchTrack {
    fn from(p: crate::studio::TakePitch) -> Self {
        Self {
            hop: p.hop,
            hz: p
                .f0
                .into_iter()
                .map(|v| (v.is_finite() && v > koeru_core::frq::UNVOICED).then_some(Finite(v)))
                .collect(),
        }
    }
}

/// 確定したテイクの音高の記録（`Take.pitch`）。
///
/// 解析がまだ置いていなければ `null`。 録っている途中には引かない（`DEC-PLT-036`）。
#[tauri::command(async)]
#[specta::specta]
pub fn take_pitch(state: State<'_, AppState>, take: i32) -> Result<Option<PitchTrack>> {
    Ok(lock(&state)?.take_pitch(take)?.map(Into::into))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 名前の付け直しの結果は、`RenameTakeOutcome` の枝の名前を `__typename` に持つ。
    /// 見つからない枝は `Problem` と同じ欄を並べる。
    #[test]
    fn テイクの名前の結果はsdlの枝の名前で返る() {
        let renamed = RenameTakeOutcome::TakeRenamed {
            take: TakeSummaryView {
                take_id: 7,
                generation: 2,
                peak: None,
                duration_ms: 1000.0,
                invalid: false,
                recorded_at: "2026-11-01T00:00:00Z".to_owned(),
                label: Some("ちょい明め".to_owned()),
            },
        };
        let json = serde_json::to_value(RenameTakePayload { outcome: renamed }).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "TakeRenamed");
        assert_eq!(json["outcome"]["take"]["label"], "ちょい明め");

        let missing = RenameTakeOutcome::ReferenceNotFound(ReferenceNotFound::of(
            koeru_core::db::LedgerError::UnknownTake.into(),
            "take.rename",
        ));
        let json = serde_json::to_value(RenameTakePayload { outcome: missing }).expect("書ける");
        let problem = &json["outcome"];
        assert_eq!(problem["__typename"], "ReferenceNotFound");
        assert_eq!(problem["code"], "ledger.unknown_take");
        for field in ["class", "commit", "message"] {
            assert!(problem[field].is_string(), "{field}: {problem}");
        }
        assert!(problem["actions"].is_array(), "{problem}");
    }

    /// 音高の記録は無声・無音を `null` にする。 0 Hz を音高として描かせない。
    #[test]
    fn 音高の記録は無声をnullにする() {
        let track = PitchTrack::from(crate::studio::TakePitch {
            hop: 256,
            f0: vec![0.0, 220.0, 221.5, 0.0, f64::NAN],
        });
        assert_eq!(track.hop, 256);
        let hz: Vec<Option<f64>> = track.hz.iter().map(|v| v.map(|f| f.0)).collect();
        assert_eq!(hz, [None, Some(220.0), Some(221.5), None, None]);
        let json = serde_json::to_value(&track).expect("書ける");
        assert_eq!(json["hz"][0], serde_json::Value::Null);
        assert_eq!(json["hz"][1], 220.0);
    }
}
