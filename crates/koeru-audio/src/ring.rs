//! 単一生産者・単一消費者のロックフリーなリングバッファ。
//!
//! **`TR-REC-40` の規律を満たすために自前で持つ。** キャプチャコールバック内で
//! できるのは「ロックフリーのリングバッファへの書き込み」だけで、
//! メモリ確保・解放、ロック獲得、ファイル I/O、ログ出力は一切できない。
//!
//! 既存の crate を使わないのは、束ねる相手を組織メンテのものに限るという方針
//! （`DEC-REC-001`）に対して、この用途の crate がいずれも個人のリポジトリだから。
//! **100行そこそこで足りるものに、その例外を作らない。**
//!
//! ## 落とすときの扱い
//!
//! **満杯なら書き込みを捨て、捨てた数を数える。** コールバックは待てないので、
//! ブロックする選択肢が無い。捨てたことは `dropped()` で読み取り側が知る。
//! **取りこぼしはレイテンシより優先して検出する**（TR-REC-40）ので、
//! 1サンプルでも捨てたテイクは無効にする（TR-REC-07）。

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// リングバッファの本体。生産者と消費者で共有する。
#[derive(Debug)]
struct Shared {
    /// 事前確保済み。**実行中は伸縮しない。**
    buf: Box<[std::cell::UnsafeCell<f32>]>,
    /// 書いた総数。生産者だけが進める。**単調に増える。**
    ///
    /// **`% cap` した値で持たない。** 剰余で持つと、環をまたいだとき
    /// `head - tail` が「差」ではなくなる。`wrapping_sub` の結果を
    /// もう一度 `% cap` して埋め合わせようとしても、
    /// **`2^64 % cap == 0` のとき、つまり容量が2の冪のときにしか合わない。**
    /// アプリの容量は 384000 で、`2^64 % 384000 = 111616`。**踏んだ。**
    ///
    /// 総数で持てば、差はそのまま溜まっている数になる。
    /// 添字にするときだけ `% cap` する。
    head: AtomicUsize,
    /// 読んだ総数。消費者だけが進める。**単調に増える。**
    tail: AtomicUsize,
    /// 満杯で捨てたサンプル数。生産者だけが進める。
    dropped: AtomicUsize,
}

// SAFETY: buf の各要素へは、head と tail の順序付けによって
// 生産者と消費者のどちらか一方しか同時に触れない。
unsafe impl Send for Shared {}
// SAFETY: 同上。Producer と Consumer が別スレッドへ渡ることを許す。
unsafe impl Sync for Shared {}

/// 書き込み側。**キャプチャコールバックが持つ。**
#[derive(Debug)]
pub struct Producer {
    shared: Arc<Shared>,
}

/// 読み出し側。**ディスクへ書くスレッドが持つ。**
#[derive(Debug)]
pub struct Consumer {
    shared: Arc<Shared>,
}

/// 容量 `capacity` サンプルのリングバッファを作る。
///
/// 実際に保持できるのは `capacity - 1` サンプル。1枠を満杯と空の区別に使う。
#[must_use]
pub fn channel(capacity: usize) -> (Producer, Consumer) {
    let capacity = capacity.max(2);
    let mut v = Vec::with_capacity(capacity);
    v.resize_with(capacity, || std::cell::UnsafeCell::new(0.0_f32));
    let shared = Arc::new(Shared {
        buf: v.into_boxed_slice(),
        head: AtomicUsize::new(0),
        tail: AtomicUsize::new(0),
        dropped: AtomicUsize::new(0),
    });
    (
        Producer {
            shared: Arc::clone(&shared),
        },
        Consumer { shared },
    )
}

impl Producer {
    /// 書けるだけ書く。**入りきらなかったぶんは捨てたと数えない。**
    ///
    /// 呼び出し側が残りを再試行できる場面で使う。リアルタイムコールバックからは
    /// [`Producer::push_or_drop`] を使うこと。**あちらは再試行できない。**
    ///
    /// 確保も解放もロックも行わない。戻り値は実際に書けたサンプル数。
    pub fn push(&self, samples: &[f32]) -> usize {
        let cap = self.shared.buf.len();
        let head = self.shared.head.load(Ordering::Relaxed);
        let tail = self.shared.tail.load(Ordering::Acquire);
        // **総数の差がそのまま溜まっている数。** 剰余は取らない。
        let used = head.wrapping_sub(tail);
        let free = (cap - 1).saturating_sub(used);
        let n = samples.len().min(free);

        for (i, s) in samples[..n].iter().enumerate() {
            let at = (head + i) % cap;
            // SAFETY: at は空き領域の中。消費者は tail より前にしか触れず、
            // head を Release で公開するまでこの領域を読まない。
            unsafe { *self.shared.buf[at].get() = *s };
        }
        self.shared
            .head
            .store(head.wrapping_add(n), Ordering::Release);
        n
    }

