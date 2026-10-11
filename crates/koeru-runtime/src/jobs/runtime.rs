//! [`JobRuntime`] — 背後で回す仕事の実行域（`DEC-PLT-034` の実行域の表「計算」）。
//!
//! 持つのは、有界の待ち・lane ごとの作業スレッド・試行・取消・期限・公開の
//! 線形化点。 **エンジンを知らない**——計算は closure で受け取るだけで、DB への
//! 書き込み権限を渡さない。 解析（F0・アライメント・oto）は今は同じプロセスの
//! 作業スレッドで走らせ、計算する口は closure なので差し替えられる（`DEC-PLT-047`）。
//!
//! **走り始めた外部エンジンの呼び出しは止められない。** 取消は合図を立てるだけで、
//! 区切りで合図を見る計算だけが早く抜ける。 止められなかったことは
//! `JobStatus::Cancelled { computation_stopped: false }` で正直に出す。
//! `spawn_blocking` や join を強制取消と呼ばない（`00-architecture-report.md` §7）。

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use koeru_failure::{Class, Failure, Outcome};

use super::config::{JobConfig, PUBLISH_WAIT_FOR_CANCEL};
use super::error::{ComputeError, JobError, PublishError};
use super::ids::{Attempt, JobId, Receipt};
use super::model::{BasedOn, CancelOutcome, JobCounts, JobFailure, JobSnapshot, LaneStats};
use super::spec::{
    Callback, CancelProbe, Completed, Compute, JobHandle, JobSpec, JobTicket, ProbeShared,
    Published, Resolve, Slot,
};
use super::state::{
    CancelEffect, End, Fate, Finisher, Reason, RunCtx, State, Submission, Task, Verdict,
};
use super::trace::Outbox;
use crate::lease::ProjectLease;

/// 表と、それを待つ条件変数。 作業スレッドと監視スレッドが `Arc` で持つ。
///
/// 条件変数はどれも表の mutex と組にして使う。
struct Shared {
    state: Mutex<State>,
    /// lane ごとの「待ちが増えた・止める」。
    lane_cv: Vec<Condvar>,
    /// 「期限が増えた・止める」。
    timer_cv: Condvar,
    /// 「公開が終わった」。 公開の途中の取消が待つ。
    publish_cv: Condvar,
    /// 「スレッドが抜けた」。 `shutdown_and_wait` が待つ。
    exit_cv: Condvar,
}

impl Shared {
    /// 表を取る。
    ///
    /// 表のロックの区間は、利用者の closure を呼ばず、短い表の操作だけをする。
    /// 毒されるのは、その操作の欠陥で panic したときだけ。 その場合も、
    /// 背後の仕事の全部を止めるより、記録して続けるほうを採る（毒を外して1度だけ記録する）。
    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poisoned| {
            self.state.clear_poison();
            koeru_failure::record(
                "jobs.poisoned",
                Class::Internal,
                Outcome::Unknown,
                "jobs.state",
            );
            poisoned.into_inner()
        })
    }

    fn notify_lanes(&self) {
        for cv in &self.lane_cv {
            cv.notify_all();
        }
    }
}

impl Finisher for Shared {
    fn finish_compute(&self, ctx: &RunCtx, end: End) -> (Verdict, Fate) {
        let mut out = Outbox::default();
        let (verdict, fate) = self.lock().finish_compute(ctx, end, &mut out);
        out.flush();
        // 昇格や、戻ってきたスレッドの復帰で、待ちが取れるようになったかもしれない。
        self.lane_cv[ctx.lane].notify_all();
        if fate == Fate::Exit {
            self.exit_cv.notify_all();
        }
        (verdict, fate)
    }
}

