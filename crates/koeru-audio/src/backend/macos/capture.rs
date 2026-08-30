//! macOS の入力キャプチャ（AUHAL）。
//!
//! **`kAudioUnitSubType_HALOutput` で開き、`kAudioUnitSubType_VoiceProcessingIO` は
//! 一切使わない**（TR-REC-11）。後者は OS 側の音声処理ユニットを通してしまう。
//!
//! ## フォーマット
//!
//! **デバイスのネイティブレートのまま 32 bit float で受ける**（TR-REC-02 / TR-REC-05）。
//! OS に暗黙のサンプルレート変換をさせない。44100 Hz のマスターへ落とすのは
//! **1回だけ**で、それはこの層より後ろで行う。
//!
//! ## コールバックの規律（TR-REC-40）
//!
//! コールバック内でできるのは、事前確保済みバッファへの `AudioUnitRender` と、
//! ロックフリーのリングバッファへの書き込みだけ。
//! **メモリ確保・解放、ロック獲得、ファイル I/O、ログ出力を一切行わない。**
//! 取りこぼしはレイテンシより優先して検出する。

use super::sys;
use crate::ring;
use std::os::raw::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// キャプチャの失敗。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CaptureError {
    /// AUHAL のコンポーネントが見つからない。
    #[error("AUHAL のコンポーネントが見つからない")]
    ComponentNotFound,

    /// AudioUnit の呼び出しが失敗した。
    #[error("AudioUnit の呼び出しが失敗した（{op}, status={status}）")]
    Unit { op: &'static str, status: i32 },

    /// 指定した識別子のデバイスが見つからない。
    #[error("デバイスが見つからない")]
    DeviceNotFound,

    /// デバイスが要求したフォーマットを受け付けなかった。
    #[error(
        "フォーマットが一致しない（要求 {wanted_hz}Hz/{wanted_ch}ch、実際 {actual_hz}Hz/{actual_ch}ch）"
    )]
    FormatMismatch {
        wanted_hz: u32,
        wanted_ch: u16,
        actual_hz: u32,
        actual_ch: u16,
    },
}

impl CaptureError {
    /// 送信層へ載せてよい固定文字列。**`Display` を送らない。**
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ComponentNotFound => "recording.macos.component_not_found",
            Self::Unit { .. } => "recording.macos.unit_failed",
            Self::DeviceNotFound => "recording.macos.device_not_found",
            Self::FormatMismatch { .. } => "recording.macos.format_mismatch",
        }
    }
}

type Result<T> = std::result::Result<T, CaptureError>;

fn check(status: sys::OSStatus, op: &'static str) -> Result<()> {
    if status == sys::kAudioHardwareNoError {
        return Ok(());
    }
    Err(CaptureError::Unit { op, status })
}

/// 実際に開けたキャプチャの条件（TR-REC-13 のスナップショットに残す）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CaptureFormat {
    /// **デバイスのネイティブレート。** OS に変換させないので、これがそのまま来る。
    pub sample_rate_hz: u32,
    pub channels: u16,
    /// 1回のコールバックで来る最大フレーム数。
    pub max_frames_per_slice: u32,
}

/// コールバックとアプリの間で共有する状態。
///
/// **すべてアトミック。** コールバックはロックを取れない。
#[derive(Debug)]
struct Shared {
    /// コールバックが `AudioUnitRender` を呼ぶために要る。
    /// **コールバックが来はじめる前に確定し、以後変わらない。**
    unit: sys::AudioUnit,
    producer: ring::Producer,
    /// 直前のコールバックの末尾サンプル位置。**連続性の判定に使う。**
    last_end: AtomicU64,
    /// タイムスタンプが飛んだ回数。**xrun の検出**（TR-REC-07）。
    discontinuities: AtomicUsize,
    /// `AudioUnitRender` が失敗した回数。
    render_errors: AtomicUsize,
    /// レンダ先のバッファ。**事前確保済みで、実行中は伸縮しない。**
    scratch: Box<[std::cell::UnsafeCell<f32>]>,
    /// 1フレームあたりのチャンネル数。
    channels: usize,
    /// 収録中か。止めている間もコールバックは来るので、ここで捨てる。
    armed: AtomicBool,
}

// SAFETY: scratch へはコールバックだけが触れる。他のフィールドはすべてアトミック。
unsafe impl Send for Shared {}
// SAFETY: 同上。
unsafe impl Sync for Shared {}

/// 開いているキャプチャストリーム。
///
/// **落とすと停止して解放する。** テイクごとに開閉しない（REQ-REC-102）ので、
/// 収録画面を離れるまで持ち続ける。
#[derive(Debug)]
pub struct Capture {
    unit: sys::AudioUnit,
    shared: Arc<Shared>,
    format: CaptureFormat,
}

