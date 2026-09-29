// JobRequest — ジョブリクエスト（キューに追加する内容）。

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::{Attempt, JobId, JobKind, JobState, JobTarget};

/// ジョブリクエスト。
///
/// `JobSystem::submit()` でキューに追加される内容。
/// immutable input stamp を持つ。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JobRequest {
    pub id: JobId,
    pub kind: JobKind,
    pub attempt: Attempt,
    pub target: JobTarget,
    pub payload: JobPayload,
    pub submitted_at: DateTime<Utc>,
}

/// ジョブのpayload（エンジン呼び出しの内容）。
///
/// Phase 4 で実際のエンジンインターフェースへ拡張する。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum JobPayload {
    /// 解析エンジンへのリクエスト。
    Analysis(AlignmentRequest),
    /// 合成エンジンへのリクエスト。
    Preview(SynthesisRequest),
    /// チャート事前計算。
    Chart(ChartRequest),
}

/// 解析エンジンへのリクエスト。
///
/// Phoneme と Grid は serde::Serialize/Deserialize を実装していないため、
/// String として格納する。実際のエンジン呼び出しは Phase 4 で行う。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AlignmentRequest {
    pub samples: Vec<f64>,
    pub sample_rate_hz: u32,
    /// 音素記号のリスト。実際の `koeru_align::Phoneme` へは Phase 4 で変換。
    pub phonemes: Vec<String>,
    /// 収録グリッド情報。実際の `koeru_align::Grid` へは Phase 4 で変換。
    pub grid: Option<GridInfo>,
}

/// 合成エンジンへのリクエスト。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SynthesisRequest {
    /// 音素記号のリスト。
    pub phonemes: Vec<String>,
    pub sample_rate_hz: u32,
}

/// グリッド情報（`koeru_align::Grid` のシリアライズ用ラッパー）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GridInfo {
    pub expected_onset_ms: f64,
    pub expected_moras: u32,
    pub tempo_bpm: Option<f64>,
}

/// チャート事前計算のリクエスト。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ChartRequest {
    pub take: JobTarget,
    pub song: Option<String>,
}

impl JobRequest {
    /// 解析リクエストを生成する。
    pub fn analysis(
        id: JobId,
        target: JobTarget,
        samples: Vec<f64>,
        sample_rate_hz: u32,
        phonemes: Vec<String>,
        grid: Option<GridInfo>,
    ) -> Self {
        Self {
            id,
            kind: JobKind::Analysis,
            attempt: Attempt::INITIAL,
            target,
            payload: JobPayload::Analysis(AlignmentRequest {
                samples,
                sample_rate_hz,
                phonemes,
                grid,
            }),
            submitted_at: Utc::now(),
        }
    }

    /// 合成リクエストを生成する。
    pub fn preview(
        id: JobId,
        target: JobTarget,
        phonemes: Vec<String>,
        sample_rate_hz: u32,
    ) -> Self {
        Self {
            id,
            kind: JobKind::PreviewRender,
            attempt: Attempt::INITIAL,
            target,
            payload: JobPayload::Preview(SynthesisRequest {
                phonemes,
                sample_rate_hz,
            }),
            submitted_at: Utc::now(),
        }
    }

    /// チャート事前計算リクエストを生成する。
    pub fn chart(id: JobId, target: JobTarget, song: Option<String>) -> Self {
        Self {
            id,
            kind: JobKind::Chart,
            attempt: Attempt::INITIAL,
            target: target.clone(),
            payload: JobPayload::Chart(ChartRequest { take: target, song }),
            submitted_at: Utc::now(),
        }
    }

    /// 既存の JobState から BackgroundJob を構築する。
    pub fn as_background_job(&self, status: JobState) -> BackgroundJob {
        BackgroundJob {
            id: self.id,
            kind: self.kind,
            attempt: self.attempt,
            status,
            target: self.target.clone(),
        }
    }
}

/// BackgroundJob — キューに追加されたジョブの現在状態（`JobQueue.job(id)` の返り値）。
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BackgroundJob {
    pub id: JobId,
    pub kind: JobKind,
    pub attempt: Attempt,
    pub status: JobState,
    pub target: JobTarget,
}
