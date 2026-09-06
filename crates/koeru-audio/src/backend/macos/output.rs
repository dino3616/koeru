//! 出力エンドポイントの種別（macOS、`TR-REC-24`）。
//!
//! 判定結果は記録するが、安全側の根拠には使わない（`TR-REC-24` の [Fact]）。
//! `TransportType` も `DataSource` もドライバの自己申告で、Unknown が正規値として存在する。
//! ヘッドホンと申告していても、装着されている保証はない。
//!
//! 回り込みは録音側でしか確認できない。 ここが返すのは、
//! 「スピーカらしいので鳴らさない」という一次の足切りだけ。
//! 実際の検査は [`koeru_core::leak`] が録った音で行う。

use super::property_scalar;
use super::sys;

/// 出力がどこへ出ているらしいか（`TR-REC-24`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// ヘッドホン・イヤホンらしい。それでも装着の保証は無い。
    Headphones,
    /// スピーカらしい。ガイド・音高提示・モニタリングを鳴らさない（`TR-REC-24`）。
    Speakers,
    /// 分からない。「スピーカではない」と読まない。
    /// 収録前に一度だけ本人へ確認する。
    Unknown,
}

impl OutputKind {
    /// 鳴らしてよいか。
    ///
    /// 分からないときは鳴らさない側に倒さない。 倒すと、
    /// ヘッドホンを使っている人が音高を聞けなくなる。代わりに本人へ確認する。
    #[must_use]
    pub const fn definitely_speakers(self) -> bool {
        matches!(self, Self::Speakers)
    }

    /// 台帳へ残す名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Headphones => "headphones",
            Self::Speakers => "speakers",
            Self::Unknown => "unknown",
        }
    }
}

/// 既定の出力デバイスの種別を見る。
#[must_use]
pub fn default_output_kind() -> OutputKind {
    let Some(device) = default_output_device() else {
        return OutputKind::Unknown;
    };

    // まず経路（内蔵スピーカ / ヘッドホン端子）。内蔵はここで分かる。
    let source = sys::AudioObjectPropertyAddress::output(sys::kAudioDevicePropertyDataSource);
    // SAFETY: device は既定出力として得た生きている AudioObjectID。DataSource は UInt32。
    if let Ok(id) =
        unsafe { property_scalar::<u32>(device, &source, "kAudioDevicePropertyDataSource") }
    {
        if id == sys::kAudioDataSourceInternalSpeaker {
            return OutputKind::Speakers;
        }
        if id == sys::kAudioDataSourceHeadphones {
            return OutputKind::Headphones;
        }
    }

    // 次に接続の種類。
    let transport = sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyTransportType);
    // SAFETY: 同上。TransportType は UInt32。
    let Ok(t) = (unsafe {
        property_scalar::<u32>(device, &transport, "kAudioDevicePropertyTransportType")
    }) else {
        return OutputKind::Unknown;
    };

    match t {
        // 内蔵で、経路が分からなかった。スピーカとみなす。
        sys::kAudioDeviceTransportTypeBuiltIn => OutputKind::Speakers,
        // 画面や AirPlay の先はスピーカ。
        sys::kAudioDeviceTransportTypeHDMI
        | sys::kAudioDeviceTransportTypeDisplayPort
        | sys::kAudioDeviceTransportTypeAirPlay => OutputKind::Speakers,
        // Bluetooth も USB も、ヘッドホンとスピーカの両方がある。決められない。
        sys::kAudioDeviceTransportTypeBluetooth
        | sys::kAudioDeviceTransportTypeBluetoothLE
        | sys::kAudioDeviceTransportTypeUSB => OutputKind::Unknown,
        _ => OutputKind::Unknown,
    }
}

fn default_output_device() -> Option<sys::AudioObjectID> {
    let address =
        sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDefaultOutputDevice);
    // SAFETY: kAudioObjectSystemObject は常に存在する。DefaultOutputDevice は AudioObjectID。
    unsafe {
        property_scalar::<sys::AudioObjectID>(
            sys::kAudioObjectSystemObject,
            &address,
            "kAudioHardwarePropertyDefaultOutputDevice",
        )
    }
    .ok()
}
