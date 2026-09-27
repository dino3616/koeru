//! 読み取り専用スナップショット（`LedgerReader` / `LedgerSnapshot`）を、本物の
//! SQLite（WAL）とファイルで確かめる（`DEC-PLT-043`）。
//!
//! T05a が `koeru-core` に置くのは貯蔵層の道具（版・トリガー・読み取り専用の接続・
//! 一時点のスナップショット）だけ。 `ProjectLease` / `ProjectEpoch` と、複数 facet の
//! 遅延解決は T05b（`koeru-runtime`）が別に積む——ここでは触らない。

use std::path::{Path, PathBuf};

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use koeru_core::db::{
    CaptureId, Commit, CommitRequest, Ledger, LedgerError, NewIntent, OperationId, SessionSnapshot,
};
use koeru_core::inventory::UnitSet;
use koeru_core::reclist::generate_single;

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

diesel::table! {
    sqlite_master (name) {
        name -> Text,
        #[sql_name = "type"]
        kind -> Text,
        tbl_name -> Text,
    }
}

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-read-snapshot-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
    d.join("project.db")
}

/// 行とセッションを持つ、開いたばかりの台帳。
fn setup(path: &Path) -> (Ledger, Vec<String>, i32) {
    let mut l = Ledger::open(path).expect("開ける");
    let list = generate_single(UnitSet::Core, 3).expect("生成できる");
    l.install_reclist(&list, 60).expect("書き込める");
    let sid = l
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
    (l, list.into_iter().map(|r| r.id).collect(), sid)
}

/// 台帳を経由せずに開く生の接続。 移行前の版の作成や、書き手を介さない
/// トリガーの確認に使う。
fn raw_connection(path: &Path) -> SqliteConnection {
    let mut conn =
        SqliteConnection::establish(&path.to_string_lossy()).expect("SQLite として開けること");
    diesel::sql_query("PRAGMA journal_mode = WAL")
        .execute(&mut conn)
        .expect("WAL にできること");
    conn
}

/// 「バイナリの知っている最後の1つがまだ当たっていない」台帳を作る
/// （`ledger_migration.rs` と同じ手順）。
fn pending_one_migration_short(path: &Path) -> SqliteConnection {
    let mut conn = raw_connection(path);
    conn.run_pending_migrations(MIGRATIONS)
        .expect("全部当てられること");
    conn.revert_last_migration(MIGRATIONS)
        .expect("最後の1つを戻せること");
    conn
}

/// `project_revision` を持つ表以外の、台帳の全表。 `sqlite_master` から機械的に読む。
fn business_tables(conn: &mut SqliteConnection) -> Vec<String> {
    sqlite_master::table
        .filter(sqlite_master::kind.eq("table"))
        .select(sqlite_master::name)
        .load::<String>(conn)
        .expect("sqlite_master を読めること")
        .into_iter()
        .filter(|n| {
            n != "project_revision"
                && n != "__diesel_schema_migrations"
                && !n.starts_with("sqlite_")
        })
        .collect()
}

/// その表に立っている `rev_<table>_*` トリガーの本数。
fn revision_trigger_count(conn: &mut SqliteConnection, table: &str) -> i64 {
    sqlite_master::table
        .filter(sqlite_master::kind.eq("trigger"))
        .filter(sqlite_master::tbl_name.eq(table))
        .filter(sqlite_master::name.like(format!("rev_{table}_%")))
        .count()
        .get_result(conn)
        .expect("sqlite_master を読めること")
}

fn declare(l: &mut Ledger, row: &str, sid: i32, path: &str) -> CaptureId {
    let capture = CaptureId::generate();
    l.declare_capture(&NewIntent {
        capture: &capture,
        row_id: row,
        session_id: sid,
        rel_path: path,
        declared_at: "2026-09-28T00:00:01Z",
    })
    .expect("予定を書ける");
    capture
}

fn commit(l: &mut Ledger, capture: &CaptureId) -> i32 {
    let Commit::Committed(receipt) = l
        .commit_capture(&CommitRequest {
            operation: &OperationId::generate(),
            capture,
            frames: 44_100,
            recorded_at: "2026-09-28T00:00:02Z",
            valid: true,
        })
        .expect("確定できる")
    else {
        panic!("確定すること");
    };
    receipt.take_id
}

/// 新しく作った台帳にも、既存の全表にちょうど3本ずつ（ins/upd/del）のトリガーが
/// 揃うこと。 足し忘れた表があれば、その名前を出す。
#[test]
fn 新規の台帳にも全表のトリガーが揃う() {
    let path = tmp("fresh-triggers");
    {
        let (writer, ..) = setup(&path);
        // 書き手を閉じてから覗く。単一の書き手を保つ（`WAL` 自体は複数読み手を許すが、
        // ここでは意図を明確にするため直列にする）。
        drop(writer);
    }
    let mut conn = raw_connection(&path);

    let tables = business_tables(&mut conn);
    assert!(tables.len() >= 20, "表の数が少なすぎる: {tables:?}");
    let missing: Vec<&String> = tables
        .iter()
        .filter(|t| revision_trigger_count(&mut conn, t) != 3)
        .collect();
    assert!(
        missing.is_empty(),
        "トリガーが3本揃っていない表がある: {missing:?}"
    );
}

