//! サブバンクの枠の口（`DEC-RCL-018`）。 SDL の `RecordingFacet.subbanks`・`voiceColors` と、
//! 枠を足す・付け替える・並べ替える・担当音域を直す・消す Mutation を、刷新のあいだ
//! tauri-specta で運ぶ（`DEC-PLT-061`）。
//!
//! 型と欄の名前は SDL（`specs/application/schema/recording.graphql`）に合わせる。 入力の
//! `lease`・`operationId`・`baseRevision` と payload の `receipt` は運ばない。 ここには
//! 貸与も受領証も無い。 落とした欄は `specs/application/README.md` の移行の表が挙げる。
//!
//! **どれも `open_project` を呼ばない**（`TR-REC-30`）。 開いている収録セッションは続く。

use serde::{Deserialize, Serialize};
use tauri::State;

use koeru_failure::Class;
use koeru_model::id::SubbankId;

use super::{AppState, count, lock};
use crate::error::{AppError, FailureAction, FailureClass, FailureOutcome, Result};
use crate::studio::{
    AddSubbank, ReviseSubbank, SubbankDenied, SubbankRemoval, SubbankSummary, Violation,
};
use koeru_core::db::subbank::SubbankRefusal;

/// 作り方（`RecordingMethod`）。 枠は多音階を名乗らない——音階は枠ごとに1つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RecordingMethod {
    Single,
    Sequential,
    Cvvc,
    MultiPitchSequential,
}

impl From<koeru_core::alias::Method> for RecordingMethod {
    fn from(m: koeru_core::alias::Method) -> Self {
        match m {
            koeru_core::alias::Method::Single => Self::Single,
            koeru_core::alias::Method::Sequential => Self::Sequential,
            koeru_core::alias::Method::Cvvc => Self::Cvvc,
        }
    }
}

/// 音高（`Tone`）。 画面に出すのは `name` で、MIDI 番号は出さない（`TR-REC-25`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Tone {
    pub midi: i32,
    pub name: String,
}

impl Tone {
    fn of(midi: i32) -> Self {
        Self {
            midi,
            name: koeru_core::tone::name(midi),
        }
    }
}

/// 担当音域（`ToneRange`、`TR-RCL-31`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct ToneRange {
    pub low: Tone,
    pub high: Tone,
    /// 本人が直した範囲なら true。 false なら floor 割り当ての既定。
    pub edited: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubbankProgress {
    /// 採用テイクのある行。
    pub recorded_rows: u32,
    pub rows: u32,
    /// 固定の見積もり（`DEC-RCL-013`）。
    pub remaining_seconds: u32,
}

/// サブバンクの枠（`Subbank`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Subbank {
    /// `SubbankId`。 wire では文字列で、画面は中身を解釈しない。
    pub id: String,
    /// `null` は声色なし。
    pub voice_color: Option<String>,
    pub method: RecordingMethod,
    /// 作ったときの方式プリセット。 古い project は持たない。
    pub preset: Option<String>,
    pub tone: Tone,
    pub tone_range: ToneRange,
    pub progress: SubbankProgress,
    /// 確定したテイクがあれば true。 方式を変えられない（`TR-RCL-30`）。
    pub has_takes: bool,
}

impl Subbank {
    fn of(s: &SubbankSummary) -> Self {
        Self {
            id: s.id.to_string(),
            voice_color: s.voice_color.as_ref().map(|c| c.as_str().to_owned()),
            method: s.method.into(),
            preset: s.preset_id.clone(),
            tone: Tone::of(s.tone),
            tone_range: ToneRange {
                low: Tone::of(s.range.low),
                high: Tone::of(s.range.high),
                edited: s.range_edited,
            },
            progress: SubbankProgress {
                recorded_rows: count(s.recorded_rows),
                rows: count(s.rows),
                remaining_seconds: s.remaining_seconds.max(0.0) as u32,
            },
            has_takes: s.takes > 0,
        }
    }
}

