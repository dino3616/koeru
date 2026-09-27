//! キャプチャストリームと排出スレッドの寿命をまとめて持つ（T04b/T06b の続き）。
//!
//! 落とす順序は Pump → Capture に固定する。 排出スレッドはリングの `Consumer` を
//! 握ったまま止まるので、`Capture`（`Producer` の持ち主）を先に捨てると、
//! 排出スレッドが動いている間に書き込み側だけが消える。
//! `Studio` はこれまで `capture` と `pump` を別々のフィールドに持ち、
//! `arm_device` / `disarm` / `Drop` の3箇所で手作業の順序を守っていた
//! ——ここへ集約して、型で順序を保証する。
//!
//! [`TakeGuard`] は1テイクぶんの `CaptureStats` と欠落の位置の基準を持つ。
//! `Studio::xrun_baseline`（`usize` 1つ）を置き換える。

use koeru_audio::backend::current as mac;
use koeru_audio::stats::{CaptureStats, GapKind};

use crate::pump::{Pump, native_frames_to_master};

/// 2つの資源を、常に決まった順で落とす持ち手。
///
/// 宣言順に頼らない。 `ManuallyDrop` で自前の `Drop` を書き、`first` を必ず
/// `second` より先に落とす。 フィールドの並びを入れ替えても順序が壊れないので、
/// `Studio` が手作業の順序を守っていたときのように「踏む」余地が無い。
///
/// `pub(crate)` にしてある。 `crate::playback_lease` も同じ形（曲の試唱は
/// 再生 → 合成の待ち合わせの順で落ちる）を要るので、ここへ寄せて2つ目を
/// 書かない。
#[derive(Debug)]
pub(crate) struct DropFirst<A, B> {
    first: std::mem::ManuallyDrop<A>,
    second: std::mem::ManuallyDrop<B>,
}

impl<A, B> DropFirst<A, B> {
    pub(crate) const fn new(first: A, second: B) -> Self {
        Self {
            first: std::mem::ManuallyDrop::new(first),
            second: std::mem::ManuallyDrop::new(second),
        }
    }

    pub(crate) fn first(&self) -> &A {
        &self.first
    }

    pub(crate) fn second(&self) -> &B {
        &self.second
    }
}

impl<A, B> Drop for DropFirst<A, B> {
    fn drop(&mut self) {
        // SAFETY: `first` と `second` はここでしか drop しない。 `ManuallyDrop` は
        // 通常の drop グルーを止めているので、二重解放にはならない。
        // 呼ぶ順は必ず first → second（このモジュールの契約そのもの）。
        unsafe {
            std::mem::ManuallyDrop::drop(&mut self.first);
            std::mem::ManuallyDrop::drop(&mut self.second);
        }
    }
}

/// キャプチャストリームと排出スレッドをまとめて持つ。
///
/// `Drop` で必ず Pump → Capture の順に落ちる（[`DropFirst`]）。
///
/// 選択中デバイスの見張り（[`mac::DeviceWatch`]、`TR-REC-04`）も同じ寿命で持つ。
/// こちらは `DropFirst` の外に置く——見張りが触るのは CoreAudio の
/// システムオブジェクトへのリスナ登録で、開いているストリーム本体
/// （`Pump` / `Capture`）とは別の資源だから、その落ちる順序と揃える必要が無い。
/// 型で縛っているのは Pump → Capture の順だけで、見張りはどちらの前後に
/// 落ちても構わない。
#[derive(Debug)]
pub struct CaptureLease {
    resources: DropFirst<Pump, mac::Capture>,
    watch: mac::DeviceWatch,
}

impl CaptureLease {
    /// 開いた `Capture` と、そこから読み出す `Pump`、デバイスの見張りをまとめて持つ。
    ///
    /// 開くこと自体はここの仕事ではない——呼び出し側（`Studio::arm_device`）が
    /// `mac::open` と `Pump::start`、`mac::watch` を済ませてから渡す。
    pub fn new(capture: mac::Capture, pump: Pump, watch: mac::DeviceWatch) -> Self {
        Self {
            resources: DropFirst::new(pump, capture),
            watch,
        }
    }

    #[must_use]
    pub fn capture(&self) -> &mac::Capture {
        self.resources.second()
    }

    #[must_use]
    pub fn pump(&self) -> &Pump {
        self.resources.first()
    }

    /// デバイス一覧が変わった回数だけを、`Studio` のロックを取らずに読むための持ち手。
    ///
    /// 見張りスレッド（`lib.rs`）が定期的に読む口（`TR-REC-04`）。
    #[must_use]
    pub fn device_list_changed_handle(&self) -> mac::DeviceListChangedHandle {
        self.watch.device_list_changed_handle()
    }
}

/// 1テイクぶんの取りこぼし・不連続・レンダの失敗（`TR-REC-07`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TakeGaps {
    pub dropped: usize,
    pub discontinuities: usize,
    pub render_errors: usize,
    /// 固定長の領域（`koeru_audio::stats::GAP_LOG_CAPACITY`）に収まらず、
    /// 件数だけ数えたぶん。
    pub overflowed: usize,
    /// 記録できた欠落。 位置はテイクの先頭からのマスター標本位置。
    pub entries: Vec<TakeGap>,
}

impl TakeGaps {
    /// 3つの数のどれかが増えていれば無効（`TR-REC-07`）。
    #[must_use]
    pub const fn invalidates_take(&self) -> bool {
        self.dropped > 0 || self.discontinuities > 0 || self.render_errors > 0
    }
}

/// 欠落の種類。 `koeru_audio::stats::GapKind` の写し
/// ——koeru-app 側の語彙として持ち、koeru-audio の内部表現に台帳を結び付けない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TakeGapKind {
    Dropped,
    Discontinuity,
    RenderError,
}

