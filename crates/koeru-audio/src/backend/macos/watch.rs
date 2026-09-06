//! デバイスの消失検知と、OS からの過負荷通知。
//!
//! - 消失検知（`TR-REC-04`）: デバイス一覧の変化を見る。
//!   `kAudioDevicePropertyDeviceIsAlive` は消えた瞬間に引けなくなるので、
//!   一覧の変化を合図にして、選択中の識別子がまだ居るかを確かめる。
//! - 過負荷通知（`TR-REC-07`）: `kAudioDeviceProcessorOverload`。
//!   キャプチャ側のタイムスタンプの飛びと合わせて、取りこぼしの一次情報にする。
//!
//! ## リスナの規律
//!
//! リスナは CoreAudio のスレッドから呼ばれる。キャプチャコールバックほど厳しくないが、
//! ここでも確保もロックも行わない。 アトミックなカウンタを進めるだけにして、
//! 判断はアプリ側のスレッドが行う。

use super::sys;
use crate::device::DeviceId;
use std::os::raw::c_void;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// リスナが進めるカウンタ。
#[derive(Debug, Default)]
struct Counters {
    /// デバイス一覧が変わった回数。着脱の合図（`TR-REC-04`）。
    device_list_changed: AtomicUsize,
    /// OS が過負荷を通知した回数。取りこぼし（`TR-REC-07`）。
    overloads: AtomicUsize,
}

/// デバイスの変化を見張る。
///
/// **落とすとリスナを外す。** 外し忘れると、解放済みの領域へ通知が来る。
#[derive(Debug)]
pub struct DeviceWatch {
    counters: Arc<Counters>,
    /// 過負荷を見ている対象。外すときに要る。
    device_object: u32,
}

/// 見張りを始める。
///
/// `device` は過負荷通知を受ける対象。デバイス一覧の変化は常にシステム全体で見る。
#[tracing::instrument(skip(device))]
pub fn watch(device: &DeviceId) -> DeviceWatch {
    let counters = Arc::new(Counters::default());
    let ptr = Arc::into_raw(Arc::clone(&counters))
        .cast::<c_void>()
        .cast_mut();

    let list_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDevices);
    // SAFETY: システムオブジェクトは常に存在する。ptr は Arc を1つ漏らして渡してあり、
    // DeviceWatch の Drop で外すまで生きている。
    let status = unsafe {
        sys::AudioObjectAddPropertyListener(
            sys::kAudioObjectSystemObject,
            &raw const list_addr,
            on_device_list_changed,
            ptr,
        )
    };
    if status != sys::kAudioHardwareNoError {
        tracing::warn!(status, "デバイス一覧のリスナを登録できなかった");
    }

    let device_object = super::object_id_for_public(device).unwrap_or(0);
    if device_object != 0 {
        let over_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioDeviceProcessorOverload);
        // SAFETY: device_object は直前の列挙で得た生きた ID。
        let status = unsafe {
            sys::AudioObjectAddPropertyListener(
                device_object,
                &raw const over_addr,
                on_overload,
                ptr,
            )
        };
        if status != sys::kAudioHardwareNoError {
            tracing::warn!(status, "過負荷のリスナを登録できなかった");
        }
    }

    DeviceWatch {
        counters,
        device_object,
    }
}

/// デバイス一覧が変わった。ここでは数えるだけ。
///
/// 選択中のデバイスがまだ居るかの判定は、アプリ側が
/// [`super::is_alive`] で確かめる（`TR-REC-04`）。
unsafe extern "C" fn on_device_list_changed(
    _object: sys::AudioObjectID,
    _count: u32,
    _addresses: *const sys::AudioObjectPropertyAddress,
    client: *mut c_void,
) -> sys::OSStatus {
    // SAFETY: client は watch() が渡した Arc<Counters> の生ポインタ。
    // DeviceWatch の Drop がリスナを外すまで生きている。
    let counters = unsafe { &*client.cast::<Counters>() };
    counters.device_list_changed.fetch_add(1, Ordering::Relaxed);
    sys::kAudioHardwareNoError
}

/// OS が過負荷を通知した。取りこぼしの一次情報（`TR-REC-07`）。
unsafe extern "C" fn on_overload(
    _object: sys::AudioObjectID,
    _count: u32,
    _addresses: *const sys::AudioObjectPropertyAddress,
    client: *mut c_void,
) -> sys::OSStatus {
    // SAFETY: 同上。
    let counters = unsafe { &*client.cast::<Counters>() };
    counters.overloads.fetch_add(1, Ordering::Relaxed);
    sys::kAudioHardwareNoError
}

impl DeviceWatch {
    /// デバイス一覧が変わった回数。増えていたら、選択中の識別子を確かめ直す。
    #[must_use]
    pub fn device_list_changed(&self) -> usize {
        self.counters.device_list_changed.load(Ordering::Relaxed)
    }

    /// OS が過負荷を通知した回数。0 でなければ取りこぼしがある（`TR-REC-07`）。
    #[must_use]
    pub fn overloads(&self) -> usize {
        self.counters.overloads.load(Ordering::Relaxed)
    }
}

impl Drop for DeviceWatch {
    fn drop(&mut self) {
        let ptr = Arc::as_ptr(&self.counters).cast::<c_void>().cast_mut();
        let list_addr = sys::AudioObjectPropertyAddress::global(sys::kAudioHardwarePropertyDevices);
        // SAFETY: watch() で登録したものと同じ組み合わせ。
        unsafe {
            sys::AudioObjectRemovePropertyListener(
                sys::kAudioObjectSystemObject,
                &raw const list_addr,
                on_device_list_changed,
                ptr,
            );
        }
        if self.device_object != 0 {
            let over_addr =
                sys::AudioObjectPropertyAddress::global(sys::kAudioDeviceProcessorOverload);
            // SAFETY: 同上。
            unsafe {
                sys::AudioObjectRemovePropertyListener(
                    self.device_object,
                    &raw const over_addr,
                    on_overload,
                    ptr,
                );
            }
        }
        // watch() で漏らした Arc を回収する。リスナを外したあとに行う。
        // SAFETY: into_raw で作ったポインタを1度だけ from_raw へ返す。
        unsafe { drop(Arc::from_raw(ptr.cast::<Counters>())) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 登録と解除が落ちないこと。 通知そのものは抜き差しが要るので手動確認。
    #[test]
    fn 見張りを始めて終われる() {
        let devices = super::super::enumerate_input_devices().expect("列挙");
        let Some(dev) = devices.first() else {
            return; // 入力デバイスが無い環境
        };
        let w = watch(&dev.id);
        assert_eq!(w.overloads(), 0, "始めた直後は過負荷なし");
        // 落として解除まで通す
        drop(w);
    }

    /// 知らない識別子でも落ちない。過負荷の対象が無いだけ。
    #[test]
    fn 知らない識別子でも見張りを作れる() {
        let w = watch(&DeviceId::new("存在しないデバイス"));
        assert_eq!(w.device_list_changed(), 0);
        assert_eq!(w.overloads(), 0);
    }
}