/// 声色（`VoiceColor`）。 同じ綴りの声色を持つ枠の束。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct VoiceColor {
    /// `null` は声色なし。
    pub name: Option<String>,
    /// 枠の並びの順。
    pub subbanks: Vec<Subbank>,
}

/// 束ねる。 並びは、その声色の枠が最初に現れた順。
fn voice_colors_of(subbanks: Vec<Subbank>) -> Vec<VoiceColor> {
    let mut out: Vec<VoiceColor> = Vec::new();
    for s in subbanks {
        match out.iter_mut().find(|v| v.name == s.voice_color) {
            Some(v) => v.subbanks.push(s),
            None => out.push(VoiceColor {
                name: s.voice_color.clone(),
                subbanks: vec![s],
            }),
        }
    }
    out
}

/// 予期できる失敗の共通の形（`Problem`）。 欄だけを持つ枝はこれを運ぶ。
///
/// 分類・確定の状態・次の手の値は [`AppError`] と同じ小文字の語で運ぶ。 SDL の大文字の値へ
/// 揃えるのは GraphQL へ移すとき。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct Problem {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
}

impl Problem {
    fn of(e: &AppError) -> Self {
        Self {
            code: e.code,
            class: e.class,
            commit: e.outcome,
            actions: vec![e.action],
            message: e.message.clone(),
        }
    }

    fn new(code: &'static str, class: Class, message: &'static str) -> Self {
        Self::of(&AppError::new(code, class, message))
    }
}

/// 同じ声色・方式・収録音階の枠が、もうある（`SubbankConflict`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct SubbankConflict {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub existing: String,
}

/// 読んだ版が古い（`RevisionConflict`）。
///
/// 画面が読んでいた版（`baseRevision`）を運ばないので、`expected` も今の版になる。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct RevisionConflict {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub expected: String,
    pub actual: String,
}

/// 入力が不正（`InvalidRequest`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct InvalidRequest {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub violations: Vec<InputViolation>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct InputViolation {
    /// 入力の中の場所。 欄の名前を . でつなぐ。
    pub field: &'static str,
    pub code: &'static str,
}

/// 同じ操作の識別子で、違う要求が来た（`OperationIdReused`）。
///
/// 操作の識別子を運ばないので、この枝は返らない。 SDL の union と同じ枝を持つために置く。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationIdReused {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub operation_id: String,
}

fn conflict(existing: SubbankId) -> SubbankConflict {
    let p = Problem::new(
        "subbank.conflict",
        Class::Conflict,
        "同じ声色・方式・収録音階の枠がもうある",
    );
    SubbankConflict {
        code: p.code,
        class: p.class,
        commit: p.commit,
        actions: p.actions,
        message: p.message,
        existing: existing.to_string(),
    }
}

fn invalid(violations: Vec<InputViolation>) -> InvalidRequest {
    let p = Problem::new(
        "subbank.invalid_input",
        Class::InvalidInput,
        "入力が正しくない",
    );
    InvalidRequest {
        code: p.code,
        class: p.class,
        commit: p.commit,
        actions: p.actions,
        message: p.message,
        violations,
    }
}

fn violation(v: Violation) -> InvalidRequest {
    invalid(vec![InputViolation {
        field: v.field,
        code: v.code,
    }])
}

/// 消えるテイクの数が、本人に示した数と違う。 版を読み直させる。
fn takes_changed(studio: &mut crate::studio::Studio) -> Result<RevisionConflict> {
    let now = studio.ledger_revision()?.to_string();
    let p = Problem::new(
        "subbank.takes_changed",
        Class::Conflict,
        "消えるテイクの数が変わった。読み直してほしい",
    );
    Ok(RevisionConflict {
        code: p.code,
        class: p.class,
        commit: p.commit,
        actions: p.actions,
        message: p.message,
        expected: now.clone(),
        actual: now,
    })
}

