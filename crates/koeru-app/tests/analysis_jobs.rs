//! テイクの解析が `JobRuntime` を通ること（`DEC-PLT-047`、T07b-2）。
//!
//! 確定の応答は当面、解析を待つ。 その間の仕事は `JobRuntime` の作業スレッドで走り、
//! 台帳へ置くのは公開（`JobRuntime::publish`）の `admit` だけ。 実音声なしで、次を確かめる。
//!
//! - 解析が `JobRuntime` を通り（`counts` / `job` で見える）、応答の値が以前と同じ形
//! - 取り消し・期限切れ・貸与の失効のとき、テイクは残り、`followup` に `jobs.*` が載る
//! - project を移ると旧貸与の仕事が取り消される（A → B → A）
//! - 待ち数が状態ロックの外から読める
//!
//! 計算は、台帳を読むだけの合成音。 構造しか見ない（`EVID-ALN-001`）。 前景の lane の作業
//! スレッドは1本なので、**その1本を関門で塞いでおけば、解析は必ず待ちに並ぶ**。 `sleep` に
//! 頼らず、塞ぐ・放すを試験の側から決める。
//!
//! `Studio::open` は MFA のモデルを読むので、書いていない OS 向けの組み立て
//! （`koeru_force_unsupported_backend`）では設計どおり失敗する（`DEC-ALN-016`）。
//! `tests/project_lifecycle.rs` と同じくモジュールごと外す。

#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
#![cfg(feature = "test-hooks")]

use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Barrier};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use koeru_app_lib::Studio;
use koeru_app_lib::commands::AppState;
use koeru_app_lib::studio::TakeResult;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_runtime::jobs::{
    CancelOutcome, JobError, JobId, JobKind, JobRuntime, JobSpec, JobStatus, Priority,
};
use koeru_runtime::{ProjectLease, RuntimeError};

/// 事象が必ず来る待ちの上限。 来なければ試験が止まらずに落ちる。
const LONG: Duration = Duration::from_secs(60);

fn opened(tag: &str) -> (Studio, PathBuf, String) {
    let root =
        std::env::temp_dir().join(format!("koeru-analysis-jobs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut s = Studio::open(root.clone()).expect("ライブラリを開ける");
    let id = s.create_project(tag).expect("作れる");
    s.open_project(id).expect("開ける");
    let row = s.rows_with_takes().expect("行を引ける")[0].row_id.clone();
    (s, root, row)
}

fn lease_of(s: &Studio) -> ProjectLease {
    *s.runtime().expect("開いている").lease()
}

/// モーラごとに「雑音の子音 + 倍音のある母音」を並べた合成音。 乱数は使わない。
fn voice(moras: usize) -> Vec<f32> {
    let rate = f64::from(MASTER_RATE_HZ);
    let silence = (0.4 * rate) as usize;
    let mut seed: u32 = 12345;
    let mut noise = || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 8) as f32 / 8_388_608.0 - 1.0
    };
    let mut out = vec![0.0_f32; silence];
    for m in 0..moras {
        let consonant = (0.10 * rate) as usize;
        for i in 0..consonant {
            out.push(noise() * (i as f32 / consonant as f32).min(1.0) * 0.15);
        }
        let vowel = (0.28 * rate) as usize;
        let f = 180.0 * (1.0 + 0.02 * m as f64);
        for i in 0..vowel {
            let t = i as f64 / rate;
            let env = (i as f64 / vowel as f64 * std::f64::consts::PI).sin();
            let v: f64 = (1..=6)
                .map(|h| (2.0 * std::f64::consts::PI * f * f64::from(h) * t).sin() / f64::from(h))
                .sum();
            out.push((v * env * 0.3) as f32);
        }
        out.extend(std::iter::repeat_n(0.0_f32, (0.06 * rate) as usize));
    }
    out.extend(std::iter::repeat_n(0.0_f32, silence));
    out
}

/// 前景の lane の作業スレッド（1本）を塞ぐ。 返る送り口で放す。
fn block_foreground(jobs: &JobRuntime, lease: ProjectLease) -> Sender<()> {
    block(jobs, lease, Priority::PostRecording)
}

