//! 録音の保存の手順を、本物の SQLite（WAL）とファイルで確かめる（`DEC-REC-010`、
//! `project-storage.fsl`）。
//!
//! WAV は `koeru-audio` の `PartialTake` と同じ順序で書く（`.wav.part` → fsync → rename →
//! ディレクトリの fsync）。 中身は見ない。 見るのは、段ごとに落ちたあと開き直したときに
//! 台帳とファイルが FSL の不変条件を満たしているか。
//!
//! 落ち方は2通り。 同じプロセスの中で台帳を閉じて開き直すものと、子プロセスを段の途中で
//! 止めて kill するもの。 子プロセスはこの試験 binary 自身で、環境変数で役を切り替える。

use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use koeru_core::capture::{self, Report};
use koeru_core::db::{
    Answer, CaptureId, Commit, CommitRequest, IntentState, Ledger, NewIntent, OperationId,
    SessionSnapshot,
};
use koeru_core::inventory::UnitSet;
use koeru_core::reclist::generate_single;

/// 子プロセスの役を指す環境変数。 値は止める段の名前。
const CHILD_PHASE: &str = "KOERU_CAPTURE_KILLPOINT";
const CHILD_ROOT: &str = "KOERU_CAPTURE_ROOT";
const CHILD_CAPTURE: &str = "KOERU_CAPTURE_ID";
const CHILD_OPERATION: &str = "KOERU_CAPTURE_OPERATION";
const CHILD_ROW: &str = "KOERU_CAPTURE_ROW";
const CHILD_SESSION: &str = "KOERU_CAPTURE_SESSION";

/// 子プロセスとして走らせる試験の名前。 `--exact` で渡す。
const KILL_TEST: &str = "段の途中で子プロセスを殺しても開き直せる";

/// 止める段。 `project-storage.fsl` の落ち方に対応する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// 予定を書いたあと、`.wav.part` を開く前（`crash_after_declare`）。
    AfterDeclare,
    /// `.wav.part` に書いている途中（`crash_losing_part`）。
    WhileWriting,
    /// rename とディレクトリの fsync のあと、台帳の確定の前（`crash_leaving_orphan`）。
    AfterRename,
    /// 台帳の確定のあと、応答を返す前（`retry_commit` を要する）。
    AfterCommit,
}

const PHASES: [Phase; 4] = [
    Phase::AfterDeclare,
    Phase::WhileWriting,
    Phase::AfterRename,
    Phase::AfterCommit,
];

impl Phase {
    const fn name(self) -> &'static str {
        match self {
            Self::AfterDeclare => "after_declare",
            Self::WhileWriting => "while_writing",
            Self::AfterRename => "after_rename",
            Self::AfterCommit => "after_commit",
        }
    }

    fn parse(s: &str) -> Self {
        PHASES
            .into_iter()
            .find(|p| p.name() == s)
            .unwrap_or_else(|| panic!("知らない段: {s}"))
    }
}

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-capture-storage-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("audio")).expect("一時ディレクトリを作れること");
    d
}

fn db_path(root: &Path) -> PathBuf {
    root.join("project.db")
}

/// 行とセッションを入れた台帳を作って閉じる。 返すのは `(行, セッション)`。
fn setup(root: &Path) -> (Vec<String>, i32) {
    let mut l = Ledger::open(db_path(root)).expect("開ける");
    let list = generate_single(UnitSet::Core, 3).expect("生成できる");
    l.install_reclist(&list, 60).expect("書き込める");
    let sid = l
        .start_session(&SessionSnapshot {
            started_at: "2026-09-26T00:00:00Z".into(),
            device_id: "test".into(),
            sample_rate_hz: 44_100,
            channels: 1,
            effects_state: "clean".into(),
            route: "test".into(),
            source_channel: 0,
            master_rate_hz: 44_100,
            resampler: "test".into(),
            upstream_conversion: "unknown".into(),
        })
        .expect("始められる");
    (list.into_iter().map(|r| r.id).collect(), sid)
}

/// 録音1回ぶんの識別子と行。
struct Take<'a> {
    row: &'a str,
    session: i32,
    capture: &'a CaptureId,
    operation: &'a OperationId,
}