fn not_found() -> Problem {
    Problem::of(&koeru_core::db::LedgerError::UnknownSubbank.into())
}

fn rejected(r: &SubbankRefusal) -> Option<Problem> {
    Some(match r {
        SubbankRefusal::MethodLocked => Problem::new(
            "subbank.method_locked",
            Class::Rejected,
            "録ってある枠の方式は変えられない",
        ),
        SubbankRefusal::Recording => Problem::new(
            "subbank.recording",
            Class::Rejected,
            "録っている途中は変えられない。録り終えてからにしてほしい",
        ),
        SubbankRefusal::RecoveryPending => Problem::new(
            "subbank.recovery_pending",
            Class::Rejected,
            "復旧候補が残っている。採るか捨ててからにしてほしい",
        ),
        _ => return None,
    })
}

/// その操作の union に無い断りが来た。 実装の欠陥で、結果ではない（`DEC-PLT-038`）。
fn unexpected() -> AppError {
    AppError::new(
        "subbank.unexpected_refusal",
        Class::Internal,
        "枠の操作が想定していない理由で断られた",
    )
}

/// 指した枠の識別子を読む。 読めない識別子は、この版に無いものとして扱う。
fn subbank_id(s: &str) -> Option<SubbankId> {
    s.parse().ok().map(SubbankId::new)
}

/// 台帳やファイルを扱えなかった失敗を、union の枝へ写す。 写せないものは `Err` のまま。
enum Escalated {
    ProjectLeaseExpired(Problem),
    StorageProblem(Problem),
}

fn escalate(e: AppError, phase: &'static str) -> Result<Escalated> {
    let escalated = if e.code == "app.no_project" {
        Escalated::ProjectLeaseExpired(Problem::of(&e))
    } else if matches!(
        e.class,
        FailureClass::Storage | FailureClass::TransientIo | FailureClass::Corrupt
    ) {
        Escalated::StorageProblem(Problem::of(&e))
    } else {
        return Err(e);
    };
    // 画面へ値として返すので、`AppError` の直列化を通らない。 ここで1回記録する。
    e.record(phase);
    Ok(escalated)
}

macro_rules! escalates {
    ($($outcome:ident),* $(,)?) => {
        $(
            impl From<Escalated> for $outcome {
                fn from(e: Escalated) -> Self {
                    match e {
                        Escalated::ProjectLeaseExpired(p) => Self::ProjectLeaseExpired(p),
                        Escalated::StorageProblem(p) => Self::StorageProblem(p),
                    }
                }
            }
        )*
    };
}

escalates!(
    AddSubbankOutcome,
    ReviseSubbankOutcome,
    ReorderSubbanksOutcome,
    SetSubbankToneRangeOutcome,
    RemoveSubbankOutcome,
);

// ## 読み

/// 枠を並びの順で（`RecordingFacet.subbanks`）。 並びが録る順の既定（`TR-RCL-28`）。
#[tauri::command(async)]
#[specta::specta]
pub fn subbanks(state: State<'_, AppState>) -> Result<Vec<Subbank>> {
    Ok(lock(&state)?.subbanks()?.iter().map(Subbank::of).collect())
}

/// 声色ごとに枠を束ねたもの（`RecordingFacet.voiceColors`）。
#[tauri::command(async)]
#[specta::specta]
pub fn voice_colors(state: State<'_, AppState>) -> Result<Vec<VoiceColor>> {
    let subbanks = lock(&state)?.subbanks()?.iter().map(Subbank::of).collect();
    Ok(voice_colors_of(subbanks))
}

