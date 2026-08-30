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

// ── AudioUnit（AUHAL）─────────────────────────────────────
//
// **`kAudioUnitSubType_HALOutput` で開き、`kAudioUnitSubType_VoiceProcessingIO` は
// 一切使わない**（TR-REC-11）。後者は OS 側の音声処理ユニットを通す。

pub(super) type AudioComponent = *mut c_void;
pub(super) type AudioComponentInstance = *mut c_void;
pub(super) type AudioUnit = AudioComponentInstance;
pub(super) type AudioUnitPropertyID = u32;
pub(super) type AudioUnitScope = u32;
pub(super) type AudioUnitElement = u32;
pub(super) type AudioUnitRenderActionFlags = u32;

pub(super) const kAudioUnitType_Output: u32 = fourcc(b"auou");
pub(super) const kAudioUnitSubType_HALOutput: u32 = fourcc(b"ahal");
/// 既定の出力デバイスへ流す。**試唱の再生に使う。**
///
/// 収録の入力は `HALOutput` でデバイスを名指しするが（TR-REC-08 の経路が要る）、
/// **再生は OS の既定でよい。** 出す側に音声加工を無効化する要求は無い。
pub(super) const kAudioUnitSubType_DefaultOutput: u32 = fourcc(b"def ");
pub(super) const kAudioUnitManufacturer_Apple: u32 = fourcc(b"appl");

pub(super) const kAudioOutputUnitProperty_EnableIO: AudioUnitPropertyID = 2003;
pub(super) const kAudioOutputUnitProperty_CurrentDevice: AudioUnitPropertyID = 2000;
pub(super) const kAudioOutputUnitProperty_SetInputCallback: AudioUnitPropertyID = 2005;
pub(super) const kAudioUnitProperty_StreamFormat: AudioUnitPropertyID = 8;
pub(super) const kAudioUnitProperty_MaximumFramesPerSlice: AudioUnitPropertyID = 14;
pub(super) const kAudioUnitProperty_SetRenderCallback: AudioUnitPropertyID = 23;

pub(super) const kAudioUnitScope_Global: AudioUnitScope = 0;
pub(super) const kAudioUnitScope_Input: AudioUnitScope = 1;
pub(super) const kAudioUnitScope_Output: AudioUnitScope = 2;

/// AUHAL の入力エレメント。出力は 0。
pub(super) const INPUT_ELEMENT: AudioUnitElement = 1;
pub(super) const OUTPUT_ELEMENT: AudioUnitElement = 0;