/// 保存の手順を `stop` の段まで進める。 `stop` が `None` なら最後まで。
///
/// `at_stop` は止める段に着いたときに呼ぶ。 同じプロセスの試験では何もせず戻り、
/// 子プロセスでは印を置いて kill を待つ。 戻ったら、それ以上何も書かずに台帳と
/// ファイルを手放す——落ちたのと同じ。
fn record(
    root: &Path,
    t: &Take<'_>,
    stop: Option<Phase>,
    at_stop: &dyn Fn(Phase),
) -> Option<Commit> {
    let mut l = Ledger::open(db_path(root)).expect("開ける");
    let dir = root.join("audio");
    let path = capture::free_take_path(&mut l, root, &dir, t.row, 1).expect("場所を決められる");
    let rel = capture::rel_path(root, &path);
    l.declare_capture(&NewIntent {
        capture: t.capture,
        row_id: t.row,
        session_id: t.session,
        rel_path: &rel,
        declared_at: "2026-09-26T00:00:01Z",
    })
    .expect("予定を書ける");
    if stop == Some(Phase::AfterDeclare) {
        at_stop(Phase::AfterDeclare);
        return None;
    }

    let mut part_path = path.clone().into_os_string();
    part_path.push(".part");
    let mut part = std::fs::File::create(&part_path).expect("書きかけを開ける");
    part.write_all(&[0_u8; 4096]).expect("書ける");
    if stop == Some(Phase::WhileWriting) {
        at_stop(Phase::WhileWriting);
        return None;
    }
    part.sync_all().expect("fsync できる");
    drop(part);
    std::fs::rename(&part_path, &path).expect("rename できる");
    koeru_core::project::sync_dir(&dir).expect("ディレクトリを fsync できる");
    if stop == Some(Phase::AfterRename) {
        at_stop(Phase::AfterRename);
        return None;
    }

    let got = l
        .commit_capture(&CommitRequest {
            operation: t.operation,
            capture: t.capture,
            frames: 1024,
            recorded_at: "2026-09-26T00:00:02Z",
            valid: true,
        })
        .expect("確定できる");
    if stop == Some(Phase::AfterCommit) {
        at_stop(Phase::AfterCommit);
        return None;
    }
    Some(got)
}

/// 開き直して、起動時の検証を走らせる。
fn reopen(root: &Path) -> (Ledger, Report) {
    let mut l = Ledger::open(db_path(root)).expect("開き直せる");
    let report = capture::verify(&mut l, root, &root.join("audio"), "2026-09-26T00:01:00Z")
        .expect("検証できる");
    (l, report)
}

/// `audio/` の下のファイルを、根からの相対パスで全部。
fn files(root: &Path) -> BTreeSet<String> {
    fn walk(root: &Path, dir: &Path, out: &mut BTreeSet<String>) {
        for e in std::fs::read_dir(dir)
            .expect("読める")
            .filter_map(Result::ok)
        {
            let p = e.path();
            if p.is_dir() {
                walk(root, &p, out);
            } else {
                out.insert(capture::rel_path(root, &p));
            }
        }
    }
    let mut out = BTreeSet::new();
    walk(root, &root.join("audio"), &mut out);
    out
}

fn all_takes(l: &mut Ledger) -> Vec<koeru_core::db::Take> {
    l.rows_with_takes()
        .expect("読める")
        .into_iter()
        .flat_map(|r| r.takes)
        .collect()
}

