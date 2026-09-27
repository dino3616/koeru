//! 実時間の経路が数えたものを、実時間の外で読む口。
//!
//! コールバックが触るのはアトミックの `fetch_add` と `store` だけで、確保もロックもログも
//! しない（`TR-REC-40`）。 読む側は [`CaptureStats`] / [`PlaybackStats`] の写しを取り、
//! 差分や判定は写しの上で行う。
//!
//! 入力の取りこぼし、タイムスタンプの不連続、レンダの失敗、出力の枯渇は、別々に数える。
//! 1つの「xrun」に畳むと、どこで何が起きたかが後から分からない。
//! どれをテイクの無効化に数えるかは呼び出し側が決める（`TR-REC-07`）。ここは数えるだけ。
//!
//! 欄ごとに別々に読むので、写しは一瞬の断面ではない。 読む間にコールバックが進めば、
//! 欄どうしの間にその分のずれが入る。 どの欄も単調に増えるので、差分は負にならない。

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// ストリームを開くたびに払い出す、プロセス全体で単調増加する番号。
///
/// [`CaptureStats::epoch`] に載せる。 開き直したストリームの写しと古い写しを
/// 取り違えて差分を取らないための識別子で、値そのものに意味は無い。
// 書いていない OS には、これを呼ぶ `CaptureCounters::default()` の呼び手（`build`）が無い。
#[cfg_attr(
    any(not(target_os = "macos"), koeru_force_unsupported_backend),
    allow(dead_code)
)]
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(1);

/// 新しい世代番号を払い出す。 [`Capture::open`](crate::backend::macos::open) のように
/// ストリームを開く関数が、開くたびに1回呼ぶ。
#[cfg_attr(
    any(not(target_os = "macos"), koeru_force_unsupported_backend),
    allow(dead_code)
)]
pub(crate) fn next_epoch() -> u64 {
    NEXT_EPOCH.fetch_add(1, Ordering::Relaxed)
}

/// キャプチャで数えたものの写し。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
    /// この写しがどのストリームのものか。 [`Self::since`] が取り違えを防ぐのに使う。
    pub epoch: u64,
    /// デバイスから受け取ったフレーム数。 収録していない間も進む。
    ///
    /// 標本の時計として読む。 デバイスが言うサンプル位置ではない。
    pub frames: u64,
    /// リングが満杯で捨てたサンプル数（[`crate::ring::Consumer::dropped`] と同じ値）。
    pub dropped: usize,
    /// タイムスタンプが飛んだ回数。 デバイス側での取りこぼし（`TR-REC-07`）。
    pub discontinuities: usize,
    /// `AudioUnitRender` が失敗した、または1回で来たフレーム数が事前確保を超えた回数。
    /// その周の音はリングへ入っていない。
    pub render_errors: usize,
}

impl CaptureStats {
    /// `earlier` から増えたぶん。 テイクの中で起きたことだけを見るときに使う。
    ///
    /// `earlier` の世代が違えば `None`。 開き直したストリームの写しと古い写しを
    /// 比べても意味を持たないので、値を返さない（型で取り違えを防ぐ）。
    #[must_use]
    pub const fn since(self, earlier: Self) -> Option<Self> {
        if self.epoch != earlier.epoch {
            return None;
        }
        Some(Self {
            epoch: self.epoch,
            frames: self.frames.saturating_sub(earlier.frames),
            dropped: self.dropped.saturating_sub(earlier.dropped),
            discontinuities: self.discontinuities.saturating_sub(earlier.discontinuities),
            render_errors: self.render_errors.saturating_sub(earlier.render_errors),
        })
    }
}

/// 再生で数えたものの写し。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaybackStats {
    /// リングから写して流したフレーム数。 無音で埋めたぶんは入らない。
    pub played: usize,
    /// 継ぎ足しが間に合わず、無音を出したコールバックの回数（`TR-SYN-03`）。
    /// 末尾まで流し終えたあとの無音は入らない。
    pub underruns: usize,
}

