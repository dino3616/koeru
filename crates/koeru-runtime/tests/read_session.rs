//! `ProjectRuntime` / `ProjectReadSession` を、本物のプロジェクト（tempdir +
//! 本物の SQLite・WAL）で確かめる（T05b-1、`DEC-PLT-044`、`DEC-PLT-043`、
//! `DEC-PLT-034`）。
//!
//! `crates/koeru-core/tests/read_snapshot.rs`（T05a）と同じ道具立てを、
//! `ProjectRuntime` の貸与・単一の書き手・facet 越しに確かめる。

use std::path::PathBuf;
use std::sync::Arc;

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;

use koeru_core::db::{FinalizedTake, SessionSnapshot, koeru_oto};
use koeru_core::inventory::UnitSet;
use koeru_core::project::{Library, Manifest, Method, ProjectDir};
use koeru_core::reclist::generate_single;
use koeru_runtime::{ProjectRuntime, RuntimeError};

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-runtime-read-session-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
    d
}

fn manifest() -> Manifest {
    Manifest {
        display_name: "テスト用の音源".to_owned(),
        method: Method::Single,
        item_count: 5,
        derived_from: None,
        preset_id: None,
        inventory_version: None,
    }
}

fn new_project(tag: &str) -> ProjectDir {
    let lib = Library::open(tmp(tag)).expect("開けること");
    let dir = lib.create(&manifest()).expect("作れること");
    // `Library::create` はディレクトリと manifest だけを作り、`project.db` は
    // まだ無い（実際は `Studio::create_project_with_presamp` がこの直後に
    // `Ledger::open` を呼んで作る）。`ProjectDir::migrate_ledger` の
    // `migration_state` は読み取り専用で開くので、無いファイルは作れない
    // ——ここで一度開いて作っておく（`crates/koeru-core/tests/ledger_migration.rs`
    // の「新規の台帳は ledger_open が全部当てる」と同じ手順）。
    koeru_core::db::Ledger::open(dir.db_path()).expect("台帳を作れること");
    dir
}

fn open_runtime(dir: ProjectDir) -> Arc<ProjectRuntime> {
    ProjectRuntime::open(dir).expect("開けること")
}

