//! 仕事の表。 待ちの列・走っている仕事・終わった仕事・lane ごとの勘定。
//!
//! **ここの操作はすべて、表のロックを握った呼び出し側が `&mut State` で呼ぶ。**
//! 利用者の closure（計算・`admit`・完了の callback）はここでは一切呼ばない。
//! 待っている側への通知と記録は [`Outbox`] に溜め、ロックの外で出す。
//!
//! # ロックの順序
//!
//! 表のロック → （外す）→ writer のロック、の一方向だけ。 `publish` は表のロックの
//! 下で `Computed` → `Publishing` に遷移したあとロックを外し、そのあとで呼び出し側の
//! `admit` が `ProjectRuntime::writer` を取る。 **表のロックを握ったまま writer を
//! 取らない**——取ると、writer を握った側が `JobRuntime::cancel` を呼んだときに
//! 輪ができる。 そのため writer を握ったまま `cancel` を呼ぶのも避ける
//! （公開の途中の取消は公開が終わるまで待つ。上限つき）。

use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use koeru_failure::Outcome;

use super::config::{CLOSED_LEASES_REMEMBERED, JobConfig, LaneConfig};
use super::error::{JobError, PublishRefused};
use super::ids::{Attempt, CoalesceKey, JobId, Receipt};
use super::model::{
    BasedOn, JobCounts, JobFailure, JobKind, JobSnapshot, JobStatus, LaneStats, OverflowPolicy,
    Priority,
};
use super::spec::{ProbeShared, Resolve};
use super::trace::{Outbox, TraceEvent};
use crate::lease::ProjectLease;
use crate::time::now_rfc3339;

/// 待ちの列の鍵。 高い優先度が先で、同じ優先度なら先に積んだものが先。
pub(crate) type QKey = (Reverse<Priority>, u64);

/// 作業スレッドが走らせる1件の道具立て。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RunCtx {
    pub job: JobId,
    pub attempt: Attempt,
    pub lane: usize,
    pub bytes: usize,
}

/// 作業スレッドのこのあとの身の振り方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Fate {
    Continue,
    Exit,
}

/// 計算の終わり方。
#[derive(Debug, Clone, Copy)]
pub(crate) enum End {
    Done,
    Cancelled,
    Failed(JobFailure),
    Panicked,
}

/// 計算が戻ったとき、表が決めた結果の扱い。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Verdict {
    /// 計算が終わった結果を、待っている側へ渡す。
    Deliver,
    /// 待っている側へこの失敗を渡す（すでに渡していれば何も起きない）。
    Resolve(JobError),
    /// この試行はもう表のものではない（置き換え・期限切れ）。 結果は捨てる。
    Abandoned,
}

/// 計算が戻ったことを表へ伝える口。 型を消した道具立て（`Task`）が呼ぶ。
pub(crate) trait Finisher {
    fn finish_compute(&self, ctx: &RunCtx, end: End) -> (Verdict, Fate);
}

/// 型を消した計算1件。 作業スレッドが、計算を走らせて表へ結果を伝え、待っている側へ渡す。
pub(crate) type Task = Box<dyn FnOnce(&dyn Finisher, RunCtx) -> Fate + Send>;

/// 取消の理由。 表の中だけで持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    Requested,
    LeaseClosed,
    Shutdown,
    Coalesced,
    Superseded,
}

impl Reason {
    const fn to_error(self) -> JobError {
        match self {
            Self::LeaseClosed => JobError::LeaseClosed,
            Self::Superseded => JobError::Superseded,
            Self::Requested | Self::Shutdown | Self::Coalesced => JobError::Cancelled,
        }
    }

    const fn to_refusal(self) -> (PublishRefused, &'static str) {
        match self {
            Self::LeaseClosed => (PublishRefused::LeaseClosed, "refused_lease_closed"),
            Self::Superseded => (PublishRefused::Superseded, "refused_superseded"),
            Self::Requested | Self::Shutdown | Self::Coalesced => {
                (PublishRefused::Cancelled, "refused_cancelled")
            }
        }
    }
}

/// 取消を表に当てた結果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CancelEffect {
    /// 取り消した。 もう `Cancelled`。
    Immediate,
    /// 走っている。 合図を立てた。
    Signalled,
    /// 公開の途中で、取り消せない。
    TooLate,
    /// 公開が済んでいた。
    Committed(Receipt),
    /// 失敗か取消で、終わっていた。
    Finished,
}

#[derive(Debug)]
pub(crate) enum Phase {
    Queued {
        since: String,
    },
    Running {
        started_at: String,
    },
    /// 計算は終わった。 公開を待っている。
    Computed {
        started_at: String,
    },
    /// 公開の途中。 `admit` が走っている。
    Publishing {
        started_at: String,
    },
    Succeeded {
        finished_at: String,
        receipt: Receipt,
    },
    Failed {
        finished_at: String,
        failure: JobFailure,
    },
    Cancelled {
        finished_at: String,
        stopped: bool,
        reason: Reason,
    },
}