    /// **リアルタイムコールバックのための書き込み。入りきらなかったぶんは失われる。**
    ///
    /// コールバックは待てないので、ブロックする選択肢が無い。捨てた数は
    /// [`Consumer::dropped`] から読める。**1サンプルでも捨てたテイクは無効にする**
    /// （TR-REC-07）。戻り値は実際に書けたサンプル数。
    pub fn push_or_drop(&self, samples: &[f32]) -> usize {
        let n = self.push(samples);
        if n < samples.len() {
            self.shared
                .dropped
                .fetch_add(samples.len() - n, Ordering::Relaxed);
        }
        n
    }

    /// 満杯で捨てたサンプルの累計。
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

impl Consumer {
    /// 読み出す。戻り値は実際に読めたサンプル数。
    pub fn pop(&self, out: &mut [f32]) -> usize {
        let cap = self.shared.buf.len();
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        let used = head.wrapping_sub(tail);
        let n = out.len().min(used);

        for (i, slot) in out[..n].iter_mut().enumerate() {
            let at = (tail + i) % cap;
            // SAFETY: at は生産者が Release で公開済みの領域。
            *slot = unsafe { *self.shared.buf[at].get() };
        }
        self.shared
            .tail
            .store(tail.wrapping_add(n), Ordering::Release);
        n
    }