/// スナップショットを開いたあとの書き込みは、そのスナップショットには映らない。
/// 新しいスナップショットは映る（`DEC-PLT-043`、
/// `08-graphql-application-contract.md` §5 の「同じ revision を観測する」）。
#[test]
fn スナップショットは開いた後の書き込みを見ない() {
    let path = tmp("stale-snapshot");
    let (mut writer, rows, sid) = setup(&path);
    let r0 = writer.revision().expect("読める");

    let reader = Ledger::open_reader(&path).expect("開ける");
    let mut snap = reader.snapshot().expect("読める");
    assert_eq!(snap.revision(), r0, "開いた時点の版を固定すること");
    let before = snap.rows_with_takes().expect("読める");

    let capture = declare(&mut writer, &rows[0], sid, "audio/a_1.wav");
    commit(&mut writer, &capture);
    let r1 = writer.revision().expect("読める");
    assert_ne!(r1, r0, "書き込みで版が進むこと");

    // 開いたままのスナップショットは、進んだ版もテイクも見ない。
    assert_eq!(snap.revision(), r0, "スナップショットの版は動かないこと");
    let still = snap.rows_with_takes().expect("読める");
    assert_eq!(still, before, "開いたあとの書き込みが見えないこと");

    let reader = snap.close();
    let mut snap2 = reader.snapshot().expect("読める");
    assert_eq!(
        snap2.revision(),
        r1,
        "新しいスナップショットは進んだ版を見ること"
    );
    let after = snap2.rows_with_takes().expect("読める");
    assert_ne!(
        after, before,
        "新しいスナップショットは新しいテイクを見ること"
    );
}

/// 挿入・更新・削除はそれぞれ版を進め、ロールバックしたトランザクションは
/// 進めない。
#[test]
fn 挿入更新削除は版を進めロールバックは進めない() {
    let path = tmp("insert-update-delete");
    let (mut writer, ..) = setup(&path);
    let mut raw = raw_connection(&path);

    let r0 = writer.revision().expect("読める");
    diesel::sql_query(
        "INSERT INTO calibrations (device_id, control, peak_dbfs, settled, measured_at) \
         VALUES ('dev-a', 'manual', -6.0, 1, 't')",
    )
    .execute(&mut raw)
    .expect("挿入できること");
    let r1 = writer.revision().expect("読める");
    assert_ne!(r1, r0, "挿入で版が進むこと");

    diesel::sql_query("UPDATE calibrations SET peak_dbfs = -3.0 WHERE device_id = 'dev-a'")
        .execute(&mut raw)
        .expect("更新できること");
    let r2 = writer.revision().expect("読める");
    assert_ne!(r2, r1, "更新で版が進むこと");

    diesel::sql_query("DELETE FROM calibrations WHERE device_id = 'dev-a'")
        .execute(&mut raw)
        .expect("削除できること");
    let r3 = writer.revision().expect("読める");
    assert_ne!(r3, r2, "削除で版が進むこと");

    diesel::sql_query("BEGIN")
        .execute(&mut raw)
        .expect("開始できること");
    diesel::sql_query(
        "INSERT INTO calibrations (device_id, control, peak_dbfs, settled, measured_at) \
         VALUES ('dev-b', 'manual', -6.0, 1, 't')",
    )
    .execute(&mut raw)
    .expect("挿入できること");
    diesel::sql_query("ROLLBACK")
        .execute(&mut raw)
        .expect("戻せること");
    let r4 = writer.revision().expect("読める");
    assert_eq!(r4, r3, "ロールバックしたトランザクションは版を進めないこと");
}

