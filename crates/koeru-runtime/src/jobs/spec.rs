//! 仕事の投入（[`JobSpec`]）と、結果の受け取り口（[`JobTicket`]・[`Completed`]）。
//!
//! **計算の closure には、取消の合図（[`CancelProbe`]）しか渡さない。** 台帳の
//! 書き込み権限（`ProjectRuntime::writer` が返す guard）はこのモジュールにも
//! `JobRuntime` にも出てこない——計算は不変の入力を読んで値を作るだけで、
//! 書くのは公開（`JobRuntime::publish`）の `admit` の中、呼び出し側が writer を
//! 取るところだけ（`DEC-PLT-034` の実行域の表「計算 — DB への書き込み権限を持たない」）。
//!
//! ただし closure が `Arc<ProjectRuntime>` を自分で捕まえて書くことは、型では
//! 止められない。 これは型で守れる約束ではなく、この crate の規約と、
//! `tests/jobs.rs` が `src/jobs/` を読んで見る「`ProjectRuntime` / `WriterGuard` /
//! `koeru_core` を引いていない」ことで守る。

use std::fmt;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use koeru_failure::{Class, Outcome};

use super::error::{Cancelled, ComputeError, JobError};
use super::ids::{Attempt, JobId, Receipt};
use super::model::{BasedOn, JobKind, OverflowPolicy, Priority};
use crate::lease::ProjectLease;

/// 計算の closure が受け取る取消の合図。 **試行ごと**に別の値で、古い試行の
/// 合図が新しい試行を止めることはない。
///
/// 区切りで [`Self::check`] を呼ぶ計算は、取消・置き換え・期限切れ・停止で
/// 早く抜けられる。 呼ばない計算は最後まで走り、結果は公開されない。
#[derive(Debug)]
pub struct CancelProbe {
    shared: Arc<ProbeShared>,
}

/// 合図の中身。 表（`JobRuntime` の状態）も同じものを持ち、そこから立てる。
#[derive(Debug)]
pub(crate) struct ProbeShared {
    cancelled: AtomicBool,
    /// `f32` のビット列。 `NO_PROGRESS` は「測れていない」。
    progress: AtomicU32,
}

/// `f32` の非数。 進捗として報告された値は `0..=1` に丸めるので、この値とは衝突しない。
const NO_PROGRESS: u32 = u32::MAX;

impl ProbeShared {
    pub(crate) fn new() -> Self {
        Self {
            cancelled: AtomicBool::new(false),
            progress: AtomicU32::new(NO_PROGRESS),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn progress(&self) -> Option<f32> {
        match self.progress.load(Ordering::Relaxed) {
            NO_PROGRESS => None,
            bits => Some(f32::from_bits(bits)),
        }
    }
}

impl CancelProbe {
    pub(crate) const fn new(shared: Arc<ProbeShared>) -> Self {
        Self { shared }
    }

    /// 取り消されていれば `Err`。 `?` で計算から抜ける。
    ///
    /// # Errors
    ///
    /// 取消・置き換え・期限切れ・停止のどれかで、合図が立っている。
    pub fn check(&self) -> Result<(), Cancelled> {
        if self.is_cancelled() {
            Err(Cancelled::new())
        } else {
            Ok(())
        }
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.shared.cancelled.load(Ordering::Acquire)
    }

    /// 進捗を報告する（`JobRunning.progress`）。 `0..=1` に丸める。 非数は捨てる。
    pub fn report_progress(&self, fraction: f32) {
        if fraction.is_nan() {
            return;
        }
        self.shared
            .progress
            .store(fraction.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }
}

pub(crate) type Compute<T> = Box<dyn FnOnce(&CancelProbe) -> Result<T, ComputeError> + Send>;

/// 仕事1件の依頼。
pub struct JobSpec<T> {
    pub(crate) lease: ProjectLease,
    pub(crate) kind: JobKind,
    pub(crate) priority: Priority,
    pub(crate) based_on: BasedOn,
    pub(crate) input_bytes: usize,
    pub(crate) overflow: OverflowPolicy,
    pub(crate) deadline: Option<Duration>,
    pub(crate) retry_of: Option<JobId>,
    pub(crate) compute: Compute<T>,
}

impl<T> fmt::Debug for JobSpec<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // closure と入力の印は出さない。
        f.debug_struct("JobSpec")
            .field("kind", &self.kind)
            .field("priority", &self.priority)
            .field("input_bytes", &self.input_bytes)
            .field("overflow", &self.overflow)
            .field("deadline", &self.deadline)
            .field("retry_of", &self.retry_of)
            .finish_non_exhaustive()
    }
}

impl<T> JobSpec<T> {
    /// 依頼を作る。 `compute` は作業スレッドで1度だけ呼ばれる。
    ///
    /// 既定は、読んだ入力の印なし・申告バイト 0・上限に当たったら断る・期限なし。
    pub fn new(
        lease: ProjectLease,
        kind: JobKind,
        priority: Priority,
        compute: impl FnOnce(&CancelProbe) -> Result<T, ComputeError> + Send + 'static,
    ) -> Self {
        Self {
            lease,
            kind,
            priority,
            based_on: BasedOn::new(),
            input_bytes: 0,
            overflow: OverflowPolicy::Reject,
            deadline: None,
            retry_of: None,
            compute: Box::new(compute),
        }
    }

