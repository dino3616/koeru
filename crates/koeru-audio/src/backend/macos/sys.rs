//! CoreAudio HAL の最小束縛。
//!
//! **objc2 を通さない。** CoreAudio の HAL（`AudioObjectGetPropertyData` 系）は
//! Objective-C ではなく素の C API なので、`extern "C"` で直接宣言できる。
//!
//! 束ねる相手を組織メンテのものに限るという方針（`DEC-REC-001`）に対して、
//! **Apple の interop で組織がメンテしている crate 族が存在しない。**
//! `coreaudio-rs` は RustAudio org だが中身は個人の `objc2` 族で、依存木に16ノード入る。
//! ここだけ自前で持つほうが、方針との食い違いが小さい。
//!
//! 宣言するのは実際に使うものだけ。**網羅しない。**

#![allow(non_upper_case_globals, non_snake_case)]

use std::os::raw::{c_char, c_void};

pub(super) type OSStatus = i32;
pub(super) type AudioObjectID = u32;
pub(super) type CFIndex = isize;
pub(super) type CFStringRef = *const c_void;
pub(super) type CFStringEncoding = u32;

pub(super) const kAudioHardwareNoError: OSStatus = 0;
pub(super) const kAudioObjectSystemObject: AudioObjectID = 1;
pub(super) const kCFStringEncodingUTF8: CFStringEncoding = 0x0800_0100;

/// 4文字コードを `u32` にする。CoreAudio のセレクタはすべてこの形。
const fn fourcc(s: &[u8; 4]) -> u32 {
    ((s[0] as u32) << 24) | ((s[1] as u32) << 16) | ((s[2] as u32) << 8) | (s[3] as u32)
}

pub(super) const kAudioHardwarePropertyDevices: u32 = fourcc(b"dev#");
pub(super) const kAudioHardwarePropertyDefaultInputDevice: u32 = fourcc(b"dIn ");
/// **TR-REC-03 が要求する永続識別子。** 表示名ではなくこれをプロジェクトに固定する。
pub(super) const kAudioDevicePropertyDeviceUID: u32 = fourcc(b"uid ");
pub(super) const kAudioObjectPropertyName: u32 = fourcc(b"lnam");
pub(super) const kAudioDevicePropertyNominalSampleRate: u32 = fourcc(b"nsrt");
pub(super) const kAudioDevicePropertyStreamConfiguration: u32 = fourcc(b"slay");
/// TR-REC-04 の消失検知に使う。
pub(super) const kAudioDevicePropertyDeviceIsAlive: u32 = fourcc(b"livn");

pub(super) const kAudioObjectPropertyScopeGlobal: u32 = fourcc(b"glob");
pub(super) const kAudioObjectPropertyScopeInput: u32 = fourcc(b"inpt");
pub(super) const kAudioObjectPropertyElementMain: u32 = 0;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct AudioObjectPropertyAddress {
    pub(super) mSelector: u32,
    pub(super) mScope: u32,
    pub(super) mElement: u32,
}

impl AudioObjectPropertyAddress {
    #[must_use]
    pub(super) const fn global(selector: u32) -> Self {
        Self {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        }
    }

    #[must_use]
    pub(super) const fn input(selector: u32) -> Self {
        Self {
            mSelector: selector,
            mScope: kAudioObjectPropertyScopeInput,
            mElement: kAudioObjectPropertyElementMain,
        }
    }
}

/// `AudioBufferList` の先頭。可変長の `AudioBuffer` 配列が後ろに続く。
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct AudioBufferListHeader {
    pub(super) mNumberBuffers: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct AudioBuffer {
    pub(super) mNumberChannels: u32,
    pub(super) mDataByteSize: u32,
    pub(super) mData: *mut c_void,
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    pub(super) fn AudioObjectGetPropertyDataSize(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const c_void,
        outDataSize: *mut u32,
    ) -> OSStatus;

    pub(super) fn AudioObjectGetPropertyData(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inQualifierDataSize: u32,
        inQualifierData: *const c_void,
        ioDataSize: *mut u32,
        outData: *mut c_void,
    ) -> OSStatus;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    pub(super) fn CFStringGetLength(theString: CFStringRef) -> CFIndex;
    pub(super) fn CFStringGetCString(
        theString: CFStringRef,
        buffer: *mut c_char,
        bufferSize: CFIndex,
        encoding: CFStringEncoding,
    ) -> u8;
}