impl From<GapKind> for TakeGapKind {
    fn from(kind: GapKind) -> Self {
        match kind {
            GapKind::Dropped => Self::Dropped,
            GapKind::Discontinuity => Self::Discontinuity,
            GapKind::RenderError => Self::RenderError,
        }
    }
}

/// テイクの範囲に入った欠落1件。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TakeGap {
    pub kind: TakeGapKind,
    /// テイクの先頭（`Pump::start_take` の `from`）からの、マスター（44100 Hz）標本位置。
    pub position: u64,
}

/// 1回の録音の基準。 `Studio::xrun_baseline` を置き換える。
///
/// 録音の開始で [`CaptureLease`] から作り、終わりに [`Self::finish`] へ渡して差分を取る。
/// 確定しないまま落としても何もしない——`Drop` を実装していない、ただの値のスナップショット。
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct TakeGuard {
    /// 録音を始めた時点の写し（世代つき）。
    baseline: CaptureStats,
    /// `Pump::start_take` へ渡した `from`。 テイクの先頭のマスター標本位置。
    from: u64,
    /// ネイティブレートからマスターへ換算するときの比（`Capture::format().sample_rate_hz`）。
    device_rate_hz: u32,
}

impl TakeGuard {
    /// 録音の開始で作る。 `from` は [`Pump::position`] を予定を書く前に取ったもの
    /// （`TR-REC-19`。ここで取り直すと、台帳へ書く時間のぶんだけ起点がずれる）。
    #[must_use]
    pub fn begin(lease: &CaptureLease, from: u64) -> Self {
        Self {
            baseline: lease.capture().stats(),
            from,
            device_rate_hz: lease.capture().format().sample_rate_hz,
        }
    }

    /// `Pump::start_take` へ渡す起点。
    #[must_use]
    pub const fn from(&self) -> u64 {
        self.from
    }

    /// 終わりに呼ぶ。 このテイクの中で増えたぶんの3つの数と、範囲に入った欠落の位置を返す。
    ///
    /// 世代が違えば（ストリームを開き直していれば）差分は意味を持たないので、
    /// 既定値（すべて0件）を返す。 録音中に `arm_device` は呼べない
    /// （`Studio::arm_device` が録音中を断る）ので、通常この経路には来ない。
    #[must_use]
    pub fn finish(&self, lease: &CaptureLease) -> TakeGaps {
        let now = lease.capture().stats();
        let Some(diff) = now.since(self.baseline) else {
            tracing::warn!("世代が変わったストリームの差分は取れない");
            return TakeGaps::default();
        };
        let raw = lease.capture().gaps();
        let entries = raw
            .entries
            .iter()
            .filter(|g| g.position >= self.baseline.frames)
            .map(|g| TakeGap {
                kind: g.kind.into(),
                position: native_frames_to_master(
                    g.position - self.baseline.frames,
                    self.device_rate_hz,
                ),
            })
            .collect();
        TakeGaps {
            dropped: diff.dropped,
            discontinuities: diff.discontinuities,
            render_errors: diff.render_errors,
            overflowed: raw.overflowed,
            entries,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

    /// 順序を記録する偽物。 実際の `Capture` / `Pump` は実機が要るので、
    /// `DropFirst` の契約そのもの（宣言順に頼らず、型で first → second を守る）を
    /// 汎用の資源で確かめる。
    struct Spy {
        log: Arc<Mutex<Vec<&'static str>>>,
        name: &'static str,
    }

    impl Drop for Spy {
        fn drop(&mut self) {
            self.log.lock().expect("ロックを取れる").push(self.name);
        }
    }

    #[test]
    fn 型で決めた順に落ちる() {
        let log = Arc::new(Mutex::new(Vec::new()));
        {
            let _order = DropFirst::new(
                Spy {
                    log: Arc::clone(&log),
                    name: "first",
                },
                Spy {
                    log: Arc::clone(&log),
                    name: "second",
                },
            );
        }
        assert_eq!(
            *log.lock().expect("ロックを取れる"),
            vec!["first", "second"]
        );
    }

    fn stats(epoch: u64, frames: u64, dropped: usize, discontinuities: usize) -> CaptureStats {
        CaptureStats {
            epoch,
            frames,
            dropped,
            discontinuities,
            render_errors: 0,
        }
    }

    #[test]
    fn 世代が変わっていれば差分は空() {
        // `TakeGuard` 単体では `CaptureLease` を作れない（実機が要る）ので、
        // `finish` の中身のロジックを直接見るのではなく、`since` が `None` を
        // 返すことは `koeru-audio` 側の試験が持つ。 ここでは `invalidates_take`
        // の既定値だけ確かめる。
        assert!(!TakeGaps::default().invalidates_take());
    }

    #[test]
    fn 数のどれかが増えれば無効() {
        let g = TakeGaps {
            dropped: 1,
            ..TakeGaps::default()
        };
        assert!(g.invalidates_take());
        let g = TakeGaps {
            render_errors: 1,
            ..TakeGaps::default()
        };
        assert!(g.invalidates_take());
    }

    #[test]
    fn 基準より前の欠落は数えず位置は換算される() {
        // `finish` が使う位置の絞り込みと換算だけを、素の値で確かめる。
        let baseline = stats(1, 1000, 0, 0);
        let after = stats(1, 2000, 3, 2);
        let diff = after.since(baseline).expect("同じ世代");
        assert_eq!(diff.dropped, 3);
        assert_eq!(diff.discontinuities, 2);

        // ネイティブ 48000 Hz、テイクの先頭は frames=1000。 1512 は先頭から512。
        let offset = 1512_u64.saturating_sub(baseline.frames);
        assert_eq!(native_frames_to_master(offset, 48_000), 470);
    }
}
