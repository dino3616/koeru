//! 単一生産者・単一消費者のロックフリーなリングバッファ。
//!
//! `TR-REC-40` の規律を満たすために自前で持つ。 キャプチャコールバック内で
//! できるのは「ロックフリーのリングバッファへの書き込み」だけで、
//! メモリ確保・解放、ロック獲得、ファイル I/O、ログ出力は一切できない。
//!
//! 既存の crate を使わないのは、束ねる相手を組織メンテのものに限るという方針
//! （`DEC-REC-001`）に対して、この用途の crate がいずれも個人のリポジトリだから。
//! 100行そこそこで足りるものに、その例外を作らない。
//!
//! ## 落とすときの扱い
//!
//! 満杯なら書き込みを捨て、捨てた数を数える。 コールバックは待てないので、
//! ブロックする選択肢が無い。捨てたことは `dropped()` で読み取り側が知る。
//! 取りこぼしはレイテンシより優先して検出する（`TR-REC-40`）ので、
//! 1サンプルでも捨てたテイクは無効にする（`TR-REC-07`）。
//!
//! ## 端点は1つずつ
//!
//! 書き込みにも読み出しにも `&mut self` が要る。 端点は [`channel`] だけが作り、
//! 複製できない。 なので、同じ端点から2つのスレッドが同時に書く（読む）形は
//! safe なコードでは組み立たない。 `buf` へ触る `unsafe` は、この一意性に乗っている。
//!
//! 以前は `&self` で書けた。 端点は `Arc<Shared>` を持つので自動で `Sync` になり、
//! 1つの `Producer` を共有して2本のスレッドから `push` できた。 そうすると
//! `head` の読み書きが競り、同じ枠へ2つが書く。
//!
//! 共有参照から呼べるのはアトミックを読むもの（`dropped`、`len`）だけにしてある。
//! これらは端点が `Sync` のままでも壊れない。
//!
//! ## 並行性の検査
//!
//! Loom のモデル（`loom_model`）が push と pop の交差を網羅する。 Loom の型へ差し替えるのは
//! `cfg(all(loom, test))` のときだけで、製品の組み立てには Loom が入らない。
//!
//! ```bash
//! RUSTFLAGS='--cfg loom' CARGO_TARGET_DIR=target/loom \
//!   cargo test -p koeru-audio --lib --release -- ring::loom_model
//! ```
//!
//! 絞り込みを外さない。 Loom の型はモデルの外で触ると落ちるので、ほかの試験は
//! この組み立てでは走らせられない。 `CARGO_TARGET_DIR` を分けるのは、`RUSTFLAGS` が
//! 変わるたびに依存ごと組み直しになるから。
//!
//! unsafe の読み書きは Miri でも走らせる。 devShell の toolchain は stable で Miri を
//! 持たないので、nightly を別に用意する（`rust-overlay` の `selectLatestNightlyWith` で
//! `miri` と `rust-src` を足したもの）。
//!
//! ```bash
//! CARGO_TARGET_DIR=target/miri cargo miri test -p koeru-audio --lib -- ring:: rt:: stats::
//! ```

#[cfg(all(loom, test))]
use loom::sync::Arc;
#[cfg(all(loom, test))]
use loom::sync::atomic::{AtomicUsize, Ordering};
#[cfg(not(all(loom, test)))]
use std::sync::Arc;
#[cfg(not(all(loom, test)))]
use std::sync::atomic::{AtomicUsize, Ordering};

/// バッファの1枠。
///
/// Loom で検査するときだけ中身を `loom::cell::UnsafeCell` に替える。 あちらは
/// 生のポインタを返さず、触った範囲をモデルが記録する形の API なので、読み書きをここへ寄せる。
struct Slot {
    #[cfg(not(all(loom, test)))]
    cell: std::cell::UnsafeCell<f32>,
    #[cfg(all(loom, test))]
    cell: loom::cell::UnsafeCell<f32>,
}

impl Slot {
    fn new() -> Self {
        Self {
            #[cfg(not(all(loom, test)))]
            cell: std::cell::UnsafeCell::new(0.0),
            #[cfg(all(loom, test))]
            cell: loom::cell::UnsafeCell::new(0.0),
        }
    }

