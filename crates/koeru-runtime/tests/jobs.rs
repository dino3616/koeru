//! `JobRuntime` を、偽の長い計算で確かめる（T07a、`DEC-PLT-047`、`DEC-PLT-034`）。
//!
//! 計算はチャネルで止めて進める closure で、`sleep` に頼らない。 待つ場所は2種類だけ:
//! 事象が必ず来る待ち（`recv_timeout` / `JobTicket::wait`）と、状態が必ずそうなる
//! 待ち（`eventually`。上限つき）。 どちらも、結果は時刻の揺れで変わらない。
//!
//! 上限は「ちょうど」・0 件・1 件でも見る。 都合のよい値だけで書かない。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::{Duration, Instant};

use koeru_core::project::{Library, Manifest, Method, ProjectDir};
use koeru_failure::{Class, Failure, Outcome};
use koeru_runtime::jobs::{
    BasedOn, CancelOutcome, CancelProbe, CoalesceKey, Completed, ComputeError, JobConfig, JobError,
    JobFailure, JobId, JobKind, JobRuntime, JobSpec, JobStatus, JobTicket, LaneConfig,
    OverflowPolicy, Priority, PublishError, PublishRefused, Published, Receipt,
};
use koeru_runtime::{ProjectLease, ProjectRuntime, RuntimeError};

/// 事象が必ず来る待ちの上限。 来なければ試験が止まらずに落ちる。
const LONG: Duration = Duration::from_secs(10);

// ## 道具

/// 貸与は値だけが要るので、決まった UUID から作る（`ProjectRuntime` を開かない）。
fn lease(n: u32) -> ProjectLease {
    format!("00000000-0000-4000-8000-{n:012}")
        .parse()
        .expect("UUID の形")
}

/// 1本の lane が全部の優先度を回す。 優先度の順を、lane の分け方と切り離して見る。
fn single_lane(max_pending: usize, max_bytes: usize) -> JobConfig {
    JobConfig::new(vec![
        LaneConfig::new("all", Priority::ALL)
            .max_pending(max_pending)
            .max_inflight_bytes(max_bytes),
    ])
    .expect("成り立つ設定")
}

fn roomy() -> JobConfig {
    single_lane(64, 1 << 30)
}

/// 計算を止めておく関門。 `wait_started` で走り出したことを、`release` で進めることを、
/// 試験の側から決める。
struct Blocker {
    started: mpsc::Receiver<()>,
    release: mpsc::Sender<()>,
}

impl Blocker {
    fn wait_started(&self) {
        self.started.recv_timeout(LONG).expect("計算が走り出すこと");
    }

    fn release(&self) {
        let _ = self.release.send(());
    }
}

type Compute<T> = Box<dyn FnOnce(&CancelProbe) -> Result<T, ComputeError> + Send>;

/// 走り出したら知らせ、放されるまで止まる計算。 放されたら `value` を返す。
fn blocking<T: Send + 'static>(value: T) -> (Blocker, Compute<T>) {
    let (started_tx, started) = mpsc::channel();
    let (release, hold) = mpsc::channel::<()>();
    let compute = Box::new(move |_: &CancelProbe| {
        let _ = started_tx.send(());
        let _ = hold.recv_timeout(LONG);
        Ok(value)
    });
    (Blocker { started, release }, compute)
}

fn spec<T: Send + 'static>(
    lease: ProjectLease,
    priority: Priority,
    compute: impl FnOnce(&CancelProbe) -> Result<T, ComputeError> + Send + 'static,
) -> JobSpec<T> {
    JobSpec::new(lease, JobKind::Analysis, priority, compute)
}

fn quick(lease: ProjectLease, priority: Priority, value: u32) -> JobSpec<u32> {
    spec(lease, priority, move |_| Ok(value))
}

/// 走っている仕事を1件、lane に抱えさせる。 返る札は放すまで終わらない。
fn occupy(
    rt: &JobRuntime,
    lease: ProjectLease,
    priority: Priority,
) -> (Blocker, JobTicket<&'static str>) {
    let (blocker, compute) = blocking("blocker");
    let ticket = rt
        .submit(spec(lease, priority, compute))
        .expect("受け付けること");
    blocker.wait_started();
    (blocker, ticket)
}

/// 状態が必ずそうなる待ち。
fn eventually(what: &str, mut cond: impl FnMut() -> bool) {
    let end = Instant::now() + LONG;
    while !cond() {
        assert!(Instant::now() < end, "{what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

fn status_of(rt: &JobRuntime, job: JobId) -> JobStatus {
    rt.job(job).expect("表にあること").status
}

fn is_cancelled(rt: &JobRuntime, job: JobId) -> bool {
    matches!(status_of(rt, job), JobStatus::Cancelled { .. })
}

fn commit<T: Send + 'static>(
    rt: &JobRuntime,
    completed: Completed<T>,
    receipt: u64,
) -> Result<Published<()>, PublishError<JobError>> {
    rt.publish(completed, &BasedOn::new(), |_| {
        Ok(((), Receipt::new(receipt)))
    })
}

/// 値が落とされたことを見る印。
struct DropFlag(Arc<AtomicBool>);

impl Drop for DropFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::SeqCst);
    }
}

// ## 優先度と lane

#[test]
fn 優先度の高い順に回る() {
    let rt = JobRuntime::start(roomy());
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut tickets = Vec::new();
    // 低い方から積む。 順序が積んだ順でなく優先度で決まることを見る。
    for (p, name) in [
        (Priority::ChartPrecompute, "chart"),
        (Priority::Reestimate, "reestimate"),
        (Priority::PreviewRequest, "preview"),
        (Priority::PostRecording, "post"),
    ] {
        let order = Arc::clone(&order);
        tickets.push(
            rt.submit(spec(lease(1), p, move |_| {
                order.lock().expect("lock").push(name);
                Ok(())
            }))
            .expect("受け付けること"),
        );
    }
    blocker.release();
    first.wait().expect("終わる");
    for t in tickets {
        t.wait().expect("終わる");
    }
    assert_eq!(
        *order.lock().expect("lock"),
        ["post", "preview", "reestimate", "chart"]
    );
}

#[test]
fn 同じ優先度なら先に積んだ順() {
    let rt = JobRuntime::start(roomy());
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let order = Arc::new(Mutex::new(Vec::new()));
    let tickets: Vec<_> = (0..5)
        .map(|i| {
            let order = Arc::clone(&order);
            rt.submit(spec(lease(1), Priority::Reestimate, move |_| {
                order.lock().expect("lock").push(i);
                Ok(())
            }))
            .expect("受け付けること")
        })
        .collect();
    blocker.release();
    first.wait().expect("終わる");
    for t in tickets {
        t.wait().expect("終わる");
    }
    assert_eq!(*order.lock().expect("lock"), [0, 1, 2, 3, 4]);
}