impl PlaybackStats {
    /// `earlier` から増えたぶん。
    #[must_use]
    pub const fn since(self, earlier: Self) -> Self {
        Self {
            played: self.played.saturating_sub(earlier.played),
            underruns: self.underruns.saturating_sub(earlier.underruns),
        }
    }
}

/// タイムスタンプの見張りを外している印。 次の1周は比べる相手を持たない。
const NO_PREVIOUS: u64 = u64::MAX;

/// 欠落の位置を記録する固定長の領域と、その形。
///
/// アトミックの型だけ Loom の型へ差し替えられるように、独立したモジュールにしてある
/// （`ring.rs` の `Slot` / `Shared` と同じ理由）。 `CaptureCounters` が使う他の
/// アトミックは Loom で検査していないので、ここだけを切り離す。
mod gap_log {
    #[cfg(all(loom, test))]
    use loom::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    #[cfg(not(all(loom, test)))]
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

    /// 欠落の種類（`TR-REC-07` の「欠落の発生数と位置をメタデータに記録する」）。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum GapKind {
        /// リングが満杯で捨てた（取りこぼし）。
        Dropped,
        /// タイムスタンプが飛んだ（不連続）。
        Discontinuity,
        /// `AudioUnitRender` が失敗した。
        RenderError,
    }

    impl GapKind {
        /// 固定長領域に書く符号。 0 は「まだ書いていない」に予約してある。
        const fn code(self) -> u8 {
            match self {
                Self::Dropped => 1,
                Self::Discontinuity => 2,
                Self::RenderError => 3,
            }
        }

        /// `code` の逆変換。 [`GapLog`] が書く符号だけを渡す前提で、ほかの値は来ない。
        fn from_code(code: u8) -> Self {
            match code {
                1 => Self::Dropped,
                3 => Self::RenderError,
                // GapLog は自分が書いた符号しか読み返さない。 2 と、それ以外の値は
                // ここでは区別する意味が無いので Discontinuity 側へ倒す。
                _ => Self::Discontinuity,
            }
        }
    }

    /// 記録した欠落1件。
    ///
    /// 位置はこのストリームの先頭からの入力フレーム数（ネイティブレート）。
    /// テイクの範囲に切り出すのも、マスター（44100 Hz）へ換算するのも呼び出し側の仕事
    /// ——ここはストリーム全体を1本の時間軸で見ている。
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Gap {
        pub kind: GapKind,
        pub position: u64,
    }

    /// [`GapLog::snapshot`] が返す写し。
    #[derive(Debug, Clone, Default, PartialEq, Eq)]
    pub struct Gaps {
        /// 記録できた欠落。 古いものから順。
        pub entries: Vec<Gap>,
        /// 領域に収まらず、件数だけ数えたぶん（[`GAP_LOG_CAPACITY`] を超えたら増える）。
        pub overflowed: usize,
    }

    /// [`GapLog`] が保持できる件数。
    ///
    /// 仮の数。 実測の分布が無いので、当面は余裕を見て決めてある。
    /// 収まらなかったぶんは件数だけ [`Gaps::overflowed`] に残る。
    pub const GAP_LOG_CAPACITY: usize = 64;

    /// 種類（下位2bit）と位置（残り62bit）を1つの `u64` に詰める。
    ///
    /// 枠ごとに種類と位置を別々のアトミックへ書くと、読み手が「種類は新しいが
    /// 位置は古い」ような途中の状態を読みうる。 1回の store にまとめれば、
    /// その枠は書く前か書いた後かのどちらかにしかならない。
    ///
    /// 62bit あれば、44100 Hz で725年ぶんのフレーム数が入る。 現実のテイクの長さに
    /// 対して十分すぎるので、詰めても実用上失う情報は無い。
    const KIND_BITS: u32 = 2;

    fn pack(kind: GapKind, position: u64) -> u64 {
        (position << KIND_BITS) | u64::from(kind.code())
    }

    fn unpack(packed: u64) -> Gap {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "下位2bitだけを見るので u8 に収まる"
        )]
        let kind = GapKind::from_code((packed & 0b11) as u8);
        Gap {
            kind,
            position: packed >> KIND_BITS,
        }
    }

    /// 欠落の位置を記録する固定長の領域。
    ///
    /// 書き手はキャプチャコールバック1つだけ、読み手は非実時間（[`GapLog::snapshot`]）。
    /// コールバックの中では確保しない——`GAP_LOG_CAPACITY` 件ぶんを構築時に確保しておき、
    /// 溢れたら件数だけ数える。
    ///
    /// 読み手が見てよいのは `len` が公開した範囲だけ。 書き手は枠へ書いてから `len` を
    /// `Release` で進め、読み手は `Acquire` で `len` を読んでから枠を読む
    /// ——単一生産者・単一消費者の公開と同じ形。 並行性は Loom で確かめる
    /// （下の `loom_tests`）。
    #[derive(Debug)]
    pub(crate) struct GapLog {
        slots: Box<[AtomicU64]>,
        /// 公開済みの件数。 書き手だけが増やす。
        len: AtomicUsize,
        overflowed: AtomicUsize,
    }

    impl GapLog {
        // 書いていない OS には、これを呼ぶ `CaptureCounters::default()` の呼び手が無い。
        #[cfg_attr(
            any(not(target_os = "macos"), koeru_force_unsupported_backend),
            allow(dead_code)
        )]
        pub(crate) fn new() -> Self {
            Self {
                slots: (0..GAP_LOG_CAPACITY).map(|_| AtomicU64::new(0)).collect(),
                len: AtomicUsize::new(0),
                overflowed: AtomicUsize::new(0),
            }
        }

        /// 欠落を1件記録する。 実時間の経路から呼ぶ。確保もロックもしない。
        ///
        /// 書き手は1つだけの前提（キャプチャコールバック）。 複数の書き手から
        /// 同時に呼ぶと `len` の読み書きが競り、枠を取り違える。
        pub(crate) fn record(&self, kind: GapKind, position: u64) {
            let idx = self.len.load(Ordering::Relaxed);
            if idx >= self.slots.len() {
                self.overflowed.fetch_add(1, Ordering::Relaxed);
                return;
            }
            self.slots[idx].store(pack(kind, position), Ordering::Relaxed);
            // 内容を書いてから公開する。読み手はここより後の Acquire で内容を必ず見る。
            self.len.store(idx + 1, Ordering::Release);
        }

        /// 実時間の外から読む。 確保する（`Vec`）ので、実時間の経路からは呼ばない。
        pub(crate) fn snapshot(&self) -> Gaps {
            let len = self.len.load(Ordering::Acquire);
            let entries = self.slots[..len]
                .iter()
                .map(|s| unpack(s.load(Ordering::Relaxed)))
                .collect();
            Gaps {
                entries,
                overflowed: self.overflowed.load(Ordering::Relaxed),
            }
        }
    }

    /// `GapLog` の単一生産者・単一消費者の公開が Loom でも壊れないことを確かめる。
    ///
    /// `ring.rs` の `loom_model` と同じ形。 書き手も読み手も生んだスレッドに回す
    /// ——主スレッドを読み手にすると、join で止まるまで書き手へ切り替わらず、
    /// 競る場面を辿らないまま通ってしまう（`ring.rs` で一度踏んだ）。
    #[cfg(all(test, loom))]
    mod loom_tests {
        use super::{GapKind, GapLog};
        use loom::sync::Arc;
        use loom::thread;

        #[test]
        fn 公開前の内容を読み手は見ない() {
            let mut b = loom::model::Builder::new();
            b.preemption_bound = None;
            b.check(|| {
                let log = Arc::new(GapLog::new());
                let writer = {
                    let log = Arc::clone(&log);
                    thread::spawn(move || {
                        log.record(GapKind::Dropped, 10);
                        log.record(GapKind::Discontinuity, 20);
                    })
                };
                let reader = {
                    let log = Arc::clone(&log);
                    thread::spawn(move || log.snapshot())
                };
                writer.join().expect("書き手が終わる");
                let snap = reader.join().expect("読み手が終わる");

                // 読めた範囲は必ず書いたとおりの中身。 公開前の枠を読まない。
                assert!(snap.entries.len() <= 2);
                if let Some(g0) = snap.entries.first() {
                    assert_eq!(g0.kind, GapKind::Dropped);
                    assert_eq!(g0.position, 10);
                }
                if let Some(g1) = snap.entries.get(1) {
                    assert_eq!(g1.kind, GapKind::Discontinuity);
                    assert_eq!(g1.position, 20);
                }
            });
        }
    }
}

