//! ドメイン層のエラー型。
//!
//! 方針は `.agents/skills/rust-conventions/SKILL.md` に準拠する。
//!
//! - **回復が必要な失敗は、この層で `thiserror` の列挙体として定義する。**
//!   呼び出し側が `match` で網羅的に分岐できることを保証する。
//! - **`anyhow::Error` に畳まない。** 畳むのはアプリケーション境界（bin / Tauri コマンド）だけ。
//! - **列挙体に `#[non_exhaustive]` は付けない。** 網羅性チェックを効かせるため。
//!   バリアントの追加は破壊的変更として扱う。

use std::path::PathBuf;

/// 録音サブシステムの失敗。
#[derive(Debug, thiserror::Error)]
pub enum RecordingError {
    #[error("録音デバイスが見つからない")]
    DeviceNotFound,

    /// 録音中にデバイスが消えた。3時間規模の収録では致命的になりうるため、
    /// 呼び出し側は必ず「収録済みテイクの保全」を先に行う。
    #[error("録音中にデバイス '{name}' が切断された")]
    DeviceDisconnected { name: String },

    /// OS 側の加工（自動ゲイン・ノイズ抑制）を回避できなかった。
    #[error("排他モードを取得できず、OS 側の音声加工を回避できない")]
    ExclusiveModeUnavailable,

    #[error("要求した形式に対応していない: {requested}")]
    UnsupportedFormat { requested: String },

    #[error("書き込み先の空き容量が不足している: {path}")]
    DiskFull { path: PathBuf },

    #[error("録音データの書き込みに失敗した: {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// 自動原音設定（強制アライメントと oto 5値推定）の失敗。
#[derive(Debug, thiserror::Error)]
pub enum AlignError {
    #[error("音響モデルを読み込めない")]
    ModelLoad {
        #[source]
        source: ModelError,
    },

    #[error("推論に失敗した")]
    Inference {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// 録音リストが期待する音素列と、アライメント結果が一致しない。
    #[error("音素列が一致しない: 期待 {expected} 個、得られたのは {actual} 個")]
    PhonemeCountMismatch { expected: usize, actual: usize },

    /// 確信度が閾値を下回った。**これはエラーではなく「人に確認させる」入力**なので、
    /// 呼び出し側は失敗として扱わずエディタへ回すこと。
    #[error("確信度が閾値を下回った: {confidence:.3} < {threshold:.3}")]
    LowConfidence { confidence: f32, threshold: f32 },
}

/// 学習済みモデルの取り扱いの失敗。
#[derive(Debug, thiserror::Error)]
pub enum ModelError {
    #[error("モデルが配置されていない: {name}")]
    NotAvailable { name: String },

    #[error("モデルのチェックサムが一致しない: {name}")]
    ChecksumMismatch { name: String },

    #[error("モデル形式に対応していない: {name}")]
    Unsupported { name: String },
}

/// 即時試唱と合成の失敗。
#[derive(Debug, thiserror::Error)]
pub enum SynthError {
    /// 部分音源では通常起きうる状態。曲を歌わせる前にカバレッジで弾くこと。
    #[error("必要なサンプルが未収録: エイリアス '{alias}'")]
    SampleMissing { alias: String },

    #[error("F0 推定に失敗した")]
    F0Estimation {
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("収録音高から離れすぎている: {semitones} 半音")]
    PitchOutOfRange { semitones: i32 },
}

/// 配布パッケージ生成と互換性の失敗。
#[derive(Debug, thiserror::Error)]
pub enum PackageError {
    /// classic UTAU は oto.ini / character.txt を CP932 で読む。
    #[error("CP932 で表現できない文字が含まれる: '{text}'")]
    NotRepresentableInCp932 { text: String },

    /// 書き出す WAV のファイル名は ASCII 固定。
    #[error("書き出すファイル名が ASCII ではない: {name}")]
    NonAsciiFileName { name: String },

    #[error("必須ファイルが欠けている: {name}")]
    MissingRequiredFile { name: String },

    #[error("ZIP の生成に失敗した: {path}")]
    Archive {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

/// プロジェクト永続化の失敗。
#[derive(Debug, thiserror::Error)]
pub enum ProjectError {
    #[error("プロジェクトを開けない: {path}")]
    Open {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("プロジェクトの形式が新しすぎる: 対応 {supported}、実際 {found}")]
    SchemaTooNew { supported: u32, found: u32 },

    #[error("プロジェクトが壊れている: {reason}")]
    Corrupted { reason: String },
}

/// ドメイン層をまとめた上位のエラー。
///
/// **アプリケーション境界より内側で使うのはここまで。** Tauri コマンドや `main` は
/// これを `anyhow::Error` に変換してよいが、その時点で網羅性は失われる。
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Recording(#[from] RecordingError),

    #[error(transparent)]
    Align(#[from] AlignError),

    #[error(transparent)]
    Model(#[from] ModelError),

    #[error(transparent)]
    Synth(#[from] SynthError),

    #[error(transparent)]
    Package(#[from] PackageError),

    #[error(transparent)]
    Project(#[from] ProjectError),
}

/// ドメイン層の `Result` 別名。
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// 利用計測へ送ってよい分類名を返す。
    ///
    /// **送信フィールドはホワイトリスト方式にする**という方針の実装点。
    /// 音源名・ファイルパス・歌詞・プロジェクト名は決して含めない。
    #[must_use]
    pub fn telemetry_kind(&self) -> &'static str {
        match self {
            Self::Recording(RecordingError::DeviceNotFound) => "recording.device_not_found",
            Self::Recording(RecordingError::DeviceDisconnected { .. }) => {
                "recording.device_disconnected"
            }
            Self::Recording(RecordingError::ExclusiveModeUnavailable) => {
                "recording.exclusive_unavailable"
            }
            Self::Recording(RecordingError::UnsupportedFormat { .. }) => {
                "recording.unsupported_format"
            }
            Self::Recording(RecordingError::DiskFull { .. }) => "recording.disk_full",
            Self::Recording(RecordingError::Write { .. }) => "recording.write_failed",
            Self::Align(AlignError::ModelLoad { .. }) => "align.model_load",
            Self::Align(AlignError::Inference { .. }) => "align.inference",
            Self::Align(AlignError::PhonemeCountMismatch { .. }) => "align.phoneme_mismatch",
            Self::Align(AlignError::LowConfidence { .. }) => "align.low_confidence",
            Self::Model(_) => "model.error",
            Self::Synth(SynthError::SampleMissing { .. }) => "synth.sample_missing",
            Self::Synth(SynthError::F0Estimation { .. }) => "synth.f0_estimation",
            Self::Synth(SynthError::PitchOutOfRange { .. }) => "synth.pitch_out_of_range",
            Self::Package(_) => "package.error",
            Self::Project(_) => "project.error",
        }
    }
}