/// 計算の closure を、表へ結果を伝える道具立てに包む。
fn make_task<T: Send + 'static>(
    compute: Compute<T>,
    probe: Arc<ProbeShared>,
    slot: Arc<Slot<T>>,
    lease: ProjectLease,
    based_on: BasedOn,
) -> Task {
    Box::new(move |finisher: &dyn Finisher, ctx: RunCtx| {
        let cancel_probe = CancelProbe::new(probe);
        // 計算が panic しても、作業スレッドは落とさない。
        let result = catch_unwind(AssertUnwindSafe(|| compute(&cancel_probe)));
        let end = match &result {
            Ok(Ok(_)) => End::Done,
            Ok(Err(ComputeError::Cancelled(_))) => End::Cancelled,
            Ok(Err(ComputeError::Failed(f))) => End::Failed(*f),
            Err(_) => End::Panicked,
        };
        let (verdict, fate) = finisher.finish_compute(&ctx, end);
        match (verdict, result) {
            (Verdict::Deliver, Ok(Ok(value))) => slot.resolve(Ok(Completed::new(
                ctx.job,
                ctx.attempt,
                lease,
                based_on,
                value,
            ))),
            (Verdict::Resolve(error), _) => slot.resolve(Err(error)),
            // 置き換え・期限切れ・（来ないはずの）届けられない結果。 結果は捨てる。
            // 待っている側には、置き換えるときか期限を迎えたときに渡してある。
            (Verdict::Abandoned | Verdict::Deliver, _) => slot.resolve(Err(JobError::Superseded)),
        }
        fate
    })
}

/// 背後で仕事を回す。 アプリの寿命で1つ（`ProjectRuntime` の中には置かない）。
///
/// 各仕事は貸与（[`ProjectLease`]）を持ち、project を閉じる・移るときは
/// [`Self::cancel_lease`] でその貸与の仕事を全部取り消す。
///
/// 落とすと止まる（[`Self::shutdown`]）。 **作業スレッドは join しない**——
/// 走っている外部エンジンの呼び出しを待つと、GUI スレッドごと止まる。
pub struct JobRuntime {
    shared: Arc<Shared>,
}

impl std::fmt::Debug for JobRuntime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JobRuntime")
            .field("counts", &self.counts())
            .finish_non_exhaustive()
    }
}

fn spawn_worker(shared: &Arc<Shared>, lane: usize, name: &'static str) -> std::io::Result<()> {
    let shared = Arc::clone(shared);
    std::thread::Builder::new()
        .name(format!("koeru-jobs-{name}"))
        .spawn(move || worker_loop(&shared, lane))
        .map(drop)
}

fn worker_loop(shared: &Arc<Shared>, lane: usize) {
    loop {
        let mut out = Outbox::default();
        let picked = {
            let mut state = shared.lock();
            loop {
                if state.shutting_down {
                    state.thread_exited(lane);
                    drop(state);
                    shared.exit_cv.notify_all();
                    return;
                }
                if let Some(picked) = state.start_next(lane, &mut out) {
                    break picked;
                }
                state = shared.lane_cv[lane]
                    .wait(state)
                    .unwrap_or_else(PoisonError::into_inner);
            }
        };
        out.flush();
        let (task, ctx) = picked;
        if task(&**shared, ctx) == Fate::Exit {
            return;
        }
    }
}

/// 期限の監視。 期限を迎えた試行を失敗にし、走っていたなら代わりの作業スレッドを立てる。
fn timer_loop(shared: &Arc<Shared>) {
    let mut state = shared.lock();
    loop {
        if state.shutting_down {
            state.timer_exited();
            drop(state);
            shared.exit_cv.notify_all();
            return;
        }
        let mut out = Outbox::default();
        let now = Instant::now();
        let mut respawn = Vec::new();
        while let Some((id, attempt)) = state.pop_expired(now) {
            if let Some(lane) = state.expire(id, attempt, &mut out) {
                respawn.push(lane);
            }
        }
        for lane in respawn {
            // 立てる勘定は `expire` が済ませてある。 起こせなければ戻す。
            if spawn_worker(shared, lane, state.lane_name(lane)).is_err() {
                state.replacement_failed(lane, &mut out);
            }
        }
        if !out.is_empty() {
            drop(state);
            out.flush();
            state = shared.lock();
            continue;
        }
        state = match state.next_deadline() {
            Some(at) => {
                let wait = at.saturating_duration_since(Instant::now());
                shared
                    .timer_cv
                    .wait_timeout(state, wait)
                    .unwrap_or_else(PoisonError::into_inner)
                    .0
            }
            None => shared
                .timer_cv
                .wait(state)
                .unwrap_or_else(PoisonError::into_inner),
        };
    }
}

