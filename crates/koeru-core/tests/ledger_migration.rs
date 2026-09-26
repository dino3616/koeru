//! 台帳の移行を「控え → 別の世代へ写す → 確かめる → 切り替える」で行うことを、
//! 本物の SQLite（WAL）とファイルで確かめる（`DEC-PLT-041`、`DEC-PKG-017`）。
//!
//! 古い版の台帳を作るのに `diesel_migrations::MigrationHarness` を直接使う。
//! `run_pending_migrations` で全部当ててから `revert_last_migration` で1つだけ戻す
//! ——「バイナリの知っている最後の1つが未適用」という、実際に起きる形に近い。
//!
//! `embed_migrations!("migrations")` はここでも呼べる。 パスは `CARGO_MANIFEST_DIR`
//! （`crates/koeru-core`）からの相対で、`src/db.rs` が埋め込むものと同じディレクトリを指す。

use std::path::{Path, PathBuf};

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};

use koeru_core::db::{self, Ledger, LedgerError, MigrationState};
use koeru_core::project::{Library, Manifest, Method, MigrationOutcome, ProjectDir, ProjectError};

const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

diesel::table! {
    rows (id) {
        id -> Text,
    }
}

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-ledger-migration-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
    d
}

fn manifest() -> Manifest {
    Manifest {
        display_name: "こえるちゃん".to_owned(),
        method: Method::Single,
        item_count: 1,
        derived_from: None,
        preset_id: None,
        inventory_version: None,
    }
}

/// 新しいプロジェクトを作る。 台帳（`project.db`）にはまだ何も無い。
fn new_project(tag: &str) -> ProjectDir {
    let lib = Library::open(tmp(tag)).expect("開けること");
    lib.create(&manifest()).expect("作れること")
}

/// 台帳を読み書きで開く。 `Ledger::open` を経ない生の接続——移行前の版を作るため、
/// このバイナリの知っている最後の1つを戻す前提で使う。
fn raw_connection(db_path: &Path) -> SqliteConnection {
    let mut conn =
        SqliteConnection::establish(&db_path.to_string_lossy()).expect("SQLite として開けること");
    diesel::sql_query("PRAGMA journal_mode = WAL")
        .execute(&mut conn)
        .expect("WAL にできること");
    conn
}

/// 「バイナリの知っている最後の1つがまだ当たっていない」台帳を作る。
///
/// 全部当ててから1つだけ戻す。 手で up.sql の一部だけを選んで当てるより、
/// 実際に「一つ前の版のバイナリが作った台帳」に近い形になる。
fn pending_one_migration_short(db_path: &Path) -> SqliteConnection {
    let mut conn = raw_connection(db_path);
    conn.run_pending_migrations(MIGRATIONS)
        .expect("全部当てられること");
    conn.revert_last_migration(MIGRATIONS)
        .expect("最後の1つを戻せること");
    conn
}

fn insert_row(conn: &mut SqliteConnection, id: &str) {
    // `file_stem` は `id` から作る。`UNIQUE (file_stem, tone)` に当たらないように
    // ID ごとに変える——同じ台帳に複数回呼ぶ試験がある。
    diesel::sql_query(
        "INSERT INTO rows (id, text, file_stem, tone, state, ordinal) \
         VALUES (?, 'てすと', ?, 60, 'unrecorded', 0)",
    )
    .bind::<diesel::sql_types::Text, _>(id)
    .bind::<diesel::sql_types::Text, _>(id)
    .execute(conn)
    .expect("行を入れられること");
}

fn count_rows(db_path: &Path) -> i64 {
    let mut conn = SqliteConnection::establish(&db_path.to_string_lossy()).expect("開けること");
    rows::table
        .count()
        .get_result(&mut conn)
        .expect("数えられること")
}

/// `-wal` の中身があるか（無ければ空でも `false`）。
fn wal_has_content(db_path: &Path) -> bool {
    let mut s = db_path.as_os_str().to_owned();
    s.push("-wal");
    std::fs::metadata(PathBuf::from(s))
        .map(|m| m.len() > 0)
        .unwrap_or(false)
}

/// `<project>/migration-backup` だけがあり、`.part` も番号違いの名前も無いこと。
/// 返すのはその中の `project.db` の行数。
fn assert_single_backup(dir: &ProjectDir) -> i64 {
    let names: Vec<String> = std::fs::read_dir(dir.root())
        .expect("読めること")
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.starts_with("migration-backup"))
        .collect();
    assert_eq!(
        names,
        ["migration-backup".to_owned()],
        "控えは常に1つ、`.part` を残さない（`DEC-PKG-017`）"
    );
    count_rows(&dir.root().join("migration-backup").join("project.db"))
}

