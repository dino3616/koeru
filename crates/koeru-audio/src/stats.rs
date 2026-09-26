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

/// キャプチャで数えたものの写し。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CaptureStats {
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
    /// `earlier` が別のストリームの写しなら意味を持たない。 ストリームを開き直したら取り直す。
    #[must_use]
    pub const fn since(self, earlier: Self) -> Self {
        Self {
            frames: self.frames.saturating_sub(earlier.frames),
            dropped: self.dropped.saturating_sub(earlier.dropped),
            discontinuities: self.discontinuities.saturating_sub(earlier.discontinuities),
            render_errors: self.render_errors.saturating_sub(earlier.render_errors),
        }
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
}

impl Default for CaptureCounters {
    fn default() -> Self {
        Self {
            last_end: AtomicU64::new(NO_PREVIOUS),
            frames: AtomicU64::new(0),
            discontinuities: AtomicUsize::new(0),
            render_errors: AtomicUsize::new(0),
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
    pub(crate) fn slice(&self, start: f64, frames: u32) {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "デバイスの時計は 0 以上の整数位置。負や非数は 0 に倒して比べる"
        )]
        let start = start.max(0.0) as u64;
        let prev_end = self.last_end.load(Ordering::Relaxed);
        if prev_end != NO_PREVIOUS && start != prev_end {
            self.discontinuities.fetch_add(1, Ordering::Relaxed);
        }
        self.last_end
            .store(start.saturating_add(u64::from(frames)), Ordering::Relaxed);
        self.frames.fetch_add(u64::from(frames), Ordering::Relaxed);
    }

    /// その周の音をリングへ入れられなかった。 実時間の経路から呼ぶ。
    pub(crate) fn render_failed(&self) {
        self.render_errors.fetch_add(1, Ordering::Relaxed);
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
            frames: self.frames.load(Ordering::Relaxed),
            dropped,
            discontinuities: self.discontinuities(),
            render_errors: self.render_errors(),
        }
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
        assert_eq!(
            s,
            CaptureStats {
                frames: 20,
                dropped: 7,
                discontinuities: 1,
                render_errors: 2,
            }
        );
    }

    #[test]
    fn 差分は増えたぶんだけ() {
        let before = CaptureStats {
            frames: 100,
            dropped: 1,
            discontinuities: 2,
            render_errors: 3,
        };
        let after = CaptureStats {
            frames: 250,
            dropped: 1,
            discontinuities: 5,
            render_errors: 3,
        };
        assert_eq!(
            after.since(before),
            CaptureStats {
                frames: 150,
                dropped: 0,
                discontinuities: 3,
                render_errors: 0,
            }
        );
        assert_eq!(
            before.since(after),
            CaptureStats::default(),
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

    /// コールバックの経路は確保しない（`TR-REC-40`）。
    #[test]
    fn 数える経路は確保しない() {
        let c = CaptureCounters::default();
        let ((), n) = crate::alloc_guard::count(|| {
            for i in 0..100_u32 {
                c.slice(f64::from(i * 64), 64);
                c.slice(f64::from(i * 64 + 7), 1); // 飛びも通す
                c.render_failed();
            }
        });
        assert_eq!(n, 0);
    }
}
