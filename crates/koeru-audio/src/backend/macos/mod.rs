//! macOS の入力デバイス列挙。
//!
//! **`TR-REC-03` が要求するのは表示名ではなく永続識別子。** macOS では
//! `kAudioDevicePropertyDeviceUID` がそれにあたる。表示名は一覧に出すためだけに取り、
//! 同一性の判定には使わない。
//!
//! `TR-REC-04` の消失検知は `kAudioDevicePropertyDeviceIsAlive` を見る。

mod capture_device;
mod sys;

pub use capture_device::{
    MicPermission, MicrophoneMode, active_microphone_mode, microphone_permission,
    preferred_microphone_mode, privacy_settings_url,
};

use crate::device::{DeviceId, DeviceInfo, RedactedName};
use std::os::raw::c_void;

/// CoreAudio の呼び出しが失敗した理由。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CoreAudioError {
    /// プロパティの取得に失敗した。`status` は OSStatus。
    #[error("CoreAudio のプロパティ取得に失敗した（selector={selector}, status={status}）")]
    Property { selector: &'static str, status: i32 },

    /// CFString を UTF-8 へ取り出せなかった。
    #[error("文字列を UTF-8 として取り出せなかった（{selector}）")]
    NotUtf8 { selector: &'static str },
}

impl CoreAudioError {
    /// 送信層へ載せてよい固定文字列。**`Display` を送らない。**
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Property { .. } => "recording.macos.property_failed",
            Self::NotUtf8 { .. } => "recording.macos.not_utf8",
        }
    }
}

type Result<T> = std::result::Result<T, CoreAudioError>;

/// プロパティのバイト列を取り出す。
///
/// # Safety
/// 呼び出し側は `object` が生きている AudioObjectID であることを保証する。
/// 戻り値のバイト列の解釈は呼び出し側の責任。
unsafe fn property_bytes(
    object: sys::AudioObjectID,
    address: &sys::AudioObjectPropertyAddress,
    selector: &'static str,
) -> Result<Vec<u8>> {
    let mut size: u32 = 0;
    // SAFETY: address は有効な参照から取ったポインタ。qualifier は使わないので null / 0。
    let status = unsafe {
        sys::AudioObjectGetPropertyDataSize(object, address, 0, std::ptr::null(), &raw mut size)
    };
    if status != sys::kAudioHardwareNoError {
        return Err(CoreAudioError::Property { selector, status });
    }

    let mut buf = vec![0_u8; size as usize];
    if size == 0 {
        return Ok(buf);
    }
    // SAFETY: buf は size バイト確保済みで、CoreAudio は ioDataSize を超えて書かない。
    let status = unsafe {
        sys::AudioObjectGetPropertyData(
            object,
            address,
            0,
            std::ptr::null(),
            &raw mut size,
            buf.as_mut_ptr().cast::<c_void>(),
        )
    };
    if status != sys::kAudioHardwareNoError {
        return Err(CoreAudioError::Property { selector, status });
    }
    buf.truncate(size as usize);
    Ok(buf)
}

/// 固定長のプロパティを1つ読む。
unsafe fn property_scalar<T: Copy>(
    object: sys::AudioObjectID,
    address: &sys::AudioObjectPropertyAddress,
    selector: &'static str,
) -> Result<T> {
    // SAFETY: 呼び出し側の保証をそのまま引き継ぐ。
    let bytes = unsafe { property_bytes(object, address, selector) }?;
    if bytes.len() < size_of::<T>() {
        return Err(CoreAudioError::Property {
            selector,
            status: -1,
        });
    }
    // SAFETY: 長さを確かめてある。T は Copy で、CoreAudio が返すのは C の POD。
    Ok(unsafe { bytes.as_ptr().cast::<T>().read_unaligned() })
}