// SAFETY: AudioUnit のハンドルは別スレッドから操作してよい。
unsafe impl Send for Capture {}

/// キャプチャを開く（REQ-REC-102）。
///
/// **開くだけで、まだ収録は始まらない。** 収録の開始は [`Capture::arm`]。
#[tracing::instrument(skip(device), err)]
pub fn open(device: &crate::DeviceId, ring_capacity: usize) -> Result<(Capture, ring::Consumer)> {
    let Some(object) = super::object_id_for_public(device) else {
        return Err(CaptureError::DeviceNotFound);
    };

    let desc = sys::AudioComponentDescription {
        componentType: sys::kAudioUnitType_Output,
        // **HALOutput。VoiceProcessingIO は使わない**（TR-REC-11）。
        componentSubType: sys::kAudioUnitSubType_HALOutput,
        componentManufacturer: sys::kAudioUnitManufacturer_Apple,
        ..Default::default()
    };
    // SAFETY: desc は有効な参照から取ったポインタ。null は「先頭から探す」。
    let component = unsafe { sys::AudioComponentFindNext(std::ptr::null_mut(), &raw const desc) };
    if component.is_null() {
        return Err(CaptureError::ComponentNotFound);
    }

    let mut unit: sys::AudioComponentInstance = std::ptr::null_mut();
    // SAFETY: component は直前に得た有効なコンポーネント。
    check(
        unsafe { sys::AudioComponentInstanceNew(component, &raw mut unit) },
        "AudioComponentInstanceNew",
    )?;

    // ここから失敗したら unit を捨てる必要がある。
    let built = build(unit, object, ring_capacity);
    match built {
        Ok(v) => Ok(v),
        Err(e) => {
            // SAFETY: unit は生成済みで、まだ初期化していない。
            unsafe { sys::AudioComponentInstanceDispose(unit) };
            Err(e)
        }
    }
}

