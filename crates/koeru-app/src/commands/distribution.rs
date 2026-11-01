//! 名前を付けて残す配布物の口（`DEC-PKG-018`）。 SDL の `DistributionFacet.distributions` と、
//! 配布物を写す・直す・消す・版を選んで書き出す Mutation を、刷新のあいだ tauri-specta で
//! 運ぶ（`DEC-PLT-061`）。
//!
//! 型と欄の名前は SDL（`specs/application/schema/distribution.graphql`）に合わせる。 入力の
//! `lease`・`operationId`・`baseRevision` と payload の `receipt` は運ばない。 ここには貸与も
//! 受領証も無い。 落とした欄は `specs/application/README.md` の移行の表が挙げる。
//!
//! **どれも `open_project` を呼ばない**（`TR-REC-30`）。 開いている収録セッションは続く。

use serde::{Deserialize, Serialize};
use tauri::State;

use koeru_core::db::shelf::DistributionRefusal;
use koeru_core::project::Method;
use koeru_core::release::{Release as StoredRelease, Validation};
use koeru_failure::Class;
use koeru_model::id::{DistributionId, SubbankId};

use super::problem::{InputViolation, InvalidRequest, Problem};
use super::subbank::{
    OperationIdReused, RecordingMethod, RevisionConflict, Subbank, Tone, ToneRange, ToneRangeInput,
};
use super::{AppState, count, lock};
use crate::error::{AppError, FailureAction, FailureClass, FailureOutcome, Result};
use crate::studio::{
    DistributionDenied, DistributionSummary, PackagedSubbankChange, PackagedSubbankSummary,
    ReviseDistribution,
};

// ## 型

/// 書き出しの記録の検証の結果（`ReleaseValidation`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReleaseValidation {
    Passed,
    PassedWithWarnings,
    NotRun,
}

impl From<Validation> for ReleaseValidation {
    fn from(v: Validation) -> Self {
        match v {
            Validation::Passed => Self::Passed,
            Validation::PassedWithWarnings => Self::PassedWithWarnings,
            Validation::NotRun => Self::NotRun,
        }
    }
}

const fn recording_method(m: Method) -> RecordingMethod {
    match m {
        Method::Single => RecordingMethod::Single,
        Method::Sequential => RecordingMethod::Sequential,
        Method::Cvvc => RecordingMethod::Cvvc,
        Method::MultiPitchSequential => RecordingMethod::MultiPitchSequential,
    }
}

const fn project_method(m: RecordingMethod) -> Method {
    match m {
        RecordingMethod::Single => Method::Single,
        RecordingMethod::Sequential => Method::Sequential,
        RecordingMethod::Cvvc => Method::Cvvc,
        RecordingMethod::MultiPitchSequential => Method::MultiPitchSequential,
    }
}

/// 書き出しの記録1件（`Release`）。 在り処は持たない（`TR-PKG-45`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    /// `ReleaseId`。 wire では文字列で、画面は中身を解釈しない。 `reveal_release` に渡す連番。
    pub id: String,
    /// 書き出した配布物（`DistributionId`）。 配布物を消しても記録は残る（`TR-PKG-44`）。
    pub distribution: String,
    /// 同じ配布物の中で、同じ番号の記録が並ぶことがある（`TR-PKG-60`）。
    pub version: String,
    pub method: RecordingMethod,
    pub alias_count: u32,
    pub validation: ReleaseValidation,
    pub archive_name: String,
    pub released_at: String,
}

impl Release {
    fn of(r: &StoredRelease) -> Self {
        Self {
            id: r.seq.to_string(),
            distribution: r.distribution.to_string(),
            version: r.version.clone(),
            method: recording_method(r.method),
            alias_count: u32::try_from(r.alias_count).unwrap_or(0),
            validation: r.validation.into(),
            archive_name: r.archive_name.clone(),
            released_at: r.released_at.clone(),
        }
    }
}

/// 配布物が入れる枠1つ（`PackagedSubbank`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackagedSubbank {
    pub subbank: Subbank,
    /// この配布物で書く担当音域（`TR-PKG-59`）。 `edited` はこの配布物で直したときだけ true。
    pub tone_range: ToneRange,
}

