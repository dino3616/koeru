//! 入力ゲイン（macOS、`TR-REC-14`, `TR-REC-15`）。
//!
//! 校正の目的は「破綻の防止」ではなく「初期値の妥当化」に限る（`TR-REC-14`）。
//! 校正しても収録中の破綻は防げない。ここが担うのは、
//! 最初のひと声を録る前に、明らかに小さすぎる／大きすぎる状態を外すことだけ。
//!
//! # ハードウェアかソフトウェアか
//!
//! ソフトウェア実装のボリュームは校正に使えない（`TR-REC-14` の [Fact]）。
//! デジタル側で掛けても A/D の手前のレベルは変わらないので、
//! 上げれば量子化ノイズごと大きくなるだけ。集約デバイスや仮想デバイスがこれにあたる。
//!
//! CoreAudio はこの区別を直接は教えてくれないので、
//! `kAudioDevicePropertyTransportType` から判定する。
//! 仮想・集約と分かったものはソフトウェア扱いにし、校正の対象から外す。

use super::sys;
use super::{CoreAudioError, object_id_for_public, property_scalar};
use crate::DeviceId;

type Result<T> = std::result::Result<T, CoreAudioError>;

/// ゲインをどう扱えるか（`TR-REC-14`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GainControl {
    /// ハードウェア側のボリューム。校正に使える。
    Hardware,
    /// ソフトウェア側のボリューム。校正に使えないので、値は読めても触らない。
    Software,
    /// 読み書きできない。自動調整せず、OS 設定での調整を1回だけ案内する。
    Unavailable,
}

impl GainControl {
    /// 校正に使えるか。
    #[must_use]
    pub const fn is_usable(self) -> bool {
        matches!(self, Self::Hardware)
    }

    /// 台帳へ残す名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Hardware => "hardware",
            Self::Software => "software",
            Self::Unavailable => "unavailable",
        }
    }
}

/// このデバイスのゲインをどう扱えるか。
#[must_use]
pub fn control(id: &DeviceId) -> GainControl {
    let Some(object) = object_id_for_public(id) else {
        return GainControl::Unavailable;
    };
    let address = sys::AudioObjectPropertyAddress::input(sys::kAudioDevicePropertyVolumeScalar);

    // SAFETY: object は列挙で得た生きている AudioObjectID。
    let has = unsafe { sys::AudioObjectHasProperty(object, &raw const address) } != 0;
    if !has {
        return GainControl::Unavailable;
    }

    let mut settable: u8 = 0;
    // SAFETY: settable は書き込み先として渡す。
    let status = unsafe {
        sys::AudioObjectIsPropertySettable(object, &raw const address, &raw mut settable)
    };
    if status != sys::kAudioHardwareNoError || settable == 0 {
        return GainControl::Unavailable;
    }

    if is_software_device(object) {
        GainControl::Software
    } else {
        GainControl::Hardware
    }
}

/// いまのゲイン（0.0〜1.0）。読めなければ `None`。
#[must_use]
pub fn read(id: &DeviceId) -> Option<f32> {
    let object = object_id_for_public(id)?;
    let address = sys::AudioObjectPropertyAddress::input(sys::kAudioDevicePropertyVolumeScalar);
    // SAFETY: object は生きている AudioObjectID。VolumeScalar は Float32。
    unsafe { property_scalar::<f32>(object, &address, "kAudioDevicePropertyVolumeScalar") }.ok()
}

/// ゲインを書く（0.0〜1.0）。
///
/// ハードウェア側でないデバイスには書かない（`TR-REC-14`）。
/// ソフトウェアのボリュームを動かしても、校正にならないうえ元の状態を壊す。
#[tracing::instrument(skip(id), fields(value), err)]
pub fn write(id: &DeviceId, value: f32) -> Result<()> {
    if !control(id).is_usable() {
        return Err(CoreAudioError::Property {
            selector: "kAudioDevicePropertyVolumeScalar",
            status: sys::kAudioHardwareUnsupportedOperationError,
        });
    }
    let Some(object) = object_id_for_public(id) else {
        return Err(CoreAudioError::Property {
            selector: "kAudioDevicePropertyVolumeScalar",
            status: sys::kAudioHardwareBadDeviceError,
        });
    };
    let address = sys::AudioObjectPropertyAddress::input(sys::kAudioDevicePropertyVolumeScalar);
    let v = value.clamp(0.0, 1.0);

    // SAFETY: v はスタック上の Float32 で、CoreAudio は size バイトだけ読む。
    let status = unsafe {
        sys::AudioObjectSetPropertyData(
            object,
            &raw const address,
            0,
            std::ptr::null(),
            u32::try_from(size_of::<f32>()).unwrap_or(4),
            (&raw const v).cast(),
        )
    };
    if status != sys::kAudioHardwareNoError {
        return Err(CoreAudioError::Property {
            selector: "kAudioDevicePropertyVolumeScalar",
            status,
        });
    }
    Ok(())
}

/// 仮想・集約デバイスか。
///
/// これらのボリュームはソフトウェア実装。 上げても A/D の手前は変わらない。
fn is_software_device(object: sys::AudioObjectID) -> bool {
    let address = sys::AudioObjectPropertyAddress::global(sys::kAudioDevicePropertyTransportType);
    // SAFETY: object は生きている AudioObjectID。TransportType は UInt32。
    let transport =
        unsafe { property_scalar::<u32>(object, &address, "kAudioDevicePropertyTransportType") };
    matches!(
        transport,
        Ok(sys::kAudioDeviceTransportTypeVirtual | sys::kAudioDeviceTransportTypeAggregate)
    )
}