fn build(
    unit: sys::AudioUnit,
    object: u32,
    ring_capacity: usize,
) -> Result<(Capture, ring::Consumer)> {
    let on: u32 = 1;
    let off: u32 = 0;

    // 入力を有効に、出力を無効にする。**AUHAL の既定は逆。**
    // SAFETY: unit は生成済み。データは u32 で、サイズも合わせている。
    check(
        unsafe {
            sys::AudioUnitSetProperty(
                unit,
                sys::kAudioOutputUnitProperty_EnableIO,
                sys::kAudioUnitScope_Input,
                sys::INPUT_ELEMENT,
                (&raw const on).cast::<c_void>(),
                size_of::<u32>() as u32,
            )
        },
        "EnableIO(input)",
    )?;
    // SAFETY: 同上。
    check(
        unsafe {
            sys::AudioUnitSetProperty(
                unit,
                sys::kAudioOutputUnitProperty_EnableIO,
                sys::kAudioUnitScope_Output,
                sys::OUTPUT_ELEMENT,
                (&raw const off).cast::<c_void>(),
                size_of::<u32>() as u32,
            )
        },
        "EnableIO(output)",
    )?;

    // 使うデバイスを固定する。**既定デバイスへ暗黙に倒れないため**（TR-REC-04）。
    // SAFETY: object は列挙で得た生きた AudioObjectID。
    check(
        unsafe {
            sys::AudioUnitSetProperty(
                unit,
                sys::kAudioOutputUnitProperty_CurrentDevice,
                sys::kAudioUnitScope_Global,
                sys::OUTPUT_ELEMENT,
                (&raw const object).cast::<c_void>(),
                size_of::<u32>() as u32,
            )
        },
        "CurrentDevice",
    )?;

    // デバイス側のフォーマットを読む。**こちらから変換を要求しない**（TR-REC-05）。
    let mut hw = sys::AudioStreamBasicDescription::default();
    let mut size = size_of::<sys::AudioStreamBasicDescription>() as u32;
    // SAFETY: hw は有効な領域で、size もその大きさ。
    check(
        unsafe {
            sys::AudioUnitGetProperty(
                unit,
                sys::kAudioUnitProperty_StreamFormat,
                sys::kAudioUnitScope_Input,
                sys::INPUT_ELEMENT,
                (&raw mut hw).cast::<c_void>(),
                &raw mut size,
            )
        },
        "GetStreamFormat(hw)",
    )?;

    // アプリ側で受ける形。**レートはデバイスに合わせ、32 bit float 非インターリーブ**
    // （TR-REC-02）。レート変換は後段で1回だけ行う。
    let channels = hw.mChannelsPerFrame.max(1);
    let app = sys::AudioStreamBasicDescription {
        mSampleRate: hw.mSampleRate,
        mFormatID: sys::kAudioFormatLinearPCM,
        mFormatFlags: sys::kAudioFormatFlagIsFloat
            | sys::kAudioFormatFlagIsPacked
            | sys::kAudioFormatFlagIsNonInterleaved,
        mBytesPerPacket: size_of::<f32>() as u32,
        mFramesPerPacket: 1,
        mBytesPerFrame: size_of::<f32>() as u32,
        mChannelsPerFrame: channels,
        mBitsPerChannel: 32,
        mReserved: 0,
    };
    // SAFETY: app は有効な領域で、サイズも合わせている。
    check(
        unsafe {
            sys::AudioUnitSetProperty(
                unit,
                sys::kAudioUnitProperty_StreamFormat,
                sys::kAudioUnitScope_Output,
                sys::INPUT_ELEMENT,
                (&raw const app).cast::<c_void>(),
                size_of::<sys::AudioStreamBasicDescription>() as u32,
            )
        },
        "SetStreamFormat(app)",
    )?;

    // 1回のコールバックで来る最大フレーム数。**事前確保の大きさを決める。**
    let mut max_frames: u32 = 0;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: max_frames は有効な領域。
    check(
        unsafe {
            sys::AudioUnitGetProperty(
                unit,
                sys::kAudioUnitProperty_MaximumFramesPerSlice,
                sys::kAudioUnitScope_Global,
                sys::OUTPUT_ELEMENT,
                (&raw mut max_frames).cast::<c_void>(),
                &raw mut size,
            )
        },
        "MaximumFramesPerSlice",
    )?;

    let (producer, consumer) = ring::channel(ring_capacity);
    let scratch_len = (max_frames as usize) * (channels as usize);
    let mut scratch = Vec::with_capacity(scratch_len);
    scratch.resize_with(scratch_len, || std::cell::UnsafeCell::new(0.0_f32));
    let shared = Arc::new(Shared {
        unit,
        producer,
        last_end: AtomicU64::new(u64::MAX),
        discontinuities: AtomicUsize::new(0),
        render_errors: AtomicUsize::new(0),
        scratch: scratch.into_boxed_slice(),
        channels: channels as usize,
        armed: AtomicBool::new(false),
    });

    let cb = sys::AURenderCallbackStruct {
        inputProc: Some(input_callback),
        // **Arc の生ポインタをコールバックへ渡す。** 解放は Capture が持つ Arc が担う。
        inputProcRefCon: Arc::as_ptr(&shared).cast::<c_void>().cast_mut(),
    };
    // SAFETY: cb は有効な領域で、サイズも合わせている。
    check(
        unsafe {
            sys::AudioUnitSetProperty(
                unit,
                sys::kAudioOutputUnitProperty_SetInputCallback,
                sys::kAudioUnitScope_Global,
                sys::OUTPUT_ELEMENT,
                (&raw const cb).cast::<c_void>(),
                size_of::<sys::AURenderCallbackStruct>() as u32,
            )
        },
        "SetInputCallback",
    )?;

    // SAFETY: 設定が済んだ unit。
    check(
        unsafe { sys::AudioUnitInitialize(unit) },
        "AudioUnitInitialize",
    )?;
    // SAFETY: 初期化済みの unit。ここからコールバックが来はじめる。
    check(
        unsafe { sys::AudioOutputUnitStart(unit) },
        "AudioOutputUnitStart",
    )?;

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let format = CaptureFormat {
        sample_rate_hz: hw.mSampleRate.max(0.0) as u32,
        channels: u16::try_from(channels).unwrap_or(u16::MAX),
        max_frames_per_slice: max_frames,
    };
    tracing::debug!(
        rate = format.sample_rate_hz,
        ch = format.channels,
        max_frames,
        "キャプチャを開いた"
    );
    Ok((
        Capture {
            unit,
            shared,
            format,
        },
        consumer,
    ))
}