/// `project-storage.fsl` の不変条件を、台帳とファイルの上で確かめる。
fn assert_invariants(l: &mut Ledger, root: &Path, report: &Report) {
    let intents = l.intents().expect("読める");
    let receipts = l.receipts().expect("読める");
    let takes = all_takes(l);
    let on_disk = files(root);

    // 単一の書き手。 検証のあとは、録音に使っている予定は残らない。
    let open = intents
        .iter()
        .filter(|i| i.state == IntentState::Open)
        .count();
    assert_eq!(open, 0, "検証のあとに録音中の予定が残っている: {intents:?}");

    // NoOrphanTakeRow: テイクの行は確定したファイルを指す。
    for t in &takes {
        assert!(on_disk.contains(&t.rel_path), "ファイルの無いテイク: {t:?}");
    }

    // EveryTakeClosedAnIntent: どのテイクにも、閉じた予定が1つずつある。
    let take_ids: BTreeSet<i32> = takes.iter().map(|t| t.id).collect();
    let closed: Vec<i32> = intents
        .iter()
        .filter(|i| i.state == IntentState::Committed)
        .map(|i| i.take_id.expect("閉じた予定はテイクを指す"))
        .collect();
    assert_eq!(closed.len(), take_ids.len(), "{intents:?}");
    assert_eq!(closed.iter().copied().collect::<BTreeSet<_>>(), take_ids);

    // ReceiptBacksATake: 受領証は、その録音を閉じたテイクを指す。
    assert_eq!(receipts.len(), take_ids.len());
    for r in &receipts {
        let i = intents
            .iter()
            .find(|i| i.capture == r.capture)
            .expect("受領証の録音に予定がある");
        assert_eq!(i.take_id, Some(r.take_id), "{r:?}");
    }

    // OrphanHasIntent: 孤児は予定を持つ（この試験の録音はどれも予定を書いてから始める）。
    for o in &report.orphans {
        let i = o.intent.as_ref().expect("孤児に予定がある");
        assert_eq!(i.state, IntentState::Orphaned, "{o:?}");
        assert!(on_disk.contains(&o.rel_path));
    }

    // RecordingUnderIntent: 手元のファイルは、書きかけも確定済みも予定の場所にある。
    let places: BTreeSet<String> = intents
        .iter()
        .flat_map(|i| [i.rel_path.clone(), format!("{}.part", i.rel_path)])
        .collect();
    for f in &on_disk {
        assert!(places.contains(f), "予定の無いファイル: {f}");
    }
}

/// 落ちた段ごとに、開き直したあとの姿を確かめる。
fn assert_after(phase: Phase, root: &Path, t: &Take<'_>) {
    let (mut l, report) = reopen(root);
    assert_invariants(&mut l, root, &report);
    let intent = l
        .intent(t.capture)
        .expect("読める")
        .expect("予定は消えない");
    assert_eq!(intent.row_id, t.row, "どの行の録音かを予定が覚えている");
    assert_eq!(
        intent.session_id, t.session,
        "どの条件の録音かを予定が覚えている"
    );

    match phase {
        Phase::AfterDeclare => {
            assert_eq!(intent.state, IntentState::Abandoned);
            assert_eq!(report.abandoned, 1);
            assert!(report.orphans.is_empty() && report.partial.is_empty());
            assert!(all_takes(&mut l).is_empty());
        }
        Phase::WhileWriting => {
            assert_eq!(intent.state, IntentState::Partial);
            assert_eq!(report.partial, std::slice::from_ref(&intent));
            assert_eq!(report.abandoned, 0);
            assert!(
                files(root).contains(&format!("{}.part", intent.rel_path)),
                "書きかけを消さない"
            );
            assert!(all_takes(&mut l).is_empty());
        }
        Phase::AfterRename => {
            assert_eq!(intent.state, IntentState::Orphaned);
            assert_eq!(report.orphans.len(), 1);
            assert_eq!(report.orphans[0].rel_path, intent.rel_path);
            assert_eq!(report.orphans[0].intent.as_ref(), Some(&intent));
            assert!(all_takes(&mut l).is_empty(), "孤児を自動で採らない");
        }
        Phase::AfterCommit => {
            assert_eq!(intent.state, IntentState::Committed);
            assert!(report.orphans.is_empty());
            let receipt = l
                .receipt(t.operation)
                .expect("読める")
                .expect("受領証が残っている");
            // 応答を失った consumer の送り直しに、同じ受領証で答える。
            let again = l
                .commit_capture(&CommitRequest {
                    operation: t.operation,
                    capture: t.capture,
                    frames: 1024,
                    recorded_at: "2026-09-26T00:02:00Z",
                    valid: true,
                })
                .expect("答えられる");
            assert_eq!(again, Commit::Answered(Answer::Replayed(receipt)));
            assert_eq!(all_takes(&mut l).len(), 1, "送り直しでテイクを増やさない");
        }
    }

    // 検証を繰り返しても、付けた印は動かない（孤児も放棄も黙って消えない）。
    let before = l.intents().expect("読める");
    drop(l);
    let (mut l, again) = reopen(root);
    assert_eq!(l.intents().expect("読める"), before);
    assert_eq!(again.orphans, report.orphans);
    assert_eq!(again.abandoned, 0, "放棄の印は一度だけ付ける");
}