/// 録音リストを入れ、セッションを1つ始める。返るのは `(行 ID の列, セッション ID)`。
fn seed_rows(rt: &ProjectRuntime) -> (Vec<String>, i32) {
    let mut w = rt.writer(rt.lease()).expect("書ける");
    let list = generate_single(UnitSet::Core, 5).expect("生成できる");
    w.install_reclist(&list, 60).expect("書き込める");
    let sid = w
        .start_session(&SessionSnapshot {
            started_at: "2026-09-28T00:00:00Z".into(),
            device_id: "test".into(),
            sample_rate_hz: 48_000,
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

fn commit(rt: &ProjectRuntime, row: &str, sid: i32, path: &str) -> i32 {
    let mut w = rt.writer(rt.lease()).expect("書ける");
    w.commit_take(&FinalizedTake {
        row_id: row.to_owned(),
        session_id: sid,
        rel_path: path.to_owned(),
        frames: 44_100,
        recorded_at: "2026-09-28T00:00:01Z".into(),
    })
    .expect("確定できること")
}

fn adopt(rt: &ProjectRuntime, row: &str, take_id: i32) {
    let mut w = rt.writer(rt.lease()).expect("書ける");
    w.adopt_take(row, take_id).expect("採用できること");
}

fn put_oto(rt: &ProjectRuntime, take_id: i32, alias: &str) {
    let mut w = rt.writer(rt.lease()).expect("書ける");
    w.put_oto(
        take_id,
        alias,
        &koeru_oto::Oto {
            offset_ms: 10.0,
            consonant_ms: 20.0,
            cutoff_ms: -30.0,
            preutterance_ms: 15.0,
            overlap_ms: 5.0,
        },
        0.5,
        None,
        false,
    )
    .expect("書けること");
}

fn raw_connection(dir: &ProjectDir) -> SqliteConnection {
    let mut conn = SqliteConnection::establish(&dir.db_path().to_string_lossy())
        .expect("SQLite として開けること");
    diesel::sql_query("PRAGMA busy_timeout = 5000")
        .execute(&mut conn)
        .expect("設定できること");
    conn
}

/// 1つの読みセッションに含まれる facet は同じ版を観測する。
///
/// `recording` が新しい書き込みを見て `review` が見ない（またはその逆）ということが
/// 無いこと——`docs/reports/architecture/08-graphql-application-contract.md` §5 の
/// 「`recording` が R42、`review` が R43 になることを禁止する」。 新しいセッションは
/// 新しい版を見る。
#[test]
fn 一つの読みセッションは全部の_facet_が同じ版を観測する() {
    let dir = new_project("cross-facet");
    let rt = open_runtime(dir);
    let (rows, sid) = seed_rows(&rt);
    let lease = *rt.lease();

    let session = rt.read(&lease).expect("読める");
    let r0 = session.revision();

    // recording と review の両方に効く書き込み。
    let take = commit(&rt, &rows[0], sid, "audio/a_1.wav");
    adopt(&rt, &rows[0], take);
    put_oto(&rt, take, "か");

    let recording = session.recording().expect("読める");
    let review = session.review().expect("読める");
    assert_eq!(session.revision(), r0, "版が動かないこと");
    assert!(
        recording.row(&rows[0]).expect("行がある").takes.is_empty(),
        "recording は開いたあとの書き込みを見ないこと"
    );
    assert!(
        review.takes.is_empty(),
        "review も同じ版のまま——recording だけ新しい版を見て review が古いままになるのを禁じる"
    );

    let session2 = rt.read(&lease).expect("読める");
    assert_ne!(
        session2.revision(),
        r0,
        "新しいセッションは新しい版を見ること"
    );
    let recording2 = session2.recording().expect("読める");
    let review2 = session2.review().expect("読める");
    assert_eq!(
        recording2.row(&rows[0]).expect("行がある").takes.len(),
        1,
        "新しいセッションは新しいテイクを見ること"
    );
    assert_eq!(review2.takes.len(), 1);

    // `recording()` を組まずに、スナップショットから直接1件だけ引く経路。
    assert!(
        session.take(take).expect("読める").is_none(),
        "古い版はこのテイクを知らないこと"
    );
    assert!(session2.take(take).expect("読める").is_some());
}

/// 読みセッションを開いている間に書き手が書き込んでも、セッションは
/// 待たされず（bounded time）、開いたセッションは新しい書き込みを見ない。
///
/// facet を書き込みの「あとで」初めて組んでも、版は `read` を呼んだ時点で
/// 固定済み——`LedgerReader::snapshot` の `BEGIN DEFERRED` が最初の読み取りで
/// 版を固定するのは facet を組む前、セッションを作った瞬間。
#[test]
fn 開いたセッションは書き手を妨げず新しい書き込みも見ない() {
    let dir = new_project("parallel-mutation");
    let rt = open_runtime(dir);
    let (rows, sid) = seed_rows(&rt);
    let lease = *rt.lease();

    let session = rt.read(&lease).expect("読める");
    let revision_at_open = session.revision();

    let rt2 = Arc::clone(&rt);
    let row0 = rows[0].clone();
    let start = std::time::Instant::now();
    let writer_thread = std::thread::spawn(move || {
        for i in 0..20 {
            let mut w = rt2.writer(&lease).expect("書ける");
            w.commit_take(&FinalizedTake {
                row_id: row0.clone(),
                session_id: sid,
                rel_path: format!("audio/a_{}.wav", i + 2),
                frames: 44_100,
                recorded_at: "2026-09-28T00:00:02Z".into(),
            })
            .expect("確定できること");
        }
    });
    writer_thread
        .join()
        .expect("書き手のスレッドが落ちないこと");
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(10),
        "開いた読みが書き手を待たせていないこと: {elapsed:?}"
    );

    let recording = session.recording().expect("読める");
    let row = recording.row(&rows[0]).expect("行がある");
    assert_eq!(row.takes.len(), 0, "開いたあとの書き込みが見えないこと");
    assert_eq!(session.revision(), revision_at_open);

    let session2 = rt.read(&lease).expect("読める");
    assert_ne!(session2.revision(), revision_at_open);
    let recording2 = session2.recording().expect("読める");
    assert_eq!(
        recording2.row(&rows[0]).expect("行がある").takes.len(),
        20,
        "新しいセッションは新しいテイクを見ること"
    );
}

/// 開く → 閉じる → 開く → 閉じる → 開く。 開くたびに違う貸与になり、
/// 古い貸与は `read` / `writer` のどちらでも失効する。
#[test]
fn 開き直すたびに違う貸与になる_a_b_a() {
    let dir = new_project("a-b-a");

    let rt1 = ProjectRuntime::open(dir.clone()).expect("開けること");
    let lease1 = *rt1.lease();
    rt1.close(&lease1).expect("閉じられること");

    let rt2 = ProjectRuntime::open(dir.clone()).expect("開けること");
    let lease2 = *rt2.lease();
    assert_ne!(lease1, lease2, "開き直すと別の貸与になること");
    rt2.close(&lease2).expect("閉じられること");

    let rt3 = ProjectRuntime::open(dir).expect("開けること");
    let lease3 = *rt3.lease();
    assert_ne!(lease3, lease1);
    assert_ne!(lease3, lease2);

    assert!(matches!(rt3.read(&lease1), Err(RuntimeError::LeaseExpired)));
    assert!(matches!(
        rt3.writer(&lease1),
        Err(RuntimeError::LeaseExpired)
    ));
    assert!(matches!(rt1.read(&lease1), Err(RuntimeError::LeaseExpired)));
}

/// 閉じたあとは `read` / `writer` が失効し、二度目の `close` も失効。
/// 閉じる前に開いていたセッションは、握っているスナップショットを読み終えられる。
#[test]
fn 閉じたあとは失効し二度目の_close_も失効する() {
    let dir = new_project("close-open");
    let rt = ProjectRuntime::open(dir).expect("開ける");
    let lease = *rt.lease();

    let session = rt.read(&lease).expect("読める");
    let _ = session.recording().expect("読める");

    rt.close(&lease).expect("閉じられること");
    assert!(matches!(rt.read(&lease), Err(RuntimeError::LeaseExpired)));
    assert!(matches!(rt.writer(&lease), Err(RuntimeError::LeaseExpired)));
    assert!(matches!(rt.close(&lease), Err(RuntimeError::LeaseExpired)));

    // 閉じる前に取ったセッションは、まだ読み終えられる。
    let _ = session
        .review()
        .expect("閉じる前に開いたセッションは読み終えられること");
}

/// 別の実行時（別の project）の貸与は、この実行時からは失効として扱う。
#[test]
fn 別の実行時の貸与は失効として扱う() {
    let dir_a = new_project("wrong-lease-a");
    let dir_b = new_project("wrong-lease-b");
    let rt_a = ProjectRuntime::open(dir_a).expect("開ける");
    let rt_b = ProjectRuntime::open(dir_b).expect("開ける");

    assert!(matches!(
        rt_a.read(rt_b.lease()),
        Err(RuntimeError::LeaseExpired)
    ));
    assert!(matches!(
        rt_a.writer(rt_b.lease()),
        Err(RuntimeError::LeaseExpired)
    ));
}

/// review だけが読む値を壊しても、同じセッションの recording は影響されない
/// （T05 の adversarial「lazy facet failure」）。
#[test]
fn 壊れた_facet_は他の_facet_を巻き込まない() {
    let dir = new_project("lazy-facet-failure");
    let rt = open_runtime(dir.clone());
    let (rows, sid) = seed_rows(&rt);
    let take = commit(&rt, &rows[0], sid, "audio/a_1.wav");
    adopt(&rt, &rows[0], take);
    put_oto(&rt, take, "か");

    // `review` の経路（`adopted_otos`）だけが読む表を落とす。
    // `oto_values` は `STRICT` 表なので、型に合わない値を直接 `UPDATE` では
    // 入れられない（SQLite が書く側で断る）——表そのものを落として
    // クエリを失敗させる。`recording`（`rows_with_takes`）はこの表を読まない。
    {
        let mut raw = raw_connection(&dir);
        diesel::sql_query("DROP TABLE oto_values")
            .execute(&mut raw)
            .expect("壊せること");
    }

    let lease = *rt.lease();
    let session = rt.read(&lease).expect("読める");
    assert!(
        session.review().is_err(),
        "壊れた値で review は失敗すること"
    );
    let recording = session
        .recording()
        .expect("review が壊れていても recording は影響されないこと");
    assert!(recording.row(&rows[0]).is_some());
}

/// プールの上限（2）より多い数の読みセッションを同時に開いても壊れない。
#[test]
fn プールの上限より多いセッションも開ける() {
    let dir = new_project("reader-pool");
    let rt = open_runtime(dir);
    let (rows, ..) = seed_rows(&rt);
    let lease = *rt.lease();

    let sessions: Vec<_> = (0..5).map(|_| rt.read(&lease).expect("読める")).collect();
    for s in &sessions {
        let recording = s.recording().expect("読める");
        assert!(recording.row(&rows[0]).is_some());
    }
    drop(sessions);

    // 読み手を返し終えたあとにもう一度開いても壊れていない。
    let s = rt.read(&lease).expect("読める");
    assert!(s.recording().expect("読める").row(&rows[0]).is_some());
}

/// `koeru_runtime::review::load` は、書き手（`Ledger`）からもスナップショット
/// （`LedgerSnapshot` 越しの `ProjectReadSession::review`）からも同じキューを組む。
#[test]
fn review_facet_は書き手から組んだキューと一致する() {
    let dir = new_project("review-parity");
    let rt = open_runtime(dir);
    let (rows, sid) = seed_rows(&rt);
    let take = commit(&rt, &rows[0], sid, "audio/a_1.wav");
    adopt(&rt, &rows[0], take);
    put_oto(&rt, take, "か");

    let lease = *rt.lease();
    let session = rt.read(&lease).expect("読める");
    let via_snapshot = session.review().expect("読める");

    let (via_writer, takes_writer) = {
        let mut w = rt.writer(&lease).expect("書ける");
        koeru_runtime::review::load(&mut *w).expect("組める")
    };

    assert_eq!(via_snapshot.takes.len(), takes_writer.len());
    assert_eq!(
        via_snapshot.queue.pending_count(),
        via_writer.pending_count()
    );
    let key = koeru_runtime::review::EntryKey::new(60, "か").handle();
    assert_eq!(via_snapshot.takes.get(&key), takes_writer.get(&key));
    assert!(via_snapshot.queue.get(&key).is_some());
    assert!(via_writer.get(&key).is_some());
}