    /// 計算が実際に読む入力の印。 公開のときに今の値と突き合わせる。
    #[must_use]
    pub fn based_on(mut self, based_on: BasedOn) -> Self {
        self.based_on = based_on;
        self
    }

    /// 入力の大きさ（バイト）。 呼び出し側の申告で、上限の勘定に使う。
    #[must_use]
    pub const fn input_bytes(mut self, bytes: usize) -> Self {
        self.input_bytes = bytes;
        self
    }

    #[must_use]
    pub fn overflow(mut self, overflow: OverflowPolicy) -> Self {
        self.overflow = overflow;
        self
    }

    /// 投入からの期限。 過ぎたら失敗（`JobError::DeadlineExceeded`）にし、
    /// **自動でやり直さない。**
    #[must_use]
    pub const fn deadline(mut self, deadline: Duration) -> Self {
        self.deadline = Some(deadline);
        self
    }

    /// 同じ仕事の新しい試行にする。 古い試行は置き換えられ、その結果は公開されない。
    #[must_use]
    pub const fn retry_of(mut self, job: JobId) -> Self {
        self.retry_of = Some(job);
        self
    }
}

/// 計算が終わった結果。 **公開は別の手続き**（`JobRuntime::publish`）で、
/// これを持っているだけでは台帳にも画面にも出ていない。
///
/// 値は `JobRuntime` だけが作る。 呼び出し側が組み立てて公開を通すことはできない。
pub struct Completed<T> {
    job: JobId,
    attempt: Attempt,
    lease: ProjectLease,
    based_on: BasedOn,
    value: T,
}

impl<T> fmt::Debug for Completed<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 値は利用者のデータでありうるので出さない。
        f.debug_struct("Completed")
            .field("job", &self.job)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

impl<T> Completed<T> {
    pub(crate) const fn new(
        job: JobId,
        attempt: Attempt,
        lease: ProjectLease,
        based_on: BasedOn,
        value: T,
    ) -> Self {
        Self {
            job,
            attempt,
            lease,
            based_on,
            value,
        }
    }

    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    #[must_use]
    pub const fn attempt(&self) -> Attempt {
        self.attempt
    }

    /// 投入したときの貸与。 公開の `admit` が `ProjectRuntime::writer` を取るときに渡す。
    #[must_use]
    pub const fn lease(&self) -> &ProjectLease {
        &self.lease
    }

    /// 計算が読んだ入力の印。
    #[must_use]
    pub const fn based_on(&self) -> &BasedOn {
        &self.based_on
    }

    #[must_use]
    pub const fn value(&self) -> &T {
        &self.value
    }

    #[must_use]
    pub fn into_value(self) -> T {
        self.value
    }
}

/// 公開が確定した結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Published<R> {
    /// `admit` が返した値。
    pub value: R,
    /// 公開の受領証。 以後の `cancel` が `AlreadyCommitted` で返すものと同じ。
    pub receipt: Receipt,
}

/// 投入した仕事を識別する値。 結果は受け取れない（コールバックで回す仕事）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobHandle {
    job: JobId,
    attempt: Attempt,
}

impl JobHandle {
    pub(crate) const fn new(job: JobId, attempt: Attempt) -> Self {
        Self { job, attempt }
    }

    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    #[must_use]
    pub const fn attempt(&self) -> Attempt {
        self.attempt
    }
}

/// 作業スレッドで呼ぶ完了の callback。 成功は `Ok(Completed)`、取消・期限・上限の
/// 失敗は `Err`。
pub(crate) type Callback<T> = Box<dyn FnOnce(Result<Completed<T>, JobError>) + Send>;