pub(crate) use gap_log::GapLog;
pub use gap_log::{GAP_LOG_CAPACITY, Gap, GapKind, Gaps};

/// キャプチャのコールバックが進める数。
///
/// 取りこぼし（リングが満杯）はリングが数える。 ここに持たないのは、同じ出来事を
/// 2箇所で数えると片方だけ直されるから。
#[derive(Debug)]
// 書いていない OS にはまだ数える側（コールバック）が無い。
#[cfg_attr(
    any(not(target_os = "macos"), koeru_force_unsupported_backend),
    allow(dead_code)
)]
pub(crate) struct CaptureCounters {
    /// 直前の周の末尾のサンプル位置。 連続しているかを見る。
    last_end: AtomicU64,
    frames: AtomicU64,
    discontinuities: AtomicUsize,
    render_errors: AtomicUsize,
    /// このストリームの世代（[`next_epoch`]）。 開くたびに変わる、ただの識別子。
    epoch: u64,
    /// 欠落の位置（`TR-REC-07`）。
    gaps: GapLog,
}

impl Default for CaptureCounters {
    /// 呼ぶたびに新しい世代を払い出す。 ストリームを開くたびに1回作る前提
    /// （`Capture::open` の外からは作らない）なので、これで世代が変わる。
    fn default() -> Self {
        Self {
            last_end: AtomicU64::new(NO_PREVIOUS),
            frames: AtomicU64::new(0),
            discontinuities: AtomicUsize::new(0),
            render_errors: AtomicUsize::new(0),
            epoch: next_epoch(),
            gaps: GapLog::new(),
        }
    }
}