#[test]
fn 段の途中で落ちても開き直せる() {
    for phase in PHASES {
        let root = tmp(phase.name());
        let (rows, session) = setup(&root);
        let (capture, operation) = (CaptureId::generate(), OperationId::generate());
        let t = Take {
            row: &rows[0],
            session,
            capture: &capture,
            operation: &operation,
        };
        assert!(record(&root, &t, Some(phase), &|_| {}).is_none());
        assert_after(phase, &root, &t);
    }
}

#[test]
fn 段の途中で子プロセスを殺しても開き直せる() {
    if let Ok(phase) = std::env::var(CHILD_PHASE) {
        run_child(Phase::parse(&phase));
    }
    for phase in PHASES {
        let root = tmp(&format!("kill-{}", phase.name()));
        let (rows, session) = setup(&root);
        let (capture, operation) = (CaptureId::generate(), OperationId::generate());
        let marker = root.join(format!("reached-{}", phase.name()));

        let mut child = Command::new(std::env::current_exe().expect("試験 binary の場所"))
            .args(["--exact", KILL_TEST, "--nocapture", "--test-threads=1"])
            .env(CHILD_PHASE, phase.name())
            .env(CHILD_ROOT, &root)
            .env(CHILD_CAPTURE, capture.as_str())
            .env(CHILD_OPERATION, operation.as_str())
            .env(CHILD_ROW, &rows[0])
            .env(CHILD_SESSION, session.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .expect("子プロセスを起こせる");

        let deadline = Instant::now() + Duration::from_secs(60);
        while !marker.exists() {
            if let Some(status) = child.try_wait().expect("待てる") {
                let mut err = String::new();
                if let Some(mut s) = child.stderr.take() {
                    let _ = std::io::Read::read_to_string(&mut s, &mut err);
                }
                panic!("{phase:?} に着く前に子プロセスが終わった（{status}）: {err}");
            }
            assert!(Instant::now() < deadline, "{phase:?} に着かない");
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().expect("kill できる");
        let status = child.wait().expect("待てる");
        assert!(
            !status.success(),
            "{phase:?}: kill したので成功では終わらない"
        );

        let t = Take {
            row: &rows[0],
            session,
            capture: &capture,
            operation: &operation,
        };
        assert_after(phase, &root, &t);
    }
}

/// 子プロセスの役。 段に着いたら印を置き、kill されるまで待つ。 戻らない。
fn run_child(phase: Phase) -> ! {
    let var = |k: &str| std::env::var(k).unwrap_or_else(|_| panic!("{k} が無い"));
    let root = PathBuf::from(var(CHILD_ROOT));
    let capture = CaptureId::parse(&var(CHILD_CAPTURE)).expect("読める");
    let operation = OperationId::parse(&var(CHILD_OPERATION)).expect("読める");
    let row = var(CHILD_ROW);
    let session: i32 = var(CHILD_SESSION).parse().expect("読める");
    let t = Take {
        row: &row,
        session,
        capture: &capture,
        operation: &operation,
    };
    let marker = root.join(format!("reached-{}", phase.name()));
    record(&root, &t, Some(phase), &|_| {
        std::fs::write(&marker, b"").expect("印を置ける");
        loop {
            std::thread::sleep(Duration::from_secs(1));
        }
    });
    unreachable!("段に着いたら戻らない");
}

#[test]
fn 台帳の確定が失敗したら孤児として残り予定から行を示せる() {
    use diesel::prelude::*;

    let root = tmp("db-busy");
    let (rows, session) = setup(&root);
    let (capture, operation) = (CaptureId::generate(), OperationId::generate());
    let t = Take {
        row: &rows[0],
        session,
        capture: &capture,
        operation: &operation,
    };
    assert!(record(&root, &t, Some(Phase::AfterRename), &|_| {}).is_none());

    // 別の接続が書き込みの錠を握っている間は、確定が落ちる。
    let mut l = Ledger::open(db_path(&root)).expect("開ける");
    let mut other = diesel::sqlite::SqliteConnection::establish(&db_path(&root).to_string_lossy())
        .expect("別の接続を開ける");
    diesel::sql_query("BEGIN IMMEDIATE")
        .execute(&mut other)
        .expect("錠を握れる");
    let e = l
        .commit_capture(&CommitRequest {
            operation: &operation,
            capture: &capture,
            frames: 1024,
            recorded_at: "2026-09-26T00:00:02Z",
            valid: true,
        })
        .expect_err("錠を握られている間は確定できない");
    assert!(matches!(e, koeru_core::db::LedgerError::Db { .. }), "{e:?}");
    diesel::sql_query("ROLLBACK")
        .execute(&mut other)
        .expect("錠を放せる");

    // 同じプロセスの中で、場所を見て印を付ける。 次の録音の予定を書ける。
    let mark = capture::settle(&mut l, &root, &capture, "2026-09-26T00:00:03Z").expect("見られる");
    assert_eq!(mark, Some(koeru_core::db::Leftover::Orphaned));
    let next = CaptureId::generate();
    let path = capture::free_take_path(&mut l, &root, &root.join("audio"), &rows[0], 1)
        .expect("場所を決められる");
    assert_ne!(
        capture::rel_path(&root, &path),
        l.intent(&capture).expect("読める").expect("ある").rel_path,
        "孤児の名前を使わない"
    );
    l.declare_capture(&NewIntent {
        capture: &next,
        row_id: &rows[1],
        session_id: session,
        rel_path: &capture::rel_path(&root, &path),
        declared_at: "2026-09-26T00:00:04Z",
    })
    .expect("次の予定を書ける");
    l.discard_capture(&next, "2026-09-26T00:00:05Z")
        .expect("閉じられる");
    drop(l);

    let (mut l, report) = reopen(&root);
    assert_invariants(&mut l, &root, &report);
    assert_eq!(report.orphans.len(), 1);
    let intent = report.orphans[0].intent.clone().expect("予定がある");
    assert_eq!(
        (intent.row_id.as_str(), intent.session_id),
        (t.row, session)
    );

    // 本人が採ると決めたら、孤児の予定からテイクにする（`adopt_orphan`）。
    let adopt = OperationId::generate();
    let got = l
        .commit_capture(&CommitRequest {
            operation: &adopt,
            capture: &capture,
            frames: 1024,
            recorded_at: "2026-09-26T00:03:00Z",
            valid: true,
        })
        .expect("採れる");
    assert!(matches!(got, Commit::Committed(_)), "{got:?}");
    let report = capture::verify(&mut l, &root, &root.join("audio"), "t").expect("検証できる");
    assert!(report.orphans.is_empty());
    assert_invariants(&mut l, &root, &report);
}

#[test]
fn 確定したあと応答を失っても開き直して受領証で答える() {
    let root = tmp("retry");
    let (rows, session) = setup(&root);
    let (capture, operation) = (CaptureId::generate(), OperationId::generate());
    let t = Take {
        row: &rows[0],
        session,
        capture: &capture,
        operation: &operation,
    };
    let Some(Commit::Committed(first)) = record(&root, &t, None, &|_| {}) else {
        panic!("確定すること");
    };

    let (mut l, report) = reopen(&root);
    assert_invariants(&mut l, &root, &report);
    assert_eq!(
        l.answer_retry(&operation, &capture).expect("答えられる"),
        Some(Answer::Replayed(first.clone()))
    );
    // 同じ識別子を別の録音に使い回したら、先の受領証を返して何も書かない。
    let other = CaptureId::generate();
    l.declare_capture(&NewIntent {
        capture: &other,
        row_id: &rows[1],
        session_id: session,
        rel_path: "audio/other_1.wav",
        declared_at: "t",
    })
    .expect("書ける");
    assert_eq!(
        l.answer_retry(&operation, &other).expect("答えられる"),
        Some(Answer::OperationIdReused(first))
    );
    assert_eq!(all_takes(&mut l).len(), 1);
}

#[test]
fn 予定は台帳の_wal_に書かれ閉じる前から読める() {
    let root = tmp("wal");
    let (rows, session) = setup(&root);
    let mut writer = Ledger::open(db_path(&root)).expect("開ける");
    let capture = CaptureId::generate();
    writer
        .declare_capture(&NewIntent {
            capture: &capture,
            row_id: &rows[0],
            session_id: session,
            rel_path: "audio/a_1.wav",
            declared_at: "t",
        })
        .expect("書ける");
    assert!(
        root.join("project.db-wal").exists(),
        "WAL モードで開いている（`TR-REC-27`）"
    );
    // 書き手を閉じる前に、別の接続から予定が見える。 録音を始める前に確定している。
    let mut reader = Ledger::open(db_path(&root)).expect("開ける");
    let seen = reader.intent(&capture).expect("読める").expect("見える");
    assert_eq!(seen.state, IntentState::Open);
}
