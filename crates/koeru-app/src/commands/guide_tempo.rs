//! ガイドのテンポの口。 SDL の `RecordingFacet.guideTempo` と `Mutation.setGuideTempo` を、
//! 刷新のあいだ tauri-specta で運ぶ（`DEC-PLT-061`）。
//!
//! 型と欄の名前は SDL（`specs/application/schema/recording.graphql`）に合わせる。
//! 画面の型名が SDL の型名になり、GraphQL へ移すときに画面の側を書き換えずに済む。
//! 入力の `lease` と `operationId` は運ばない。 貸与と操作の照合はまだこの経路に無く、
//! 照合しない値を受け取ると、照合したように見える。

use serde::{Deserialize, Serialize};
use tauri::State;

use koeru_core::guide::GuideTempo as Tempo;
use koeru_failure::Class;

use super::problem::{InputViolation, InvalidRequest, Problem};
use super::{AppState, Finite, lock};
use crate::error::{AppError, FailureClass, Result};
use crate::studio::GuideTempoChange;

/// ガイドのテンポ（`GuideTempo`）。 範囲と刻みも渡すので、画面は数を写さない
/// （`DEC-REC-012`）。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GuideTempo {
    pub bpm: u16,
    pub beat_ms: Finite,
    pub default_bpm: u16,
    pub min_bpm: u16,
    pub max_bpm: u16,
    pub step_bpm: u16,
    /// 発声を始めるまでの助走（`TR-REC-23`）。 画面のカウントはこれを拍で数える。
    pub lead_in_ms: Finite,
}

