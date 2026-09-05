//! まだ書いていない OS の音声入出力。
//!
//! 黙って何もしないのではなく、書いていないことを言う。
//! ここが返すのは常に [`UnsupportedError`] で、
//! 呼び出し側は「この OS ではまだ録れない」と表示できる。
//!
//! # なぜ空の実装を置くのか
//!
//! `koeru-app` を全 OS でコンパイルできるようにするため。
//! macOS だけで通る形にしておくと、他 OS の CI が
//! 「アプリが組み立たない」ところで止まり、**その先にあるドメイン層の
//! 回帰にも気づけなくなる。**
//!
//! Windows と Linux のバックエンドは `DEC-REC-001` のとおり
//! それぞれの API を直接叩いて書く（`TR-REC-08`〜`12`）。ここはその席。

use std::path::PathBuf;

use crate::{DeviceId, DeviceInfo, ring};

/// この OS ではまだ書いていない。
#[derive(Debug, thiserror::Error)]
#[error("この OS の音声入出力はまだ書いていない")]
pub struct UnsupportedError;

impl UnsupportedError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        "audio.unsupported_platform"
    }
}

/// 列挙の失敗。この OS では常に失敗する。
pub type CoreAudioError = UnsupportedError;
/// キャプチャの失敗。
pub type CaptureError = UnsupportedError;
/// 再生の失敗。
pub type PlaybackError = UnsupportedError;

/// 全チャンネルを混ぜる（`TR-REC-06`）。
pub const MIX_ALL: usize = usize::MAX;

/// 開けたキャプチャの条件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CaptureFormat {
    pub sample_rate_hz: u32,
    pub channels: u16,
    /// 1回のコールバックで来る最大フレーム数。
    pub max_frames_per_slice: u32,
}

/// 開いているキャプチャ。この OS では作れない。
#[derive(Debug)]
pub struct Capture {
    _private: (),
}

impl Capture {
    /// 実際に開けた条件。
    #[must_use]
    pub const fn format(&self) -> CaptureFormat {
        CaptureFormat {
            sample_rate_hz: 0,
            channels: 0,
            max_frames_per_slice: 0,
        }
    }
    /// 収録を始める。
    pub const fn arm(&self) {}
    /// 収録を止める。
    pub const fn disarm(&self) {}
    /// 取りこぼしの回数。
    #[must_use]
    pub const fn discontinuities(&self) -> usize {
        0
    }
    /// レンダの失敗回数。
    #[must_use]
    pub const fn render_errors(&self) -> usize {
        0
    }
    /// チャンネルごとの RMS（`TR-REC-06`）。
    #[must_use]
    pub fn channel_rms(&self) -> Vec<f32> {
        Vec::new()
    }
    /// 測り直す。
    pub const fn reset_channel_rms(&self) {}
    /// モノラルの元にするチャンネルを決める。
    pub const fn set_source_channel(&self, _channel: usize) {}
    /// 全チャンネルを混ぜる。
    pub const fn set_source_mix(&self) {}
    /// いまどこから取っているか。
    #[must_use]
    pub const fn source_channel(&self) -> usize {
        0
    }
}

/// 継ぎ足す口（`TR-SYN-03`）。
#[derive(Debug, Clone)]
pub struct Feed {
    _private: (),
}

impl Feed {
    /// 続きを継ぎ足す。
    pub const fn push(&self, _more: &[f32]) {}
    /// もう継ぎ足さないと宣言する。
    pub const fn seal(&self) {}
    /// まだ鳴らしていない長さ。
    #[must_use]
    pub const fn buffered(&self) -> usize {
        0
    }
}

/// 鳴っている最中の再生。この OS では作れない。
#[derive(Debug)]
pub struct Playback {
    _private: (),
}