/// `priority` を回す lane の作業スレッド（1本）を塞ぐ。
fn block(jobs: &JobRuntime, lease: ProjectLease, priority: Priority) -> Sender<()> {
    let (started_tx, started) = mpsc::channel();
    let (release, hold) = mpsc::channel::<()>();
    let _ticket = jobs
        .submit(JobSpec::new(
            lease,
            JobKind::Analysis,
            priority,
            move |_| {
                let _ = started_tx.send(());
                let _ = hold.recv_timeout(LONG);
                Ok(())
            },
        ))
        .expect("受け付けること");
    started.recv_timeout(LONG).expect("塞ぎが走り出すこと");
    release
}

/// 状態が必ずそうなる待ち。
fn eventually<T>(what: &str, mut probe: impl FnMut() -> Option<T>) -> T {
    let end = Instant::now() + LONG;
    loop {
        if let Some(v) = probe() {
            return v;
        }
        assert!(Instant::now() < end, "{what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// 待ちに並んだ解析の仕事。
fn queued_analysis(jobs: &JobRuntime, lease: &ProjectLease) -> JobId {
    eventually("解析が待ちに並ぶこと", || {
        jobs.active(lease)
            .into_iter()
            .find(|j| j.kind == JobKind::Analysis && matches!(j.status, JobStatus::Queued { .. }))
            .map(|j| j.id)
    })
}

type Derived = (Studio, TakeResult);

/// 解析まで進める呼び出しを別のスレッドに置く（待ちに並ぶ間、試験が動けるように）。
fn derive_in_thread(mut studio: Studio, row: String) -> JoinHandle<Derived> {
    std::thread::spawn(move || {
        let result = studio
            .test_derive_take(&row, voice)
            .expect("テイクを確定できる");
        (studio, result)
    })
}

fn join(handle: JoinHandle<Derived>) -> Derived {
    handle.join().expect("確定が panic しない")
}

/// 解析は `JobRuntime` を通る。 待ちに並んで見え、終われば `Succeeded`（受領証はテイクの ID）。
/// 応答の中身は以前と同じ形（oto・確信度・計測）で、失敗は載らない。
#[test]
fn 確定の解析は_jobruntime_を通り応答の形は変わらない() {
    let (studio, root, row) = opened("through");
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    let release = block_foreground(&jobs, lease);

    let handle = derive_in_thread(studio, row);
    let job = queued_analysis(&jobs, &lease);
    assert_eq!(
        jobs.counts().pending,
        1,
        "解析が1件待っている（待ち数に出る）"
    );
    let _ = release.send(());
    let (_studio, result) = join(handle);

    assert!(result.followup.is_none(), "{:?}", result.followup);
    assert!(result.oto.is_some(), "oto が付く");
    let confidence = result.confidence.expect("確信度が付く");
    assert!((0.0..=1.0).contains(&confidence), "{confidence}");
    assert!(result.metrics.peak_dbfs.is_finite());
    assert!(result.peak > 0.0);
    assert!(!result.thumbnail.is_empty());
    let JobStatus::Succeeded { receipt, .. } = jobs.job(job).expect("表にある").status else {
        panic!("成功になること");
    };
    assert_eq!(
        receipt.get(),
        u64::try_from(result.take_id).expect("正の ID")
    );
    assert_eq!(jobs.counts().pending, 0);

    let _ = std::fs::remove_dir_all(&root);
}

/// 再推定は背景の lane に載る。 前景の lane（解析）を塞がず、背景が塞がっていれば待つ。
/// 応答の形（`Result<()>`）は変わらない。
#[test]
fn 再推定は背景の_lane_に載り応答の形は変わらない() {
    let (mut studio, root, row) = opened("reestimate");
    let take = studio.test_derive_take(&row, voice).expect("確定できる");
    assert!(take.oto.is_some());
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    // 背景を塞ぐ。 前景は空いているので、解析は待たずに進めるはず。
    let release = block(&jobs, lease, Priority::Reestimate);

    let take_id = take.take_id;
    let handle = std::thread::spawn(move || {
        let r = studio.re_estimate_take(take_id);
        (studio, r)
    });
    let job = queued_analysis(&jobs, &lease);
    let lanes = jobs.lanes();
    assert_eq!(lanes[2].name, "background");
    assert_eq!(lanes[2].queued, 1, "再推定は背景の待ちに並ぶ");
    assert_eq!(lanes[0].queued, 0, "前景には並ばない");
    let _ = release.send(());
    let (_studio, result) = handle.join().expect("panic しない");
    result.expect("推定し直せる");
    assert!(matches!(
        jobs.job(job).expect("表にある").status,
        JobStatus::Succeeded { .. }
    ));

    let _ = std::fs::remove_dir_all(&root);
}

/// 同じ入力なら、同じ oto・確信度・計測になる（`TR-ALN-29`）。 作業スレッドで走っても
/// 結果は揺れない。
#[test]
fn 同じ入力なら作業スレッドで走っても同じ結果になる() {
    let results: Vec<TakeResult> = ["det-a", "det-b"]
        .into_iter()
        .map(|tag| {
            let (mut studio, root, row) = opened(tag);
            let result = studio.test_derive_take(&row, voice).expect("確定できる");
            let _ = std::fs::remove_dir_all(&root);
            result
        })
        .collect();
    assert!(results[0].oto.is_some());
    assert_eq!(results[0].oto, results[1].oto);
    assert_eq!(results[0].confidence, results[1].confidence);
    assert_eq!(results[0].metrics, results[1].metrics);
    assert_eq!(results[0].peak, results[1].peak);
}

/// 解析の仕事が取り消されても、テイクは確定したまま。 `oto` 無しで返り、`followup` に
/// `jobs.cancelled` が確定済みで載る。 画面へ返す値（波形・計測）は手元の標本から作る。
#[test]
fn 解析を取り消してもテイクは残り_followup_に_jobs_が載る() {
    let (studio, root, row) = opened("cancel");
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    let release = block_foreground(&jobs, lease);

    let handle = derive_in_thread(studio, row);
    let job = queued_analysis(&jobs, &lease);
    assert_eq!(jobs.cancel(job), CancelOutcome::Cancelled);
    let _ = release.send(());
    let (mut studio, result) = join(handle);

    let followup = result.followup.expect("取り消しが載る");
    assert_eq!(followup.code, "jobs.cancelled");
    assert_eq!(
        followup.outcome,
        koeru_app_lib::error::FailureOutcome::Committed
    );
    assert!(result.oto.is_none() && result.confidence.is_none());
    assert!(result.peak > 0.0, "画面へ返す値は手元の標本から作る");
    assert!(!result.thumbnail.is_empty());
    // テイクは台帳に残っている。
    studio
        .waveform_window(result.take_id, 0.0, 100.0, 8)
        .expect("確定したテイクを引ける");

    let _ = std::fs::remove_dir_all(&root);
}

/// 期限切れは失敗として返し、自動でやり直さない。 テイクは残る。
#[test]
fn 期限切れは失敗で返り_テイクは残り_やり直さない() {
    let (mut studio, root, row) = opened("deadline");
    studio.test_set_analysis_deadline(Duration::from_millis(50));
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    // 塞いだままなので、解析は待ちのうちに期限を迎える。
    let release = block_foreground(&jobs, lease);

    let (mut studio, result) = join(derive_in_thread(studio, row));
    let followup = result.followup.expect("期限切れが載る");
    assert_eq!(followup.code, "jobs.deadline_exceeded");
    assert!(result.oto.is_none());
    assert_eq!(jobs.counts().pending, 0, "やり直して積み直していない");
    studio
        .waveform_window(result.take_id, 0.0, 100.0, 8)
        .expect("確定したテイクを引ける");
    assert!(
        studio
            .take_pitch(result.take_id)
            .expect("失敗にしない")
            .is_none(),
        "解析が落ちたテイクの音高の記録は値で無いと返す"
    );
    let _ = release.send(());

    let _ = std::fs::remove_dir_all(&root);
}

/// 待っている間に貸与が失効（`cancel_lease`）したら、公開されず、`jobs.lease_closed` が載る。
#[test]
fn 貸与が失効した解析は公開されずテイクは残る() {
    let (studio, root, row) = opened("lease");
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    let release = block_foreground(&jobs, lease);

    let handle = derive_in_thread(studio, row);
    let _ = queued_analysis(&jobs, &lease);
    assert_eq!(jobs.cancel_lease(&lease), 2, "塞ぎと解析");
    let _ = release.send(());
    let (_studio, result) = join(handle);

    let followup = result.followup.expect("失効が載る");
    assert_eq!(followup.code, "jobs.lease_closed");
    assert!(result.oto.is_none());

    let _ = std::fs::remove_dir_all(&root);
}

/// A → B → A。 移ると旧貸与の仕事が取り消され、開き直した A へも書き戻されない。
#[test]
fn project_を移ると旧貸与の仕事が取り消される() {
    let root = std::env::temp_dir().join(format!("koeru-analysis-jobs-ab-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut s = Studio::open(root.clone()).expect("ライブラリを開ける");
    let a = s.create_project("A").expect("作れる");
    let b = s.create_project("B").expect("作れる");
    s.open_project(a).expect("開ける");
    let jobs = s.jobs();
    let lease_a = lease_of(&s);
    let held = s.runtime().expect("開いている");

    // A の仕事。 1件は走っていて（関門）、1件は待っている。
    let release = block_foreground(&jobs, lease_a);
    let (done_tx, done_rx) = mpsc::channel();
    let waiting = jobs
        .submit_notify(
            JobSpec::new(lease_a, JobKind::Analysis, Priority::PostRecording, |_| {
                Ok(1_u32)
            }),
            move |r| {
                let _ = done_tx.send(r.map(|c| c.into_value()));
            },
        )
        .expect("受け付けること");

    s.open_project(b).expect("B へ移れる");
    assert_eq!(
        done_rx.recv_timeout(LONG).expect("取消が届く"),
        Err(JobError::LeaseClosed),
        "待っていた A の仕事は外れる"
    );
    assert!(
        jobs.active(&lease_a).iter().all(|j| j.id != waiting.job()),
        "待ちに残らない"
    );

    // A を開き直す。 別の貸与。
    s.open_project(a).expect("A へ戻れる");
    let lease_a2 = lease_of(&s);
    assert_ne!(lease_a2, lease_a);
    // 旧貸与では、走っていた計算が戻っても書き戻せず、投入も断られる。
    let _ = release.send(());
    assert!(matches!(
        held.writer(&lease_a),
        Err(RuntimeError::LeaseExpired)
    ));
    let late = jobs
        .submit(JobSpec::new(
            lease_a,
            JobKind::Analysis,
            Priority::PostRecording,
            |_| Ok(()),
        ))
        .expect_err("閉じた貸与では投入できない");
    assert_eq!(late, JobError::LeaseClosed);
    // 開き直した A の貸与は使える。
    jobs.submit(JobSpec::new(
        lease_a2,
        JobKind::Analysis,
        Priority::PostRecording,
        |_| Ok(()),
    ))
    .expect("新しい貸与は受ける")
    .wait()
    .expect("終わる");

    let _ = std::fs::remove_dir_all(&root);
}

/// 待ち数は状態ロックの外から読める。 確定が状態ロックを握って解析を待つあいだも、
/// `pending_count` は返る（`TR-SYN-33`）。
#[test]
fn 待ち数は状態ロックの外から読める() {
    let (studio, root, row) = opened("lockfree");
    let jobs = studio.jobs();
    let lease = lease_of(&studio);
    let state = Arc::new(AppState::new(studio));
    let release = block_foreground(&jobs, lease);

    // 確定の呼び出しが状態ロックを握ったまま、解析の完了を待つ。
    let start = Arc::new(Barrier::new(2));
    let handle = {
        let (state, start) = (Arc::clone(&state), Arc::clone(&start));
        std::thread::spawn(move || {
            let mut guard = state.test_lock_studio();
            start.wait();
            guard
                .test_derive_take(&row, voice)
                .expect("テイクを確定できる")
        })
    };
    start.wait();
    let _ = queued_analysis(&jobs, &lease);

    // 状態ロックを取る読み方なら、ここで固まる。 返らなければ試験を落とす。
    let (tx, rx) = mpsc::channel();
    {
        let state = Arc::clone(&state);
        std::thread::spawn(move || {
            let _ = tx.send(state.pending_count());
        });
    }
    assert_eq!(
        rx.recv_timeout(LONG).expect("ロックを待たずに返ること"),
        1,
        "解析が1件待っている"
    );
    let _ = release.send(());
    let result = handle.join().expect("確定が panic しない");
    assert!(result.followup.is_none(), "{:?}", result.followup);
    assert_eq!(state.pending_count(), 0);

    let _ = std::fs::remove_dir_all(&root);
}