/// 低い段は、高い段が途切れるまで待つ。 今の意味（`TR-SYN-34`）の帰結で、
/// 誰も待っていない仕事が待たされることを、受け入れたものとして固定する。
#[test]
fn 低い段は高い段が途切れるまで待つ() {
    let rt = JobRuntime::start(roomy());
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut tickets = Vec::new();
    // 先に積むのは低い段。 そのあとに高い段が2つ来る。
    for (p, name) in [
        (Priority::ChartPrecompute, "chart"),
        (Priority::PostRecording, "post1"),
        (Priority::PostRecording, "post2"),
    ] {
        let order = Arc::clone(&order);
        tickets.push(
            rt.submit(spec(lease(1), p, move |_| {
                order.lock().expect("lock").push(name);
                Ok(())
            }))
            .expect("受け付けること"),
        );
    }
    blocker.release();
    first.wait().expect("終わる");
    for t in tickets {
        t.wait().expect("終わる");
    }
    assert_eq!(*order.lock().expect("lock"), ["post1", "post2", "chart"]);
}

#[test]
fn 背景の長い仕事が前景を塞がない() {
    let rt = JobRuntime::start(JobConfig::default());
    let (blocker, long_chart) = occupy(&rt, lease(1), Priority::ChartPrecompute);
    // 背景の lane は塞がっている。 前景の解析は待たずに終わる。
    let done = rt
        .submit(quick(lease(1), Priority::PostRecording, 7))
        .expect("受け付けること")
        .wait()
        .expect("背景を待たずに終わる");
    assert_eq!(done.into_value(), 7);
    let preview = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 8))
        .expect("受け付けること")
        .wait()
        .expect("背景を待たずに終わる");
    assert_eq!(preview.into_value(), 8);
    let lanes = rt.lanes();
    assert_eq!(lanes[2].name, "background");
    assert_eq!(lanes[2].running, 1, "背景はまだ走っている");
    blocker.release();
    long_chart.wait().expect("終わる");
}

/// 既定の lane の分け方（`DEC-PLT-051`）。 どの優先度も、ちょうど1本の lane が回す。
#[test]
fn 既定の_lane_は前景_試唱_背景の3本で優先度が正しい_lane_に属する() {
    let rt = JobRuntime::start(JobConfig::default());
    let names: Vec<&str> = rt.lanes().iter().map(|l| l.name).collect();
    assert_eq!(names, ["foreground", "preview", "background"]);

    // 優先度ごとに、塞いだ lane の名前を見る。 塞いだ仕事は走っていて、
    // 他の lane は空のまま。
    for (priority, lane) in [
        (Priority::PostRecording, "foreground"),
        (Priority::PreviewRequest, "preview"),
        (Priority::Reestimate, "background"),
        (Priority::ChartPrecompute, "background"),
    ] {
        let (blocker, ticket) = occupy(&rt, lease(1), priority);
        let running: Vec<(&str, usize)> = rt.lanes().iter().map(|l| (l.name, l.running)).collect();
        for (name, n) in running {
            assert_eq!(
                n,
                usize::from(name == lane),
                "{priority:?} は {lane} で走る"
            );
        }
        blocker.release();
        ticket.wait().expect("終わる");
    }
}

/// 試唱は曲の長さの間ずっと走る。 その間も、前景の解析は待たずに走って終わる（`TR-SYN-34`
/// の並びが意味を持つのは同じ lane の中だけ。 lane どうしは並んで走る）。
#[test]
fn 試唱の長い仕事が前景の解析を塞がない() {
    let rt = JobRuntime::start(JobConfig::default());
    let (blocker, long_preview) = occupy(&rt, lease(1), Priority::PreviewRequest);
    // 試唱の lane は塞がっている。 前景の解析は待たずに終わる。
    let analysis = rt
        .submit(quick(lease(1), Priority::PostRecording, 7))
        .expect("受け付けること")
        .wait()
        .expect("試唱を待たずに終わる");
    assert_eq!(analysis.into_value(), 7);
    // 背景も同じ。
    let chart = rt
        .submit(quick(lease(1), Priority::ChartPrecompute, 9))
        .expect("受け付けること")
        .wait()
        .expect("試唱を待たずに終わる");
    assert_eq!(chart.into_value(), 9);
    let lanes = rt.lanes();
    assert_eq!(lanes[1].name, "preview");
    assert_eq!(lanes[1].running, 1, "試唱はまだ走っている");
    assert_eq!(lanes[0].running, 0);
    blocker.release();
    long_preview.wait().expect("終わる");
}

/// 逆も言える。 前景の長い仕事は、試唱を塞がない。
#[test]
fn 前景の長い仕事が試唱を塞がない() {
    let rt = JobRuntime::start(JobConfig::default());
    let (blocker, long_analysis) = occupy(&rt, lease(1), Priority::PostRecording);
    let preview = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 8))
        .expect("受け付けること")
        .wait()
        .expect("解析を待たずに終わる");
    assert_eq!(preview.into_value(), 8);
    blocker.release();
    long_analysis.wait().expect("終わる");
}

/// 区切りで合図を見る長い試唱は、取り消すと 200 ms 以内に抜ける（`TR-SYN-27`）。
/// 抜けられたので、状態は `computation_stopped: true` で、結果は公開されない。
#[test]
fn 試唱の仕事を取り消すと_200ms_以内に合図で抜ける() {
    let rt = JobRuntime::start(JobConfig::default());
    let (started_tx, started) = mpsc::channel();
    let ticket = rt
        .submit(spec::<()>(
            lease(1),
            Priority::PreviewRequest,
            move |probe| {
                let _ = started_tx.send(());
                // フレーズの区切りを、1ms の仕事に見立てる。
                loop {
                    probe.check()?;
                    std::thread::sleep(Duration::from_millis(1));
                }
            },
        ))
        .expect("受け付けること");
    let job = ticket.job();
    started.recv_timeout(LONG).expect("走り出す");

    let asked = Instant::now();
    assert_eq!(rt.cancel(job), CancelOutcome::Requested);
    eventually("計算が合図で抜ける", || is_cancelled(&rt, job));
    assert!(
        asked.elapsed() < Duration::from_millis(200),
        "取り消してから抜けるまで {:?}",
        asked.elapsed()
    );
    assert!(matches!(
        status_of(&rt, job),
        JobStatus::Cancelled {
            computation_stopped: true,
            ..
        }
    ));
    assert_eq!(
        ticket.wait().expect_err("待つ側は解放される"),
        JobError::Cancelled
    );
}

/// 新しい試唱は、古い試唱を取り消してから積む。 古いのが待ちにいれば、まとめられて消え
/// （`OverflowPolicy::Coalesce`）、走っていれば合図で抜けて、新しいのがその後に走る。
#[test]
fn 新しい試唱で古い試唱が取り消される() {
    let rt = JobRuntime::start(JobConfig::default());
    let key = || OverflowPolicy::Coalesce(CoalesceKey::new("preview_stream"));

    // 走っている古い試唱。 取り消すと合図で抜ける。
    let (started_tx, started) = mpsc::channel();
    let old = rt
        .submit(
            spec::<()>(lease(1), Priority::PreviewRequest, move |probe| {
                let _ = started_tx.send(());
                loop {
                    probe.check()?;
                    std::thread::sleep(Duration::from_millis(1));
                }
            })
            .overflow(key()),
        )
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("古い試唱が走り出す");

    // 待ちにいる、もう1つ古い試唱は、新しい試唱にまとめられる。
    let waiting = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 1).overflow(key()))
        .expect("受け付けること");
    let newest = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 2).overflow(key()))
        .expect("まとめて受ける");
    assert_eq!(
        waiting.wait().expect_err("待ちの古い試唱は置き換えられる"),
        JobError::Cancelled
    );

    // 走っている古い試唱を取り消す。 新しい試唱は、その後で走る。
    let old_job = old.job();
    assert_eq!(rt.cancel(old_job), CancelOutcome::Requested);
    assert_eq!(newest.wait().expect("新しい試唱は走る").into_value(), 2);
    eventually("古い試唱は取り消しで終わる", || {
        is_cancelled(&rt, old_job)
    });
    assert!(matches!(
        status_of(&rt, old_job),
        JobStatus::Cancelled {
            computation_stopped: true,
            ..
        }
    ));
}

