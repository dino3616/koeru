//! 収録セッションの失敗。
//!
//! **ドメイン層なので `anyhow::Error` を返さない。** 呼び出し側が `match` で
//! 網羅的に分岐できることが、回復の前提になる。

use crate::session::{Device, Effects, Gain, Liveness};

/// 収録セッションの状態遷移が拒まれた理由。
///
/// **`#[non_exhaustive]` を付けない。** 網羅性チェックを効かせるため、
/// バリアントの追加は破壊的変更として扱う。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SessionError {
    /// アプリを終了したあとに操作しようとした。
    #[error("セッションは終了している")]
    Exited,

    /// デバイスの状態が操作の前提に合わない。
    #[error("デバイスが {actual:?} なので、この操作には {expected:?} が要る")]
    DeviceState { expected: Device, actual: Device },

    /// ストリームの開閉状態が操作の前提に合わない。
    #[error("ストリームが開いている必要がある: {want_open}")]
    StreamState { want_open: bool },

    /// 効果の列挙状態が操作の前提に合わない。
    #[error("効果の状態が {actual:?} なので、この操作には {expected:?} が要る")]
    EffectsState { expected: Effects, actual: Effects },

    /// 入力レベルの校正状態が操作の前提に合わない。
    #[error("ゲインが {actual:?} なので、この操作には {expected:?} が要る")]
    GainState { expected: Gain, actual: Gain },

    /// 入力経路の生死判定が操作の前提に合わない。
    #[error("入力の生死が {actual:?} なので、この操作には {expected:?} が要る")]
    LivenessState {
        expected: Liveness,
        actual: Liveness,
    },

    /// 収録中／収録していない、が操作の前提に合わない。
    #[error("収録中である必要がある: {want_recording}")]
    RecordingState { want_recording: bool },

    /// 手順の提示は多くとも一度しか出さない（INV-REC-108）。
    #[error("手順はすでに提示済み")]
    PromptAlreadyShown,

    /// 回り込みの確認をしていない（INV-REC-105）。
    #[error("回り込みを確認していない")]
    LeakNotChecked,

    /// ガイドはすでに有効。
    #[error("ガイドはすでに鳴らしている")]
    GuideAlreadyEnabled,

    /// 残量を見積もっていない、または足りない（TR-REC-41）。
    #[error("保存先の残量が確かめられていない、または足りない")]
    NotEnoughSpace,

    /// 保持できるテイク数の上限に達した。
    #[error("テイク数が上限に達している")]
    TakeLimitReached,
}

impl SessionError {
    /// 送信層へ載せてよい固定文字列。
    ///
    /// **`Display` を送らない。** `Display` にはデバイス名やパスが入りうる。
    /// 送ってよいのはこの固定語彙だけで、ホワイトリストはここが唯一の出どころ。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Exited => "recording.session_exited",
            Self::DeviceState { .. } => "recording.device_state",
            Self::StreamState { .. } => "recording.stream_state",
            Self::EffectsState { .. } => "recording.effects_state",
            Self::GainState { .. } => "recording.gain_state",
            Self::LivenessState { .. } => "recording.liveness_state",
            Self::RecordingState { .. } => "recording.recording_state",
            Self::PromptAlreadyShown => "recording.prompt_already_shown",
            Self::LeakNotChecked => "recording.leak_not_checked",
            Self::GuideAlreadyEnabled => "recording.guide_already_enabled",
            Self::NotEnoughSpace => "recording.not_enough_space",
            Self::TakeLimitReached => "recording.take_limit_reached",
        }
    }
}
