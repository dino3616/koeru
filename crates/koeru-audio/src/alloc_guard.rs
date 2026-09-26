//! 試験のあいだだけ差し込む global allocator。 実時間の経路が確保しないことを数えて確かめる
//! （`TR-REC-40`）。
//!
//! 数えるのは、[`count`] に渡した関数を走らせているスレッドの確保と解放だけ。
//! 試験は並行に走るので、ほかの試験の確保を混ぜない。
//!
//! 見られるのは、実際に走った経路だけ。 分岐の先で確保していても、その分岐を通さなければ
//! 0 と出る。 試験の側で、環をまたぐ・満杯で捨てる・枯渇する、を通してから数える。

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

thread_local! {
    // `const` で初期化し、`Drop` を持たないものだけを置く。 そうでないと、
    // 初めて触ったときに確保が走り、allocator の中から allocator を呼ぶ。
    static WATCHING: Cell<bool> = const { Cell::new(false) };
    static SEEN: Cell<usize> = const { Cell::new(0) };
}

fn note() {
    // スレッドの後始末の最中は読めない。 そこで数え損ねても、見ているのは `count` の中だけ。
    let _ = WATCHING.try_with(|w| {
        if w.get() {
            let _ = SEEN.try_with(|s| s.set(s.get() + 1));
        }
    });
}

#[derive(Debug)]
struct Counting;

// SAFETY: 確保そのものは `System` に委ね、渡された値をそのまま渡し返す。
// 足しているのはスレッドローカルの数え上げだけで、確保は走らない（上の `const` の初期化）。
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: 呼び出し側が `GlobalAlloc::alloc` の約束を守っている。
        unsafe { System.alloc(layout) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        note();
        // SAFETY: 同上。
        unsafe { System.alloc_zeroed(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        note();
        // SAFETY: `ptr` はこの allocator（つまり `System`）が同じ `layout` で返したもの。
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        note();
        // SAFETY: 同上。
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// `f` を走らせ、その間にこのスレッドで起きた確保と解放の回数を返す。
pub(crate) fn count<R>(f: impl FnOnce() -> R) -> (R, usize) {
    // `f` が落ちても見張りを下ろす。 下ろさないと、同じスレッドで次に走る試験まで数える。
    struct Stop;
    impl Drop for Stop {
        fn drop(&mut self) {
            WATCHING.set(false);
        }
    }

    SEEN.set(0);
    WATCHING.set(true);
    let stop = Stop;
    let r = f();
    drop(stop);
    (r, SEEN.get())
}

#[cfg(test)]
mod tests {
    #[test]
    fn 確保すれば数える() {
        let (v, n) = super::count(|| vec![1_u8; 16]);
        assert!(n >= 1, "Vec を作ったのに数えていない");
        drop(v);
    }

    #[test]
    fn 外で確保したものは数えない() {
        let v = vec![0_u8; 16];
        let ((), n) = super::count(|| {
            std::hint::black_box(&v);
        });
        assert_eq!(n, 0);
    }
}