impl JobRuntime {
    /// 回しはじめる。
    ///
    /// # Panics
    ///
    /// 作業スレッドを1本も起こせないとき。 OS がスレッドを与えないなら、アプリは
    /// 回復できない（ブートストラップ層の `expect`）。
    #[must_use]
    pub fn start(config: JobConfig) -> Arc<Self> {
        let shared = Arc::new(Shared {
            state: Mutex::new(State::new(&config)),
            lane_cv: config.lanes.iter().map(|_| Condvar::new()).collect(),
            timer_cv: Condvar::new(),
            publish_cv: Condvar::new(),
            exit_cv: Condvar::new(),
        });
        for (lane, cfg) in config.lanes.iter().enumerate() {
            for _ in 0..cfg.threads {
                spawn_worker(&shared, lane, cfg.name).expect("作業スレッドを起こせること");
            }
        }
        let timer = Arc::clone(&shared);
        std::thread::Builder::new()
            .name("koeru-jobs-deadline".to_owned())
            .spawn(move || timer_loop(&timer))
            .expect("期限の監視スレッドを起こせること");
        Arc::new(Self { shared })
    }

    // ## 投入

    /// 仕事を投入し、結果を待つ札を返す。
    ///
    /// # Errors
    ///
    /// 上限に当たった（`OverflowPolicy::Reject`）、lane が使えない・止めている最中・
    /// 貸与が閉じている、やり直す相手が無い（[`JobError`]）。
    pub fn submit<T: Send + 'static>(&self, spec: JobSpec<T>) -> Result<JobTicket<T>, JobError> {
        let (accepted, slot) = self.enqueue(spec, None)?;
        Ok(JobTicket::new(accepted.job, accepted.attempt, slot))
    }

    /// 仕事を投入し、誰も待たずに、完了を `on_complete` で受ける。
    ///
    /// 成功は作業スレッドで、取消・期限・上限の失敗はその事象を起こした thread
    /// （`cancel` を呼んだ側、期限の監視）で呼ぶ。 表のロックは握らない。
    /// callback は短く返す——作業スレッドを占有する。
    ///
    /// # Errors
    ///
    /// [`Self::submit`] と同じ。
    pub fn submit_notify<T: Send + 'static>(
        &self,
        spec: JobSpec<T>,
        on_complete: impl FnOnce(Result<Completed<T>, JobError>) + Send + 'static,
    ) -> Result<JobHandle, JobError> {
        let callback: Callback<T> = Box::new(on_complete);
        let (accepted, _slot) = self.enqueue(spec, Some(callback))?;
        Ok(JobHandle::new(accepted.job, accepted.attempt))
    }

    fn enqueue<T: Send + 'static>(
        &self,
        spec: JobSpec<T>,
        callback: Option<Callback<T>>,
    ) -> Result<(super::state::Accepted, Arc<Slot<T>>), JobError> {
        let JobSpec {
            lease,
            kind,
            priority,
            based_on,
            input_bytes,
            overflow,
            deadline,
            retry_of,
            compute,
        } = spec;
        let slot = Slot::new(callback);
        let probe = Arc::new(ProbeShared::new());
        let task = make_task(
            compute,
            Arc::clone(&probe),
            Arc::clone(&slot),
            lease,
            based_on,
        );
        let resolver: Arc<dyn Resolve> = Arc::clone(&slot) as Arc<dyn Resolve>;
        let submission = Submission {
            lease,
            kind,
            priority,
            bytes: input_bytes,
            overflow,
            deadline,
            retry_of,
            probe,
            resolver,
            task,
        };
        let mut out = Outbox::default();
        let result = self.shared.lock().submit(submission, &mut out);
        out.flush();
        let accepted = result?;
        self.shared.lane_cv[accepted.lane].notify_one();
        if deadline.is_some() {
            self.shared.timer_cv.notify_all();
        }
        Ok((accepted, slot))
    }

    // ## 公開（受理の線形化点）

    /// 計算が終わった結果を公開する。
    ///
    /// **ここが受理の線形化点。** 表のロックの下で、取消が先に確定していない・
    /// 試行が最新・貸与の仕事がまだ有効・読んだ入力（`completed` の
    /// [`BasedOn`]）が `current` と一致、を1度に確かめ、`Publishing` に遷移させる。
    /// そのあとロックを外して `admit` を走らせ、成功なら `Succeeded`、
    /// 失敗なら `Failed` にする。
    ///
    /// **`admit` は表のロックの外で走る。** `admit` が `ProjectRuntime::writer` を
    /// 取って台帳へ書く。 ロックの順序は「表 → writer」の一方向で、`admit` の中から
    /// `JobRuntime` の API を呼ばない（writer を握ったまま `cancel` を呼ぶと、
    /// 公開が終わるまで待たされる）。
    ///
    /// **貸与が生きているかの最終確認は `admit` の中の `ProjectRuntime::writer`
    /// （`RuntimeError::LeaseExpired`）が担う。** ここで見るのは `cancel_lease` 済みか
    /// だけで、`ProjectRuntime::close` は知らない。
    ///
    /// 断る理由は[`PublishRefused`]。 **エンジンの失敗ではない。** 呼び出し側は
    /// 最新を読み直して、必要なら新しい試行を投入する。
    ///
    /// # Errors
    ///
    /// 受理の条件を満たさず公開を始めなかった（[`PublishError::Refused`]）、
    /// `admit` が失敗した（[`PublishError::Admit`]。仕事は `Failed`）。
    pub fn publish<T, R, E>(
        &self,
        completed: Completed<T>,
        current: &BasedOn,
        admit: impl FnOnce(Completed<T>) -> Result<(R, Receipt), E>,
    ) -> Result<Published<R>, PublishError<E>>
    where
        E: Failure + 'static,
    {
        let (job, attempt) = (completed.job(), completed.attempt());
        let mut out = Outbox::default();
        let begun = self.shared.lock().begin_publish(
            job,
            attempt,
            completed.lease(),
            completed.based_on(),
            current,
            &mut out,
        );
        out.flush();
        begun.map_err(PublishError::Refused)?;

        let mut guard = PublishGuard {
            shared: &self.shared,
            job,
            attempt,
            armed: true,
        };
        match admit(completed) {
            Ok((value, receipt)) => {
                guard.finish(Ok(receipt));
                Ok(Published { value, receipt })
            }
            Err(e) => {
                // 確定の状態は、失敗の分類が言える範囲で。 書いたかを知っているのは
                // 呼び出し側で、必要ならそちらで上書きする。
                guard.finish(Err(JobFailure::of(&e, e.class().outcome())));
                Err(PublishError::Admit(e))
            }
        }
    }

    // ## 取消

    /// 仕事を取り消す。 **成功済みを取り消しと偽らない。**
    ///
    /// - 待ち中 → 外して `Cancelled { computation_stopped: true }`（[`CancelOutcome::Cancelled`]）
    /// - 走行中 → 合図を立てる（[`CancelOutcome::Requested`]）。 計算が合図で抜ければ
    ///   `computation_stopped: true`、走り切ったら結果を捨てて `false`
    /// - 計算が終わって公開を待っている → 結果を捨てて `Cancelled { false }`
    /// - 公開の途中 → 公開が終わるまで待つ（上限つき）。 終われば
    ///   [`CancelOutcome::AlreadyCommitted`]
    /// - 公開が済んでいる → [`CancelOutcome::AlreadyCommitted`]（公開が返した受領証）
    ///
    /// 待っている側は、計算が戻るのを待たずに `JobError::Cancelled` で解放する。
    pub fn cancel(&self, job: JobId) -> CancelOutcome {
        let give_up = Instant::now() + PUBLISH_WAIT_FOR_CANCEL;
        let mut out = Outbox::default();
        let mut state = self.shared.lock();
        let outcome = loop {
            match state.cancel_entry(job, Reason::Requested, &mut out) {
                None => break CancelOutcome::NotFound,
                Some(CancelEffect::Immediate) => break CancelOutcome::Cancelled,
                Some(CancelEffect::Signalled) => break CancelOutcome::Requested,
                Some(CancelEffect::Committed(receipt)) => {
                    break CancelOutcome::AlreadyCommitted(receipt);
                }
                Some(CancelEffect::Finished) => break CancelOutcome::AlreadyFinished,
                Some(CancelEffect::TooLate) => {
                    let left = give_up.saturating_duration_since(Instant::now());
                    if left.is_zero() {
                        break CancelOutcome::Publishing;
                    }
                    state = self
                        .shared
                        .publish_cv
                        .wait_timeout(state, left)
                        .unwrap_or_else(PoisonError::into_inner)
                        .0;
                }
            }
        };
        drop(state);
        out.flush();
        // 取り消した待ちの後ろで、脇に置いていたものが昇格したかもしれない。
        self.shared.notify_lanes();
        outcome
    }

    /// 貸与の仕事を全部取り消す（project を閉じる・移るとき）。 取り消した数
    /// （走っているものは合図を立てた数）。
    ///
    /// 以後、その貸与での投入と公開を断る。 `ProjectRuntime::close` の前後どちらで
    /// 呼んでもよい——台帳への書き込みは、貸与が閉じていれば writer が断る。
    pub fn cancel_lease(&self, lease: &ProjectLease) -> usize {
        let mut out = Outbox::default();
        let cancelled = {
            let mut state = self.shared.lock();
            state.remember_closed(*lease);
            state.cancel_lease(lease, &mut out)
        };
        out.flush();
        self.shared.notify_lanes();
        cancelled
    }

    // ## 読み

    /// 待ちと走っているものの数（`TR-SYN-33`）。
    ///
    /// **アプリの状態ロックの外から読める。** `Arc<JobRuntime>` から直接引け、
    /// 表のロックは短い表の操作だけが握る（計算も `admit` も握らない）ので、
    /// テイクの確定が数秒かかっても止まらない。
    #[must_use]
    pub fn counts(&self) -> JobCounts {
        self.shared.lock().counts()
    }

    /// 仕事1件の今の状態。 保持の期間を過ぎた終わった仕事は `None`。
    #[must_use]
    pub fn job(&self, job: JobId) -> Option<JobSnapshot> {
        self.shared.lock().snapshot(job)
    }

    /// 貸与の終わっていない仕事。 積んだ順。
    #[must_use]
    pub fn active(&self, lease: &ProjectLease) -> Vec<JobSnapshot> {
        self.shared.lock().active(lease)
    }

    /// lane ごとの内訳。
    #[must_use]
    pub fn lanes(&self) -> Vec<LaneStats> {
        self.shared.lock().lane_stats()
    }

    // ## 停止

    /// 止める。 新しい仕事を受けず、待ちは取り消し、走っている仕事へ合図を立てる。
    /// **作業スレッドを待たない**——走っている外部エンジンの呼び出しを待つと、
    /// 呼び出した thread（GUI スレッドでありうる）が止まる。 合図に気づかない計算は
    /// 走り切り、結果は公開されない。
    pub fn shutdown(&self) {
        let mut out = Outbox::default();
        self.shared.lock().begin_shutdown(&mut out);
        out.flush();
        self.shared.notify_lanes();
        self.shared.timer_cv.notify_all();
    }

    /// [`Self::shutdown`] して、作業スレッドが抜けるのを `limit` まで待つ。
    /// 全部抜けたら `true`。 固まった計算が残っていれば `false`。
    ///
    /// アプリの終了処理など、GUI スレッドではない所から呼ぶ。
    pub fn shutdown_and_wait(&self, limit: Duration) -> bool {
        self.shutdown();
        let end = Instant::now() + limit;
        let mut state = self.shared.lock();
        while state.threads_alive > 0 {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return false;
            }
            state = self
                .shared
                .exit_cv
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
        true
    }
}

impl Drop for JobRuntime {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// 公開の途中で `admit` が panic したとき、仕事を `Publishing` のまま残さない。
struct PublishGuard<'a> {
    shared: &'a Shared,
    job: JobId,
    attempt: Attempt,
    armed: bool,
}

impl PublishGuard<'_> {
    fn finish(&mut self, result: Result<Receipt, JobFailure>) {
        self.armed = false;
        self.settle(result);
    }

    fn settle(&self, result: Result<Receipt, JobFailure>) {
        let mut out = Outbox::default();
        self.shared
            .lock()
            .finish_publish(self.job, self.attempt, result, &mut out);
        out.flush();
        self.shared.publish_cv.notify_all();
    }
}

impl Drop for PublishGuard<'_> {
    fn drop(&mut self) {
        if self.armed {
            self.settle(Err(JobError::PublishAborted.as_failure()));
        }
    }
}