/// `CFStringRef` を返すプロパティを `String` として読む。
unsafe fn property_string(
    object: sys::AudioObjectID,
    address: &sys::AudioObjectPropertyAddress,
    selector: &'static str,
) -> Result<String> {
    // SAFETY: 呼び出し側の保証をそのまま引き継ぐ。
    let cf: sys::CFStringRef = unsafe { property_scalar(object, address, selector) }?;
    if cf.is_null() {
        return Err(CoreAudioError::NotUtf8 { selector });
    }
    // SAFETY: cf は CoreAudio が返した生きた CFStringRef。
    let len = unsafe { sys::CFStringGetLength(cf) };
    // UTF-8 は1文字あたり最大4バイト。終端の1バイトを足す。
    let cap = len * 4 + 1;
    let mut buf = vec![0_i8; cap as usize];
    // SAFETY: buf は cap バイト確保済み。CFStringGetCString は終端を書く。
    let ok =
        unsafe { sys::CFStringGetCString(cf, buf.as_mut_ptr(), cap, sys::kCFStringEncodingUTF8) };
    // SAFETY: プロパティで得た CFStringRef は Get 規則なので解放しない。
    // Copy 規則のものだけ CFRelease が要る。ここは Get なので何もしない。
    if ok == 0 {
        return Err(CoreAudioError::NotUtf8 { selector });
    }
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    let bytes: Vec<u8> = buf[..end].iter().map(|&c| c as u8).collect();
    String::from_utf8(bytes).map_err(|_| CoreAudioError::NotUtf8 { selector })
}

/// 入力チャンネル数を数える。0 なら入力デバイスではない。
unsafe fn input_channels(device: sys::AudioObjectID) -> Result<u16> {
    let address =
        sys::AudioObjectPropertyAddress::input(sys::kAudioDevicePropertyStreamConfiguration);
    // SAFETY: 呼び出し側の保証をそのまま引き継ぐ。
    let bytes = unsafe { property_bytes(device, &address, "stream_configuration") }?;
    if bytes.len() < size_of::<sys::AudioBufferListHeader>() {
        return Ok(0);
    }
    // SAFETY: 長さを確かめてある。AudioBufferList の先頭は mNumberBuffers。
    let header = unsafe {
        bytes
            .as_ptr()
            .cast::<sys::AudioBufferListHeader>()
            .read_unaligned()
    };
    let mut total: u32 = 0;
    // **`AudioBufferList` は `{ UInt32 mNumberBuffers; AudioBuffer mBuffers[1]; }`。**
    // `AudioBuffer` はポインタを含むので8バイト境界に揃い、`mNumberBuffers` の後ろに
    // 4バイトの詰め物が入る。**ヘッダのサイズ（4）を配列の開始位置にすると全件ずれる。**
    let base =
        size_of::<sys::AudioBufferListHeader>().next_multiple_of(align_of::<sys::AudioBuffer>());
    for i in 0..header.mNumberBuffers as usize {
        let at = base + i * size_of::<sys::AudioBuffer>();
        if at + size_of::<sys::AudioBuffer>() > bytes.len() {
            break;
        }
        // SAFETY: 上で範囲を確かめている。
        let b = unsafe {
            bytes
                .as_ptr()
                .add(at)
                .cast::<sys::AudioBuffer>()
                .read_unaligned()
        };
        total += b.mNumberChannels;
    }
    Ok(u16::try_from(total).unwrap_or(u16::MAX))
}

/// 入力デバイスを列挙する（TR-REC-03）。
///
/// **表示名は同一性に使わない。** 返す `DeviceId` は `kAudioDevicePropertyDeviceUID`。
#[tracing::instrument(err)]
pub fn enumerate_input_devices() -> Result<Vec<DeviceInfo>> {
    let addr = sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDevices);
    // SAFETY: システムオブジェクトは常に存在する。
    let bytes = unsafe { property_bytes(sys::kAudioObjectSystemObject, &addr, "devices") }?;
    let count = bytes.len() / size_of::<sys::AudioObjectID>();

    let default_addr =
        sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDefaultInputDevice);
    // SAFETY: 同上。既定デバイスが無い場合もあるので、失敗は 0 として扱う。
    let default_id: sys::AudioObjectID = unsafe {
        property_scalar(
            sys::kAudioObjectSystemObject,
            &default_addr,
            "default_input",
        )
    }
    .unwrap_or(0);

    let mut out = Vec::new();
    for i in 0..count {
        // SAFETY: 長さから導いた添字。
        let id = unsafe {
            bytes
                .as_ptr()
                .add(i * size_of::<sys::AudioObjectID>())
                .cast::<sys::AudioObjectID>()
                .read_unaligned()
        };

        // SAFETY: 列挙で得た生きた AudioObjectID。
        let channels = unsafe { input_channels(id) }?;
        if channels == 0 {
            continue; // 出力専用のデバイス
        }

        let uid_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyDeviceUID);
        // SAFETY: 同上。
        let uid = unsafe { property_string(id, &uid_addr, "device_uid") }?;

        let name_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioObjectPropertyName);
        // SAFETY: 同上。名前が取れないデバイスもあるので、失敗は空文字にする。
        let name = unsafe { property_string(id, &name_addr, "device_name") }.unwrap_or_default();

        let rate_addr =
            sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyNominalSampleRate);
        // SAFETY: 同上。
        let rate: f64 = unsafe { property_scalar(id, &rate_addr, "nominal_sample_rate") }?;

        out.push(DeviceInfo {
            id: DeviceId::new(uid),
            name: RedactedName::new(name),
            is_default: id == default_id,
            // FFI が返すのは倍精度。負や巨大な値は来ない前提だが、飽和で受ける。
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            native_sample_rate_hz: rate.max(0.0) as u32,
            channels,
        });
    }
    tracing::debug!(found = out.len(), "入力デバイスを列挙した");
    Ok(out)
}