impl Phase {
    const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

pub(crate) struct JobEntry {
    pub lease: ProjectLease,
    pub kind: JobKind,
    pub priority: Priority,
    pub lane: usize,
    pub attempt: Attempt,
    pub phase: Phase,
    /// 積んだ順。 まとめて置き換えた仕事は、置き換えた相手の順を引き継ぐ。
    pub seq: u64,
    pub submitted: Instant,
    pub started: Option<Instant>,
    /// 終わった（または計算が終わった）時刻。 保持の期間の起点。
    pub finished: Option<Instant>,
    /// 待ちの列の鍵と、脇に置いた列か。
    pub queue: Option<(QKey, bool)>,
    pub coalesce: Option<CoalesceKey>,
    pub probe: Arc<ProbeShared>,
    pub resolver: Arc<dyn Resolve>,
    pub cancel_requested: Option<Reason>,
    pub deadline: Option<Instant>,
}

struct QueueItem {
    job: JobId,
    bytes: usize,
    task: Task,
}

pub(crate) struct Lane {
    cfg: LaneConfig,
    max_hung: usize,
    queue: BTreeMap<QKey, QueueItem>,
    /// 上限に当たって脇に置いた待ち。 上限には数えない。
    deferred: BTreeMap<QKey, QueueItem>,
    /// 仕事を取れる作業スレッドの数。 固まったと見なしたものは含まない。
    pub live: usize,
    pub running: usize,
    /// 期限を過ぎて固まったと見なし、まだ戻ってきていない試行。
    hung: HashSet<(JobId, Attempt)>,
    /// 待ち＋走っている（固まったと見なしたものを含む）仕事の申告バイト数。
    inflight_bytes: usize,
}

impl Lane {
    /// 仕事を受けられるか。 固まったスレッドが多すぎる lane は受けない。
    fn available(&self) -> bool {
        self.live > 0 && self.hung.len() <= self.max_hung
    }

    /// 上限の範囲で、あと1件を待ちに積めるか。
    ///
    /// 空いている作業スレッドが取るはずの仕事は「待ち」に数えない——取られる
    /// 前に次が来ても、本当に溜まっていないなら断らない（作業スレッドが起きる
    /// 速さで可否が変わらない）。 `freed` は、同時に取り除く待ちの（件数, バイト数）。
    fn fits(&self, bytes: usize, freed: (usize, usize)) -> bool {
        let queued = self.queue.len().saturating_sub(freed.0);
        let idle = self.live.saturating_sub(self.running);
        let count_ok = (queued + 1).saturating_sub(idle) <= self.cfg.max_pending;
        let inflight = self.inflight_bytes.saturating_sub(freed.1);
        // 何も抱えていない lane は、上限より大きい仕事も1つ受ける。
        let bytes_ok =
            inflight == 0 || inflight.saturating_add(bytes) <= self.cfg.max_inflight_bytes;
        count_ok && bytes_ok
    }
}

/// 投入の依頼。 型を消してある。
pub(crate) struct Submission {
    pub lease: ProjectLease,
    pub kind: JobKind,
    pub priority: Priority,
    pub bytes: usize,
    pub overflow: OverflowPolicy,
    pub deadline: Option<Duration>,
    pub retry_of: Option<JobId>,
    pub probe: Arc<ProbeShared>,
    pub resolver: Arc<dyn Resolve>,
    pub task: Task,
}

/// 受け付けた結果。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Accepted {
    pub job: JobId,
    pub attempt: Attempt,
    pub lane: usize,
}

pub(crate) struct State {
    jobs: HashMap<JobId, JobEntry>,
    lanes: Vec<Lane>,
    lane_of: [usize; Priority::ALL.len()],
    next_seq: u64,
    pub shutting_down: bool,
    closed_order: VecDeque<ProjectLease>,
    closed: HashSet<ProjectLease>,
    deadlines: BTreeSet<(Instant, JobId, Attempt)>,
    /// 終わった仕事を終わった順に。 古い記録（新しい試行に置き換えた後のもの）も混ざる。
    finished: VecDeque<(JobId, Attempt)>,
    /// 計算が終わって公開を待っている仕事を、終わった順に。
    computed: VecDeque<(JobId, Attempt)>,
    /// 新しい試行に置き換えられたが、まだ計算が戻っていない試行（と、その lane）。
    /// 期限を過ぎて戻らなければ、固まったと見なす。
    orphans: HashMap<(JobId, Attempt), usize>,
    /// 生きている作業スレッドと監視スレッドの数。
    pub threads_alive: usize,
    retain_max: usize,
    retain_for: Duration,
}

impl State {
    pub(crate) fn new(config: &JobConfig) -> Self {
        let mut lane_of = [0; Priority::ALL.len()];
        let mut threads = 1; // 期限の監視
        let lanes = config
            .lanes
            .iter()
            .enumerate()
            .map(|(i, cfg)| {
                for p in &cfg.priorities {
                    lane_of[p.index()] = i;
                }
                threads += cfg.threads;
                Lane {
                    cfg: cfg.clone(),
                    max_hung: config.max_hung_per_lane,
                    queue: BTreeMap::new(),
                    deferred: BTreeMap::new(),
                    live: cfg.threads,
                    running: 0,
                    hung: HashSet::new(),
                    inflight_bytes: 0,
                }
            })
            .collect();
        Self {
            jobs: HashMap::new(),
            lanes,
            lane_of,
            next_seq: 0,
            shutting_down: false,
            closed_order: VecDeque::new(),
            closed: HashSet::new(),
            deadlines: BTreeSet::new(),
            finished: VecDeque::new(),
            computed: VecDeque::new(),
            orphans: HashMap::new(),
            threads_alive: threads,
            retain_max: config.retain_finished_max,
            retain_for: config.retain_finished_for,
        }
    }