impl PackagedSubbank {
    fn of(m: &PackagedSubbankSummary) -> Self {
        Self {
            subbank: Subbank::of(&m.subbank),
            tone_range: ToneRange {
                low: Tone::of(m.range.low),
                high: Tone::of(m.range.high),
                edited: m.range_edited,
            },
        }
    }
}

/// 入れた枠のどれも受け持たない高さ（`UncoveredRange`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UncoveredRange {
    /// `null` は声色なし。
    pub voice_color: Option<String>,
    pub low: Tone,
    pub high: Tone,
}

/// 名前を付けて残す配布物（`Distribution`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Distribution {
    /// `DistributionId`。 wire では文字列で、画面は中身を解釈しない。
    pub id: String,
    pub name: String,
    /// 音源ルートフォルダ名（`DEC-PKG-008`）。 配布物のあいだで重ならない。
    pub archive_name: String,
    /// 入れる枠。 並びは `subbanks` と同じ。
    pub subbanks: Vec<PackagedSubbank>,
    /// 声色ごとに、入れた枠のどれも受け持たない高さ（`TR-PKG-59`）。
    pub uncovered: Vec<UncoveredRange>,
    /// この配布物だけの利用規約。 `null` なら音源の規約。
    pub terms: Option<String>,
    /// 次に書き出す版の番号（`TR-PKG-60`）。 下書きが無ければ `null`。
    pub draft_version: Option<String>,
    /// この配布物の書き出し。 新しい順。
    pub releases: Vec<Release>,
    /// 最後に書き出したあとに採用が変わった、入れた枠の行（`TR-PKG-61`）。
    /// 書き出していなければ `null`。
    pub rows_changed_since_release: Option<u32>,
}

impl Distribution {
    fn of(d: &DistributionSummary) -> Self {
        Self {
            id: d.id.to_string(),
            name: d.name.clone(),
            archive_name: d.archive_name.clone(),
            subbanks: d.subbanks.iter().map(PackagedSubbank::of).collect(),
            uncovered: d
                .uncovered
                .iter()
                .map(|(color, r)| UncoveredRange {
                    voice_color: color.as_ref().map(|c| c.as_str().to_owned()),
                    low: Tone::of(r.low),
                    high: Tone::of(r.high),
                })
                .collect(),
            terms: d.terms.clone(),
            draft_version: d.draft_version.clone(),
            releases: d.releases.iter().rev().map(Release::of).collect(),
            rows_changed_since_release: d.rows_changed_since_release.map(count),
        }
    }
}

/// フォルダ名が別の配布物と重なった（`DistributionConflict`）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
pub struct DistributionConflict {
    pub code: &'static str,
    pub class: FailureClass,
    pub commit: FailureOutcome,
    pub actions: Vec<FailureAction>,
    pub message: String,
    pub existing: String,
}

// ## 失敗の写し

fn conflict(existing: DistributionId) -> DistributionConflict {
    let p = Problem::new(
        "distribution.archive_name_taken",
        Class::Conflict,
        "そのフォルダ名は別の配布物が使っている",
    );
    DistributionConflict {
        code: p.code,
        class: p.class,
        commit: p.commit,
        actions: p.actions,
        message: p.message,
        existing: existing.to_string(),
    }
}

fn invalid(field: &'static str, code: &'static str) -> InvalidRequest {
    let p = Problem::new(
        "distribution.invalid_input",
        Class::InvalidInput,
        "入力が正しくない",
    );
    InvalidRequest::of(p, vec![InputViolation { field, code }])
}

fn not_found() -> Problem {
    Problem::new(
        "distribution.unknown",
        Class::Conflict,
        "その配布物はもう無い",
    )
}

/// 台帳が断った理由のうち、`ActionRejected` で返すもの。 その操作の union に無い理由は `None`。
fn rejected(r: DistributionRefusal) -> Option<Problem> {
    Some(match r {
        DistributionRefusal::LastOne => Problem::new(
            "distribution.last_one",
            Class::Rejected,
            "最後の1つの配布物は消せない",
        ),
        DistributionRefusal::VersionTaken => Problem::new(
            "distribution.version_taken",
            Class::Rejected,
            "その番号は、この配布物でもう書き出している",
        ),
        _ => return None,
    })
}