/// デバイスが生きているか（TR-REC-04 の消失検知）。
#[tracing::instrument(skip(id), err)]
pub fn is_alive(id: &DeviceId) -> Result<bool> {
    let Some(object) = object_id_for(id)? else {
        return Ok(false);
    };
    let addr = sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyDeviceIsAlive);
    // SAFETY: object は直前の列挙で得た生きた ID。
    let alive: u32 = unsafe { property_scalar(object, &addr, "device_is_alive") }?;
    Ok(alive != 0)
}

/// 永続識別子から、いまの `AudioObjectID` を引く。
///
/// **`AudioObjectID` は再起動や抜き差しで変わる。** 固定してよいのは UID だけ。
fn object_id_for(id: &DeviceId) -> Result<Option<sys::AudioObjectID>> {
    let addr = sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDevices);
    // SAFETY: システムオブジェクトは常に存在する。
    let bytes = unsafe { property_bytes(sys::kAudioObjectSystemObject, &addr, "devices") }?;
    let count = bytes.len() / size_of::<sys::AudioObjectID>();
    for i in 0..count {
        // SAFETY: 長さから導いた添字。
        let object = unsafe {
            bytes
                .as_ptr()
                .add(i * size_of::<sys::AudioObjectID>())
                .cast::<sys::AudioObjectID>()
                .read_unaligned()
        };
        let uid_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyDeviceUID);
        // SAFETY: 列挙で得た生きた ID。
        if let Ok(uid) = unsafe { property_string(object, &uid_addr, "device_uid") }
            && uid == id.as_str()
        {
            return Ok(Some(object));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **実機のデバイスを列挙する。** 入力デバイスが無い環境もあるので、
    /// 「落ちないこと」と「取れたものの形が正しいこと」を見る。
    #[test]
    fn 入力デバイスを列挙できる() {
        let devices = enumerate_input_devices().expect("列挙が成功する");
        for d in &devices {
            assert!(!d.id.as_str().is_empty(), "UID が空でない");
            assert!(d.channels > 0, "入力チャンネルがある");
            assert!(d.native_sample_rate_hz > 0, "ネイティブレートが正");
        }
    }

    /// 列挙で得た識別子は、そのまま生死判定に使える。
    #[test]
    fn 列挙した識別子で生死を引ける() {
        let devices = enumerate_input_devices().expect("列挙が成功する");
        for d in &devices {
            assert!(
                is_alive(&d.id).expect("生死を引ける"),
                "いま列挙できたデバイスは生きている"
            );
        }
    }

    /// 存在しない識別子は「生きていない」になる。**別のデバイスへ倒れない。**
    #[test]
    fn 知らない識別子は生きていない() {
        let unknown = DeviceId::new("存在しないデバイスの UID");
        assert!(!is_alive(&unknown).expect("引けること自体は成功する"));
    }

    /// **回帰: `AudioBufferList` の配列は詰め物のぶんだけ後ろから始まる。**
    ///
    /// ヘッダのサイズ（4）を配列の開始位置にすると全デバイスの入力チャンネルが 0 になり、
    /// **入力デバイスが1件も見つからない**。実機で踏んだ。
    #[test]
    fn バッファ配列の開始位置は八バイト境界に揃う() {
        let base = size_of::<sys::AudioBufferListHeader>()
            .next_multiple_of(align_of::<sys::AudioBuffer>());
        assert_eq!(size_of::<sys::AudioBufferListHeader>(), 4);
        assert_eq!(align_of::<sys::AudioBuffer>(), 8);
        assert_eq!(base, 8, "ヘッダのサイズ 4 ではなく 8 から始まる");
    }
}