/// 保留の migration が1つある台帳に行を入れると、移行後も行が残り、
/// 控えが1つだけ出ること（`DEC-PLT-041`、`DEC-PKG-017`）。
#[test]
fn 保留の移行を当てても行が残り控えが1つ出る() {
    let dir = new_project("pending");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        insert_row(&mut conn, "r1");
        assert_eq!(
            db::migration_state(&dir.db_path()).expect("読めること"),
            MigrationState::Pending
        );
    }

    let outcome = dir.migrate_ledger().expect("移行できること");
    assert_eq!(outcome, MigrationOutcome::Migrated);
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Current
    );
    assert_eq!(count_rows(&dir.db_path()), 1, "行が残ること");
    assert_eq!(assert_single_backup(&dir), 1, "控えは移行前の行を持つこと");
}

/// まだ `-wal` にしかないコミットも、移行後に残ること。
///
/// 台帳を開いたまま（チェックポイントさせずに）移行を呼ぶ。 最後の接続が
/// 閉じるとチェックポイントが走ってしまうので、挿した接続を握ったままにする。
#[test]
fn wal_だけにあるコミットも移行後に残る() {
    let dir = new_project("wal-only");
    let mut conn = pending_one_migration_short(&dir.db_path());
    insert_row(&mut conn, "r1");
    assert!(
        wal_has_content(&dir.db_path()),
        "前提: チェックポイント前であること"
    );

    let outcome = dir.migrate_ledger().expect("移行できること");
    assert_eq!(outcome, MigrationOutcome::Migrated);
    assert_eq!(
        count_rows(&dir.db_path()),
        1,
        "WAL だけにあった行が残ること"
    );

    drop(conn);
}

/// バイナリの知らない版が当たっている台帳は、何も書かずに断る。
#[test]
fn 新しい版の台帳は何も書かずに断る() {
    let dir = new_project("newer");
    // 今の版まで開いて、すぐ閉じる（最後の接続が閉じるとチェックポイントが走る）。
    Ledger::open(dir.db_path()).expect("今の版までは開けること");

    {
        let mut conn = raw_connection(&dir.db_path());
        diesel::sql_query(
            "INSERT INTO __diesel_schema_migrations (version) VALUES ('9999-12-31-999999')",
        )
        .execute(&mut conn)
        .expect("知らない版を足せること");
    }
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Newer
    );

    let before = std::fs::read(dir.db_path()).expect("読めること");
    let err = dir.migrate_ledger().expect_err("断ること");
    assert!(matches!(err, ProjectError::MigrationNewer), "{err:?}");
    let after = std::fs::read(dir.db_path()).expect("読めること");
    assert_eq!(before, after, "バイト列が変わらないこと");

    // `Ledger::open` も同じ理由で断る。
    let open_err = Ledger::open(dir.db_path()).expect_err("開けないこと");
    assert!(
        matches!(open_err, LedgerError::MigrationNewer),
        "{open_err:?}"
    );
}

/// 確かめ（`foreign_key_check`）に落ちる台帳は、`project.db` を変えず、
/// 失敗の記録を残す。
#[test]
fn 確かめに落ちる台帳は元を残し失敗を記録する() {
    let dir = new_project("invalid");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        // foreign_keys を明示的に切り、存在しない行・セッションを指すテイクを作る。
        diesel::sql_query("PRAGMA foreign_keys = OFF")
            .execute(&mut conn)
            .expect("切れること");
        diesel::sql_query(
            "INSERT INTO takes (row_id, session_id, rel_path, frames, recorded_at, generation) \
             VALUES ('ghost-row', 424242, 'masters/ghost.wav', 1, '2026-01-01T00:00:00Z', 1)",
        )
        .execute(&mut conn)
        .expect("外部キー違反のまま入ること");
    }

    let before = std::fs::read(dir.db_path()).expect("読めること");
    let err = dir.migrate_ledger().expect_err("落ちること");
    assert!(
        matches!(
            err,
            ProjectError::MigrationDb(LedgerError::MigrationValidation)
        ),
        "{err:?}"
    );
    let after = std::fs::read(dir.db_path()).expect("読めること");
    assert_eq!(before, after, "稼働中の台帳は変わらないこと");

    let failure = dir.migration_failure().expect("記録が残ること");
    assert_eq!(failure.code, koeru_failure::Failure::code(&err));

    // 別の版の写しも、書きかけの控えも残らない。
    assert!(!dir.root().join("project.db.migrating").exists());
    assert!(!dir.root().join("migration-backup.part").exists());
}