/// その操作の union に無い断りが来た。 実装の欠陥で、結果ではない（`DEC-PLT-038`）。
fn unexpected() -> AppError {
    AppError::new(
        "distribution.unexpected_refusal",
        Class::Internal,
        "配布物の操作が想定していない理由で断られた",
    )
}

/// 指した配布物の識別子を読む。 読めない識別子は、この版に無いものとして扱う。
fn distribution_id(s: &str) -> Option<DistributionId> {
    s.parse().ok().map(DistributionId::new)
}

fn subbank_id(s: &str) -> Option<SubbankId> {
    s.parse().ok().map(SubbankId::new)
}

/// プロジェクトを開いていないことを、貸与が切れた枝へ写す。 ほかの失敗は `Err` のまま。
///
/// 配布物の操作の union は `StorageProblem` を持たない（SDL）。 台帳やファイルの失敗は
/// `AppError` として返る。
fn lease_expired(e: AppError, phase: &'static str) -> Result<Problem> {
    if e.code != "app.no_project" {
        return Err(e);
    }
    // 画面へ値として返すので、`AppError` の直列化を通らない。 ここで1回記録する。
    e.record(phase);
    Ok(Problem::of(&e))
}

// ## 読み

/// 配布物を足した順で（`DistributionFacet.distributions`）。 少なくとも1つ。
#[tauri::command(async)]
#[specta::specta]
pub fn distributions(state: State<'_, AppState>) -> Result<Vec<Distribution>> {
    Ok(lock(&state)?
        .distributions()?
        .iter()
        .map(Distribution::of)
        .collect())
}