/// 表から、型を消して結果の口を閉じるための trait。
///
/// 表（`JobRuntime` の状態）は `T` を知らない。 取消・期限・置き換えは、
/// 待っている側へ `Err` を渡すだけでよい。
pub(crate) trait Resolve: Send + Sync {
    fn resolve_err(&self, e: JobError);
}

struct SlotInner<T> {
    resolved: bool,
    value: Option<Result<Completed<T>, JobError>>,
    callback: Option<Callback<T>>,
}

/// 1試行の結果の口。 **最初に閉じた者が勝つ**——取消と完了が競ったとき、
/// 待っている側が見る結果は1つだけ。
pub(crate) struct Slot<T> {
    inner: Mutex<SlotInner<T>>,
    cv: Condvar,
}

fn relock<'a, T>(
    r: Result<MutexGuard<'a, T>, PoisonError<MutexGuard<'a, T>>>,
) -> MutexGuard<'a, T> {
    // 口のロックは値を出し入れするだけで、利用者の closure はロックの外で呼ぶ。
    // 毒されても中身は一貫しているので、そのまま使う。
    r.unwrap_or_else(PoisonError::into_inner)
}

impl<T: Send + 'static> Slot<T> {
    pub(crate) fn new(callback: Option<Callback<T>>) -> Arc<Self> {
        Arc::new(Self {
            inner: Mutex::new(SlotInner {
                resolved: false,
                value: None,
                callback,
            }),
            cv: Condvar::new(),
        })
    }

    /// 結果を渡す。 すでに閉じていれば何もしない（`r` は捨てる）。
    ///
    /// callback は呼び出した thread で、ロックの外で呼ぶ。 panic しても
    /// 作業スレッドを巻き込まない。
    pub(crate) fn resolve(&self, r: Result<Completed<T>, JobError>) {
        let callback = {
            let mut inner = relock(self.inner.lock());
            if inner.resolved {
                return;
            }
            inner.resolved = true;
            let Some(callback) = inner.callback.take() else {
                inner.value = Some(r);
                drop(inner);
                self.cv.notify_all();
                return;
            };
            callback
        };
        if catch_unwind(AssertUnwindSafe(|| callback(r))).is_err() {
            koeru_failure::record(
                "jobs.callback_panicked",
                Class::Internal,
                Outcome::Unknown,
                "jobs.complete",
            );
        }
    }
}

impl<T: Send + 'static> Resolve for Slot<T> {
    fn resolve_err(&self, e: JobError) {
        self.resolve(Err(e));
    }
}

/// 投入した仕事の結果を待つための札。
///
/// [`Self::wait`] は、結果か失敗が出るまで待つ。 期限は投入のときの `deadline` が
/// `JobRuntime` の監視で効くので、ここでは持たない。
pub struct JobTicket<T> {
    job: JobId,
    attempt: Attempt,
    slot: Arc<Slot<T>>,
}

impl<T> fmt::Debug for JobTicket<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("JobTicket")
            .field("job", &self.job)
            .field("attempt", &self.attempt)
            .finish_non_exhaustive()
    }
}