// ## 試行

#[test]
fn 古い試行の完了は公開されない() {
    let rt = JobRuntime::start(roomy());
    let first = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    assert_eq!(first.attempt().get(), 1);
    let job = first.job();

    // 同じ仕事をやり直す。
    let second = rt
        .submit(quick(lease(1), Priority::PreviewRequest, 2).retry_of(job))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    assert_eq!((second.job(), second.attempt().get()), (job, 2));

    let refused = commit(&rt, first, 1).expect_err("古い試行は公開されない");
    assert!(matches!(
        refused,
        PublishError::Refused(PublishRefused::Superseded)
    ));
    let published = commit(&rt, second, 2).expect("最新の試行は公開できる");
    assert_eq!(published.receipt, Receipt::new(2));
}

#[test]
fn 走っている古い試行は置き換えで解放され試行ごとの合図で抜ける() {
    // 1本の作業スレッド。 古い試行が抜けないと、新しい試行は走れない。
    let rt = JobRuntime::start(roomy());
    let (started_tx, started) = mpsc::channel();
    let old = rt
        .submit(spec::<u32>(
            lease(1),
            Priority::PreviewRequest,
            move |probe| {
                let _ = started_tx.send(());
                loop {
                    probe.check()?;
                    std::thread::yield_now();
                }
            },
        ))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("走り出す");
    let job = old.job();

    let new = rt
        .submit(
            // 新しい試行の合図は、古い試行を止めた合図とは別。 立っていない。
            spec(lease(1), Priority::PreviewRequest, |probe| {
                probe.check()?;
                Ok(2_u32)
            })
            .retry_of(job),
        )
        .expect("受け付けること");
    assert_eq!(
        old.wait().expect_err("置き換えられた"),
        JobError::Superseded
    );
    let done = new.wait().expect("新しい試行は止まらない");
    assert_eq!(done.attempt().get(), 2);
    assert_eq!(done.into_value(), 2);
}

#[test]
fn 終わった仕事もやり直せるが無い仕事はやり直せない() {
    let rt = JobRuntime::start(roomy());
    let failing = rt
        .submit(spec::<u32>(lease(1), Priority::Reestimate, |_| {
            Err(ComputeError::failed(&JobError::Busy))
        }))
        .expect("受け付けること");
    let job = failing.job();
    failing.wait().expect_err("失敗する");
    let again = rt
        .submit(quick(lease(1), Priority::Reestimate, 5).retry_of(job))
        .expect("終わった仕事はやり直せる")
        .wait()
        .expect("終わる");
    assert_eq!(again.attempt().get(), 2);

    let unknown: JobId = "00000000-0000-4000-8000-0000000000aa"
        .parse()
        .expect("UUID の形");
    assert_eq!(
        rt.submit(quick(lease(1), Priority::Reestimate, 5).retry_of(unknown))
            .expect_err("無い仕事"),
        JobError::UnknownJob
    );
}

// ## 貸与（A→B→A）

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::AtomicU32;
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-runtime-jobs-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
    d
}

fn new_project(tag: &str) -> ProjectDir {
    let lib = Library::open(tmp(tag)).expect("開けること");
    let dir = lib
        .create(&Manifest {
            display_name: "テスト用の音源".to_owned(),
            method: Method::Single,
            item_count: 5,
            derived_from: None,
            preset_id: None,
            inventory_version: None,
        })
        .expect("作れること");
    // `Library::create` は `project.db` までは作らない。 `read_session.rs` と同じ手順。
    koeru_core::db::Ledger::open(dir.db_path()).expect("台帳を作れること");
    dir
}