// ## 写す

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct DuplicateDistributionInput {
    /// 写す元の `DistributionId`。
    pub source: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DuplicateDistributionPayload {
    pub outcome: DuplicateDistributionOutcome,
}

/// `DuplicateDistributionOutcome` の union。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum DuplicateDistributionOutcome {
    DistributionAdded(DistributionAdded),
    ReferenceNotFound(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DistributionAdded {
    pub distribution: Distribution,
}

/// 配布物を写して足す（`Mutation.duplicateDistribution`、`TR-PKG-58`）。 名前は「〇〇 のコピー」。
#[tauri::command(async)]
#[specta::specta]
pub fn duplicate_distribution(
    state: State<'_, AppState>,
    input: DuplicateDistributionInput,
) -> Result<DuplicateDistributionPayload> {
    let Some(source) = distribution_id(&input.source) else {
        return Ok(DuplicateDistributionPayload {
            outcome: DuplicateDistributionOutcome::ReferenceNotFound(not_found()),
        });
    };
    let outcome = match lock(&state)?.duplicate_distribution(source) {
        Ok(Ok(d)) => DuplicateDistributionOutcome::DistributionAdded(DistributionAdded {
            distribution: Distribution::of(&d),
        }),
        Ok(Err(DistributionDenied::Refused(DistributionRefusal::UnknownDistribution))) => {
            DuplicateDistributionOutcome::ReferenceNotFound(not_found())
        }
        Ok(Err(_)) => return Err(unexpected()),
        Err(e) => DuplicateDistributionOutcome::ProjectLeaseExpired(lease_expired(
            e,
            "distribution.duplicate",
        )?),
    };
    Ok(DuplicateDistributionPayload { outcome })
}

// ## 直す

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackagedSubbankInput {
    pub subbank: String,
    /// 省くと枠の担当音域（`TR-RCL-31`）。 直してもこの配布物にだけ効く（`TR-PKG-59`）。
    pub tone_range: Option<ToneRangeInput>,
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReviseDistributionInput {
    pub distribution: String,
    pub name: Option<String>,
    /// 配布物のあいだで重なると `DistributionConflict`。
    pub archive_name: Option<String>,
    /// 渡すと入れる枠をこれに置き換える。 空にはできない。
    pub subbanks: Option<Vec<PackagedSubbankInput>>,
    pub terms: Option<String>,
    /// この配布物の規約を消し、音源の規約に戻す。
    pub clear_terms: Option<bool>,
    /// 書き出し済みの番号とは重ねない。
    pub draft_version: Option<String>,
    /// 下書きを消す。 書き出し済みの版は消えない。
    pub clear_draft_version: Option<bool>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ReviseDistributionPayload {
    pub outcome: ReviseDistributionOutcome,
}

/// `ReviseDistributionOutcome` の union。 下書きの番号が書き出し済みの番号と重なると
/// `ActionRejected`。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum ReviseDistributionOutcome {
    DistributionRevised(DistributionRevised),
    DistributionConflict(DistributionConflict),
    ActionRejected(Problem),
    InvalidRequest(InvalidRequest),
    ReferenceNotFound(Problem),
    RevisionConflict(RevisionConflict),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DistributionRevised {
    pub distribution: Distribution,
}

/// 配布物の名前・入れる枠・担当音域・規約・下書きの番号を直す（`Mutation.reviseDistribution`、
/// `TR-PKG-58`〜`TR-PKG-60`）。 渡したものだけ変える。
///
/// 担当音域を直しても、枠の担当音域は変わらない（`TR-PKG-59`）。
#[tauri::command(async)]
#[specta::specta]
pub fn revise_distribution(
    state: State<'_, AppState>,
    input: ReviseDistributionInput,
) -> Result<ReviseDistributionPayload> {
    let reply = |outcome| Ok(ReviseDistributionPayload { outcome });
    let Some(id) = distribution_id(&input.distribution) else {
        return reply(ReviseDistributionOutcome::ReferenceNotFound(not_found()));
    };
    let members = match input.subbanks.as_deref() {
        None => None,
        Some(all) => {
            let parsed = all
                .iter()
                .map(|m| {
                    Some(PackagedSubbankChange {
                        subbank: subbank_id(&m.subbank)?,
                        range: m.tone_range.map(|r| (r.low, r.high)),
                    })
                })
                .collect::<Option<Vec<_>>>();
            // 読めない枠の識別子は、この版に無い枠を指している。 入力の誤りとして返す。
            let Some(parsed) = parsed else {
                return reply(ReviseDistributionOutcome::InvalidRequest(invalid(
                    "subbanks",
                    "distribution.unknown_subbank",
                )));
            };
            Some(parsed)
        }
    };
    let change = ReviseDistribution {
        name: input.name.as_deref(),
        archive_name: input.archive_name.as_deref(),
        subbanks: members.as_deref(),
        terms: input.terms.as_deref(),
        clear_terms: input.clear_terms.unwrap_or(false),
        draft_version: input.draft_version.as_deref(),
        clear_draft_version: input.clear_draft_version.unwrap_or(false),
    };
    let outcome = match lock(&state)?.revise_distribution(id, &change) {
        Ok(Ok(d)) => ReviseDistributionOutcome::DistributionRevised(DistributionRevised {
            distribution: Distribution::of(&d),
        }),
        Ok(Err(DistributionDenied::Invalid(v))) => {
            ReviseDistributionOutcome::InvalidRequest(invalid(v.field, v.code))
        }
        Ok(Err(DistributionDenied::Refused(r))) => match r {
            DistributionRefusal::UnknownDistribution => {
                ReviseDistributionOutcome::ReferenceNotFound(not_found())
            }
            DistributionRefusal::UnknownSubbank => ReviseDistributionOutcome::InvalidRequest(
                invalid("subbanks", "distribution.unknown_subbank"),
            ),
            DistributionRefusal::Conflict { existing } => {
                ReviseDistributionOutcome::DistributionConflict(conflict(existing))
            }
            r => ReviseDistributionOutcome::ActionRejected(rejected(r).ok_or_else(unexpected)?),
        },
        Err(e) => {
            ReviseDistributionOutcome::ProjectLeaseExpired(lease_expired(e, "distribution.revise")?)
        }
    };
    reply(outcome)
}

// ## 消す

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct RemoveDistributionInput {
    pub distribution: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RemoveDistributionPayload {
    pub outcome: RemoveDistributionOutcome,
}

/// `RemoveDistributionOutcome` の union。 最後の1つを消そうとすると `ActionRejected`。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum RemoveDistributionOutcome {
    DistributionRemoved(DistributionRemoved),
    ActionRejected(Problem),
    ReferenceNotFound(Problem),
    ProjectLeaseExpired(Problem),
    OperationIdReused(OperationIdReused),
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DistributionRemoved {
    /// 消した配布物の `DistributionId`。
    pub distribution: String,
}

/// 配布物を消す（`Mutation.removeDistribution`、`TR-PKG-58`）。 書き出しの記録と ZIP は残す
/// （`TR-PKG-44`）。 最後の1つは消せない。
#[tauri::command(async)]
#[specta::specta]
pub fn remove_distribution(
    state: State<'_, AppState>,
    input: RemoveDistributionInput,
) -> Result<RemoveDistributionPayload> {
    let reply = |outcome| Ok(RemoveDistributionPayload { outcome });
    let Some(id) = distribution_id(&input.distribution) else {
        return reply(RemoveDistributionOutcome::ReferenceNotFound(not_found()));
    };
    let outcome = match lock(&state)?.remove_distribution(id) {
        Ok(Ok(())) => RemoveDistributionOutcome::DistributionRemoved(DistributionRemoved {
            distribution: id.to_string(),
        }),
        Ok(Err(DistributionDenied::Refused(r))) => match r {
            DistributionRefusal::UnknownDistribution => {
                RemoveDistributionOutcome::ReferenceNotFound(not_found())
            }
            r => RemoveDistributionOutcome::ActionRejected(rejected(r).ok_or_else(unexpected)?),
        },
        Ok(Err(DistributionDenied::Invalid(_))) => return Err(unexpected()),
        Err(e) => {
            RemoveDistributionOutcome::ProjectLeaseExpired(lease_expired(e, "distribution.remove")?)
        }
    };
    reply(outcome)
}

// ## 版を選んで書き出す

#[derive(Debug, Clone, Deserialize, specta::Type)]
pub struct ExportDistributionInput {
    /// 省くと先頭の配布物。
    pub distribution: Option<String>,
    /// 省くと下書きの番号。 下書きが無ければ、最後の版の末尾を1つ上げた番号。
    /// 書き出し済みの番号なら、同じ番号で記録を1件足す（`TR-PKG-60`）。
    pub version: Option<String>,
    /// 省くと音源の方式。 下位の方式なら独立した音源として出す（`TR-PKG-24`）。
    pub method: Option<RecordingMethod>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ExportDistributionPayload {
    pub outcome: ExportDistributionOutcome,
}

/// `PrepareExportOutcome` と `CommitExportOutcome` を1つにしたもの。 計画を作ってから確定する
/// 2手を、ここでは1回の呼び出しで行う。
///
/// 書き出せない理由（被覆・検証・確認の残り）は `ExportBlocked` ではなく失敗（`AppError`）で
/// 返る。 何が足りないかは `package_state` が持つ。
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "__typename")]
pub enum ExportDistributionOutcome {
    ReleasePublished(ReleasePublished),
    InvalidRequest(InvalidRequest),
    ReferenceNotFound(Problem),
    ProjectLeaseExpired(Problem),
}

#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ReleasePublished {
    pub release: Release,
}

/// 配布物と版を選んで書き出す（`Mutation.prepareExport` → `Mutation.commitExport`、
/// `TR-PKG-58`、`TR-PKG-60`）。
///
/// 書き出し済みの番号を選ぶと、同じ番号で記録を1件足す。 前の ZIP は消さない
/// （`DEC-PKG-018`、`TR-PKG-44`）。
#[tauri::command(async)]
#[specta::specta]
pub fn export_distribution(
    state: State<'_, AppState>,
    input: ExportDistributionInput,
) -> Result<ExportDistributionPayload> {
    let reply = |outcome| Ok(ExportDistributionPayload { outcome });
    let id = match input.distribution.as_deref().map(distribution_id) {
        None => None,
        Some(Some(id)) => Some(id),
        Some(None) => return reply(ExportDistributionOutcome::ReferenceNotFound(not_found())),
    };
    let outcome = match lock(&state)?.export_distribution(
        id,
        input.version.as_deref(),
        input.method.map(project_method),
    ) {
        Ok(Ok(e)) => ExportDistributionOutcome::ReleasePublished(ReleasePublished {
            release: Release::of(&e.release),
        }),
        Ok(Err(DistributionDenied::Invalid(v))) => {
            ExportDistributionOutcome::InvalidRequest(invalid(v.field, v.code))
        }
        Ok(Err(DistributionDenied::Refused(DistributionRefusal::UnknownDistribution))) => {
            ExportDistributionOutcome::ReferenceNotFound(not_found())
        }
        Ok(Err(DistributionDenied::Refused(_))) => return Err(unexpected()),
        Err(e) => {
            ExportDistributionOutcome::ProjectLeaseExpired(lease_expired(e, "distribution.export")?)
        }
    };
    reply(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 読めない識別子はこの版に無い配布物として扱う() {
        assert_eq!(distribution_id("3"), Some(DistributionId::new(3)));
        assert_eq!(distribution_id("d3"), None);
    }

    #[test]
    fn 結果の枝は_typename_で分かれる() {
        let removed = RemoveDistributionPayload {
            outcome: RemoveDistributionOutcome::DistributionRemoved(DistributionRemoved {
                distribution: "2".into(),
            }),
        };
        let json = serde_json::to_value(&removed).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "DistributionRemoved");
        assert_eq!(json["outcome"]["distribution"], "2");

        let revised = ReviseDistributionPayload {
            outcome: ReviseDistributionOutcome::DistributionConflict(conflict(
                DistributionId::new(7),
            )),
        };
        let json = serde_json::to_value(&revised).expect("書ける");
        assert_eq!(json["outcome"]["__typename"], "DistributionConflict");
        assert_eq!(json["outcome"]["existing"], "7");
        assert_eq!(json["outcome"]["code"], "distribution.archive_name_taken");
    }

    #[test]
    fn 記録は_sdl_の欄の名前で運ぶ() {
        let stored = StoredRelease {
            seq: 4,
            distribution: DistributionId::new(2),
            version: "1.0.1".into(),
            method: Method::Single,
            alias_count: 102,
            validation: Validation::Passed,
            oto_hash: String::new(),
            terms_hash: String::new(),
            archive_name: "000004-1.0.1.zip".into(),
            released_at: "2026-11-01T00:00:00Z".into(),
        };
        let json = serde_json::to_value(Release::of(&stored)).expect("書ける");
        assert_eq!(
            json,
            serde_json::json!({
                "id": "4",
                "distribution": "2",
                "version": "1.0.1",
                "method": "SINGLE",
                "aliasCount": 102,
                "validation": "PASSED",
                "archiveName": "000004-1.0.1.zip",
                "releasedAt": "2026-11-01T00:00:00Z",
            })
        );
    }

    #[test]
    fn 画面から来る入力は_sdl_の欄の名前で読む() {
        let input: ReviseDistributionInput = serde_json::from_str(
            r#"{"distribution":"2","clearDraftVersion":true,
                "subbanks":[{"subbank":"5","toneRange":{"low":0,"high":60}},{"subbank":"6"}]}"#,
        )
        .expect("読める");
        assert_eq!(input.clear_draft_version, Some(true));
        assert!(input.name.is_none());
        let subbanks = input.subbanks.expect("ある");
        assert_eq!(subbanks[0].tone_range.expect("ある").high, 60);
        assert!(subbanks[1].tone_range.is_none());

        let input: ExportDistributionInput =
            serde_json::from_str(r#"{"version":"1.0.0","method":"SEQUENTIAL"}"#).expect("読める");
        assert_eq!(input.method, Some(RecordingMethod::Sequential));
        assert!(input.distribution.is_none());
    }

    #[test]
    fn 開いていないときだけ貸与が切れた枝へ写す() {
        let e = AppError::new(
            "app.no_project",
            Class::Rejected,
            "プロジェクトを開いていない",
        );
        assert!(lease_expired(e, "test").is_ok());
        let e = AppError::new("app.bad_id", Class::InvalidInput, "その識別子は読めない");
        assert!(lease_expired(e, "test").is_err());
    }
}