#[cfg_attr(
    any(not(target_os = "macos"), koeru_force_unsupported_backend),
    allow(dead_code)
)]
impl CaptureCounters {
    /// 1周ぶんを受け取った。 `start` はその周の先頭のサンプル位置（デバイスの時計）。
    ///
    /// 実時間の経路から呼ぶ。 確保もロックもしない。
    ///
    /// 戻り値は、このストリームの先頭から数えた入力フレーム数（`self.frames` の、
    /// この周を足す前の値）。 呼び出し側はこれを、同じ周で起きた取りこぼしの位置を
    /// 記録するのに使える（`rt::deliver` が返す書けた数と組み合わせる）。
    pub(crate) fn slice(&self, start: f64, frames: u32) -> u64 {
        let position = self.frames.load(Ordering::Relaxed);
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "デバイスの時計は 0 以上の整数位置。負や非数は 0 に倒して比べる"
        )]
        let start = start.max(0.0) as u64;
        let prev_end = self.last_end.load(Ordering::Relaxed);
        if prev_end != NO_PREVIOUS && start != prev_end {
            self.discontinuities.fetch_add(1, Ordering::Relaxed);
            // 位置は「途切れる直前まで連続していた」ところ。 デバイスの時計ではなく
            // 自分の frames で持つ——テイクの位置へ換算するときの基準を統一するため。
            self.gaps.record(GapKind::Discontinuity, position);
        }
        self.last_end
            .store(start.saturating_add(u64::from(frames)), Ordering::Relaxed);
        self.frames.fetch_add(u64::from(frames), Ordering::Relaxed);
        position
    }

    /// その周の音をリングへ入れられなかった。 実時間の経路から呼ぶ。
    ///
    /// この周ぶんは `slice` を呼んでいない（呼ぶ前に分かる失敗と、呼んだ後に
    /// レンダ自体が失敗する経路の両方がある）ので、位置は `frames` の現在値
    /// ——「ここまでは連続して受け取れた」ところ——で記録する。
    pub(crate) fn render_failed(&self) {
        self.render_errors.fetch_add(1, Ordering::Relaxed);
        self.gaps
            .record(GapKind::RenderError, self.frames.load(Ordering::Relaxed));
    }

    /// リングが満杯で捨てた分を、位置つきで記録する。 実時間の経路から呼ぶ。
    ///
    /// `position` は [`Self::slice`] が返した値に、実際に書けたフレーム数を
    /// 足したもの（`rt::deliver` の呼び出し側が組み立てる）。
    pub(crate) fn record_dropped(&self, position: u64) {
        self.gaps.record(GapKind::Dropped, position);
    }

    /// 次の1周をタイムスタンプの比べ始めにする。 収録を始めるときに呼ぶ。
    ///
    /// 収録していない間の飛びを、次のテイクの取りこぼしに数えない。
    pub(crate) fn restart_clock(&self) {
        self.last_end.store(NO_PREVIOUS, Ordering::Relaxed);
    }

    pub(crate) fn discontinuities(&self) -> usize {
        self.discontinuities.load(Ordering::Relaxed)
    }

    pub(crate) fn render_errors(&self) -> usize {
        self.render_errors.load(Ordering::Relaxed)
    }

    /// 写しを取る。 `dropped` はリングの数を渡す。
    pub(crate) fn snapshot(&self, dropped: usize) -> CaptureStats {
        CaptureStats {
            epoch: self.epoch,
            frames: self.frames.load(Ordering::Relaxed),
            dropped,
            discontinuities: self.discontinuities(),
            render_errors: self.render_errors(),
        }
    }

    /// 欠落の位置の写し（実時間の外から呼ぶ）。
    pub(crate) fn gaps(&self) -> Gaps {
        self.gaps.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 続いていれば飛びと数えない() {
        let c = CaptureCounters::default();
        c.slice(0.0, 512);
        c.slice(512.0, 512);
        c.slice(1024.0, 256);
        let s = c.snapshot(0);
        assert_eq!(s.discontinuities, 0);
        assert_eq!(s.frames, 1280);
    }

    #[test]
    fn 位置が飛べば1回と数える() {
        let c = CaptureCounters::default();
        c.slice(0.0, 512);
        c.slice(1024.0, 512); // 512 フレーム失った
        c.slice(1536.0, 512);
        assert_eq!(c.snapshot(0).discontinuities, 1);
    }

    /// 収録していない間に飛んでも、次のテイクへ持ち越さない（`Capture::arm`）。
    #[test]
    fn 時計を取り直した直後の周は比べない() {
        let c = CaptureCounters::default();
        c.slice(0.0, 512);
        c.restart_clock();
        c.slice(99_999.0, 512);
        c.slice(100_511.0, 512);
        assert_eq!(c.snapshot(0).discontinuities, 0);
    }

    #[test]
    fn 取りこぼしと飛びとレンダの失敗は別々に数える() {
        let c = CaptureCounters::default();
        c.slice(0.0, 10);
        c.slice(20.0, 10);
        c.render_failed();
        c.render_failed();
        let s = c.snapshot(7);
        assert_eq!(s.frames, 20);
        assert_eq!(s.dropped, 7);
        assert_eq!(s.discontinuities, 1);
        assert_eq!(s.render_errors, 2);
    }

    #[test]
    fn 差分は増えたぶんだけ() {
        let before = CaptureStats {
            epoch: 1,
            frames: 100,
            dropped: 1,
            discontinuities: 2,
            render_errors: 3,
        };
        let after = CaptureStats {
            epoch: 1,
            frames: 250,
            dropped: 1,
            discontinuities: 5,
            render_errors: 3,
        };
        assert_eq!(
            after.since(before),
            Some(CaptureStats {
                epoch: 1,
                frames: 150,
                dropped: 0,
                discontinuities: 3,
                render_errors: 0,
            })
        );
        assert_eq!(
            before.since(after),
            Some(CaptureStats {
                epoch: 1,
                ..CaptureStats::default()
            }),
            "逆に引いても負にならない"
        );
        let p = PlaybackStats {
            played: 10,
            underruns: 1,
        };
        assert_eq!(
            p.since(PlaybackStats::default()),
            p,
            "何も無いところからの差分は自分"
        );
    }

    /// 開き直したストリームの写しと古い写しを比べても意味を持たない。
    #[test]
    fn 世代が違えば差分を返さない() {
        let a = CaptureStats {
            epoch: 1,
            frames: 100,
            ..CaptureStats::default()
        };
        let b = CaptureStats {
            epoch: 2,
            frames: 10,
            ..CaptureStats::default()
        };
        assert_eq!(b.since(a), None);
    }

    /// `CaptureCounters::default()` は呼ぶたびに新しい世代を払い出す。
    #[test]
    fn 作るたびに世代が変わる() {
        let a = CaptureCounters::default();
        let b = CaptureCounters::default();
        assert_ne!(a.snapshot(0).epoch, b.snapshot(0).epoch);
    }

    /// 不連続とレンダの失敗の位置が、ストリームの先頭からのフレーム数で残る
    /// （`TR-REC-07` の「欠落の発生数と位置をメタデータに記録する」）。
    #[test]
    fn 欠落の位置が種類つきで残る() {
        let c = CaptureCounters::default();
        c.slice(0.0, 512); // 0..512、連続
        c.slice(1024.0, 512); // 512 フレーム失った。この周の先頭は 512
        c.render_failed(); // レンダに失敗。ここまでの frames は 1024
        c.record_dropped(1024); // リングが満杯で捨てた

        let gaps = c.gaps();
        assert_eq!(gaps.overflowed, 0);
        assert_eq!(
            gaps.entries,
            vec![
                Gap {
                    kind: GapKind::Discontinuity,
                    position: 512,
                },
                Gap {
                    kind: GapKind::RenderError,
                    position: 1024,
                },
                Gap {
                    kind: GapKind::Dropped,
                    position: 1024,
                },
            ]
        );
    }

    /// 領域に収まらないぶんは件数だけ数える。
    #[test]
    fn 溢れたら件数だけ数える() {
        let c = CaptureCounters::default();
        for _ in 0..(GAP_LOG_CAPACITY + 5) {
            c.render_failed();
        }
        let gaps = c.gaps();
        assert_eq!(gaps.entries.len(), GAP_LOG_CAPACITY);
        assert_eq!(gaps.overflowed, 5);
    }

    /// コールバックの経路は確保しない（`TR-REC-40`）。
    #[test]
    fn 数える経路は確保しない() {
        let c = CaptureCounters::default();
        let ((), n) = crate::alloc_guard::count(|| {
            for i in 0..100_u32 {
                c.slice(f64::from(i * 64), 64);
                c.slice(f64::from(i * 64 + 7), 1); // 飛びも通す
                c.render_failed();
                c.record_dropped(u64::from(i));
            }
        });
        assert_eq!(n, 0);
    }
}