impl Playback {
    /// 末尾まで流し終えたか。
    #[must_use]
    pub const fn is_finished(&self) -> bool {
        true
    }
    /// いま何フレーム目まで流したか。
    #[must_use]
    pub const fn position(&self) -> usize {
        0
    }
    /// 別のスレッドから継ぎ足すための口。
    #[must_use]
    pub const fn feed(&self) -> Feed {
        Feed { _private: () }
    }
    /// 続きを継ぎ足す。
    pub const fn push(&self, _more: &[f32]) {}
    /// もう継ぎ足さないと宣言する。
    pub const fn seal(&self) {}
    /// 枯渇の回数。
    #[must_use]
    pub const fn starved(&self) -> usize {
        0
    }
    /// 止める。
    ///
    /// # Errors
    ///
    /// この OS では常に失敗する。
    pub const fn stop(&self) -> Result<(), PlaybackError> {
        Err(UnsupportedError)
    }
}

// 本物と同じ形にする。 どちらも「落とすと止まる」という約束を持つので、
// 片方に `Drop` が無いと、呼び出し側の `drop(...)` が lint に引っかかる。
// 構築できない型なので、ここは実際には走らない。
impl Drop for Playback {
    fn drop(&mut self) {}
}

impl Drop for Capture {
    fn drop(&mut self) {}
}

/// マイクモード（`TR-REC-11`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrophoneMode {
    /// 分からない。`is_clean()` ではない。
    Unknown,
}

impl MicrophoneMode {
    /// 画面と IPC へ渡す識別子。`Debug` を wire 形式にしない。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "Unknown",
        }
    }

    /// OS 側の加工が入っていないと言えるか。
    #[must_use]
    pub const fn is_clean(self) -> bool {
        false
    }
}

/// ゲインをどう扱えるか（`TR-REC-14`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainControl {
    /// 読み書きできない。
    Unavailable,
}

impl GainControl {
    /// 校正に使えるか。
    #[must_use]
    pub const fn is_usable(self) -> bool {
        false
    }
    /// 台帳へ残す名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "unavailable"
    }
}

/// 出力の種別（`TR-REC-24`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// 分からない。「スピーカではない」と読まない。
    Unknown,
}

impl OutputKind {
    /// スピーカと断定できるか。
    #[must_use]
    pub const fn definitely_speakers(self) -> bool {
        false
    }
    /// 台帳へ残す名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        "unknown"
    }
}

/// 入力デバイスを挙げる。
///
/// # Errors
///
/// この OS では常に失敗する。
pub const fn enumerate_input_devices() -> Result<Vec<DeviceInfo>, CoreAudioError> {
    Err(UnsupportedError)
}

/// キャプチャを開く。
///
/// # Errors
///
/// この OS では常に失敗する。
pub const fn open(
    _device: &DeviceId,
    _ring_capacity: usize,
) -> Result<(Capture, ring::Consumer), CaptureError> {
    Err(UnsupportedError)
}

/// 鳴らす。
///
/// # Errors
///
/// この OS では常に失敗する。
pub fn play(_samples: Vec<f32>, _rate_hz: u32) -> Result<Playback, PlaybackError> {
    Err(UnsupportedError)
}

/// 継ぎ足せる再生を始める（`TR-SYN-03`）。
///
/// # Errors
///
/// この OS では常に失敗する。
pub fn play_streaming(_head: Vec<f32>, _rate_hz: u32) -> Result<Playback, PlaybackError> {
    Err(UnsupportedError)
}

/// いまのマイクモード。
#[must_use]
pub const fn active_microphone_mode() -> MicrophoneMode {
    MicrophoneMode::Unknown
}

/// 既定の出力デバイスの種別。
#[must_use]
pub const fn default_output_kind() -> OutputKind {
    OutputKind::Unknown
}

/// このデバイスのゲインをどう扱えるか。
#[must_use]
pub const fn gain_control(_id: &DeviceId) -> GainControl {
    GainControl::Unavailable
}

/// いまのゲイン。
#[must_use]
pub const fn read_gain(_id: &DeviceId) -> Option<f32> {
    None
}

/// ゲインを書く。
///
/// # Errors
///
/// この OS では常に失敗する。
pub const fn write_gain(_id: &DeviceId, _value: f32) -> Result<(), CoreAudioError> {
    Err(UnsupportedError)
}

/// プライバシー設定を開く URL（`TR-PLT-18`）。
#[must_use]
pub fn privacy_settings_url() -> PathBuf {
    PathBuf::new()
}