impl From<Tempo> for GuideTempo {
    fn from(t: Tempo) -> Self {
        Self {
            bpm: t.bpm(),
            beat_ms: t.beat_ms().into(),
            default_bpm: Tempo::DEFAULT.bpm(),
            min_bpm: Tempo::MIN_BPM,
            max_bpm: Tempo::MAX_BPM,
            step_bpm: Tempo::STEP_BPM,
            lead_in_ms: koeru_core::guide::DEFAULT_LEAD_IN_MS.into(),
        }
    }
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct SetGuideTempoInput {
    pub bpm: i32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SetGuideTempoPayload {
    pub outcome: SetGuideTempoOutcome,
}

/// `SetGuideTempoOutcome` の union。 枝の名前は `__typename` に載る。
///
/// SDL の `ProjectLeaseExpired` と `OperationIdReused` は、照合する入力を運ばないので枝も無い。
/// プロジェクトを開いていないときは、ほかのコマンドと同じく失敗で返す。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum SetGuideTempoOutcome {
    GuideTempoSet { tempo: GuideTempo },
    InvalidRequest(InvalidRequest),
    ActionRejected(Problem),
    StorageProblem(Problem),
}

/// 画面へ値として返すので、`AppError` の直列化を通らない。 ここで1回記録する
/// （`DEC-PLT-038`）。
fn problem(e: &AppError) -> Problem {
    e.record("recording.set_guide_tempo");
    Problem::of(e)
}

const OUT_OF_RANGE: &str = "recording.guide_tempo_out_of_range";

fn out_of_range() -> InvalidRequest {
    let e = AppError::new(
        OUT_OF_RANGE,
        Class::InvalidInput,
        "選べないテンポ。範囲と刻みの中から選んでほしい",
    );
    InvalidRequest::of(
        problem(&e),
        vec![InputViolation {
            field: "bpm",
            code: OUT_OF_RANGE,
        }],
    )
}

fn while_recording() -> Problem {
    problem(&AppError::new(
        "recording.guide_tempo_while_recording",
        Class::Rejected,
        "録っている途中はガイドのテンポを変えない。止めてから変えてほしい",
    ))
}

/// 変えた結果を union へ写す。 台帳に書けなかったものは `StorageProblem` の枝へ、
/// 枝の無い失敗（プロジェクトを開いていないなど）は `Err` のまま返す。
fn outcome_of(r: Result<GuideTempoChange>) -> Result<SetGuideTempoOutcome> {
    match r {
        Ok(GuideTempoChange::Set(t)) => Ok(SetGuideTempoOutcome::GuideTempoSet { tempo: t.into() }),
        Ok(GuideTempoChange::Recording) => {
            Ok(SetGuideTempoOutcome::ActionRejected(while_recording()))
        }
        Err(e) => match e.class {
            FailureClass::Storage | FailureClass::TransientIo | FailureClass::Corrupt => {
                Ok(SetGuideTempoOutcome::StorageProblem(problem(&e)))
            }
            _ => Err(e),
        },
    }
}

/// ガイドのテンポ（`RecordingFacet.guideTempo`）。
#[tauri::command(async)]
#[specta::specta]
pub fn guide_tempo(state: State<'_, AppState>) -> Result<GuideTempo> {
    Ok(lock(&state)?.guide_tempo()?.into())
}

/// ガイドのテンポを変える（`Mutation.setGuideTempo`）。 録ったテイクは変えない。
#[tauri::command(async)]
#[specta::specta]
pub fn set_guide_tempo(
    state: State<'_, AppState>,
    input: SetGuideTempoInput,
) -> Result<SetGuideTempoPayload> {
    let Some(tempo) = Tempo::new(input.bpm) else {
        return Ok(SetGuideTempoPayload {
            outcome: SetGuideTempoOutcome::InvalidRequest(out_of_range()),
        });
    };
    let outcome = outcome_of(lock(&state)?.set_guide_tempo(tempo))?;
    Ok(SetGuideTempoPayload { outcome })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 範囲と刻みを数で渡す() {
        let t = GuideTempo::from(Tempo::new(90).expect("選べる"));
        let v = serde_json::to_value(t).expect("直列化できる");
        assert_eq!(
            v,
            serde_json::json!({
                "bpm": 90,
                "beatMs": 60_000.0 / 90.0,
                "defaultBpm": 120,
                "minBpm": 60,
                "maxBpm": 180,
                "stepBpm": 5,
                "leadInMs": 1250.0,
            })
        );
    }

    #[test]
    fn 枝の名前を_typename_に載せる() {
        let set = SetGuideTempoOutcome::GuideTempoSet {
            tempo: Tempo::DEFAULT.into(),
        };
        let v = serde_json::to_value(&set).expect("直列化できる");
        assert_eq!(v["__typename"], "GuideTempoSet");
        assert_eq!(v["tempo"]["bpm"], 120);

        let bad = serde_json::to_value(SetGuideTempoOutcome::InvalidRequest(out_of_range()))
            .expect("直列化できる");
        assert_eq!(bad["__typename"], "InvalidRequest");
        assert_eq!(bad["code"], OUT_OF_RANGE);
        assert_eq!(bad["class"], "invalid_input");
        assert_eq!(bad["violations"][0]["field"], "bpm");
    }

    #[test]
    fn 録っている途中は拒否の枝で返す() {
        let out = outcome_of(Ok(GuideTempoChange::Recording)).expect("枝へ写る");
        let v = serde_json::to_value(&out).expect("直列化できる");
        assert_eq!(v["__typename"], "ActionRejected");
        assert_eq!(v["code"], "recording.guide_tempo_while_recording");
        assert_eq!(v["class"], "rejected");
    }

    #[test]
    fn 書けなければ保存の枝で返し_開いていなければ失敗のまま返す() {
        let full = AppError::new("storage.full", Class::Storage, "空きが無い");
        let v =
            serde_json::to_value(outcome_of(Err(full)).expect("枝へ写る")).expect("直列化できる");
        assert_eq!(v["__typename"], "StorageProblem");

        let none = AppError::new("app.no_project", Class::Rejected, "無い");
        assert!(outcome_of(Err(none)).is_err());
    }
}