#[test]
fn 貸与が_a_から_b_へ移り_a_に戻っても_a_の遅れた完了は公開されない() {
    let jobs = JobRuntime::start(JobConfig::default());
    let dir_a = new_project("a");
    let dir_b = new_project("b");
    let project_a = ProjectRuntime::open(dir_a.clone()).expect("開けること");
    let lease_a = *project_a.lease();

    // A の仕事。 1つは計算が終わって公開を待ち、1つは走っている。
    let waiting = jobs
        .submit(quick(lease_a, Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let (blocker, compute) = blocking(2_u32);
    let running = jobs
        .submit(spec(lease_a, Priority::PostRecording, compute))
        .expect("受け付けること");
    blocker.wait_started();

    // A を閉じて B へ移る。
    assert_eq!(
        jobs.cancel_lease(&lease_a),
        2,
        "走っている1件と、公開待ちの1件"
    );
    project_a.close(&lease_a).expect("閉じられること");
    // 走っていた方は、計算が戻るのを待たずに解放されている。 同じ lane の次の仕事が
    // 進めるよう、止めていた計算を戻す。 結果は公開されない。
    let running_job = running.job();
    assert_eq!(
        running.wait().expect_err("解放されている"),
        JobError::LeaseClosed
    );
    blocker.release();
    eventually(
        "走り切った結果は捨てられ、止められなかったと出る",
        || {
            matches!(
                status_of(&jobs, running_job),
                JobStatus::Cancelled {
                    computation_stopped: false,
                    ..
                }
            )
        },
    );
    let project_b = ProjectRuntime::open(dir_b).expect("開けること");
    let lease_b = *project_b.lease();

    // B の仕事は普通に公開できる。
    let on_b = jobs
        .submit(quick(lease_b, Priority::PostRecording, 3))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let published = jobs
        .publish(on_b, &BasedOn::new(), |c| {
            let _writer = project_b.writer(c.lease())?;
            Ok::<_, RuntimeError>(((), Receipt::new(30)))
        })
        .expect("B の仕事は公開できる");
    assert_eq!(published.receipt, Receipt::new(30));

    // A を開き直す。 別の貸与になる。
    let project_a2 = ProjectRuntime::open(dir_a).expect("開けること");
    let lease_a2 = *project_a2.lease();
    assert_ne!(lease_a2, lease_a);

    // 閉じたあとの A の貸与では、投入も断る。
    assert_eq!(
        jobs.submit(quick(lease_a, Priority::PostRecording, 9))
            .expect_err("閉じた貸与"),
        JobError::LeaseClosed
    );

    // A の遅れた完了。 公開待ちだった方は、開き直した A' へも書き戻せない。 `admit` は呼ばれない。
    let admitted = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&admitted);
    let refused = jobs
        .publish(waiting, &BasedOn::new(), move |c| {
            seen.store(true, Ordering::SeqCst);
            let _writer = project_a2.writer(c.lease())?;
            Ok::<_, RuntimeError>(((), Receipt::new(10)))
        })
        .expect_err("遅れた完了は公開されない");
    assert!(matches!(
        refused,
        PublishError::Refused(PublishRefused::LeaseClosed)
    ));
    assert!(!admitted.load(Ordering::SeqCst));
}

#[test]
fn 貸与の最終確認は_admit_の中の_writer_が担う() {
    let jobs = JobRuntime::start(JobConfig::default());
    let project = ProjectRuntime::open(new_project("final")).expect("開けること");
    let lease_now = *project.lease();
    let completed = jobs
        .submit(quick(lease_now, Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    // `JobRuntime` へ知らせずに、project だけが閉じた。
    project.close(&lease_now).expect("閉じられること");
    let refused = jobs
        .publish(completed, &BasedOn::new(), |c| {
            let _writer = project.writer(c.lease())?;
            Ok::<_, RuntimeError>(((), Receipt::new(1)))
        })
        .expect_err("writer が貸与の失効で断る");
    assert!(matches!(
        refused,
        PublishError::Admit(RuntimeError::LeaseExpired)
    ));
    let JobStatus::Failed { failure, .. } = status_of(&jobs, job) else {
        panic!("失敗になること");
    };
    assert_eq!(failure.code, "runtime.lease_expired");
    assert_eq!(failure.class, Class::Conflict);
}

#[test]
fn 貸与を閉じると待ちも走っているものも取り消す() {
    let rt = JobRuntime::start(roomy());
    let (blocker, running) = occupy(&rt, lease(1), Priority::PostRecording);
    let queued = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること");
    let other = rt
        .submit(quick(lease(2), Priority::PostRecording, 2))
        .expect("受け付けること");
    assert_eq!(rt.cancel_lease(&lease(1)), 2);
    assert_eq!(
        queued.wait().expect_err("取り消された"),
        JobError::LeaseClosed
    );
    assert_eq!(
        running.wait().expect_err("解放される"),
        JobError::LeaseClosed
    );
    // 別の貸与の仕事には触れない。
    blocker.release();
    assert_eq!(other.wait().expect("終わる").into_value(), 2);
    assert_eq!(rt.cancel_lease(&lease(1)), 0, "もう取り消すものが無い");
}

// ## 取消

#[test]
fn 待ち中の取消は計算を走らせず_computation_stopped_は_true() {
    let rt = JobRuntime::start(roomy());
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let queued = rt
        .submit(spec(lease(1), Priority::PostRecording, move |_| {
            flag.store(true, Ordering::SeqCst);
            Ok(())
        }))
        .expect("受け付けること");
    let job = queued.job();
    assert_eq!(rt.cancel(job), CancelOutcome::Cancelled);
    assert!(matches!(
        status_of(&rt, job),
        JobStatus::Cancelled {
            computation_stopped: true,
            ..
        }
    ));
    assert_eq!(
        queued.wait().expect_err("取り消された"),
        JobError::Cancelled
    );
    assert_eq!(rt.counts().pending, 0);
    blocker.release();
    first.wait().expect("終わる");
    assert!(!ran.load(Ordering::SeqCst), "走らなかった");
    assert_eq!(rt.cancel(job), CancelOutcome::AlreadyFinished);
}

#[test]
fn 走行中の取消は合図で抜ければ止められたと出す() {
    let rt = JobRuntime::start(roomy());
    let (started_tx, started) = mpsc::channel();
    let ticket = rt
        .submit(spec::<()>(
            lease(1),
            Priority::PostRecording,
            move |probe| {
                let _ = started_tx.send(());
                loop {
                    probe.check()?;
                    std::thread::yield_now();
                }
            },
        ))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("走り出す");
    let job = ticket.job();
    assert_eq!(rt.cancel(job), CancelOutcome::Requested);
    assert_eq!(ticket.wait().expect_err("解放される"), JobError::Cancelled);
    eventually(
        "計算が合図で抜けて止まった状態になる",
        || {
            matches!(
                status_of(&rt, job),
                JobStatus::Cancelled {
                    computation_stopped: true,
                    ..
                }
            )
        },
    );
}

#[test]
fn 走行中の取消で合図を見ずに走り切った結果は公開せず止められなかったと出す() {
    let rt = JobRuntime::start(roomy());
    let dropped = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&dropped);
    let (started_tx, started) = mpsc::channel();
    let (release, hold) = mpsc::channel::<()>();
    let ticket = rt
        .submit(spec(lease(1), Priority::PostRecording, move |_| {
            let _ = started_tx.send(());
            let _ = hold.recv_timeout(LONG);
            Ok(DropFlag(flag))
        }))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("走り出す");
    let job = ticket.job();
    assert_eq!(rt.cancel(job), CancelOutcome::Requested);
    assert_eq!(ticket.wait().expect_err("解放される"), JobError::Cancelled);
    assert!(
        matches!(status_of(&rt, job), JobStatus::Running { .. }),
        "計算が戻るまでは走っている"
    );
    let _ = release.send(());
    eventually("走り切って止められなかったと出る", || {
        matches!(
            status_of(&rt, job),
            JobStatus::Cancelled {
                computation_stopped: false,
                ..
            }
        )
    });
    eventually("結果は捨てられる", || {
        dropped.load(Ordering::SeqCst)
    });
}

#[test]
fn 計算が終わって公開を待つ結果を取り消すと公開は断られる() {
    let rt = JobRuntime::start(roomy());
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    assert_eq!(rt.cancel(job), CancelOutcome::Cancelled);
    assert!(matches!(
        status_of(&rt, job),
        JobStatus::Cancelled {
            computation_stopped: false,
            ..
        }
    ));
    let refused = commit(&rt, completed, 1).expect_err("取消が先に確定している");
    assert!(matches!(
        refused,
        PublishError::Refused(PublishRefused::Cancelled)
    ));
}

#[test]
fn 公開のあとの取消は_already_committed_で受領証が一致する() {
    let rt = JobRuntime::start(roomy());
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    let published = commit(&rt, completed, 42).expect("公開できる");
    assert_eq!(published.receipt, Receipt::new(42));
    // 成功済みを、取り消したと偽らない。
    assert_eq!(
        rt.cancel(job),
        CancelOutcome::AlreadyCommitted(Receipt::new(42))
    );
    assert!(matches!(
        status_of(&rt, job),
        JobStatus::Succeeded { receipt, .. } if receipt == Receipt::new(42)
    ));
    assert_eq!(
        rt.cancel(job),
        CancelOutcome::AlreadyCommitted(Receipt::new(42)),
        "何度でも同じ"
    );
}

#[test]
fn 公開の途中の取消は公開が終わるのを待って_already_committed() {
    let rt = JobRuntime::start(roomy());
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    let (admit_started_tx, admit_started) = mpsc::channel();
    let (admit_release, admit_hold) = mpsc::channel::<()>();
    let publisher = {
        let rt = Arc::clone(&rt);
        std::thread::spawn(move || {
            rt.publish(completed, &BasedOn::new(), move |_| {
                let _ = admit_started_tx.send(());
                let _ = admit_hold.recv_timeout(LONG);
                Ok::<_, JobError>(((), Receipt::new(77)))
            })
        })
    };
    admit_started.recv_timeout(LONG).expect("公開が始まる");
    assert!(
        matches!(status_of(&rt, job), JobStatus::Running { .. }),
        "公開の途中は成功と見せない"
    );
    let canceller = {
        let rt = Arc::clone(&rt);
        std::thread::spawn(move || rt.cancel(job))
    };
    let _ = admit_release.send(());
    let published = publisher.join().expect("join").expect("公開できる");
    assert_eq!(published.receipt, Receipt::new(77));
    assert_eq!(
        canceller.join().expect("join"),
        CancelOutcome::AlreadyCommitted(Receipt::new(77)),
        "取消は公開が決めた受領証を返す"
    );
}

#[test]
fn 失敗した仕事と無い仕事の取消() {
    let rt = JobRuntime::start(roomy());
    let failing = rt
        .submit(spec::<()>(lease(1), Priority::PostRecording, |_| {
            Err(ComputeError::failed(&JobError::Busy))
        }))
        .expect("受け付けること");
    let job = failing.job();
    failing.wait().expect_err("失敗する");
    assert_eq!(rt.cancel(job), CancelOutcome::AlreadyFinished);
    let unknown: JobId = "00000000-0000-4000-8000-0000000000bb"
        .parse()
        .expect("UUID の形");
    assert_eq!(rt.cancel(unknown), CancelOutcome::NotFound);
}

// ## 上限

#[test]
fn 同じ鍵の待ちは新しい仕事に置き換わる() {
    let rt = JobRuntime::start(single_lane(4, 1 << 30));
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let key = || OverflowPolicy::Coalesce(CoalesceKey::new("take-1"));
    let old = rt
        .submit(quick(lease(1), Priority::Reestimate, 1).overflow(key()))
        .expect("受け付けること");
    let other = rt
        .submit(
            quick(lease(1), Priority::Reestimate, 3)
                .overflow(OverflowPolicy::Coalesce(CoalesceKey::new("take-2"))),
        )
        .expect("受け付けること");
    let new = rt
        .submit(quick(lease(1), Priority::Reestimate, 2).overflow(key()))
        .expect("受け付けること");
    let old_job = old.job();
    assert!(matches!(
        status_of(&rt, old_job),
        JobStatus::Cancelled {
            computation_stopped: true,
            ..
        }
    ));
    assert_eq!(old.wait().expect_err("置き換えられた"), JobError::Cancelled);
    assert_eq!(rt.counts().pending, 2, "別の鍵と新しい仕事");
    blocker.release();
    first.wait().expect("終わる");
    assert_eq!(new.wait().expect("終わる").into_value(), 2);
    assert_eq!(other.wait().expect("終わる").into_value(), 3);
}

#[test]
fn まとめる相手が無く上限にも当たっていれば断る() {
    let rt = JobRuntime::start(single_lane(1, 1 << 30));
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let waiting = rt
        .submit(
            quick(lease(1), Priority::Reestimate, 1)
                .overflow(OverflowPolicy::Coalesce(CoalesceKey::new("a"))),
        )
        .expect("受け付けること");
    let busy = rt
        .submit(
            quick(lease(1), Priority::Reestimate, 2)
                .overflow(OverflowPolicy::Coalesce(CoalesceKey::new("b"))),
        )
        .expect_err("鍵が違えば置き換えず、上限で断る");
    assert_eq!(busy, JobError::Busy);
    blocker.release();
    first.wait().expect("終わる");
    waiting.wait().expect("終わる");
}

#[test]
fn 待ちの上限ちょうどまでは受け超えれば_busy() {
    let rt = JobRuntime::start(single_lane(1, 1 << 30));
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let waiting = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("上限ちょうどの1件は受ける");
    let busy = rt
        .submit(quick(lease(1), Priority::PostRecording, 2))
        .expect_err("2件目は断る");
    assert_eq!(busy, JobError::Busy);
    assert_eq!(busy.class(), Class::Busy);
    assert_eq!(busy.code(), "jobs.busy");
    assert_eq!(rt.counts().pending, 1);
    blocker.release();
    first.wait().expect("終わる");
    waiting.wait().expect("終わる");
    // 空けば、また受ける。
    eventually("待ちが空く", || rt.counts().pending == 0);
    rt.submit(quick(lease(1), Priority::PostRecording, 3))
        .expect("空けば受ける")
        .wait()
        .expect("終わる");
}

/// 作業スレッドが仕事を取る速さで、断るかどうかが変わらない。
#[test]
fn 取られる前の仕事は待ちに数えず可否は作業スレッドの速さで変わらない() {
    let rt = JobRuntime::start(single_lane(1, 1 << 30));
    // 走り出したかを待たずに積む。 1本目が待ちにいても走っていても、結果は同じ。
    let (a, ca) = blocking(1_u32);
    let (b, cb) = blocking(2_u32);
    let first = rt
        .submit(spec(lease(1), Priority::PostRecording, ca))
        .expect("1本目は受ける");
    let second = rt
        .submit(spec(lease(1), Priority::PostRecording, cb))
        .expect("2本目は受ける（1件待ち）");
    assert_eq!(
        rt.submit(quick(lease(1), Priority::PostRecording, 3))
            .expect_err("3本目は断る"),
        JobError::Busy
    );
    a.release();
    b.release();
    first.wait().expect("終わる");
    second.wait().expect("終わる");
}

#[test]
fn 脇に置く仕事は上限に当たっても失敗せず全部いつか走る() {
    let rt = JobRuntime::start(single_lane(1, 1 << 30));
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let order = Arc::new(Mutex::new(Vec::new()));
    let mut tickets = Vec::new();
    // 1件は上限の中。 残りの5件は上限を超えるが、失敗にしない。
    for i in 0..6 {
        let order = Arc::clone(&order);
        tickets.push(
            rt.submit(
                spec(lease(1), Priority::PostRecording, move |_| {
                    order.lock().expect("lock").push(i);
                    Ok(())
                })
                .overflow(OverflowPolicy::Defer),
            )
            .expect("脇に置く仕事は断らない"),
        );
    }
    let stats = rt.lanes()[0];
    assert_eq!((stats.queued, stats.deferred), (1, 5));
    assert_eq!(rt.counts().pending, 6, "脇に置いたものも待ちに数える");
    blocker.release();
    first.wait().expect("終わる");
    for t in tickets {
        t.wait().expect("全部いつか走る");
    }
    assert_eq!(*order.lock().expect("lock"), [0, 1, 2, 3, 4, 5]);
}

#[test]
fn 申告バイト数の上限ちょうどまでは受け超えれば断る() {
    let rt = JobRuntime::start(single_lane(64, 100));
    let (b60, c60) = blocking(());
    let first = rt
        .submit(spec(lease(1), Priority::PostRecording, c60).input_bytes(60))
        .expect("受け付けること");
    b60.wait_started();
    let (b40, c40) = blocking(());
    let second = rt
        .submit(spec(lease(1), Priority::PostRecording, c40).input_bytes(40))
        .expect("60 + 40 = 100 は上限ちょうど");
    assert_eq!(rt.lanes()[0].inflight_bytes, 100);
    assert_eq!(
        rt.submit(quick(lease(1), Priority::PostRecording, 1).input_bytes(1))
            .expect_err("101 は超える"),
        JobError::Busy
    );
    // 申告が 0 の仕事は、バイト数の上限に触れない。
    let zero = rt
        .submit(quick(lease(1), Priority::PostRecording, 2))
        .expect("0 バイトは上限のうち");
    b60.release();
    b40.release();
    first.wait().expect("終わる");
    second.wait().expect("終わる");
    zero.wait().expect("終わる");
    eventually("勘定が戻る", || rt.lanes()[0].inflight_bytes == 0);
}

#[test]
fn 何も走っていない_lane_は上限より大きい仕事を1つだけ受ける() {
    let rt = JobRuntime::start(single_lane(64, 100));
    let (big_blocker, big_compute) = blocking(());
    let big = rt
        .submit(spec(lease(1), Priority::PostRecording, big_compute).input_bytes(500))
        .expect("空の lane は大きい仕事も受ける");
    big_blocker.wait_started();
    assert_eq!(
        rt.submit(quick(lease(1), Priority::PostRecording, 1).input_bytes(1))
            .expect_err("大きい仕事が走っている間は次を受けない"),
        JobError::Busy
    );
    big_blocker.release();
    big.wait().expect("終わる");
    eventually("勘定が戻る", || rt.lanes()[0].inflight_bytes == 0);
    rt.submit(quick(lease(1), Priority::PostRecording, 1).input_bytes(1))
        .expect("終われば受ける")
        .wait()
        .expect("終わる");
}

#[test]
fn バイト数で脇に置いた仕事は空けば昇格して走る() {
    let rt = JobRuntime::start(single_lane(64, 100));
    let (blocker, compute) = blocking(());
    let first = rt
        .submit(spec(lease(1), Priority::PostRecording, compute).input_bytes(100))
        .expect("受け付けること");
    blocker.wait_started();
    let deferred = rt
        .submit(
            quick(lease(1), Priority::PostRecording, 9)
                .input_bytes(50)
                .overflow(OverflowPolicy::Defer),
        )
        .expect("脇に置く");
    assert_eq!(rt.lanes()[0].deferred, 1);
    assert_eq!(
        rt.lanes()[0].inflight_bytes,
        100,
        "脇のものは勘定に入らない"
    );
    blocker.release();
    first.wait().expect("終わる");
    assert_eq!(deferred.wait().expect("昇格して走る").into_value(), 9);
}

// ## 期限

#[test]
fn 走っている仕事の期限切れは失敗で代わりのスレッドが次を回し遅れた結果は公開されない() {
    let rt = JobRuntime::start(roomy());
    let calls = Arc::new(AtomicUsize::new(0));
    let dropped = Arc::new(AtomicBool::new(false));
    let (started_tx, started) = mpsc::channel();
    let (release, hold) = mpsc::channel::<()>();
    let hung = {
        let (calls, flag) = (Arc::clone(&calls), Arc::clone(&dropped));
        rt.submit(
            spec(lease(1), Priority::PostRecording, move |_| {
                calls.fetch_add(1, Ordering::SeqCst);
                let _ = started_tx.send(());
                let _ = hold.recv_timeout(LONG);
                Ok(DropFlag(flag))
            })
            .deadline(Duration::from_millis(30)),
        )
        .expect("受け付けること")
    };
    started.recv_timeout(LONG).expect("走り出す");
    let job = hung.job();
    let error = hung.wait().expect_err("期限を過ぎる");
    assert_eq!(error, JobError::DeadlineExceeded);
    assert_eq!(error.code(), "jobs.deadline_exceeded");
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("失敗になること");
    };
    assert_eq!(failure.code, "jobs.deadline_exceeded");
    assert_eq!(rt.lanes()[0].hung, 1);

    // 固まった呼び出しはそのまま。 代わりのスレッドが次の仕事を回す。
    let next = rt
        .submit(quick(lease(1), Priority::PostRecording, 5))
        .expect("受け付けること")
        .wait()
        .expect("代わりのスレッドで回る");
    assert_eq!(next.into_value(), 5);

    // 遅れて戻っても、結果は公開されず、やり直されない。
    let _ = release.send(());
    eventually("遅れた結果は捨てられる", || {
        dropped.load(Ordering::SeqCst)
    });
    eventually(
        "固まったスレッドが戻って勘定が戻る",
        || {
            let lane = rt.lanes()[0];
            lane.hung == 0 && lane.threads == 1
        },
    );
    assert!(matches!(status_of(&rt, job), JobStatus::Failed { .. }));
    assert_eq!(calls.load(Ordering::SeqCst), 1, "自動でやり直さない");
}

#[test]
fn 待っている間の期限切れは計算を走らせず固まったスレッドも作らない() {
    let rt = JobRuntime::start(roomy());
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let late = rt
        .submit(
            spec(lease(1), Priority::PostRecording, move |_| {
                flag.store(true, Ordering::SeqCst);
                Ok(())
            })
            .deadline(Duration::from_millis(20)),
        )
        .expect("受け付けること");
    assert_eq!(
        late.wait().expect_err("待っている間に期限を過ぎる"),
        JobError::DeadlineExceeded
    );
    assert_eq!(rt.counts().pending, 0);
    assert_eq!(rt.lanes()[0].hung, 0);
    blocker.release();
    first.wait().expect("終わる");
    assert!(!ran.load(Ordering::SeqCst));
}

#[test]
fn 固まったスレッドが上限を超えた_lane_は仕事を断り戻れば受ける() {
    let rt = JobRuntime::start(roomy().max_hung_per_lane(1));
    // 1本目が固まる。 上限の範囲なので、代わりのスレッドが立つ。
    let (b1, c1) = blocking(());
    let t1 = rt
        .submit(spec(lease(1), Priority::PostRecording, c1).deadline(Duration::from_millis(20)))
        .expect("受け付けること");
    b1.wait_started();
    assert_eq!(t1.wait().expect_err("期限"), JobError::DeadlineExceeded);

    // 2本目も固まる。 上限を超える。 代わりは立たず、待っている仕事は失敗にする。
    let (b2, c2) = blocking(());
    let t2 = rt
        .submit(spec(lease(1), Priority::PostRecording, c2).deadline(Duration::from_millis(20)))
        .expect("受け付けること");
    b2.wait_started();
    let queued = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("2本目が走っている間は受ける");
    assert_eq!(t2.wait().expect_err("期限"), JobError::DeadlineExceeded);
    assert_eq!(
        queued.wait().expect_err("取る者がいない待ちは失敗にする"),
        JobError::LaneUnavailable
    );
    assert_eq!(
        rt.submit(quick(lease(1), Priority::PostRecording, 2))
            .expect_err("固まったスレッドが多すぎる lane は断る"),
        JobError::LaneUnavailable
    );
    assert_eq!(rt.lanes()[0].threads, 0);

    // 固まっていた呼び出しが1つ戻れば、スレッドが仕事に戻り、また受けられる。
    b1.release();
    eventually("lane が戻る", || {
        rt.submit(quick(lease(1), Priority::PostRecording, 3))
            .is_ok_and(|t| t.wait().is_ok())
    });
    b2.release();
    eventually("勘定が戻る", || {
        let lane = rt.lanes()[0];
        lane.hung == 0 && lane.threads == 1
    });
}

// ## 入力の印

#[test]
fn 読んだ入力が変わっていれば_stale_でエンジンの失敗ではない() {
    let rt = JobRuntime::start(roomy());
    let based = BasedOn::new().with("asset", "a1").with("rules", "r1");
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1).based_on(based))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    let admitted = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&admitted);
    let current = BasedOn::new().with("asset", "a2").with("rules", "r1");
    let refused = rt
        .publish(completed, &current, move |_| {
            seen.store(true, Ordering::SeqCst);
            Ok::<_, JobError>(((), Receipt::new(1)))
        })
        .expect_err("入力が変わっている");
    let PublishError::Refused(reason) = refused else {
        panic!("公開を始める前に断ること");
    };
    assert_eq!(reason, PublishRefused::Stale);
    assert_eq!(reason.class(), Class::Conflict);
    assert_ne!(reason.class(), Class::EngineFailed);
    assert!(!admitted.load(Ordering::SeqCst));
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("古い結果は失敗として残る");
    };
    assert_eq!(failure.code, "jobs.publish_stale");
    assert_eq!(failure.class, Class::Conflict);
    assert_eq!(failure.outcome, Outcome::NotCommitted);
}

