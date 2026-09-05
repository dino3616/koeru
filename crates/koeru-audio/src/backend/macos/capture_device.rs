//! macOS のマイクモード検知とマイク権限。
//!
//! ここだけ Objective-C なので、自前の `extern "C"` では届かない（`DEC-REC-001`）。
//! CoreAudio の HAL は素の C API なので [`super::sys`] で自前束縛しているが、
//! `AVCaptureDevice` はメッセージ送信で、同じ手が使えない。
//!
//! ## なぜマイクモードを見るのか
//!
//! macOS 12 以降、システム設定にマイクモード（standard / voiceIsolation / wideSpectrum）がある。
//! `voiceIsolation` が有効だと、macOS が全キャプチャにノイズ抑制と音声強調をかける。
//! これは KOERU が避けなければならない「OS 側の音声加工」そのもので、
//! アプリから standard へ戻すことはできない（`TR-REC-11`）。
//!
//! できるのは検知して本人に伝えることだけ。検知できないと、本人の声ではない音が
//! 本人の声として録れて、誰も気づかない。

use objc2_av_foundation::{
    AVAuthorizationStatus, AVCaptureDevice, AVCaptureMicrophoneMode, AVMediaTypeAudio,
};

/// OS 側が入力に掛けている加工の種類（`TR-REC-11`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicrophoneMode {
    /// 加工なし。KOERU が要求する状態。
    Standard,
    /// ノイズ抑制と音声強調が掛かる。 本人の声ではなくなる。
    VoiceIsolation,
    /// 周囲音を広く拾う。標準ではない加工が掛かる。
    WideSpectrum,
    /// この macOS では判定できない（12.0 未満、または未知の値）。
    Unknown,
}

impl MicrophoneMode {
    /// 画面と IPC へ渡す識別子。
    ///
    /// `Debug` を wire 形式にしない。 variant を改名すると、
    /// 画面側のリテラル union が黙って外れる。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Standard => "Standard",
            Self::VoiceIsolation => "VoiceIsolation",
            Self::WideSpectrum => "WideSpectrum",
            Self::Unknown => "Unknown",
        }
    }

    /// 収録してよい状態か。
    ///
    /// `Unknown` は「よい」に倒さない。 分からないまま録ると、
    /// 加工された声を本人の声として渡すことになる。
    #[must_use]
    pub const fn is_clean(self) -> bool {
        matches!(self, Self::Standard)
    }

    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Standard => "recording.macos.mic_mode.standard",
            Self::VoiceIsolation => "recording.macos.mic_mode.voice_isolation",
            Self::WideSpectrum => "recording.macos.mic_mode.wide_spectrum",
            Self::Unknown => "recording.macos.mic_mode.unknown",
        }
    }
}

fn from_raw(raw: AVCaptureMicrophoneMode) -> MicrophoneMode {
    match raw {
        AVCaptureMicrophoneMode::Standard => MicrophoneMode::Standard,
        AVCaptureMicrophoneMode::VoiceIsolation => MicrophoneMode::VoiceIsolation,
        AVCaptureMicrophoneMode::WideSpectrum => MicrophoneMode::WideSpectrum,
        _ => MicrophoneMode::Unknown,
    }
}

/// いま実際に効いているマイクモード（`TR-REC-11`）。
///
/// `preferred` と食い違うことがある。 現在の音声経路が希望のモードを支えない場合、
/// OS は別のモードで動かす。セッションメタデータに残すのはこちら。
#[must_use]
pub fn active_microphone_mode() -> MicrophoneMode {
    // SAFETY: クラスメソッドで、引数も戻り値も POD。macOS 12.0 未満では
    // Standard 相当が返る（Apple の既定）。
    let raw = unsafe { AVCaptureDevice::activeMicrophoneMode() };
    from_raw(raw)
}

/// 本人がシステム設定で選んでいるマイクモード。
#[must_use]
pub fn preferred_microphone_mode() -> MicrophoneMode {
    // SAFETY: 同上。
    let raw = unsafe { AVCaptureDevice::preferredMicrophoneMode() };
    from_raw(raw)
}

/// マイク権限の状態（`TR-PLT-18`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MicPermission {
    /// まだ一度も要求していない。要求すれば OS のダイアログが出る。
    NotDetermined,
    /// 使ってよい。
    Granted,
    /// 本人が拒否した。解除手順を提示する（`TR-PLT-18`）。
    Denied,
    /// 組織のポリシー等で禁じられている。本人の操作では解除できない。
    Restricted,
    /// 未知の値。
    Unknown,
}

impl MicPermission {
    #[must_use]
    pub const fn is_granted(self) -> bool {
        matches!(self, Self::Granted)
    }

    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NotDetermined => "recording.macos.mic_permission.not_determined",
            Self::Granted => "recording.macos.mic_permission.granted",
            Self::Denied => "recording.macos.mic_permission.denied",
            Self::Restricted => "recording.macos.mic_permission.restricted",
            Self::Unknown => "recording.macos.mic_permission.unknown",
        }
    }
}