/// テイクの確定（受領証と同じトランザクション、T04b の commit path）は版を進める。
/// 途中で失敗すれば、テイクも予定の状態も受領証も版も、何も変わらない。
///
/// 失敗は `insert_receipt`（`commit_capture` の最後の一手）を
/// `commit_receipts.capture_id UNIQUE` へ当てて起こす。 その手前の
/// テイク挿入・予定の更新は同じトランザクションの中で既に走っているので、
/// これは「途中まで書けてから失敗する」形になる。
#[test]
fn テイクの確定失敗はテイクも予定も受領証も版も戻す() {
    let path = tmp("commit-fails-midway");
    let (mut writer, rows, sid) = setup(&path);

    // 確定の成功はここで別に確かめる。
    let r0 = writer.revision().expect("読める");
    let ok_capture = declare(&mut writer, &rows[0], sid, "audio/a_1.wav");
    commit(&mut writer, &ok_capture);
    let r_after_ok = writer.revision().expect("読める");
    assert_ne!(r_after_ok, r0, "確定は版を進めること");

    // 対象の予定（row 1）を用意する。
    let target = declare(&mut writer, &rows[1], sid, "audio/b_1.wav");

    // 罠を仕掛ける：既存の `commit_receipts` 行の `capture_id` を、対象の
    // capture_id へ挿し替える。 `capture_id UNIQUE` があるので、対象を確定
    // しようとする `insert_receipt` が必ず一意制約違反になる。
    let mut raw = raw_connection(&path);
    diesel::sql_query(format!(
        "UPDATE commit_receipts SET capture_id = '{}'",
        target.as_str()
    ))
    .execute(&mut raw)
    .expect("罠を仕掛けられること");
    drop(raw);

    let r_before_fail = writer.revision().expect("読める");
    let err = writer
        .commit_capture(&CommitRequest {
            operation: &OperationId::generate(),
            capture: &target,
            frames: 44_100,
            recorded_at: "2026-09-28T00:00:03Z",
            valid: true,
        })
        .expect_err("一意制約違反で失敗すること");
    assert!(matches!(err, LedgerError::Db { .. }), "{err:?}");

    let r_after_fail = writer.revision().expect("読める");
    assert_eq!(
        r_after_fail, r_before_fail,
        "失敗したトランザクションは版を進めないこと"
    );
    assert!(
        writer.takes_of(&rows[1]).expect("読める").is_empty(),
        "失敗したのでテイクは増えないこと"
    );
    let intent = writer.intent(&target).expect("読める").expect("残る");
    assert_eq!(
        intent.state,
        koeru_core::db::IntentState::Open,
        "予定は開いたまま戻ること"
    );
}

/// 値を変えてから元へ戻しても、版は元の値とは等しくならない（等値だけを比べ、
/// 値を使い回さない、`DEC-PLT-043`）。
#[test]
fn 値を戻しても版は元と等しくならない() {
    let path = tmp("a-b-a");
    let (mut writer, ..) = setup(&path);
    let r0 = writer.revision().expect("読める");

    writer
        .set_recording_order(koeru_core::order::Mode::CoverageEfficiency, true)
        .expect("書ける");
    let r1 = writer.revision().expect("読める");
    assert_ne!(r1, r0);

    // 元の既定（`song_bank_first` / pinned なし）へ戻す。
    writer
        .set_recording_order(koeru_core::order::Mode::SongBankFirst, false)
        .expect("書ける");
    let r2 = writer.revision().expect("読める");
    assert_ne!(r2, r0, "内容を元へ戻しても版は元の値と等しくならないこと");
    assert_ne!(r2, r1);
}

/// 読み手は移行待ちの台帳を拒む（`DEC-PLT-041` のエラーを再利用する）。
#[test]
fn 読み手は移行待ちの台帳を拒む() {
    let path = tmp("reader-refuses-pending");
    {
        let _conn = pending_one_migration_short(&path);
    }
    let err = Ledger::open_reader(&path).expect_err("拒むこと");
    assert!(matches!(err, LedgerError::MigrationPending), "{err:?}");
}

/// 読み手は何も版が無い（`Fresh`）台帳も拒む。読める中身がまだ無い。
#[test]
fn 読み手は版の無い台帳も拒む() {
    let path = tmp("reader-refuses-fresh");
    drop(raw_connection(&path));
    let err = Ledger::open_reader(&path).expect_err("拒むこと");
    assert!(matches!(err, LedgerError::MigrationPending), "{err:?}");
}

/// 書き手の `wal_checkpoint(PASSIVE)` は、開いているスナップショットを壊さない。
#[test]
fn チェックポイントはスナップショットを壊さない() {
    let path = tmp("checkpoint-safe");
    let (mut writer, rows, sid) = setup(&path);

    let reader = Ledger::open_reader(&path).expect("開ける");
    let mut snap = reader.snapshot().expect("読める");
    let before = snap.rows_with_takes().expect("読める");

    let capture = declare(&mut writer, &rows[0], sid, "audio/a_1.wav");
    commit(&mut writer, &capture);

    let mut raw = raw_connection(&path);
    diesel::sql_query("PRAGMA wal_checkpoint(PASSIVE)")
        .execute(&mut raw)
        .expect("チェックポイントできること");
    drop(raw);

    // チェックポイントのあとも、開いていたスナップショットは同じものを見る。
    let still = snap.rows_with_takes().expect("読める（壊れていない）");
    assert_eq!(still, before);
    let reader = snap.close();
    drop(reader);
}

/// `close` を呼ばずに `LedgerSnapshot` を drop しても、読み取りトランザクションを
/// 残さない——書き手はそのあとも問題なくコミットできる（DB がロックされない）。
#[test]
fn closeを呼ばずに落としても書き手は困らない() {
    let path = tmp("drop-without-close");
    let (mut writer, rows, sid) = setup(&path);

    {
        let reader = Ledger::open_reader(&path).expect("開ける");
        let mut snap = reader.snapshot().expect("読める");
        // 読み取りトランザクションを実際に始めさせてから、`close` を呼ばずに落とす。
        let _ = snap.rows_with_takes().expect("読める");
    }

    let capture = declare(&mut writer, &rows[0], sid, "audio/a_1.wav");
    commit(&mut writer, &capture);
    assert_eq!(writer.takes_of(&rows[0]).expect("読める").len(), 1);
}