#[test]
fn 読んでいない入力の変化では古くならず_消えた入力は古い() {
    let rt = JobRuntime::start(roomy());
    let based = BasedOn::new().with("asset", "a1");

    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1).based_on(based.clone()))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    // 無関係な編集（project 全体の版だけが進んだ）。 捨てない。
    let current = BasedOn::new().with("asset", "a1").with("settings", "s9");
    rt.publish(completed, &current, |_| {
        Ok::<_, JobError>(((), Receipt::new(1)))
    })
    .expect("読んでいない入力が変わっても公開できる");

    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 2).based_on(based))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let refused = rt
        .publish(completed, &BasedOn::new(), |_| {
            Ok::<_, JobError>(((), Receipt::new(2)))
        })
        .expect_err("入力が消えている");
    assert!(matches!(
        refused,
        PublishError::Refused(PublishRefused::Stale)
    ));
}

#[test]
fn 公開の書き込みが失敗した仕事は失敗で残る() {
    let rt = JobRuntime::start(roomy());
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    let failed = rt
        .publish(completed, &BasedOn::new(), |_| {
            Err::<((), Receipt), _>(JobError::Busy)
        })
        .expect_err("書き込みが失敗した");
    assert!(matches!(failed, PublishError::Admit(JobError::Busy)));
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("失敗になること");
    };
    assert_eq!(failure.code, "jobs.busy");
    assert_eq!(rt.cancel(job), CancelOutcome::AlreadyFinished);
}