    /// # Safety
    ///
    /// ほかに同じ枠を読み書きしている者がいないこと。
    unsafe fn write(&self, v: f32) {
        #[cfg(not(all(loom, test)))]
        // SAFETY: 呼び出し側が排他を約束する。
        unsafe {
            *self.cell.get() = v;
        }
        #[cfg(all(loom, test))]
        // SAFETY: 同上。 約束が破れていれば Loom が交差として報告する。
        self.cell.with_mut(|p| unsafe { *p = v });
    }

    /// # Safety
    ///
    /// ほかに同じ枠へ書いている者がいないこと。
    unsafe fn read(&self) -> f32 {
        #[cfg(not(all(loom, test)))]
        // SAFETY: 呼び出し側が排他を約束する。
        unsafe {
            *self.cell.get()
        }
        #[cfg(all(loom, test))]
        // SAFETY: 同上。
        self.cell.with(|p| unsafe { *p })
    }
}

// 中身を読むと排他の約束を破るので、枠があることだけを出す。
impl std::fmt::Debug for Slot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Slot")
    }
}

/// リングバッファの本体。生産者と消費者で共有する。
#[derive(Debug)]
struct Shared {
    /// 事前確保済み。実行中は伸縮しない。
    buf: Box<[Slot]>,
    /// 書いた総数。生産者だけが進める。
    ///
    /// 単調に増え、剰余を取らない。 添字にするときだけ `% cap` する。
    /// そうすれば `head - tail` がそのまま溜まっている数になる。
    /// 剰余で持つと容量が2の冪のときにしか合わない——理由と実測は `DEC-REC-007`。
    head: AtomicUsize,
    /// 読んだ総数。消費者だけが進める。単調に増える。
    tail: AtomicUsize,
    /// 満杯で捨てたサンプル数。生産者だけが進める。
    dropped: AtomicUsize,
}

// SAFETY: buf の各要素へは、head と tail の順序付けによって
// 生産者と消費者のどちらか一方しか同時に触れない。 生産者と消費者は
// それぞれ1つしか無い（`channel` だけが作り、複製できない）。
unsafe impl Send for Shared {}
// SAFETY: 同上。 Producer と Consumer が別スレッドへ渡ることを許す。
// buf へ触るのは `&mut self` の `push` と `pop` だけなので、1つの端点を
// 共有しても buf へ同時に書く（読む）者は増えない。 `Meter` はアトミックしか読まない。
unsafe impl Sync for Shared {}

/// 書き込み側。収録ではキャプチャコールバック、再生では継ぎ足す側が持つ。
///
/// ```
/// let (mut p, mut c) = koeru_audio::ring::channel(8);
/// assert_eq!(p.push(&[1.0, 2.0]), 2);
/// let mut out = [0.0_f32; 2];
/// assert_eq!(c.pop(&mut out), 2);
/// ```
///
/// 共有した参照からは書けない。
///
/// ```compile_fail,E0596
/// let (p, _c) = koeru_audio::ring::channel(8);
/// let shared = &p;
/// shared.push(&[1.0]);
/// ```
///
/// ```compile_fail,E0596
/// let (p, _c) = koeru_audio::ring::channel(8);
/// let shared = &p;
/// shared.push_or_drop(&[1.0]);
/// ```
///
/// 複製できない。
///
/// ```compile_fail,E0599
/// let (p, _c) = koeru_audio::ring::channel(8);
/// let _twin = p.clone();
/// ```
#[derive(Debug)]
pub struct Producer {
    shared: Arc<Shared>,
}

/// 読み出し側。収録ではディスクへ書くスレッド、再生ではレンダーコールバックが持つ。
///
/// 共有した参照からは読めない。
///
/// ```compile_fail,E0596
/// let (_p, c) = koeru_audio::ring::channel(8);
/// let shared = &c;
/// let mut out = [0.0_f32; 1];
/// shared.pop(&mut out);
/// ```
///
/// 複製できない。
///
/// ```compile_fail,E0599
/// let (_p, c) = koeru_audio::ring::channel(8);
/// let _twin = c.clone();
/// ```
#[derive(Debug)]
pub struct Consumer {
    shared: Arc<Shared>,
}