    pub(crate) fn lane_name(&self, lane_ix: usize) -> &'static str {
        self.lanes[lane_ix].cfg.name
    }

    // ## 記録

    fn event(
        &self,
        id: JobId,
        admission: &'static str,
        outcome: &'static str,
        failure: Option<JobFailure>,
    ) -> Option<TraceEvent> {
        let e = self.jobs.get(&id)?;
        let lane = &self.lanes[e.lane];
        let waited = e
            .started
            .unwrap_or_else(Instant::now)
            .saturating_duration_since(e.submitted);
        Some(TraceEvent {
            job: id,
            kind: e.kind,
            attempt: e.attempt,
            priority: e.priority,
            lane: lane.cfg.name,
            queue_wait_ms: u64::try_from(waited.as_millis()).unwrap_or(u64::MAX),
            inflight_bytes: lane.inflight_bytes,
            admission,
            outcome,
            failure,
        })
    }

    // ## 貸与

    /// 貸与が閉じたことを覚える。 覚えている間、その貸与での投入を断る。
    pub(crate) fn remember_closed(&mut self, lease: ProjectLease) {
        if self.closed.insert(lease) {
            self.closed_order.push_back(lease);
            while self.closed_order.len() > CLOSED_LEASES_REMEMBERED {
                if let Some(old) = self.closed_order.pop_front() {
                    self.closed.remove(&old);
                }
            }
        }
    }

    // ## 投入

    /// 受け付けるか、断るか。 **断るときは何も変えない。**
    pub(crate) fn submit(&mut self, s: Submission, out: &mut Outbox) -> Result<Accepted, JobError> {
        let lane_ix = self.lane_of[s.priority.index()];
        let (id, attempt) = match s.retry_of {
            Some(id) => {
                let e = self.jobs.get(&id).ok_or(JobError::UnknownJob)?;
                if matches!(e.phase, Phase::Publishing { .. }) {
                    // 公開の途中の仕事に、やり直しは重ねない。
                    return Err(JobError::Busy);
                }
                (id, e.attempt.next())
            }
            None => (JobId::generate(), Attempt::FIRST),
        };

        let ids = (id, attempt, lane_ix);
        if self.shutting_down {
            return Err(self.rejected(out, ids, &s, "rejected_shutdown", JobError::ShuttingDown));
        }
        if self.closed.contains(&s.lease) {
            return Err(self.rejected(out, ids, &s, "rejected_lease", JobError::LeaseClosed));
        }
        if !self.lanes[lane_ix].available() {
            return Err(self.rejected(out, ids, &s, "rejected_lane", JobError::LaneUnavailable));
        }

        // 同時に取り除く待ち（やり直しの古い試行、まとめられる相手）が空ける枠。
        let mut freed = (0_usize, 0_usize);
        if let Some(rid) = s.retry_of
            && self.jobs.get(&rid).is_some_and(|e| e.lane == lane_ix)
            && let Some(bytes) = self.queued_bytes(rid)
        {
            freed = (1, bytes);
        }
        let coalesce_target = match &s.overflow {
            OverflowPolicy::Coalesce(key) => self.find_waiting(lane_ix, key, s.retry_of),
            _ => None,
        };
        if let Some(cid) = coalesce_target
            && let Some(bytes) = self.queued_bytes(cid)
        {
            freed = (freed.0 + 1, freed.1 + bytes);
        }

        let lane = &self.lanes[lane_ix];
        let defers = matches!(s.overflow, OverflowPolicy::Defer);
        let placement = if !(defers && !lane.deferred.is_empty()) && lane.fits(s.bytes, freed) {
            Placement::Queue
        } else if defers {
            Placement::Defer
        } else {
            return Err(self.rejected(out, ids, &s, "rejected_busy", JobError::Busy));
        };

        // ここから先は受け付ける。
        let mut seq = None;
        if s.retry_of.is_some() {
            self.cancel_entry(id, Reason::Superseded, out);
        }
        if let Some(cid) = coalesce_target {
            seq = self.jobs.get(&cid).map(|e| e.seq);
            self.cancel_entry(cid, Reason::Coalesced, out);
        }
        let coalesced = seq.is_some();
        let seq = seq.unwrap_or_else(|| {
            let n = self.next_seq;
            self.next_seq += 1;
            n
        });
        let key: QKey = (Reverse(s.priority), seq);
        let deferred = matches!(placement, Placement::Defer);
        let item = QueueItem {
            job: id,
            bytes: s.bytes,
            task: s.task,
        };
        let lane = &mut self.lanes[lane_ix];
        if deferred {
            lane.deferred.insert(key, item);
        } else {
            lane.queue.insert(key, item);
            lane.inflight_bytes += s.bytes;
        }
        let now = Instant::now();
        let deadline = s.deadline.map(|d| now + d);
        if let Some(at) = deadline {
            self.deadlines.insert((at, id, attempt));
        }
        self.jobs.insert(
            id,
            JobEntry {
                lease: s.lease,
                kind: s.kind,
                priority: s.priority,
                lane: lane_ix,
                attempt,
                phase: Phase::Queued {
                    since: now_rfc3339(),
                },
                seq,
                submitted: now,
                started: None,
                finished: None,
                queue: Some((key, deferred)),
                coalesce: match s.overflow {
                    OverflowPolicy::Coalesce(k) => Some(k),
                    _ => None,
                },
                probe: s.probe,
                resolver: s.resolver,
                cancel_requested: None,
                deadline,
            },
        );
        let admission = if deferred {
            "deferred"
        } else if coalesced {
            "coalesced"
        } else {
            "accepted"
        };
        out.trace(self.event(id, admission, "queued", None));
        Ok(Accepted {
            job: id,
            attempt,
            lane: lane_ix,
        })
    }

    /// 断ったことを記録して、断る理由を返す。 `ids` は（仕事, 試行, lane）。
    fn rejected(
        &self,
        out: &mut Outbox,
        ids: (JobId, Attempt, usize),
        s: &Submission,
        admission: &'static str,
        error: JobError,
    ) -> JobError {
        let (id, attempt, lane_ix) = ids;
        let lane = &self.lanes[lane_ix];
        out.trace(Some(TraceEvent {
            job: id,
            kind: s.kind,
            attempt,
            priority: s.priority,
            lane: lane.cfg.name,
            queue_wait_ms: 0,
            inflight_bytes: lane.inflight_bytes,
            admission,
            outcome: "refused",
            failure: Some(error.as_failure()),
        }));
        error
    }

    /// 待ち（上限に数える方）にいる仕事の申告バイト数。
    fn queued_bytes(&self, id: JobId) -> Option<usize> {
        let e = self.jobs.get(&id)?;
        let (key, deferred) = e.queue?;
        if deferred {
            return None;
        }
        self.lanes[e.lane].queue.get(&key).map(|i| i.bytes)
    }

    /// 同じ lane で、同じ鍵で待っている仕事。 やり直しの相手自身は除く。
    fn find_waiting(&self, lane: usize, key: &CoalesceKey, except: Option<JobId>) -> Option<JobId> {
        self.jobs
            .iter()
            .filter(|(id, e)| {
                Some(**id) != except
                    && e.lane == lane
                    && e.queue.is_some()
                    && matches!(e.phase, Phase::Queued { .. })
                    && e.coalesce.as_ref() == Some(key)
            })
            .min_by_key(|(_, e)| e.seq)
            .map(|(id, _)| *id)
    }

    /// 脇に置いたものを、空いた分だけ待ちへ上げる。
    fn promote(&mut self, lane_ix: usize, out: &mut Outbox) {
        loop {
            let lane = &self.lanes[lane_ix];
            let Some((_, item)) = lane.deferred.first_key_value() else {
                break;
            };
            if !lane.fits(item.bytes, (0, 0)) {
                break;
            }
            let lane = &mut self.lanes[lane_ix];
            let Some((key, item)) = lane.deferred.pop_first() else {
                break;
            };
            lane.inflight_bytes += item.bytes;
            let id = item.job;
            lane.queue.insert(key, item);
            if let Some(e) = self.jobs.get_mut(&id) {
                e.queue = Some((key, false));
            }
            out.trace(self.event(id, "promoted", "queued", None));
        }
    }

    // ## 走らせる

    /// lane の次の仕事を1件取り、走っていることにする。
    pub(crate) fn start_next(
        &mut self,
        lane_ix: usize,
        out: &mut Outbox,
    ) -> Option<(Task, RunCtx)> {
        loop {
            let lane = &mut self.lanes[lane_ix];
            let (_, item) = lane.queue.pop_first()?;
            let Some(entry) = self.jobs.get_mut(&item.job) else {
                lane.inflight_bytes = lane.inflight_bytes.saturating_sub(item.bytes);
                continue;
            };
            if !matches!(entry.phase, Phase::Queued { .. }) {
                lane.inflight_bytes = lane.inflight_bytes.saturating_sub(item.bytes);
                continue;
            }
            lane.running += 1;
            let now = Instant::now();
            entry.phase = Phase::Running {
                started_at: now_rfc3339(),
            };
            entry.started = Some(now);
            entry.queue = None;
            let ctx = RunCtx {
                job: item.job,
                attempt: entry.attempt,
                lane: lane_ix,
                bytes: item.bytes,
            };
            out.trace(self.event(item.job, "none", "running", None));
            self.promote(lane_ix, out);
            return Some((item.task, ctx));
        }
    }

    /// 計算が戻った。 結果の扱いを決める。
    pub(crate) fn finish_compute(
        &mut self,
        ctx: &RunCtx,
        end: End,
        out: &mut Outbox,
    ) -> (Verdict, Fate) {
        let key = (ctx.job, ctx.attempt);
        let lane = &mut self.lanes[ctx.lane];
        lane.inflight_bytes = lane.inflight_bytes.saturating_sub(ctx.bytes);

        if lane.hung.remove(&key) {
            // 固まったと見なして外した作業スレッドが、遅れて戻ってきた。
            // 結果は公開しない。 代わりのスレッドが足りなければ、仕事に戻る。
            let rejoin = !self.shutting_down && lane.live < lane.cfg.threads;
            let fate = if rejoin {
                lane.live += 1;
                Fate::Continue
            } else {
                self.threads_alive = self.threads_alive.saturating_sub(1);
                Fate::Exit
            };
            self.promote(ctx.lane, out);
            return (Verdict::Abandoned, fate);
        }
        lane.running = lane.running.saturating_sub(1);

        let current = self
            .jobs
            .get(&ctx.job)
            .is_some_and(|e| e.attempt == ctx.attempt && matches!(e.phase, Phase::Running { .. }));
        if !current {
            self.orphans.remove(&key);
            self.promote(ctx.lane, out);
            return (Verdict::Abandoned, Fate::Continue);
        }
        let Some((cancel, started_at)) = self.jobs.get(&ctx.job).map(|e| {
            let started_at = match &e.phase {
                Phase::Running { started_at } => started_at.clone(),
                _ => String::new(),
            };
            (e.cancel_requested, started_at)
        }) else {
            return (Verdict::Abandoned, Fate::Continue);
        };

        let verdict = match (cancel, end) {
            (Some(reason), end) => {
                // 取消の合図を受けていた。 走り切った結果は公開しない。
                let stopped = matches!(end, End::Cancelled);
                self.set_terminal(
                    ctx.job,
                    Phase::Cancelled {
                        finished_at: now_rfc3339(),
                        stopped,
                        reason,
                    },
                    None,
                    "none",
                    out,
                );
                Verdict::Resolve(reason.to_error())
            }
            (None, End::Done) => {
                if let Some(e) = self.jobs.get_mut(&ctx.job) {
                    e.phase = Phase::Computed { started_at };
                    e.finished = Some(Instant::now());
                    if let Some(at) = e.deadline.take() {
                        self.deadlines.remove(&(at, ctx.job, ctx.attempt));
                    }
                }
                self.computed.push_back((ctx.job, ctx.attempt));
                out.trace(self.event(ctx.job, "none", "computed", None));
                self.evict(Instant::now());
                Verdict::Deliver
            }
            (None, End::Cancelled) => {
                // probe を通さずに「取り消された」は作れないので、ここへは来ない。
                // 来たら、止まったものとして扱う。
                self.set_terminal(
                    ctx.job,
                    Phase::Cancelled {
                        finished_at: now_rfc3339(),
                        stopped: true,
                        reason: Reason::Requested,
                    },
                    None,
                    "none",
                    out,
                );
                Verdict::Resolve(JobError::Cancelled)
            }
            (None, End::Failed(f)) => {
                self.set_terminal(
                    ctx.job,
                    Phase::Failed {
                        finished_at: now_rfc3339(),
                        failure: f,
                    },
                    None,
                    "none",
                    out,
                );
                Verdict::Resolve(JobError::ComputeFailed(f))
            }
            (None, End::Panicked) => {
                let e = JobError::ComputePanicked;
                self.set_terminal(
                    ctx.job,
                    Phase::Failed {
                        finished_at: now_rfc3339(),
                        failure: e.as_failure(),
                    },
                    None,
                    "none",
                    out,
                );
                Verdict::Resolve(e)
            }
        };
        self.promote(ctx.lane, out);
        (verdict, Fate::Continue)
    }

    // ## 終わらせる・取り消す

    /// 終わった状態にする。 期限の登録を外し、待っている側へ `resolve` を渡し、記録する。
    fn set_terminal(
        &mut self,
        id: JobId,
        phase: Phase,
        resolve: Option<JobError>,
        admission: &'static str,
        out: &mut Outbox,
    ) {
        let Some(entry) = self.jobs.get_mut(&id) else {
            return;
        };
        if let Some(at) = entry.deadline.take() {
            self.deadlines.remove(&(at, id, entry.attempt));
        }
        let (outcome, failure) = match &phase {
            Phase::Succeeded { .. } => ("succeeded", None),
            Phase::Failed { failure, .. } => ("failed", Some(*failure)),
            _ => ("cancelled", None),
        };
        let attempt = entry.attempt;
        entry.phase = phase;
        let now = Instant::now();
        entry.finished = Some(now);
        entry.queue = None;
        if let Some(err) = resolve {
            out.resolve(Arc::clone(&entry.resolver), err);
        }
        out.trace(self.event(id, admission, outcome, failure));
        self.finished.push_back((id, attempt));
        self.evict(now);
    }

    /// 待ちの列から外す。 勘定も戻す。
    fn detach_queued(&mut self, id: JobId) {
        let Some(entry) = self.jobs.get_mut(&id) else {
            return;
        };
        let lane = &mut self.lanes[entry.lane];
        if let Some((key, deferred)) = entry.queue.take() {
            if deferred {
                lane.deferred.remove(&key);
            } else if let Some(item) = lane.queue.remove(&key) {
                lane.inflight_bytes = lane.inflight_bytes.saturating_sub(item.bytes);
            }
        }
    }

    /// 仕事の今の試行を取り消す。 無ければ `None`。
    pub(crate) fn cancel_entry(
        &mut self,
        id: JobId,
        reason: Reason,
        out: &mut Outbox,
    ) -> Option<CancelEffect> {
        let entry = self.jobs.get_mut(&id)?;
        let lane_ix = entry.lane;
        match &entry.phase {
            Phase::Queued { .. } => {
                self.detach_queued(id);
                self.set_terminal(
                    id,
                    Phase::Cancelled {
                        finished_at: now_rfc3339(),
                        stopped: true,
                        reason,
                    },
                    Some(reason.to_error()),
                    "none",
                    out,
                );
                self.promote(lane_ix, out);
                Some(CancelEffect::Immediate)
            }
            Phase::Running { .. } => {
                entry.probe.cancel();
                if reason == Reason::Superseded {
                    // この表は新しい試行で置き換わる。 走っているスレッドは、
                    // 期限の監視のために孤児として覚える。
                    self.orphans.insert((id, entry.attempt), lane_ix);
                }
                if entry.cancel_requested.is_none() {
                    entry.cancel_requested = Some(reason);
                    // 待っている側は、計算が戻るのを待たずに解放する。
                    out.resolve(Arc::clone(&entry.resolver), reason.to_error());
                    out.trace(self.event(id, "none", "cancel_requested", None));
                }
                Some(CancelEffect::Signalled)
            }
            Phase::Computed { .. } => {
                // 計算は走り切っている。 結果は捨てる。
                self.set_terminal(
                    id,
                    Phase::Cancelled {
                        finished_at: now_rfc3339(),
                        stopped: false,
                        reason,
                    },
                    Some(reason.to_error()),
                    "none",
                    out,
                );
                Some(CancelEffect::Immediate)
            }
            Phase::Publishing { .. } => Some(CancelEffect::TooLate),
            Phase::Succeeded { receipt, .. } => Some(CancelEffect::Committed(*receipt)),
            Phase::Failed { .. } | Phase::Cancelled { .. } => Some(CancelEffect::Finished),
        }
    }

    /// 貸与が閉じたので、その貸与の仕事を全部取り消す。 取り消した（合図を立てた）数。
    pub(crate) fn cancel_lease(&mut self, lease: &ProjectLease, out: &mut Outbox) -> usize {
        let ids: Vec<JobId> = self
            .jobs
            .iter()
            .filter(|(_, e)| e.lease == *lease)
            .map(|(id, _)| *id)
            .collect();
        ids.into_iter()
            .filter(|id| {
                matches!(
                    self.cancel_entry(*id, Reason::LeaseClosed, out),
                    Some(CancelEffect::Immediate | CancelEffect::Signalled)
                )
            })
            .count()
    }

    /// 止めはじめる。 待ちは取り消し、走っている仕事へ合図を立てる。
    pub(crate) fn begin_shutdown(&mut self, out: &mut Outbox) {
        if self.shutting_down {
            return;
        }
        self.shutting_down = true;
        let ids: Vec<JobId> = self.jobs.keys().copied().collect();
        for id in ids {
            self.cancel_entry(id, Reason::Shutdown, out);
        }
    }

    // ## 期限

    /// 期限を迎えた試行を、失敗にする。 走っていたなら、代わりのスレッドが要る
    /// lane を返す（勘定は先に済ませてある。起こせなければ `replacement_failed`）。
    pub(crate) fn expire(
        &mut self,
        id: JobId,
        attempt: Attempt,
        out: &mut Outbox,
    ) -> Option<usize> {
        if let Some(lane_ix) = self.orphans.remove(&(id, attempt)) {
            return self.strike(lane_ix, id, attempt, out);
        }
        let entry = self.jobs.get_mut(&id)?;
        if entry.attempt != attempt {
            return None;
        }
        entry.deadline = None;
        let lane_ix = entry.lane;
        match &entry.phase {
            Phase::Queued { .. } => {
                self.detach_queued(id);
                let e = JobError::DeadlineExceeded;
                self.set_terminal(
                    id,
                    Phase::Failed {
                        finished_at: now_rfc3339(),
                        failure: e.as_failure(),
                    },
                    Some(e),
                    "none",
                    out,
                );
                self.promote(lane_ix, out);
                None
            }
            Phase::Running { .. } => {
                entry.probe.cancel();
                let cancel = entry.cancel_requested;
                let e = JobError::DeadlineExceeded;
                let (phase, resolve) = match cancel {
                    // 先に取消の合図を受けていた。 止められなかったことを正直に出す。
                    Some(reason) => (
                        Phase::Cancelled {
                            finished_at: now_rfc3339(),
                            stopped: false,
                            reason,
                        },
                        None,
                    ),
                    None => (
                        Phase::Failed {
                            finished_at: now_rfc3339(),
                            failure: e.as_failure(),
                        },
                        Some(e),
                    ),
                };
                self.set_terminal(id, phase, resolve, "none", out);
                self.strike(lane_ix, id, attempt, out)
            }
            _ => None,
        }
    }

    /// 走っている試行を、固まったと見なして lane から外す。 代わりのスレッドが要れば
    /// その lane を返す。 固まったスレッドが上限を超えたら、立てずに lane を閉じる。
    fn strike(
        &mut self,
        lane_ix: usize,
        id: JobId,
        attempt: Attempt,
        out: &mut Outbox,
    ) -> Option<usize> {
        let lane = &mut self.lanes[lane_ix];
        lane.running = lane.running.saturating_sub(1);
        lane.live = lane.live.saturating_sub(1);
        lane.hung.insert((id, attempt));
        if lane.hung.len() <= lane.max_hung {
            lane.live += 1;
            self.threads_alive += 1;
            Some(lane_ix)
        } else {
            self.drain_lane(lane_ix, out);
            None
        }
    }

    /// 代わりのスレッドを起こせなかった。
    pub(crate) fn replacement_failed(&mut self, lane_ix: usize, out: &mut Outbox) {
        let lane = &mut self.lanes[lane_ix];
        lane.live = lane.live.saturating_sub(1);
        self.threads_alive = self.threads_alive.saturating_sub(1);
        if !self.lanes[lane_ix].available() {
            self.drain_lane(lane_ix, out);
        }
    }

    /// 回せなくなった lane の待ちを、全部失敗にする。 取る者がいない待ちを
    /// 黙って溜めない。
    fn drain_lane(&mut self, lane_ix: usize, out: &mut Outbox) {
        let lane = &mut self.lanes[lane_ix];
        let queued = std::mem::take(&mut lane.queue);
        let deferred = std::mem::take(&mut lane.deferred);
        for item in queued.values() {
            lane.inflight_bytes = lane.inflight_bytes.saturating_sub(item.bytes);
        }
        let e = JobError::LaneUnavailable;
        for id in queued.values().chain(deferred.values()).map(|i| i.job) {
            if let Some(entry) = self.jobs.get_mut(&id) {
                entry.queue = None;
            }
            self.set_terminal(
                id,
                Phase::Failed {
                    finished_at: now_rfc3339(),
                    failure: e.as_failure(),
                },
                Some(e),
                "none",
                out,
            );
        }
    }

    /// 次に迎える期限。
    pub(crate) fn next_deadline(&self) -> Option<Instant> {
        self.deadlines.first().map(|(at, _, _)| *at)
    }

    /// 期限を迎えた試行を1つ取り出す。
    pub(crate) fn pop_expired(&mut self, now: Instant) -> Option<(JobId, Attempt)> {
        let &(at, id, attempt) = self.deadlines.first()?;
        if at > now {
            return None;
        }
        self.deadlines.pop_first();
        Some((id, attempt))
    }

    /// 作業スレッドが（止めているので）抜ける。
    pub(crate) fn thread_exited(&mut self, lane_ix: usize) {
        let lane = &mut self.lanes[lane_ix];
        lane.live = lane.live.saturating_sub(1);
        self.threads_alive = self.threads_alive.saturating_sub(1);
    }

    /// 監視のスレッドが抜ける。
    pub(crate) fn timer_exited(&mut self) {
        self.threads_alive = self.threads_alive.saturating_sub(1);
    }

    // ## 公開

    /// 公開を始められるか確かめ、始める（`Computed` → `Publishing`）。
    ///
    /// これが**受理の線形化点**。 取消が先に確定していないか・試行が最新か・
    /// 貸与の仕事がまだ有効か・読んだ入力が今も同じか、を表のロックの下で
    /// 1度に見る。
    pub(crate) fn begin_publish(
        &mut self,
        id: JobId,
        attempt: Attempt,
        lease: &ProjectLease,
        based_on: &BasedOn,
        current: &BasedOn,
        out: &mut Outbox,
    ) -> Result<(), PublishRefused> {
        if self.closed.contains(lease) {
            out.trace(self.event(id, "refused_lease_closed", "refused", None));
            return Err(PublishRefused::LeaseClosed);
        }
        let Some(entry) = self.jobs.get(&id) else {
            return Err(PublishRefused::Gone);
        };
        if entry.attempt != attempt {
            out.trace(self.event(id, "refused_superseded", "refused", None));
            return Err(PublishRefused::Superseded);
        }
        let started_at = match &entry.phase {
            Phase::Computed { started_at } => started_at.clone(),
            Phase::Cancelled { reason, .. } => {
                let (refusal, admission) = reason.to_refusal();
                out.trace(self.event(id, admission, "refused", None));
                return Err(refusal);
            }
            _ => return Err(PublishRefused::Gone),
        };
        if !based_on.is_current_in(current) {
            let failure = JobFailure::of(&PublishRefused::Stale, Outcome::NotCommitted);
            self.set_terminal(
                id,
                Phase::Failed {
                    finished_at: now_rfc3339(),
                    failure,
                },
                None,
                "refused_stale",
                out,
            );
            return Err(PublishRefused::Stale);
        }
        if let Some(e) = self.jobs.get_mut(&id) {
            e.phase = Phase::Publishing { started_at };
        }
        out.trace(self.event(id, "admitted", "publishing", None));
        Ok(())
    }

    /// 公開が済んだ（または失敗した）。
    pub(crate) fn finish_publish(
        &mut self,
        id: JobId,
        attempt: Attempt,
        result: Result<Receipt, JobFailure>,
        out: &mut Outbox,
    ) {
        let publishing = self
            .jobs
            .get(&id)
            .is_some_and(|e| e.attempt == attempt && matches!(e.phase, Phase::Publishing { .. }));
        if !publishing {
            return;
        }
        let phase = match result {
            Ok(receipt) => Phase::Succeeded {
                finished_at: now_rfc3339(),
                receipt,
            },
            Err(failure) => Phase::Failed {
                finished_at: now_rfc3339(),
                failure,
            },
        };
        self.set_terminal(id, phase, None, "none", out);
    }

    // ## 保持

    /// 保持の件数か期間を超えた、終わった仕事を表から外す。 計算が終わったまま
    /// 公開されない仕事も、期間を過ぎれば外す（公開は `Gone` で断る）。
    fn evict(&mut self, now: Instant) {
        while let Some(&(id, attempt)) = self.finished.front() {
            let Some(entry) = self.jobs.get(&id) else {
                self.finished.pop_front();
                continue;
            };
            if entry.attempt != attempt || !entry.phase.is_terminal() {
                // 新しい試行に置き換わった後の古い記録。
                self.finished.pop_front();
                continue;
            }
            let too_many = self.finished.len() > self.retain_max;
            let too_old = entry
                .finished
                .is_some_and(|f| now.saturating_duration_since(f) > self.retain_for);
            if !(too_many || too_old) {
                break;
            }
            self.finished.pop_front();
            self.jobs.remove(&id);
        }
        while let Some(&(id, attempt)) = self.computed.front() {
            let state = self.jobs.get(&id).map(|e| {
                (
                    e.attempt == attempt && matches!(e.phase, Phase::Computed { .. }),
                    e.finished,
                )
            });
            match state {
                None | Some((false, _)) => {
                    self.computed.pop_front();
                }
                Some((true, finished)) => {
                    if finished.is_some_and(|f| now.saturating_duration_since(f) > self.retain_for)
                    {
                        self.computed.pop_front();
                        self.jobs.remove(&id);
                    } else {
                        break;
                    }
                }
            }
        }
    }

    // ## 読み

    fn status_of(entry: &JobEntry) -> JobStatus {
        match &entry.phase {
            Phase::Queued { since } => JobStatus::Queued {
                since: since.clone(),
            },
            Phase::Running { started_at } => JobStatus::Running {
                started_at: started_at.clone(),
                progress: entry.probe.progress(),
            },
            // 計算は終わったが、成功は公開が確定したときだけ。
            Phase::Computed { started_at } | Phase::Publishing { started_at } => {
                JobStatus::Running {
                    started_at: started_at.clone(),
                    progress: Some(1.0),
                }
            }
            Phase::Succeeded {
                finished_at,
                receipt,
            } => JobStatus::Succeeded {
                finished_at: finished_at.clone(),
                receipt: *receipt,
            },
            Phase::Failed {
                finished_at,
                failure,
            } => JobStatus::Failed {
                finished_at: finished_at.clone(),
                failure: *failure,
            },
            Phase::Cancelled {
                finished_at,
                stopped,
                ..
            } => JobStatus::Cancelled {
                finished_at: finished_at.clone(),
                computation_stopped: *stopped,
            },
        }
    }

    fn snapshot_of(id: JobId, e: &JobEntry) -> JobSnapshot {
        JobSnapshot {
            id,
            kind: e.kind,
            priority: e.priority,
            attempt: e.attempt,
            lease: e.lease,
            status: Self::status_of(e),
        }
    }

    pub(crate) fn snapshot(&mut self, id: JobId) -> Option<JobSnapshot> {
        self.evict(Instant::now());
        self.jobs.get(&id).map(|e| Self::snapshot_of(id, e))
    }

    /// 終わっていない仕事。 積んだ順。
    pub(crate) fn active(&mut self, lease: &ProjectLease) -> Vec<JobSnapshot> {
        self.evict(Instant::now());
        let mut v: Vec<(u64, JobSnapshot)> = self
            .jobs
            .iter()
            .filter(|(_, e)| e.lease == *lease && !e.phase.is_terminal())
            .map(|(id, e)| (e.seq, Self::snapshot_of(*id, e)))
            .collect();
        v.sort_by_key(|(seq, _)| *seq);
        v.into_iter().map(|(_, s)| s).collect()
    }

    pub(crate) fn counts(&self) -> JobCounts {
        self.lanes
            .iter()
            .fold(JobCounts::default(), |acc, l| JobCounts {
                pending: acc.pending + l.queue.len() + l.deferred.len(),
                running: acc.running + l.running,
            })
    }

    pub(crate) fn lane_stats(&self) -> Vec<LaneStats> {
        self.lanes
            .iter()
            .map(|l| LaneStats {
                name: l.cfg.name,
                queued: l.queue.len(),
                deferred: l.deferred.len(),
                running: l.running,
                inflight_bytes: l.inflight_bytes,
                hung: l.hung.len(),
                threads: l.live,
            })
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Placement {
    Queue,
    Defer,
}