impl<T: Send + 'static> JobTicket<T> {
    pub(crate) const fn new(job: JobId, attempt: Attempt, slot: Arc<Slot<T>>) -> Self {
        Self { job, attempt, slot }
    }

    #[must_use]
    pub const fn job(&self) -> JobId {
        self.job
    }

    #[must_use]
    pub const fn attempt(&self) -> Attempt {
        self.attempt
    }

    #[must_use]
    pub const fn handle(&self) -> JobHandle {
        JobHandle::new(self.job, self.attempt)
    }

    /// 結果が出るまで待つ。
    ///
    /// # Errors
    ///
    /// 取り消された・置き換えられた・期限を過ぎた・計算が失敗した
    /// （[`JobError`]）。
    pub fn wait(self) -> Result<Completed<T>, JobError> {
        let mut inner = relock(self.slot.inner.lock());
        loop {
            if let Some(r) = inner.value.take() {
                return r;
            }
            inner = relock(self.slot.cv.wait(inner));
        }
    }

    /// `wait` に待つ上限をつけたもの。 出なければ札を返す（待ち続けるなら
    /// もう一度呼ぶ）。
    ///
    /// # Errors
    ///
    /// 上限までに出なかったときだけ `Err` で札が戻る。 結果の失敗は、
    /// 内側の `Result` で返る。
    pub fn wait_timeout(self, limit: Duration) -> Result<Result<Completed<T>, JobError>, Self> {
        let end = std::time::Instant::now() + limit;
        let mut inner = relock(self.slot.inner.lock());
        loop {
            if let Some(r) = inner.value.take() {
                return Ok(r);
            }
            let left = end.saturating_duration_since(std::time::Instant::now());
            if left.is_zero() {
                drop(inner);
                return Err(self);
            }
            let (next, _) = self
                .slot
                .cv
                .wait_timeout(inner, left)
                .unwrap_or_else(PoisonError::into_inner);
            inner = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::mpsc;

    use super::*;

    fn completed(value: u32) -> Completed<u32> {
        Completed::new(
            JobId::generate(),
            Attempt::FIRST,
            ProjectLease::generate(),
            BasedOn::new(),
            value,
        )
    }

    #[test]
    fn 口は最初に閉じた結果だけを渡す() {
        let slot = Slot::<u32>::new(None);
        let ticket = JobTicket::new(JobId::generate(), Attempt::FIRST, Arc::clone(&slot));
        slot.resolve(Err(JobError::Cancelled));
        slot.resolve(Ok(completed(1)));
        assert_eq!(
            ticket.wait().expect_err("先の失敗が残る"),
            JobError::Cancelled
        );
    }

    #[test]
    fn 口を閉じると待っている側が起きる() {
        let slot = Slot::<u32>::new(None);
        let ticket = JobTicket::new(JobId::generate(), Attempt::FIRST, Arc::clone(&slot));
        let waiter = std::thread::spawn(move || ticket.wait().map(Completed::into_value));
        slot.resolve(Ok(completed(7)));
        assert_eq!(waiter.join().expect("join"), Ok(7));
    }

    #[test]
    fn 上限つきの待ちは出なければ札を返す() {
        let slot = Slot::<u32>::new(None);
        let ticket = JobTicket::new(JobId::generate(), Attempt::FIRST, Arc::clone(&slot));
        let ticket = ticket
            .wait_timeout(Duration::from_millis(1))
            .expect_err("まだ出ていない");
        slot.resolve(Ok(completed(3)));
        let done = ticket
            .wait_timeout(Duration::from_secs(5))
            .expect("出ている")
            .expect("成功");
        assert_eq!(done.into_value(), 3);
    }

    #[test]
    fn callback_があれば結果は_callback_へ渡り札には残らない() {
        let (tx, rx) = mpsc::channel();
        let slot = Slot::<u32>::new(Some(Box::new(move |r| {
            let _ = tx.send(r.map(Completed::into_value));
        })));
        slot.resolve(Ok(completed(9)));
        assert_eq!(rx.recv().expect("callback が呼ばれる"), Ok(9));
        // 2度目は呼ばれない。
        slot.resolve(Err(JobError::Cancelled));
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn callback_が_panic_しても呼んだ側は続く() {
        let slot = Slot::<u32>::new(Some(Box::new(|_| panic!("callback の panic"))));
        slot.resolve(Ok(completed(1)));
    }

    #[test]
    fn 合図は立てるまで立っていない() {
        let shared = Arc::new(ProbeShared::new());
        let probe = CancelProbe::new(Arc::clone(&shared));
        assert!(probe.check().is_ok());
        shared.cancel();
        assert!(probe.check().is_err());
        assert!(probe.is_cancelled());
    }

    #[test]
    fn 進捗は報告するまで測れていない_範囲に丸める() {
        let shared = Arc::new(ProbeShared::new());
        let probe = CancelProbe::new(Arc::clone(&shared));
        assert_eq!(shared.progress(), None);
        probe.report_progress(0.25);
        assert_eq!(shared.progress(), Some(0.25));
        probe.report_progress(7.0);
        assert_eq!(shared.progress(), Some(1.0));
        probe.report_progress(-1.0);
        assert_eq!(shared.progress(), Some(0.0));
        probe.report_progress(f32::NAN);
        assert_eq!(shared.progress(), Some(0.0), "非数は捨てる");
    }

    #[test]
    fn 完了した結果は_debug_で値を出さない() {
        let c = Completed::new(
            JobId::generate(),
            Attempt::FIRST,
            ProjectLease::generate(),
            BasedOn::new(),
            "秘密の歌詞".to_owned(),
        );
        assert!(!format!("{c:?}").contains("秘密"));
    }
}