/// いまのマイク権限を見る。要求はしない。
///
/// 要求は「初回の録音画面に入る直前の1回だけ」と決めてあるので（`TR-PLT-18`）、
/// 状態を見るだけの経路と、要求する経路を分けてある。
#[must_use]
pub fn microphone_permission() -> MicPermission {
    // AVMediaTypeAudio は Option の extern static。フレームワークが載っていなければ取れない。
    // その場合は Unknown にする。 権限の判定で落とさない。
    // SAFETY: AVFoundation が持つ不変の静的文字列で、書き換えられることはない。
    let media_type = unsafe { AVMediaTypeAudio };
    let Some(media_type) = media_type else {
        tracing::warn!("AVMediaTypeAudio を引けなかった");
        return MicPermission::Unknown;
    };
    // SAFETY: media_type は AVFoundation が持つ静的な文字列で、
    // authorizationStatusForMediaType: はクラスメソッド。
    let status = unsafe { AVCaptureDevice::authorizationStatusForMediaType(media_type) };
    match status {
        AVAuthorizationStatus::NotDetermined => MicPermission::NotDetermined,
        AVAuthorizationStatus::Authorized => MicPermission::Granted,
        AVAuthorizationStatus::Denied => MicPermission::Denied,
        AVAuthorizationStatus::Restricted => MicPermission::Restricted,
        _ => MicPermission::Unknown,
    }
}

/// システム設定のマイク欄を開く URL（`TR-PLT-18`）。
///
/// 拒否されたときに「設定画面を開くボタン」を出すために使う。
/// 開く操作そのものはこのクレートの外（シェル層）が持つ。
#[must_use]
pub const fn privacy_settings_url() -> &'static str {
    "x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone"
}

/// マイク権限を要求する（`TR-PLT-18`）。
///
/// 要求してよいのは「初回の録音画面に入る直前」の1回だけ。
/// 起動時やプロジェクト作成時には呼ばない。
///
/// この呼び出しはブロックしない。OS のダイアログが出て、本人が答えると
/// `on_result` が CoreAudio 側の任意のスレッドから1度だけ呼ばれる。
///
/// 権限が無い間、macOS はエラーを返さず無音を流す（Apple の仕様）。
/// だから「無音のまま録音が進む状態にしない」ことが要件になっている（`TR-PLT-18`）。
/// 権限が付いたら、アプリを再起動せずに収録へ戻れること。
pub fn request_microphone_permission<F>(on_result: F)
where
    F: Fn(bool) + Send + Sync + 'static,
{
    // SAFETY: AVFoundation が持つ不変の静的文字列。
    let media_type = unsafe { AVMediaTypeAudio };
    let Some(media_type) = media_type else {
        tracing::warn!("AVMediaTypeAudio を引けなかった。要求できない");
        on_result(false);
        return;
    };
    let handler = block2::RcBlock::new(move |granted: objc2::runtime::Bool| {
        on_result(granted.as_bool());
    });
    // SAFETY: media_type と handler はどちらも有効。handler は RcBlock が
    // 保持しており、Objective-C 側が copy して持つ。
    unsafe {
        AVCaptureDevice::requestAccessForMediaType_completionHandler(media_type, &handler);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実機のマイクモードを読む。 値が何であれ、判定が返ることを見る。
    #[test]
    fn マイクモードを読める() {
        let active = active_microphone_mode();
        let preferred = preferred_microphone_mode();
        // どちらも既知の4値のいずれかに落ちる
        assert!(matches!(
            active,
            MicrophoneMode::Standard
                | MicrophoneMode::VoiceIsolation
                | MicrophoneMode::WideSpectrum
                | MicrophoneMode::Unknown
        ));
        assert!(matches!(
            preferred,
            MicrophoneMode::Standard
                | MicrophoneMode::VoiceIsolation
                | MicrophoneMode::WideSpectrum
                | MicrophoneMode::Unknown
        ));
    }

    /// 未知は「よい」に倒さない。 分からないまま録らせない。
    #[test]
    fn 未知のマイクモードは収録してよい状態にしない() {
        assert!(MicrophoneMode::Standard.is_clean());
        assert!(!MicrophoneMode::Unknown.is_clean());
        assert!(!MicrophoneMode::VoiceIsolation.is_clean());
        assert!(!MicrophoneMode::WideSpectrum.is_clean());
    }

    /// 実機の権限状態を読む。要求はしない。
    #[test]
    fn マイク権限を読める() {
        let p = microphone_permission();
        assert!(matches!(
            p,
            MicPermission::NotDetermined
                | MicPermission::Granted
                | MicPermission::Denied
                | MicPermission::Restricted
                | MicPermission::Unknown
        ));
    }

    /// 送信層へ出す語彙は固定文字列で、状態の中身を含まない。
    #[test]
    fn 種別は固定文字列になる() {
        assert_eq!(
            MicrophoneMode::VoiceIsolation.kind(),
            "recording.macos.mic_mode.voice_isolation"
        );
        assert_eq!(
            MicPermission::Denied.kind(),
            "recording.macos.mic_permission.denied"
        );
    }
}