// ## 失敗

#[test]
fn 計算の失敗は名乗った_code_のまま自動でやり直さない() {
    let rt = JobRuntime::start(roomy());
    let calls = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&calls);
    let ticket = rt
        .submit(spec::<()>(lease(1), Priority::Reestimate, move |_| {
            counter.fetch_add(1, Ordering::SeqCst);
            Err(ComputeError::Failed(JobFailure {
                code: "align.model_missing",
                class: Class::Unsupported,
                outcome: Outcome::NotCommitted,
            }))
        }))
        .expect("受け付けること");
    let job = ticket.job();
    let error = ticket.wait().expect_err("失敗する");
    assert_eq!(error.code(), "align.model_missing");
    assert_eq!(error.class(), Class::Unsupported);
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("失敗になること");
    };
    assert_eq!(
        failure.outcome,
        Outcome::NotCommitted,
        "計算は台帳へ書かない"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn 計算が_panic_しても作業スレッドは次の仕事を回す() {
    let rt = JobRuntime::start(roomy());
    let ticket = rt
        .submit(spec::<()>(lease(1), Priority::PostRecording, |_| {
            panic!("計算の panic")
        }))
        .expect("受け付けること");
    assert_eq!(
        ticket.wait().expect_err("異常終了"),
        JobError::ComputePanicked
    );
    let next = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("同じ lane が次を回す");
    assert_eq!(next.into_value(), 1);
}

/// 計算が panic しても `wait` は返る（永遠に待たない）。 仕事は `Failed` で残り、
/// code は `jobs.compute_panicked`。 **回復できるのは Rust の panic だけ**——C++ の
/// segfault などは `catch_unwind` では止められず、プロセスごと落ちる
/// （`DEC-PLT-047` の見直しの条件。 別プロセスへ移す根拠になる）。
#[test]
fn 計算が_panic_しても_wait_は返り仕事は失敗で残る() {
    let rt = JobRuntime::start(roomy());
    let ticket = rt
        .submit(spec::<()>(lease(1), Priority::PostRecording, |_| {
            panic!("計算の panic")
        }))
        .expect("受け付けること");
    let job = ticket.job();
    let (tx, rx) = mpsc::channel();
    // 待つ側を別のスレッドに置き、返らなければ試験が止まらずに落ちるようにする。
    std::thread::spawn(move || {
        let _ = tx.send(ticket.wait().map(|_| ()));
    });
    let waited = rx.recv_timeout(LONG).expect("wait が返ること");
    assert_eq!(waited, Err(JobError::ComputePanicked));
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("失敗になること");
    };
    assert_eq!(failure.code, "jobs.compute_panicked");
    assert_eq!(failure.class, Class::Internal);
    assert_eq!(rt.counts().running, 0, "走っている勘定が戻る");
    assert_eq!(rt.lanes()[0].threads, 1, "作業スレッドは生きている");
}