/// キャプチャコールバック。**リアルタイムスレッドで走る。**
///
/// ここでできるのは事前確保済みバッファへの `AudioUnitRender` と、
/// ロックフリーのリングバッファへの書き込みだけ（TR-REC-40）。
/// **確保も解放もロックもログ出力もしない。**
unsafe extern "C" fn input_callback(
    ref_con: *mut c_void,
    flags: *mut sys::AudioUnitRenderActionFlags,
    time_stamp: *const sys::AudioTimeStamp,
    bus: u32,
    frames: u32,
    _io_data: *mut c_void,
) -> sys::OSStatus {
    // SAFETY: ref_con は open() が渡した Arc<Shared> の生ポインタ。
    // Capture が生きている間だけコールバックが来る。
    let shared = unsafe { &*ref_con.cast::<Shared>() };

    let need = (frames as usize) * shared.channels;
    if need > shared.scratch.len() {
        // 事前確保を超えた。**確保し直さない。** 取りこぼしとして数える。
        shared.render_errors.fetch_add(1, Ordering::Relaxed);
        return sys::kAudioHardwareNoError;
    }

    // 非インターリーブなので、チャンネルごとにバッファを指す。
    // AudioBufferList は事前確保済み領域の上に組み立てる。
    let mut list_storage = [0_u8; 256];
    let max_buffers = (list_storage.len() - size_of::<sys::AudioBufferListHeader>())
        / size_of::<sys::AudioBuffer>();
    if shared.channels > max_buffers {
        shared.render_errors.fetch_add(1, Ordering::Relaxed);
        return sys::kAudioHardwareNoError;
    }
    let list = list_storage.as_mut_ptr();
    // SAFETY: list_storage は256バイトあり、ヘッダぶんは確実に入る。
    unsafe {
        list.cast::<sys::AudioBufferListHeader>()
            .write_unaligned(sys::AudioBufferListHeader {
                mNumberBuffers: shared.channels as u32,
            });
    }
    let base =
        size_of::<sys::AudioBufferListHeader>().next_multiple_of(align_of::<sys::AudioBuffer>());
    for ch in 0..shared.channels {
        let at = base + ch * size_of::<sys::AudioBuffer>();
        let data = shared.scratch[ch * frames as usize].get();
        // SAFETY: 上で max_buffers を確かめてあるので範囲内。
        unsafe {
            list.add(at)
                .cast::<sys::AudioBuffer>()
                .write_unaligned(sys::AudioBuffer {
                    mNumberChannels: 1,
                    mDataByteSize: frames * size_of::<f32>() as u32,
                    mData: data.cast::<c_void>(),
                });
        }
    }

    // SAFETY: unit はコールバックの発火元なので生きている。list は上で組み立てた。
    let status = unsafe {
        sys::AudioUnitRender(
            shared.unit,
            flags,
            time_stamp,
            bus,
            frames,
            list.cast::<c_void>(),
        )
    };
    if status != sys::kAudioHardwareNoError {
        shared.render_errors.fetch_add(1, Ordering::Relaxed);
        return sys::kAudioHardwareNoError;
    }

    // **タイムスタンプの連続性を見る**（TR-REC-07 の xrun 検出）。
    // SAFETY: time_stamp は CoreAudio が渡した有効なポインタ。
    let start = unsafe { (*time_stamp).mSampleTime };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let start_u = start.max(0.0) as u64;
    let prev_end = shared.last_end.load(Ordering::Relaxed);
    if prev_end != u64::MAX && start_u != prev_end {
        shared.discontinuities.fetch_add(1, Ordering::Relaxed);
    }
    shared
        .last_end
        .store(start_u + u64::from(frames), Ordering::Relaxed);

    if !shared.armed.load(Ordering::Relaxed) {
        return sys::kAudioHardwareNoError; // 収録していないので捨てる
    }

    // 先頭チャンネルだけをリングへ流す。**モノラル化の規則はこの層より後ろ**（TR-REC-06）。
    // SAFETY: scratch[0..frames] は直前の AudioUnitRender が書いた領域。
    let first = unsafe {
        std::slice::from_raw_parts(shared.scratch[0].get().cast_const(), frames as usize)
    };
    shared.producer.push_or_drop(first);

    sys::kAudioHardwareNoError
}

impl Capture {
    /// 実際に開けた条件。
    #[must_use]
    pub const fn format(&self) -> CaptureFormat {
        self.format
    }

    /// 収録を始める。**ここからリングへ流れる。**
    pub fn arm(&self) {
        self.shared.last_end.store(u64::MAX, Ordering::Relaxed);
        self.shared.armed.store(true, Ordering::Release);
    }

    /// 収録を止める。**ストリームは開いたまま**（REQ-REC-102）。
    pub fn disarm(&self) {
        self.shared.armed.store(false, Ordering::Release);
    }

    /// タイムスタンプが飛んだ回数。**0 でなければ取りこぼしがある**（TR-REC-07）。
    #[must_use]
    pub fn discontinuities(&self) -> usize {
        self.shared.discontinuities.load(Ordering::Relaxed)
    }

    /// `AudioUnitRender` が失敗した回数。
    #[must_use]
    pub fn render_errors(&self) -> usize {
        self.shared.render_errors.load(Ordering::Relaxed)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: unit は open() が生成し、まだ解放していない。
        unsafe {
            sys::AudioOutputUnitStop(self.unit);
            sys::AudioUnitUninitialize(self.unit);
            sys::AudioComponentInstanceDispose(self.unit);
        }
    }
}