// ## 足す

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddSubbankInput {
    /// 省くと声色なし。
    pub voice_color: Option<String>,
    /// `Catalog.methodPresets` の `id`。 方式はプリセットが決める。
    pub preset: String,
    /// 収録音階（MIDI）。
    pub tone: i32,
    /// この枠の後ろに置く。 省くと末尾。
    pub after: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct AddSubbankPayload {
    pub outcome: AddSubbankOutcome,
}

/// `AddSubbankOutcome` の union。 枝の名前は `__typename` に載る。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum AddSubbankOutcome {
    SubbankAdded { subbank: Subbank },
    SubbankConflict(SubbankConflict),
    ReferenceNotFound(Problem),
    InvalidRequest(InvalidRequest),
    StorageProblem(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

/// 枠を足す（`Mutation.addSubbank`、`TR-RCL-28`）。 足した枠は未収録から始まる。
#[tauri::command(async)]
#[specta::specta]
pub fn add_subbank(
    state: State<'_, AppState>,
    input: AddSubbankInput,
) -> Result<AddSubbankPayload> {
    let after = match input.after.as_deref().map(subbank_id) {
        None => None,
        Some(Some(id)) => Some(id),
        Some(None) => {
            return Ok(AddSubbankPayload {
                outcome: AddSubbankOutcome::ReferenceNotFound(not_found()),
            });
        }
    };
    let answer = lock(&state)?.add_subbank(&AddSubbank {
        voice_color: input.voice_color.as_deref(),
        preset: &input.preset,
        tone: input.tone,
        after,
    });
    let outcome = match answer {
        Ok(Ok(s)) => AddSubbankOutcome::SubbankAdded {
            subbank: Subbank::of(&s),
        },
        Ok(Err(SubbankDenied::Invalid(v))) => AddSubbankOutcome::InvalidRequest(violation(v)),
        Ok(Err(SubbankDenied::Refused(r))) => match r {
            SubbankRefusal::Conflict { existing } => {
                AddSubbankOutcome::SubbankConflict(conflict(existing))
            }
            SubbankRefusal::UnknownSubbank => AddSubbankOutcome::ReferenceNotFound(not_found()),
            _ => return Err(unexpected()),
        },
        Err(e) => escalate(e, "subbank.add")?.into(),
    };
    Ok(AddSubbankPayload { outcome })
}

// ## 付け替える

/// 省いた欄は変えない。 声色を声色なしへ戻すときは `clearVoiceColor` を使う。
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviseSubbankInput {
    pub subbank: String,
    pub voice_color: Option<String>,
    pub clear_voice_color: Option<bool>,
    /// 収録音階（MIDI）。 付け替えても音声は変えない（`TR-RCL-30`）。
    pub tone: Option<i32>,
    /// 録ってある枠では断る（`TR-RCL-30`）。
    pub preset: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ReviseSubbankPayload {
    pub outcome: ReviseSubbankOutcome,
}

/// `ReviseSubbankOutcome` の union。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum ReviseSubbankOutcome {
    SubbankRevised { subbank: Subbank },
    SubbankConflict(SubbankConflict),
    RevisionConflict(RevisionConflict),
    ActionRejected(Problem),
    ReferenceNotFound(Problem),
    InvalidRequest(InvalidRequest),
    StorageProblem(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

/// 枠の声色・収録音階・方式を付け替える（`Mutation.reviseSubbank`、`TR-RCL-30`）。
#[tauri::command(async)]
#[specta::specta]
pub fn revise_subbank(
    state: State<'_, AppState>,
    input: ReviseSubbankInput,
) -> Result<ReviseSubbankPayload> {
    let Some(id) = subbank_id(&input.subbank) else {
        return Ok(ReviseSubbankPayload {
            outcome: ReviseSubbankOutcome::ReferenceNotFound(not_found()),
        });
    };
    let answer = lock(&state)?.revise_subbank(
        id,
        &ReviseSubbank {
            voice_color: input.voice_color.as_deref(),
            clear_voice_color: input.clear_voice_color.unwrap_or(false),
            tone: input.tone,
            preset: input.preset.as_deref(),
        },
    );
    let outcome = match answer {
        Ok(Ok(s)) => ReviseSubbankOutcome::SubbankRevised {
            subbank: Subbank::of(&s),
        },
        Ok(Err(SubbankDenied::Invalid(v))) => ReviseSubbankOutcome::InvalidRequest(violation(v)),
        Ok(Err(SubbankDenied::Refused(r))) => match r {
            SubbankRefusal::Conflict { existing } => {
                ReviseSubbankOutcome::SubbankConflict(conflict(existing))
            }
            SubbankRefusal::UnknownSubbank => ReviseSubbankOutcome::ReferenceNotFound(not_found()),
            r => ReviseSubbankOutcome::ActionRejected(rejected(&r).ok_or_else(unexpected)?),
        },
        Err(e) => escalate(e, "subbank.revise")?.into(),
    };
    Ok(ReviseSubbankPayload { outcome })
}

// ## 並べ替える

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct ReorderSubbanksInput {
    /// すべての枠を、新しい並びで。 欠けや重複は `InvalidRequest`。
    pub order: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ReorderSubbanksPayload {
    pub outcome: ReorderSubbanksOutcome,
}

/// `ReorderSubbanksOutcome` の union。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum ReorderSubbanksOutcome {
    SubbanksReordered { subbanks: Vec<Subbank> },
    RevisionConflict(RevisionConflict),
    InvalidRequest(InvalidRequest),
    StorageProblem(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

/// 枠を並べ替える（`Mutation.reorderSubbanks`）。 並びが録る順の既定になる（`TR-RCL-28`）。
#[tauri::command(async)]
#[specta::specta]
pub fn reorder_subbanks(
    state: State<'_, AppState>,
    input: ReorderSubbanksInput,
) -> Result<ReorderSubbanksPayload> {
    let bad_order = || {
        ReorderSubbanksOutcome::InvalidRequest(invalid(vec![InputViolation {
            field: "order",
            code: "subbank.bad_order",
        }]))
    };
    let Some(order) = input
        .order
        .iter()
        .map(|s| subbank_id(s))
        .collect::<Option<Vec<_>>>()
    else {
        return Ok(ReorderSubbanksPayload {
            outcome: bad_order(),
        });
    };
    let outcome = match lock(&state)?.reorder_subbanks(&order) {
        Ok(Ok(all)) => ReorderSubbanksOutcome::SubbanksReordered {
            subbanks: all.iter().map(Subbank::of).collect(),
        },
        Ok(Err(SubbankDenied::Refused(SubbankRefusal::BadOrder))) => bad_order(),
        Ok(Err(SubbankDenied::Invalid(v))) => ReorderSubbanksOutcome::InvalidRequest(violation(v)),
        Ok(Err(SubbankDenied::Refused(_))) => return Err(unexpected()),
        Err(e) => escalate(e, "subbank.reorder")?.into(),
    };
    Ok(ReorderSubbanksPayload { outcome })
}

// ## 担当音域

#[derive(Debug, Clone, Copy, Deserialize, specta::Type)]
pub struct ToneRangeInput {
    /// MIDI。
    pub low: i32,
    /// MIDI。
    pub high: i32,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct SetSubbankToneRangeInput {
    pub subbank: String,
    /// 省くと既定（floor 割り当て）へ戻す。
    pub range: Option<ToneRangeInput>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SetSubbankToneRangePayload {
    pub outcome: SetSubbankToneRangeOutcome,
}

/// `SetSubbankToneRangeOutcome` の union。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum SetSubbankToneRangeOutcome {
    /// 直した枠と、既定が動いた同じ声色の枠。
    SubbankToneRangeSet {
        subbanks: Vec<Subbank>,
    },
    RevisionConflict(RevisionConflict),
    ActionRejected(Problem),
    ReferenceNotFound(Problem),
    InvalidRequest(InvalidRequest),
    StorageProblem(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

/// 枠の担当音域を直す（`Mutation.setSubbankToneRange`、`TR-RCL-31`）。
#[tauri::command(async)]
#[specta::specta]
pub fn set_subbank_tone_range(
    state: State<'_, AppState>,
    input: SetSubbankToneRangeInput,
) -> Result<SetSubbankToneRangePayload> {
    let Some(id) = subbank_id(&input.subbank) else {
        return Ok(SetSubbankToneRangePayload {
            outcome: SetSubbankToneRangeOutcome::ReferenceNotFound(not_found()),
        });
    };
    let range = input.range.map(|r| (r.low, r.high));
    let outcome = match lock(&state)?.set_subbank_tone_range(id, range) {
        Ok(Ok(changed)) => SetSubbankToneRangeOutcome::SubbankToneRangeSet {
            subbanks: changed.iter().map(Subbank::of).collect(),
        },
        Ok(Err(SubbankDenied::Invalid(v))) => {
            SetSubbankToneRangeOutcome::InvalidRequest(violation(v))
        }
        Ok(Err(SubbankDenied::Refused(r))) => match r {
            SubbankRefusal::UnknownSubbank => {
                SetSubbankToneRangeOutcome::ReferenceNotFound(not_found())
            }
            r => SetSubbankToneRangeOutcome::ActionRejected(rejected(&r).ok_or_else(unexpected)?),
        },
        Err(e) => escalate(e, "subbank.set_tone_range")?.into(),
    };
    Ok(SetSubbankToneRangePayload { outcome })
}

// ## 消す

/// 画面は消えるテイクの数を示して確認を取り、その数を `acknowledgedTakes` に入れる。
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemoveSubbankInput {
    pub subbank: String,
    /// 本人に示した、消えるテイクの数。
    pub acknowledged_takes: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RemoveSubbankPayload {
    pub outcome: RemoveSubbankOutcome,
}

/// `RemoveSubbankOutcome` の union。 録っている途中の枠と、復旧候補が残っている枠は
/// 消せない（`ActionRejected`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum RemoveSubbankOutcome {
    SubbankRemoved(SubbankRemoved),
    RevisionConflict(RevisionConflict),
    ActionRejected(Problem),
    ResourceBusy(Problem),
    ReferenceNotFound(Problem),
    StorageProblem(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SubbankRemoved {
    pub subbank: String,
    pub removed_takes: u32,
}

impl SubbankRemoved {
    fn of(r: &SubbankRemoval) -> Self {
        Self {
            subbank: r.subbank.to_string(),
            removed_takes: count(r.removed_takes),
        }
    }
}

/// 枠を消す（`Mutation.removeSubbank`、`TR-RCL-29`）。 その枠のテイクも消える。
#[tauri::command(async)]
#[specta::specta]
pub fn remove_subbank(
    state: State<'_, AppState>,
    input: RemoveSubbankInput,
) -> Result<RemoveSubbankPayload> {
    let Some(id) = subbank_id(&input.subbank) else {
        return Ok(RemoveSubbankPayload {
            outcome: RemoveSubbankOutcome::ReferenceNotFound(not_found()),
        });
    };
    let mut studio = lock(&state)?;
    let acknowledged = usize::try_from(input.acknowledged_takes).unwrap_or(usize::MAX);
    let outcome = match studio.remove_subbank(id, acknowledged) {
        Ok(Ok(r)) => RemoveSubbankOutcome::SubbankRemoved(SubbankRemoved::of(&r)),
        Ok(Err(SubbankDenied::Refused(r))) => match r {
            SubbankRefusal::UnknownSubbank => RemoveSubbankOutcome::ReferenceNotFound(not_found()),
            SubbankRefusal::TakesChanged { .. } => match takes_changed(&mut studio) {
                Ok(c) => RemoveSubbankOutcome::RevisionConflict(c),
                Err(e) => escalate(e, "subbank.remove")?.into(),
            },
            r => RemoveSubbankOutcome::ActionRejected(rejected(&r).ok_or_else(unexpected)?),
        },
        Ok(Err(SubbankDenied::Invalid(_))) => return Err(unexpected()),
        Err(e) => escalate(e, "subbank.remove")?.into(),
    };
    Ok(RemoveSubbankPayload { outcome })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn summary(id: i32, color: Option<&str>, tone: i32) -> SubbankSummary {
        SubbankSummary {
            id: SubbankId::new(id),
            voice_color: color.and_then(koeru_model::subbank::VoiceColor::parse),
            method: koeru_core::alias::Method::Sequential,
            preset_id: Some("sequential".into()),
            tone,
            range: koeru_model::subbank::ToneRange { low: 0, high: 127 },
            range_edited: false,
            rows: 10,
            recorded_rows: 3,
            remaining_seconds: 12.4,
            takes: 4,
        }
    }

    #[test]
    fn 枠は_sdl_の欄の名前で運ぶ() {
        let json = serde_json::to_value(Subbank::of(&summary(3, Some("強"), 60))).expect("書ける");
        assert_eq!(
            json,
            serde_json::json!({
                "id": "3",
                "voiceColor": "強",
                "method": "SEQUENTIAL",
                "preset": "sequential",
                "tone": { "midi": 60, "name": "C4" },
                "toneRange": {
                    "low": { "midi": 0, "name": "C-1" },
                    "high": { "midi": 127, "name": "G9" },
                    "edited": false,
                },
                "progress": { "recordedRows": 3, "rows": 10, "remainingSeconds": 12 },
                "hasTakes": true,
            })
        );
    }

    #[test]
    fn 声色は最初に現れた順に束ねる() {
        let all = [
            summary(1, None, 55),
            summary(2, Some("強"), 55),
            summary(3, None, 62),
        ];
        let grouped = voice_colors_of(all.iter().map(Subbank::of).collect());
        let names: Vec<_> = grouped.iter().map(|v| v.name.as_deref()).collect();
        assert_eq!(names, [None, Some("強")]);
        let first: Vec<_> = grouped[0].subbanks.iter().map(|s| s.id.as_str()).collect();
        assert_eq!(first, ["1", "3"]);
    }

    #[test]
    fn 結果の枝は_typename_で分かれる() {
        let added = AddSubbankPayload {
            outcome: AddSubbankOutcome::SubbankConflict(conflict(SubbankId::new(7))),
        };
        let json = serde_json::to_value(&added).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "SubbankConflict");
        assert_eq!(json["outcome"]["existing"], "7");
        assert_eq!(json["outcome"]["code"], "subbank.conflict");

        let removed = RemoveSubbankPayload {
            outcome: RemoveSubbankOutcome::SubbankRemoved(SubbankRemoved {
                subbank: "7".into(),
                removed_takes: 2,
            }),
        };
        let json = serde_json::to_value(&removed).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "SubbankRemoved");
        assert_eq!(json["outcome"]["removedTakes"], 2);
    }

    #[test]
    fn 画面から来る入力は_sdl_の欄の名前で読む() {
        let input: RemoveSubbankInput =
            serde_json::from_str(r#"{"subbank":"4","acknowledgedTakes":3}"#).expect("読める");
        assert_eq!(input.acknowledged_takes, 3);
        let input: ReviseSubbankInput =
            serde_json::from_str(r#"{"subbank":"4","clearVoiceColor":true}"#).expect("読める");
        assert_eq!(input.clear_voice_color, Some(true));
        assert!(input.tone.is_none());
    }

    #[test]
    fn 読めない識別子はこの版に無い枠として扱う() {
        assert_eq!(subbank_id("12"), Some(SubbankId::new(12)));
        assert_eq!(subbank_id("s12"), None);
    }

    #[test]
    fn 開いていないときは貸与が切れた枝へ写す() {
        let e = AppError::new(
            "app.no_project",
            Class::Rejected,
            "プロジェクトを開いていない",
        );
        assert!(matches!(
            escalate(e, "test"),
            Ok(Escalated::ProjectLeaseExpired(_))
        ));
        let e = AppError::new("app.bad_id", Class::InvalidInput, "その識別子は読めない");
        assert!(escalate(e, "test").is_err());
    }
}