/// 公開の途中で `admit` が panic しても、仕事を公開中のまま残さない。
#[test]
fn admit_が_panic_しても仕事は公開中のまま残らない() {
    let rt = JobRuntime::start(roomy());
    let completed = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること")
        .wait()
        .expect("終わる");
    let job = completed.job();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = rt.publish(completed, &BasedOn::new(), |_| {
            panic!("admit の panic");
            #[allow(unreachable_code)]
            Ok::<_, JobError>(((), Receipt::new(1)))
        });
    }));
    assert!(caught.is_err(), "panic は呼び出し側へ伝わる");
    let JobStatus::Failed { failure, .. } = status_of(&rt, job) else {
        panic!("失敗になること");
    };
    assert_eq!(failure.code, "jobs.publish_aborted");
    assert_eq!(failure.outcome, Outcome::Unknown, "書いたかは分からない");
    assert_eq!(rt.cancel(job), CancelOutcome::AlreadyFinished);
}

// ## 停止

#[test]
fn 止めても固まった計算を待たずに抜ける() {
    let rt = JobRuntime::start(roomy());
    let (blocker, running) = occupy(&rt, lease(1), Priority::PostRecording);
    let queued = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること");
    let began = Instant::now();
    rt.shutdown();
    // 計算は `LONG`（10 秒）止まっている。 待っていれば、ここで超える。
    assert!(
        began.elapsed() < LONG / 2,
        "走っている計算を待たずに戻ること: {:?}",
        began.elapsed()
    );
    assert_eq!(
        queued.wait().expect_err("待ちは取り消す"),
        JobError::Cancelled
    );
    assert_eq!(
        running.wait().expect_err("走っているものは解放する"),
        JobError::Cancelled
    );
    assert_eq!(
        rt.submit(quick(lease(1), Priority::PostRecording, 2))
            .expect_err("止めたあとは受けない"),
        JobError::ShuttingDown
    );
    // 上限つきで待つ口は、固まったスレッドがいれば `false`。
    assert!(!rt.shutdown_and_wait(Duration::from_millis(30)));
    blocker.release();
    assert!(rt.shutdown_and_wait(LONG), "戻れば全部抜ける");
}