    /// いま読める数。
    #[must_use]
    pub fn len(&self) -> usize {
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        head.wrapping_sub(tail)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// **満杯で捨てたサンプルの累計。0 でなければ取りこぼしがある**（TR-REC-07）。
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 書いた順に読める() {
        let (p, c) = channel(16);
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0_f32; 3];
        assert_eq!(c.pop(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert!(c.is_empty());
    }

    #[test]
    fn 環をまたいでも順序が保たれる() {
        let (p, c) = channel(4); // 実効容量 3
        let mut out = [0.0_f32; 2];
        for round in 0..10 {
            let a = round as f32;
            let b = a + 0.5;
            assert_eq!(p.push(&[a, b]), 2, "空きがあるので2つ書ける");
            assert_eq!(c.pop(&mut out), 2);
            assert_eq!(out, [a, b], "{round} 周目");
        }
        assert_eq!(c.dropped(), 0);
    }

    /// **満杯なら捨てる。待たない。** コールバックはブロックできない。
    #[test]
    fn コールバックの書き込みは満杯なら捨てて数える() {
        let (p, c) = channel(4); // 実効容量 3
        assert_eq!(
            p.push_or_drop(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            3,
            "3つだけ書ける"
        );
        assert_eq!(p.dropped(), 2, "2つ捨てた");
        assert_eq!(c.dropped(), 2, "読み取り側から見える");
    }

    /// **再試行する側の書き込みは、入りきらなくても捨てたと数えない。**
    #[test]
    fn 再試行する書き込みは捨てたと数えない() {
        let (p, c) = channel(4); // 実効容量 3
        assert_eq!(p.push(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3, "3つだけ書ける");
        assert_eq!(c.dropped(), 0, "残りは呼び出し側が持っている");
    }

    /// **容量が2の冪でなくても、環をまたいで順序が保たれる。**
    ///
    /// # なぜここが抜けていたか
    ///
    /// 既存の試験は容量を 4 / 8 / 16 / 1024 でしか作っていなかった。
    /// **どれも2の冪。** `head` と `tail` を `% cap` した値で持ちながら
    /// 差を `wrapping_sub(...) % cap` で取ると、
    /// **`2^64 % cap == 0` の場合にだけ**答えが合う——つまり2の冪でだけ合う。
    ///
    /// アプリが使う容量は 48000 × 8 = 384000 で、`2^64 % 384000 = 111616`。
    /// **環をまたいだ瞬間から、消費側が 111616 サンプル余分に読めると誤認し、
    /// 読み終えたはずの古い音をもう一度読む**（2.3 秒ぶん）。
    /// 波形に「前に流れたものがまた流れる」と出た。**踏んだ。**
    #[test]
    fn 容量が2の冪でなくても環をまたげる() {
        let cap = 300; // **2の冪ではない。**
        let (p, c) = channel(cap);
        let mut out = vec![0.0_f32; 64];
        let mut next_written = 0_u32;
        let mut next_read = 0_u32;

        // 環を何周もさせる。**書いた順にしか出てこないこと。**
        for _ in 0..200 {
            let block: Vec<f32> = (0..50).map(|i| (next_written + i) as f32).collect();
            let wrote = p.push(&block);
            assert_eq!(
                wrote,
                50,
                "実効容量 {} に対して書けないのはおかしい",
                cap - 1
            );
            next_written += 50;

            let got = c.pop(&mut out);
            for v in &out[..got] {
                assert!(
                    (*v - next_read as f32).abs() < f32::EPSILON,
                    "順序が壊れた: {v} が来たが {next_read} のはず"
                );
                next_read += 1;
            }
        }
        assert!(next_read > 0);
    }

    /// **読めると答えた数だけ、実際に読める。**
    ///
    /// 環をまたいだあとに過大な数を返すと、消費側は古い領域を読み直す。
    #[test]
    fn 読める数は実際に読める数を超えない() {
        let cap = 300;
        let (p, c) = channel(cap);
        let mut sink = vec![0.0_f32; 8];

        for round in 0..200 {
            p.push(&[round as f32; 7]);
            let claimed = c.len();
            assert!(
                claimed < cap,
                "{round} 周目: 実効容量 {} を超える数を返した: {claimed}",
                cap - 1
            );
            let got = c.pop(&mut sink);
            assert!(got <= claimed, "答えた数より多く読めた: {got} > {claimed}");
        }
    }

    /// **アプリが実際に使う容量で、環をまたいでも壊れない。**
    ///
    /// 48000 Hz × 8 秒 = 384000。**8 秒ごとに環をまたぐ。**
    /// 収録では、またいだ先で消費側が古い領域を読み直し、
    /// **実時間より長いテイク**（12 秒）や**7 秒の先頭余白**として現れていた。
    #[test]
    fn 実際の容量で何周しても順序が保たれる() {
        let cap = 48_000 * 8;
        let (p, c) = channel(cap);
        let mut out = vec![0.0_f32; 4096];
        let mut written = 0_u64;
        let mut read = 0_u64;

        // 3周ぶん。**またぐ瞬間を必ず含む。**
        while written < (cap as u64) * 3 {
            let block: Vec<f32> = (0..2048)
                .map(|i| ((written + i) % 1_000_000) as f32)
                .collect();
            let wrote = p.push(&block);
            written += wrote as u64;

            let got = c.pop(&mut out);
            for v in &out[..got] {
                let want = (read % 1_000_000) as f32;
                assert!(
                    (*v - want).abs() < f32::EPSILON,
                    "{read} サンプル目で順序が壊れた: {v} が来たが {want} のはず"
                );
                read += 1;
            }
        }
        assert_eq!(read, written, "書いた数と読んだ数が合わない");
    }

    #[test]
    fn 空なら何も読めない() {
        let (_p, c) = channel(8);
        let mut out = [9.9_f32; 4];
        assert_eq!(c.pop(&mut out), 0);
        assert_eq!(out, [9.9; 4], "触らない");
    }

    /// **別スレッドから書いても順序と総数が保たれる。**
    #[test]
    fn 別スレッドとの受け渡しで取りこぼさない() {
        const N: usize = 100_000;
        // **2の冪でない容量にする。** 冪だと剰余の誤りが隠れる。
        let (p, c) = channel(1000);
        let writer = std::thread::spawn(move || {
            let mut sent = 0_usize;
            while sent < N {
                let chunk: Vec<f32> = (sent..(sent + 64).min(N)).map(|i| i as f32).collect();
                let mut at = 0;
                while at < chunk.len() {
                    at += p.push(&chunk[at..]);
                    if at < chunk.len() {
                        std::thread::yield_now();
                    }
                }
                sent += chunk.len();
            }
            p.dropped()
        });

        let mut got = 0_usize;
        let mut buf = [0.0_f32; 128];
        while got < N {
            let n = c.pop(&mut buf);
            for (i, v) in buf[..n].iter().enumerate() {
                assert_eq!(*v, (got + i) as f32, "順序が保たれる");
            }
            got += n;
            if n == 0 {
                std::thread::yield_now();
            }
        }
        let dropped = writer.join().expect("書き手が終わる");
        assert_eq!(dropped, 0, "空くまで待って書いたので捨てていない");
        assert_eq!(got, N);
    }
}