pub(super) const kAudioFormatLinearPCM: u32 = fourcc(b"lpcm");
pub(super) const kAudioFormatFlagIsFloat: u32 = 1 << 0;
pub(super) const kAudioFormatFlagIsPacked: u32 = 1 << 3;
pub(super) const kAudioFormatFlagIsNonInterleaved: u32 = 1 << 5;

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct AudioComponentDescription {
    pub(super) componentType: u32,
    pub(super) componentSubType: u32,
    pub(super) componentManufacturer: u32,
    pub(super) componentFlags: u32,
    pub(super) componentFlagsMask: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct AudioStreamBasicDescription {
    pub(super) mSampleRate: f64,
    pub(super) mFormatID: u32,
    pub(super) mFormatFlags: u32,
    pub(super) mBytesPerPacket: u32,
    pub(super) mFramesPerPacket: u32,
    pub(super) mBytesPerFrame: u32,
    pub(super) mChannelsPerFrame: u32,
    pub(super) mBitsPerChannel: u32,
    pub(super) mReserved: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct SMPTETime {
    pub(super) mSubframes: i16,
    pub(super) mSubframeDivisor: i16,
    pub(super) mCounter: u32,
    pub(super) mType: u32,
    pub(super) mFlags: u32,
    pub(super) mHours: i16,
    pub(super) mMinutes: i16,
    pub(super) mSeconds: i16,
    pub(super) mFrames: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct AudioTimeStamp {
    /// **連続性の判定に使う**（xrun の検出、TR-REC-07）。
    pub(super) mSampleTime: f64,
    pub(super) mHostTime: u64,
    pub(super) mRateScalar: f64,
    pub(super) mWordClockTime: u64,
    pub(super) mSMPTETime: SMPTETime,
    pub(super) mFlags: u32,
    pub(super) mReserved: u32,
}

pub(super) type AURenderCallback = unsafe extern "C" fn(
    inRefCon: *mut c_void,
    ioActionFlags: *mut AudioUnitRenderActionFlags,
    inTimeStamp: *const AudioTimeStamp,
    inBusNumber: u32,
    inNumberFrames: u32,
    ioData: *mut c_void,
) -> OSStatus;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub(super) struct AURenderCallbackStruct {
    pub(super) inputProc: Option<AURenderCallback>,
    pub(super) inputProcRefCon: *mut c_void,
}

#[link(name = "AudioToolbox", kind = "framework")]
unsafe extern "C" {
    pub(super) fn AudioComponentFindNext(
        inComponent: AudioComponent,
        inDesc: *const AudioComponentDescription,
    ) -> AudioComponent;

    pub(super) fn AudioComponentInstanceNew(
        inComponent: AudioComponent,
        outInstance: *mut AudioComponentInstance,
    ) -> OSStatus;

    pub(super) fn AudioComponentInstanceDispose(inInstance: AudioComponentInstance) -> OSStatus;

    pub(super) fn AudioUnitSetProperty(
        inUnit: AudioUnit,
        inID: AudioUnitPropertyID,
        inScope: AudioUnitScope,
        inElement: AudioUnitElement,
        inData: *const c_void,
        inDataSize: u32,
    ) -> OSStatus;

    pub(super) fn AudioUnitGetProperty(
        inUnit: AudioUnit,
        inID: AudioUnitPropertyID,
        inScope: AudioUnitScope,
        inElement: AudioUnitElement,
        outData: *mut c_void,
        ioDataSize: *mut u32,
    ) -> OSStatus;

    pub(super) fn AudioUnitInitialize(inUnit: AudioUnit) -> OSStatus;
    pub(super) fn AudioUnitUninitialize(inUnit: AudioUnit) -> OSStatus;
    pub(super) fn AudioOutputUnitStart(ci: AudioUnit) -> OSStatus;
    pub(super) fn AudioOutputUnitStop(ci: AudioUnit) -> OSStatus;

    pub(super) fn AudioUnitRender(
        inUnit: AudioUnit,
        ioActionFlags: *mut AudioUnitRenderActionFlags,
        inTimeStamp: *const AudioTimeStamp,
        inOutputBusNumber: u32,
        inNumberFrames: u32,
        ioData: *mut c_void,
    ) -> OSStatus;
}

// ── プロパティリスナ ──────────────────────────────────────
//
// TR-REC-04 の消失検知と、TR-REC-07 の取りこぼし検出に使う。

/// **OS が過負荷を通知する。取りこぼしの一次情報**（TR-REC-07）。
pub(super) const kAudioDeviceProcessorOverload: u32 = fourcc(b"over");

pub(super) type AudioObjectPropertyListenerProc = unsafe extern "C" fn(
    inObjectID: AudioObjectID,
    inNumberAddresses: u32,
    inAddresses: *const AudioObjectPropertyAddress,
    inClientData: *mut c_void,
) -> OSStatus;

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    pub(super) fn AudioObjectAddPropertyListener(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inListener: AudioObjectPropertyListenerProc,
        inClientData: *mut c_void,
    ) -> OSStatus;

    pub(super) fn AudioObjectRemovePropertyListener(
        inObjectID: AudioObjectID,
        inAddress: *const AudioObjectPropertyAddress,
        inListener: AudioObjectPropertyListenerProc,
        inClientData: *mut c_void,
    ) -> OSStatus;
}