#[test]
fn 落とすと待ちを取り消し走っている仕事へ合図を立てる() {
    let rt = JobRuntime::start(roomy());
    let (exited_tx, exited) = mpsc::channel();
    let (started_tx, started) = mpsc::channel();
    let running = rt
        .submit(spec::<()>(
            lease(1),
            Priority::PostRecording,
            move |probe| {
                let _ = started_tx.send(());
                let r = loop {
                    if let Err(c) = probe.check() {
                        break Err(c);
                    }
                    std::thread::yield_now();
                };
                let _ = exited_tx.send(());
                r.map_err(ComputeError::from)
            },
        ))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("走り出す");
    let queued = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること");
    drop(rt);
    exited.recv_timeout(LONG).expect("合図で抜ける");
    assert_eq!(
        queued.wait().expect_err("取り消される"),
        JobError::Cancelled
    );
    assert_eq!(running.wait().expect_err("解放される"), JobError::Cancelled);
}

// ## 受け取り方と読み

#[test]
fn 完了を作業スレッドで回す仕事は成功も取消も_callback_で受ける() {
    let rt = JobRuntime::start(roomy());
    let (tx, rx) = mpsc::channel();
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);

    let ok_tx = tx.clone();
    rt.submit_notify(quick(lease(1), Priority::PostRecording, 11), move |r| {
        let thread = std::thread::current().name().map(str::to_owned);
        let _ = ok_tx.send((r.map(Completed::into_value), thread));
    })
    .expect("受け付けること");
    let cancel_tx = tx.clone();
    let cancelled = rt
        .submit_notify(quick(lease(1), Priority::PostRecording, 12), move |r| {
            let _ = cancel_tx.send((r.map(Completed::into_value), None));
        })
        .expect("受け付けること");
    assert_eq!(rt.cancel(cancelled.job()), CancelOutcome::Cancelled);
    let (r, _) = rx.recv_timeout(LONG).expect("取消が callback へ届く");
    assert_eq!(r, Err(JobError::Cancelled));

    blocker.release();
    first.wait().expect("終わる");
    let (r, thread) = rx.recv_timeout(LONG).expect("成功が callback へ届く");
    assert_eq!(r, Ok(11));
    assert!(
        thread.is_some_and(|n| n.starts_with("koeru-jobs-")),
        "作業スレッドで呼ばれる"
    );
}

#[test]
fn 待ちと走行の数を読める() {
    let rt = JobRuntime::start(roomy());
    let counts = rt.counts();
    assert_eq!((counts.pending, counts.running), (0, 0), "何も無い");
    let (blocker, first) = occupy(&rt, lease(1), Priority::PostRecording);
    let tickets: Vec<_> = (0..2)
        .map(|i| {
            rt.submit(quick(lease(1), Priority::PostRecording, i))
                .expect("受け付けること")
        })
        .collect();
    // 走っている間も読める。
    let counts = rt.counts();
    assert_eq!((counts.pending, counts.running), (2, 1));
    blocker.release();
    first.wait().expect("終わる");
    for t in tickets {
        t.wait().expect("終わる");
    }
    eventually("全部終わる", || {
        let c = rt.counts();
        (c.pending, c.running) == (0, 0)
    });
}

#[test]
fn 走っている仕事の進捗と終わっていない仕事の一覧() {
    let rt = JobRuntime::start(roomy());
    let (started_tx, started) = mpsc::channel();
    let (release, hold) = mpsc::channel::<()>();
    let running = rt
        .submit(spec(lease(1), Priority::PostRecording, move |probe| {
            probe.report_progress(0.5);
            let _ = started_tx.send(());
            let _ = hold.recv_timeout(LONG);
            Ok(())
        }))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("走り出す");
    let queued = rt
        .submit(quick(lease(1), Priority::PostRecording, 1))
        .expect("受け付けること");
    let elsewhere = rt
        .submit(quick(lease(2), Priority::Reestimate, 2))
        .expect("受け付けること");

    assert!(matches!(
        status_of(&rt, running.job()),
        JobStatus::Running { progress: Some(p), .. } if (p - 0.5).abs() < f32::EPSILON
    ));
    let active = rt.active(&lease(1));
    assert_eq!(
        active.iter().map(|s| s.id).collect::<Vec<_>>(),
        [running.job(), queued.job()],
        "その貸与の終わっていない仕事だけを、積んだ順に"
    );
    assert!(matches!(active[1].status, JobStatus::Queued { .. }));
    assert!(!active.iter().any(|s| s.id == elsewhere.job()));

    let _ = release.send(());
    running.wait().expect("終わる");
    queued.wait().expect("終わる");
    elsewhere.wait().expect("終わる");
}

#[test]
fn 終わった仕事は保持の件数と期間を過ぎれば引けない() {
    // 件数。 古い方から消える。
    let rt = JobRuntime::start(roomy().retention(2, Duration::from_secs(600)));
    let ids: Vec<JobId> = (0..4)
        .map(|_| {
            let t = rt
                .submit(spec::<()>(lease(1), Priority::Reestimate, |_| {
                    Err(ComputeError::failed(&JobError::Busy))
                }))
                .expect("受け付けること");
            let id = t.job();
            t.wait().expect_err("失敗する");
            id
        })
        .collect();
    assert!(rt.job(ids[0]).is_none(), "古い方は引けない");
    assert!(rt.job(ids[1]).is_none());
    assert!(rt.job(ids[2]).is_some());
    assert!(rt.job(ids[3]).is_some(), "新しい方は引ける");

    // 期間。
    let rt = JobRuntime::start(roomy().retention(100, Duration::ZERO));
    let t = rt
        .submit(spec::<()>(lease(1), Priority::Reestimate, |_| {
            Err(ComputeError::failed(&JobError::Busy))
        }))
        .expect("受け付けること");
    let id = t.job();
    t.wait().expect_err("失敗する");
    eventually("期間を過ぎれば引けない", || rt.job(id).is_none());
}

// ## 型の約束

/// 計算の closure と公開の型に、台帳の書き込み権限が出てこない。
///
/// 型では「closure が `Arc<ProjectRuntime>` を自分で捕まえる」ことまでは止められない。
/// 止められるのは、`JobRuntime` の側が書き手へ繋がる型を持たず、引かないこと。
/// `src/jobs/` を読んで、台帳・writer・`ProjectRuntime` を引いていないことを見る
/// （コメントは除く。コメントは禁止の理由を書くために名指しする）。
#[test]
fn 仕事の実行域は台帳と_writer_を引かない() {
    const FORBIDDEN: [&str; 4] = ["ProjectRuntime", "WriterGuard", "koeru_core", "Ledger"];
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/jobs");
    let mut files = 0_usize;
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&dir).expect("src/jobs を読めること") {
        let path = entry.expect("読めること").path();
        if path.extension().is_none_or(|e| e != "rs") {
            continue;
        }
        files += 1;
        let text = std::fs::read_to_string(&path).expect("読めること");
        for (i, line) in text.lines().enumerate() {
            if line.trim_start().starts_with("//") {
                continue;
            }
            for word in FORBIDDEN {
                if line.contains(word) {
                    offenders.push(format!("{}:{}: {word}", path.display(), i + 1));
                }
            }
        }
    }
    assert!(files >= 8, "src/jobs の走査が {files} 件しか読めていない");
    assert!(
        offenders.is_empty(),
        "書き手へ繋がる型を引いている: {offenders:?}"
    );
}