/// 前回の途中の残り（`.migrating` / `migration-backup.part`）があっても、
/// 片付けてから始められる。
#[test]
fn 段の途中の残りがあっても開き直せる() {
    let dir = new_project("leftover");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        insert_row(&mut conn, "r1");
    }

    // 前回落ちたときの残りを手で再現する。
    std::fs::write(dir.root().join("project.db.migrating"), b"not a real db").expect("書けること");
    std::fs::create_dir(dir.root().join("migration-backup.part")).expect("作れること");
    std::fs::write(
        dir.root().join("migration-backup.part").join("project.db"),
        b"stale",
    )
    .expect("書けること");

    let outcome = dir.migrate_ledger().expect("片付けて移行できること");
    assert_eq!(outcome, MigrationOutcome::Migrated);
    assert_eq!(count_rows(&dir.db_path()), 1);
    assert!(!dir.root().join("project.db.migrating").exists());
    assert!(!dir.root().join("migration-backup.part").exists());
    assert_single_backup(&dir);
}

/// 2回移行しても控えは1つのまま。 中身は2回目の直前の版
/// （＝1回目の移行後の姿）を持つ（`DEC-PKG-017`）。
#[test]
fn 二回移行しても控えは1つで前の版を持つ() {
    let dir = new_project("twice");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        insert_row(&mut conn, "r1");
    }
    assert_eq!(
        dir.migrate_ledger().expect("1回目"),
        MigrationOutcome::Migrated
    );
    assert_eq!(count_rows(&dir.db_path()), 1);

    // もう一度「バイナリの知っている最後の1つが未適用」の状態へ戻し、行を足す。
    {
        let mut conn = raw_connection(&dir.db_path());
        conn.revert_last_migration(MIGRATIONS).expect("戻せること");
        insert_row(&mut conn, "r2");
    }
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending
    );
    // 2回目の移行の直前の姿（＝控えが持つべき姿）。
    let before_second = count_rows(&dir.db_path());
    assert_eq!(before_second, 2, "2回目の前は r1 と r2 の両方があること");

    assert_eq!(
        dir.migrate_ledger().expect("2回目"),
        MigrationOutcome::Migrated
    );
    assert_eq!(count_rows(&dir.db_path()), 2, "2回目で足した行も残ること");

    // 控えは1つで、2回目の直前の姿（r1 と r2 の両方）を持つ。 1回目の姿ではない。
    let backup_rows = assert_single_backup(&dir);
    assert_eq!(backup_rows, before_second, "控えは2回目の前の版であること");
}

/// 一覧のための読み（`migration_state` / `migration_failure`）は台帳を書き換えない。
#[test]
fn 一覧を読んでも台帳は変わらない() {
    let dir = new_project("listing");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        insert_row(&mut conn, "r1");
    }

    let before = std::fs::read(dir.db_path()).expect("読めること");
    for _ in 0..3 {
        let _ = db::migration_state(&dir.db_path());
        let _ = dir.migration_failure();
    }
    let after = std::fs::read(dir.db_path()).expect("読めること");
    assert_eq!(before, after, "読むだけでは1バイトも変わらないこと");
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending,
        "読んだだけでは移行しないこと"
    );
}

/// まだ何も版が無い新規の台帳は、今までどおり `Ledger::open` が全部当てて作る。
#[test]
fn 新規の台帳はledger_openが全部当てる() {
    let dir = new_project("fresh");
    // ファイルだけ作る（migration はまだ何も当てていない）。
    // `migration_state` は読み取り専用で開くので、無い物は作れない——
    // ここで作らないと「ファイルごと無い」という別の状態になる。
    drop(raw_connection(&dir.db_path()));
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Fresh
    );

    Ledger::open(dir.db_path()).expect("開けること");
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Current
    );

    // 台帳が既に今の版なので、`migrate_ledger` は何もしない。
    assert_eq!(
        dir.migrate_ledger().expect("何もしないこと"),
        MigrationOutcome::AlreadyCurrent
    );
}