/// 端点を持たずに数だけ読む口。
///
/// 書き込み側をコールバックへ渡してしまうと、ほかのスレッドからは `Producer::dropped` を
/// 呼べない（端点への参照を作ると、コールバックの `&mut` と重なる）。 これはアトミックしか
/// 読まないので、複製してどのスレッドから読んでもよい。
///
/// ```
/// let (mut p, _c) = koeru_audio::ring::channel(3);
/// let meter = p.meter();
/// p.push_or_drop(&[1.0, 2.0, 3.0]);
/// assert_eq!(meter.dropped(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct Meter {
    shared: Arc<Shared>,
}

impl Meter {
    /// 満杯で捨てたサンプルの累計（[`Consumer::dropped`] と同じ値）。
    #[must_use]
    pub fn dropped(&self) -> usize {
        self.shared.dropped.load(Ordering::Relaxed)
    }
}

/// 容量 `capacity` サンプルのリングバッファを作る。
///
/// 実際に保持できるのは `capacity - 1` サンプル。1枠を満杯と空の区別に使う。
#[must_use]
pub fn channel(capacity: usize) -> (Producer, Consumer) {
    let capacity = capacity.max(2);
    let mut v = Vec::with_capacity(capacity);
    v.resize_with(capacity, Slot::new);
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
    /// 書けるだけ書く。入りきらなかったぶんは捨てたと数えない。
    ///
    /// 呼び出し側が残りを再試行できる場面で使う。リアルタイムコールバックからは
    /// [`Producer::push_or_drop`] を使うこと。あちらは再試行できない。
    ///
    /// 確保も解放もロックも行わない。戻り値は実際に書けたサンプル数。
    pub fn push(&mut self, samples: &[f32]) -> usize {
        let cap = self.shared.buf.len();
        let head = self.shared.head.load(Ordering::Relaxed);
        let tail = self.shared.tail.load(Ordering::Acquire);
        // 総数の差がそのまま溜まっている数。 剰余は取らない。
        let used = head.wrapping_sub(tail);
        let free = (cap - 1).saturating_sub(used);
        let n = samples.len().min(free);

        for (i, s) in samples[..n].iter().enumerate() {
            let at = (head + i) % cap;
            // SAFETY: at は空き領域の中。消費者は tail より前にしか触れず、
            // head を Release で公開するまでこの領域を読まない。
            // 生産者は1つで、`&mut self` なので、ほかに同じ枠へ書く者はいない。
            unsafe { self.shared.buf[at].write(*s) };
        }
        self.shared
            .head
            .store(head.wrapping_add(n), Ordering::Release);
        n
    }

    /// リアルタイムコールバックのための書き込み。入りきらなかったぶんは失われる。
    ///
    /// コールバックは待てないので、ブロックする選択肢が無い。捨てた数は
    /// [`Consumer::dropped`] から読める。1サンプルでも捨てたテイクは無効にする
    /// （`TR-REC-07`）。戻り値は実際に書けたサンプル数。
    pub fn push_or_drop(&mut self, samples: &[f32]) -> usize {
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

    /// 端点を渡したあとも捨てた数を読むための口。 渡す前に取っておく。
    #[must_use]
    pub fn meter(&self) -> Meter {
        Meter {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl Consumer {
    /// 読み出す。戻り値は実際に読めたサンプル数。
    pub fn pop(&mut self, out: &mut [f32]) -> usize {
        let cap = self.shared.buf.len();
        let tail = self.shared.tail.load(Ordering::Relaxed);
        let head = self.shared.head.load(Ordering::Acquire);
        let used = head.wrapping_sub(tail);
        let n = out.len().min(used);

        for (i, slot) in out[..n].iter_mut().enumerate() {
            let at = (tail + i) % cap;
            // SAFETY: at は生産者が Release で公開済みの領域。 生産者は tail を
            // Release で返されるまでここへ書かない。 消費者は1つで、`&mut self` なので、
            // ほかに同じ枠を読んで tail を進める者はいない。
            *slot = unsafe { self.shared.buf[at].read() };
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

    /// 満杯で捨てたサンプルの累計。0 でなければ取りこぼしがある（`TR-REC-07`）。
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
        let (mut p, mut c) = channel(16);
        assert_eq!(p.push(&[1.0, 2.0, 3.0]), 3);
        let mut out = [0.0_f32; 3];
        assert_eq!(c.pop(&mut out), 3);
        assert_eq!(out, [1.0, 2.0, 3.0]);
        assert!(c.is_empty());
    }

    #[test]
    fn 環をまたいでも順序が保たれる() {
        let (mut p, mut c) = channel(4); // 実効容量 3
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

    /// 満杯なら捨てる。待たない。 コールバックはブロックできない。
    #[test]
    fn コールバックの書き込みは満杯なら捨てて数える() {
        let (mut p, c) = channel(4); // 実効容量 3
        assert_eq!(
            p.push_or_drop(&[1.0, 2.0, 3.0, 4.0, 5.0]),
            3,
            "3つだけ書ける"
        );
        assert_eq!(p.dropped(), 2, "2つ捨てた");
        assert_eq!(c.dropped(), 2, "読み取り側から見える");
    }

    /// 再試行する側の書き込みは、入りきらなくても捨てたと数えない。
    #[test]
    fn 再試行する書き込みは捨てたと数えない() {
        let (mut p, c) = channel(4); // 実効容量 3
        assert_eq!(p.push(&[1.0, 2.0, 3.0, 4.0, 5.0]), 3, "3つだけ書ける");
        assert_eq!(c.dropped(), 0, "残りは呼び出し側が持っている");
    }

    /// 容量が2の冪でなくても、環をまたいで順序が保たれる。
    ///
    /// # なぜここが抜けていたか
    ///
    /// 既存の試験は容量を 4 / 8 / 16 / 1024 でしか作っていなかった——どれも2の冪。
    /// 剰余で持つ実装は2の冪でだけ答えが合うので、この形の試験は素通りする（`DEC-REC-007`）。
    #[test]
    fn 容量が2の冪でなくても環をまたげる() {
        let cap = 300; // **2の冪ではない。**
        let (mut p, mut c) = channel(cap);
        let mut out = vec![0.0_f32; 64];
        let mut next_written = 0_u32;
        let mut next_read = 0_u32;

        // 環を何周もさせる。書いた順にしか出てこないこと。
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

    /// 読めると答えた数だけ、実際に読める。
    ///
    /// 環をまたいだあとに過大な数を返すと、消費側は古い領域を読み直す。
    #[test]
    fn 読める数は実際に読める数を超えない() {
        let cap = 300;
        let (mut p, mut c) = channel(cap);
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

    /// アプリが実際に使う容量で、環をまたいでも壊れない。
    ///
    /// 容量は `koeru-app` 側が決める（`studio.rs` の `RING_SECONDS`）。
    /// ここで固定するのは「2の冪でない容量でも順序が保たれる」ことだけ（`DEC-REC-007`）。
    #[test]
    #[cfg_attr(
        miri,
        ignore = "115 万サンプルを回すので Miri では終わらない。 2の冪でない容量は 300 の試験が見る"
    )]
    fn 実際の容量で何周しても順序が保たれる() {
        let cap = 48_000 * 8;
        let (mut p, mut c) = channel(cap);
        let mut out = vec![0.0_f32; 4096];
        let mut written = 0_u64;
        let mut read = 0_u64;

        // 3周ぶん。またぐ瞬間を必ず含む。
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
        let (_p, mut c) = channel(8);
        let mut out = [9.9_f32; 4];
        assert_eq!(c.pop(&mut out), 0);
        assert_eq!(out, [9.9; 4], "触らない");
    }

    /// 別スレッドから書いても順序と総数が保たれる。
    ///
    /// Miri では数を減らす。 減らしても環は何周もまたぎ、枠への読み書きの交差は Miri が見る。
    #[test]
    fn 別スレッドとの受け渡しで取りこぼさない() {
        const N: usize = if cfg!(miri) { 3_000 } else { 100_000 };
        // 2の冪でない容量にする。 冪だと剰余の誤りが隠れる。
        let (mut p, mut c) = channel(1000);
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

    /// 書き込み側をよそへ渡したあとも、捨てた数を読める。
    #[test]
    fn 端点を渡したあとも捨てた数を読める() {
        let (mut p, c) = channel(4); // 実効容量 3
        let meter = p.meter();
        let writer = std::thread::spawn(move || {
            p.push_or_drop(&[1.0, 2.0, 3.0, 4.0, 5.0]);
        });
        writer.join().expect("書き手が終わる");
        assert_eq!(meter.dropped(), 2);
        assert_eq!(meter.dropped(), c.dropped(), "同じ数を指す");
    }

    /// 書く・読む・捨てるは、どれも確保も解放もしない（`TR-REC-40`）。
    ///
    /// 環をまたぐ瞬間と、満杯で捨てる経路を含める。
    #[test]
    fn 読み書きは確保しない() {
        let (mut p, mut c) = channel(7); // 2の冪ではない
        let meter = p.meter();
        let block = [0.5_f32; 5];
        let mut out = [0.0_f32; 4];
        let ((), allocations) = crate::alloc_guard::count(|| {
            for _ in 0..10 {
                p.push(&block);
                p.push_or_drop(&block);
                c.pop(&mut out);
                let _ = (c.len(), c.dropped(), p.dropped(), meter.dropped());
            }
        });
        assert_eq!(allocations, 0);
        assert!(c.dropped() > 0, "捨てる経路も通っている");
    }
}

/// push と pop の交差を Loom で網羅する。
///
/// 容量は2の冪にしない（`DEC-REC-007`）。 3 と 5 で、先に環を回して添字をまたぐ手前に
/// 置いてから競らせる。 待ちの回り（`yield_now` で空くのを待つ形）は状態が爆発するので、
/// 各スレッドの操作回数を固定し、終わったあとに残りを読み切って総数を突き合わせる。
///
/// 書き手も読み手も、生んだスレッドに回す。 主スレッドを読み手にすると、主スレッドが
/// join で止まるまで書き手へ切り替わらない経路しか辿らず、競る場面を1つも見ないまま通った。
/// **踏んだ。** 書き込みの公開を `Relaxed` に落としても緑のままだった。
/// 競る場面を見たかどうかを [`check`] が数え、1つも無ければ落とす。
#[cfg(all(test, loom))]
mod loom_model {
    use super::*;
    use loom::thread;

    /// `f` をモデルの上で網羅する。 `f` は、書き手の途中の状態を読み手が見たときに真を返す。
    fn check(f: impl Fn() -> bool + Sync + Send + 'static) {
        // 数える側はモデルの外のアトミック。 Loom の型はモデルの外で読めない。
        let raced = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let counter = std::sync::Arc::clone(&raced);
        // 交差の深さに上限を置かない（網羅する）。 操作回数を固定してあるので 1 秒かからない。
        let mut b = loom::model::Builder::new();
        b.preemption_bound = None;
        b.check(move || {
            if f() {
                counter.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
        });
        assert!(
            raced.load(std::sync::atomic::Ordering::Relaxed) > 0,
            "競る場面を1つも辿っていない。 試験の組み方が交差を生んでいない"
        );
    }

    /// `n` 回書いて `n` 回読んで、先に環を回しておく。
    fn rotate(p: &mut Producer, c: &mut Consumer, n: usize) {
        let mut out = [0.0_f32; 1];
        for _ in 0..n {
            assert_eq!(p.push(&[-1.0]), 1);
            assert_eq!(c.pop(&mut out), 1);
        }
    }

    fn drain(c: &mut Consumer, into: &mut Vec<f32>) {
        let mut out = [0.0_f32; 8];
        loop {
            let n = c.pop(&mut out);
            if n == 0 {
                return;
            }
            into.extend_from_slice(&out[..n]);
        }
    }

    /// 1回だけ読む。 読めた数と中身を返す。
    fn pop_once(c: &mut Consumer, into: &mut Vec<f32>) -> usize {
        let mut out = [0.0_f32; 2];
        let n = c.pop(&mut out);
        into.extend_from_slice(&out[..n]);
        n
    }

    /// 4つ目は、読み手がこの中で読み終えた枠にしか入らない。 読み手の `tail` の公開
    /// （読み終えるまで上書きしない）と、書き手の `head` の公開（書き終えるまで読まない）の
    /// 両方を通る。
    #[test]
    fn 競っても書いた順にしか読めない() {
        check(|| {
            let (mut p, mut c) = channel(3); // 実効容量 2
            rotate(&mut p, &mut c, 2); // 次の書き込みは末尾の枠から先頭へまたぐ
            let writer = thread::spawn(move || {
                let data = [1.0, 2.0, 3.0, 4.0];
                let mut at = p.push(&data);
                at += p.push(&data[at..]);
                at += p.push(&data[at..]);
                (p, at)
            });
            let reader = thread::spawn(move || {
                let mut got = Vec::new();
                let first = pop_once(&mut c, &mut got);
                pop_once(&mut c, &mut got);
                (c, got, first)
            });

            let (_p, wrote) = writer.join().expect("書き手が終わる");
            let (mut c, mut got, first) = reader.join().expect("読み手が終わる");
            let raced = first > 0 && first < wrote;
            drain(&mut c, &mut got);
            assert_eq!(got.len(), wrote, "書いた数だけ読める");
            assert_eq!(
                got,
                [1.0, 2.0, 3.0, 4.0][..wrote],
                "書いた順に、重複も欠落も無く読める"
            );
            raced
        });
    }

    #[test]
    fn 捨てた数と読めた数を足すと書こうとした数になる() {
        check(|| {
            let (mut p, mut c) = channel(3); // 実効容量 2
            rotate(&mut p, &mut c, 1);
            let meter = p.meter();
            let writer = thread::spawn(move || {
                p.push_or_drop(&[1.0, 2.0, 3.0]);
                p.push_or_drop(&[4.0]);
                p
            });
            let reader = thread::spawn(move || {
                let mut got = Vec::new();
                let first = pop_once(&mut c, &mut got);
                (c, got, first)
            });

            let _p = writer.join().expect("書き手が終わる");
            let (mut c, mut got, first) = reader.join().expect("読み手が終わる");
            drain(&mut c, &mut got);
            let dropped = meter.dropped();
            assert_eq!(got.len() + dropped, 4, "読めた {got:?}、捨てた {dropped}");
            assert!(
                got.windows(2).all(|w| w[0] < w[1]),
                "捨てても順序は前後しない: {got:?}"
            );
            // 読み手が先に空けたので、捨てずに入った4つ目がある。
            first > 0 && got.contains(&4.0)
        });
    }

    #[test]
    fn 読めると答えた数は実効容量を超えず実際に読める() {
        check(|| {
            let (mut p, mut c) = channel(5); // 実効容量 4
            rotate(&mut p, &mut c, 3);
            let writer = thread::spawn(move || {
                p.push(&[1.0, 2.0]);
                p.push(&[3.0, 4.0, 5.0]);
                p
            });
            let reader = thread::spawn(move || {
                let claimed = c.len();
                assert!(claimed <= 4, "実効容量を超えた: {claimed}");
                let mut out = [0.0_f32; 5];
                let got = c.pop(&mut out);
                // 読む側は自分しか減らさないので、答えた数より少なくはならない。
                assert!(got >= claimed, "答えた {claimed} より少ない {got}");
                assert!(
                    out[..got].windows(2).all(|w| w[0] < w[1]),
                    "書いた順: {:?}",
                    &out[..got]
                );
                claimed
            });

            let _p = writer.join().expect("書き手が終わる");
            let claimed = reader.join().expect("読み手が終わる");
            claimed > 0 && claimed < 4
        });
    }
}
