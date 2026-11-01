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

diesel::table! {
    project_revision (id) {
        id -> Integer,
        value -> BigInt,
    }
}

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

/// 音高 60 の行を1つ入れる。 枠を持つ版の台帳なら、先頭の枠に入れる（無ければ作る）。
fn insert_row(conn: &mut SqliteConnection, id: &str) {
    insert_row_at(conn, id, 60);
}

fn insert_row_at(conn: &mut SqliteConnection, id: &str, tone: i32) {
    // `file_stem` は `id` から作る。 一意制約に当たらないように ID ごとに変える
    // ——同じ台帳に複数回呼ぶ試験がある。
    if has_column(conn, "rows", "subbank_id") {
        plain_subbank(conn);
        diesel::sql_query(
            "INSERT INTO rows (id, text, file_stem, tone, state, ordinal, subbank_id) \
             VALUES (?, 'てすと', ?, ?, 'unrecorded', 0, (SELECT MIN(id) FROM subbanks))",
        )
        .bind::<diesel::sql_types::Text, _>(id)
        .bind::<diesel::sql_types::Text, _>(id)
        .bind::<diesel::sql_types::Integer, _>(tone)
        .execute(conn)
        .expect("行を入れられること");
        return;
    }
    diesel::sql_query(
        "INSERT INTO rows (id, text, file_stem, tone, state, ordinal) \
         VALUES (?, 'てすと', ?, ?, 'unrecorded', 0)",
    )
    .bind::<diesel::sql_types::Text, _>(id)
    .bind::<diesel::sql_types::Text, _>(id)
    .bind::<diesel::sql_types::Integer, _>(tone)
    .execute(conn)
    .expect("行を入れられること");
}

/// 枠が1つも無ければ、声色なし・単独音・音高 60 の枠を1つ作る。
///
/// 有無は SQL の外で見る。 1行も入れない `INSERT` でも版のトリガーは組み立てられ、
/// 版の表を戻した台帳では落ちる。
fn plain_subbank(conn: &mut SqliteConnection) {
    let present: ColumnCount = diesel::sql_query("SELECT COUNT(*) AS n FROM subbanks")
        .get_result(conn)
        .expect("数えられること");
    if present.n > 0 {
        return;
    }
    diesel::sql_query(
        "INSERT INTO subbanks (voice_color, method, preset_id, tone, ordinal) \
         VALUES (NULL, 'single', NULL, 60, 0)",
    )
    .execute(conn)
    .expect("枠を入れられること");
}

fn count_rows(db_path: &Path) -> i64 {
    let mut conn = SqliteConnection::establish(&db_path.to_string_lossy()).expect("開けること");
    rows::table
        .count()
        .get_result(&mut conn)
        .expect("数えられること")
}

/// `-wal` サイドカーの場所。
fn wal_path(db_path: &Path) -> PathBuf {
    let mut s = db_path.as_os_str().to_owned();
    s.push("-wal");
    PathBuf::from(s)
}

/// `-wal` の中身があるか（無ければ空でも `false`）。
fn wal_has_content(db_path: &Path) -> bool {
    std::fs::metadata(wal_path(db_path))
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
/// **`migrate_ledger` を呼ぶ台帳を開いたままにしない。** 単一の書き手の下では
/// `open_project` が `Ledger::open` より前に移行するので、移行の最中に別の接続が
/// 同じ台帳を開いていることは無い。 それに Windows は開いているファイルの上へ
/// rename できず（`PermissionDenied`）、unix でも開いた接続が古い inode を
/// 掴んだまま検証することになり、どちらも実際には起きない形を確かめてしまう。
///
/// 代わりに、別のプロジェクトで台帳を開いたまま行を入れ、`project.db` と
/// `project.db-wal` を（`-shm` を除いて）試験対象のプロジェクトへ `fs::copy` する。
/// `-shm` は写さなくてよい——SQLite が開くときに作り直す。 これで「落ちて WAL に
/// コミットが残ったまま」の状態を、開いた接続を持ち込まずに再現できる。
#[test]
fn wal_だけにあるコミットも移行後に残る() {
    let src = new_project("wal-only-src");
    let mut conn = pending_one_migration_short(&src.db_path());
    insert_row(&mut conn, "r1");
    assert!(
        wal_has_content(&src.db_path()),
        "前提: チェックポイント前であること"
    );

    // `conn` を開いたまま写す。 SQLite は共有読み書きで開くので読める。
    let dir = new_project("wal-only");
    std::fs::copy(src.db_path(), dir.db_path()).expect("project.db を写せること");
    std::fs::copy(wal_path(&src.db_path()), wal_path(&dir.db_path()))
        .expect("project.db-wal を写せること");
    assert!(
        wal_has_content(&dir.db_path()),
        "写した先にも WAL の中身が残っていること"
    );

    drop(conn);

    let outcome = dir.migrate_ledger().expect("移行できること");
    assert_eq!(outcome, MigrationOutcome::Migrated);
    assert_eq!(
        count_rows(&dir.db_path()),
        1,
        "WAL だけにあった行が残ること"
    );
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

/// `project_revision` を持つ表以外の、台帳の全表（`DEC-PLT-043`）。
///
/// `sqlite_master` から機械的に読む。 手で並べた一覧と突き合わせない——
/// 一覧を足し忘れる形の見落としを、この試験自身が再現してしまう。
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

/// 取り込み済みの曲を持つ前の版の台帳を移行すると、曲は残り、原文の表が足される
/// （`DEC-PLT-054`）。 移行より前に取り込んだ曲は原文を持たない。
///
/// 戻して当て直しても同じ形になる（`down.sql` が壊れていると、次の移行の試験が
/// 最後の1つを戻せずに落ちる）。
#[test]
fn 移行後も取り込み済みの曲が残り原文の表が足される() {
    let dir = new_project("song-imports-migration");
    {
        let mut conn = pending_one_migration_short(&dir.db_path());
        // この移行より後に別の移行が入っても、この移行の前まで戻す。
        while business_tables(&mut conn).contains(&"song_imports".to_owned()) {
            conn.revert_last_migration(MIGRATIONS)
                .expect("1つ戻せること");
        }
        for table in ["song_imports", "song_import_songs"] {
            assert!(
                !business_tables(&mut conn).contains(&table.to_owned()),
                "前提: 移行前には {table} が無い"
            );
        }
        diesel::sql_query(
            "INSERT INTO songs (id, title, source, license, bundled, in_bank, added_at) \
             VALUES ('old', 'ふるさと', '本人が持ち込んだ UST', '不明（配布物には含めない）', 0, 1, '2026-09-01T00:00:00Z')",
        )
        .execute(&mut conn)
        .expect("曲を入れられること");
    }

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let songs = l.all_songs().expect("引けること");
    assert_eq!(songs.len(), 1, "取り込み済みの曲が残る");
    assert_eq!(songs[0].0, "old");
    assert_eq!(
        l.song_source("old").expect("引けること"),
        None,
        "移行より前の曲は原文を持たない（ファイルが残っていない）"
    );

    // 移行後は取り込みを確定できる。
    let op = db::OperationId::generate();
    let new_song = songs[0].1.clone();
    let result = l
        .commit_song_import(&db::SongImportRequest {
            operation: &op,
            file_name: "ふるさと.ust",
            bytes: b"x",
            songs: &[("new".to_owned(), new_song)],
            imported_at: "2026-10-13T00:00:00Z",
        })
        .expect("取り込める");
    assert!(matches!(result, db::SongImport::Imported(_)));
    drop(l);

    let mut conn =
        SqliteConnection::establish(&dir.db_path().to_string_lossy()).expect("開けること");
    for table in ["song_imports", "song_import_songs"] {
        assert_eq!(
            revision_trigger_count(&mut conn, table),
            3,
            "{table} に版のトリガーが3本ある"
        );
    }
    // この移行を戻して、当て直せる。
    while business_tables(&mut conn).contains(&"song_imports".to_owned()) {
        conn.revert_last_migration(MIGRATIONS)
            .expect("1つ戻せること");
    }
    conn.run_pending_migrations(MIGRATIONS)
        .expect("当て直せること");
    assert!(business_tables(&mut conn).contains(&"song_imports".to_owned()));
}

/// 前の版から移行すると、`project_revision` が種を持って現れ、
/// 既存の全表にちょうど3本ずつ（ins/upd/del）のトリガーが揃うこと（`DEC-PLT-043`）。
///
/// 移行前に入れた行は、移行そのもの（DDL）ではトリガーを遡って発火させない
/// ——`value` は種のままの 0 で、以後の書き込みで初めて動き出す。
#[test]
fn 移行後にproject_revisionと全表のトリガーが揃う() {
    let dir = new_project("revision-migration");
    {
        // 「最後の1つ」ではなく名指しで戻す。 後ろに別の移行が足されても、この試験が
        // 見たいのは `project_revision` の移行だけ。
        let mut conn = raw_connection(&dir.db_path());
        conn.run_pending_migrations(MIGRATIONS)
            .expect("全部当てられること");
        // 枠を入れると版のトリガーが動く。 版の表を戻す前に入れておく。
        plain_subbank(&mut conn);
        revert_named(&mut conn, "project_revision");
        insert_row(&mut conn, "r1");
    }

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );
    assert_eq!(count_rows(&dir.db_path()), 1, "移行前に入れた行が残ること");

    let mut conn =
        SqliteConnection::establish(&dir.db_path().to_string_lossy()).expect("開けること");
    let value: i64 = project_revision::table
        .find(1)
        .select(project_revision::value)
        .first(&mut conn)
        .expect("project_revision の種があること");
    assert_eq!(value, 0, "移行そのものはトリガーを遡って発火させないこと");

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

    // 書き込めば実際に進む。
    insert_row(&mut conn, "r2");
    let after: i64 = project_revision::table
        .find(1)
        .select(project_revision::value)
        .first(&mut conn)
        .expect("読めること");
    assert!(after > value, "書き込みで版が進むこと");
}

/// 名前に `fragment` を含む移行だけを戻す。 「最後の1つ」を戻す手順は、後ろに別の移行が
/// 足されると別のものを戻す。
fn revert_named(conn: &mut SqliteConnection, fragment: &str) {
    use diesel::migration::MigrationSource as _;
    let all = MIGRATIONS.migrations().expect("移行を読めること");
    let target = all
        .iter()
        .find(|m| m.name().to_string().contains(fragment))
        .unwrap_or_else(|| panic!("{fragment} の移行がある"));
    conn.revert_migration(&**target).expect("戻せること");
}

#[derive(diesel::QueryableByName)]
struct ColumnCount {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    n: i64,
}

fn has_column(conn: &mut SqliteConnection, table: &str, column: &str) -> bool {
    diesel::sql_query(format!(
        "SELECT COUNT(*) AS n FROM pragma_table_info('{table}') WHERE name = '{column}'"
    ))
    .get_result::<ColumnCount>(conn)
    .expect("列を調べられること")
    .n == 1
}

/// マスターの指紋の列を足す前の台帳が、移行で列を得ても中身を落とさず、指紋が NULL のまま
/// 残る。 要るときに書き、書いたあとは変えない（`DEC-PLT-053`）。
///
/// **移行は指紋を埋めない。** SQL はファイルを読めない。 埋めるのはアプリで、ここは
/// 「埋まっていない」ことと、台帳が埋められる形であることを見る。
#[test]
fn 旧い台帳は指紋が空のまま開き_書いたあとは変わらない() {
    use koeru_core::db::{FinalizedTake, SessionSnapshot};
    use koeru_core::fingerprint::Digest;
    use koeru_core::inventory::UnitSet;
    use koeru_core::reclist::generate_single;

    let dir = new_project("master-digest");
    // 今の版の台帳でテイクを2つ作る。
    {
        let mut l = Ledger::open(dir.db_path()).expect("開けること");
        let list = generate_single(UnitSet::Core, 3).expect("生成できること");
        l.install_reclist(&list, 60).expect("書けること");
        let session = l
            .start_session(&SessionSnapshot {
                started_at: "2026-10-13T00:00:00Z".into(),
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
            .expect("始められること");
        for (i, row) in list.iter().take(2).enumerate() {
            l.commit_take(&FinalizedTake {
                row_id: row.id.clone(),
                session_id: session,
                rel_path: format!("masters/m{i}.wav"),
                frames: 44_100,
                recorded_at: "2026-10-13T00:00:01Z".into(),
            })
            .expect("載せられること");
        }
    }
    // 列を足す前の姿へ戻す。 戻すと列ごと指紋が消えるが、テイクは残る。
    {
        let mut conn = raw_connection(&dir.db_path());
        revert_named(&mut conn, "master_content_hash");
        assert!(!has_column(&mut conn, "takes", "master_sha256"));
    }
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending
    );

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );
    {
        let mut conn = raw_connection(&dir.db_path());
        assert!(has_column(&mut conn, "takes", "master_sha256"));
    }

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let (id_a, none_a) = l
        .master_digest_at("masters/m0.wav")
        .expect("読めること")
        .expect("テイクが残っていること");
    assert_eq!(none_a, None, "移行は指紋を埋めない");
    let (id_b, none_b) = l
        .master_digest_at("masters\\m1.wav")
        .expect("読めること")
        .expect("区切りが違っても同じ場所を引けること");
    assert_eq!(none_b, None);
    assert!(
        l.master_digest_at("masters/none.wav")
            .expect("読めること")
            .is_none()
    );

    // 要るときに書く。 先に書かれたものが残る。
    let first = Digest::of(b"first");
    assert_eq!(l.record_master_digest(id_a, &first).expect("書ける"), first);
    assert_eq!(
        l.record_master_digest(id_a, &Digest::of(b"second"))
            .expect("返ること"),
        first,
        "2回目は書き換えず、残っている値を返す"
    );
    assert_eq!(l.master_digest_of(id_a).expect("読める"), Some(first));
    assert_eq!(l.master_digest_of(id_b).expect("読める"), None);
    assert!(matches!(
        l.record_master_digest(424_242, &first),
        Err(LedgerError::UnknownTake)
    ));
    assert!(matches!(
        l.master_digest_of(424_242),
        Err(LedgerError::UnknownTake)
    ));
}

/// 枠（`DEC-RCL-018`）を持つ前の版の台帳を作る。 後ろに別の移行が足されても、枠の移行の前まで戻す。
fn pending_before_subbanks(db_path: &Path) -> SqliteConnection {
    let mut conn = raw_connection(db_path);
    conn.run_pending_migrations(MIGRATIONS)
        .expect("全部当てられること");
    while business_tables(&mut conn).contains(&"subbanks".to_owned()) {
        conn.revert_last_migration(MIGRATIONS)
            .expect("1つ戻せること");
    }
    assert!(
        !has_column(&mut conn, "rows", "subbank_id"),
        "前提: 枠が無い"
    );
    conn
}

/// テイクを1つ入れ、採用する。 収録セッションが無ければ作る。
fn insert_adopted_take(conn: &mut SqliteConnection, row_id: &str) {
    diesel::sql_query(
        "INSERT INTO sessions (started_at, device_id, sample_rate_hz, channels, effects_state, \
         route) SELECT 't', 'd', 48000, 1, 'clean', 'r' WHERE NOT EXISTS (SELECT 1 FROM sessions)",
    )
    .execute(conn)
    .expect("セッションを入れられること");
    diesel::sql_query(
        "INSERT INTO takes (row_id, session_id, rel_path, frames, recorded_at, generation) \
         VALUES (?, (SELECT MIN(id) FROM sessions), 'audio/' || ? || '_1.wav', 1, 't', 1)",
    )
    .bind::<diesel::sql_types::Text, _>(row_id)
    .bind::<diesel::sql_types::Text, _>(row_id)
    .execute(conn)
    .expect("テイクを入れられること");
    diesel::sql_query(
        "INSERT INTO adopted_takes (row_id, take_id) VALUES (?, (SELECT MAX(id) FROM takes))",
    )
    .bind::<diesel::sql_types::Text, _>(row_id)
    .execute(conn)
    .expect("採用できること");
}

fn project_with(tag: &str, m: &Manifest) -> ProjectDir {
    Library::open(tmp(tag))
        .expect("開けること")
        .create(m)
        .expect("作れること")
}

/// 既存の project は、作ったときの収録音階ごとに声色なしの枠を1つずつ持つ（`DEC-RCL-018`）。
/// 方式とプリセットは manifest から、並びは音階の昇順。 行もテイクも、自分の音階の枠へ
/// そのまま属する。
#[test]
fn 既存の台帳は収録音階ごとに声色なしの枠を1つずつ持つ() {
    use koeru_core::alias::Method as AliasMethod;

    let m = Manifest {
        method: Method::MultiPitchSequential,
        preset_id: Some("sequential".to_owned()),
        ..manifest()
    };
    let dir = project_with("subbanks-multi", &m);
    {
        let mut conn = pending_before_subbanks(&dir.db_path());
        // 入れた順ではなく、音階の昇順に並ぶ。
        insert_row_at(&mut conn, "s001@A4", 69);
        insert_row_at(&mut conn, "s001@G3", 55);
        insert_row_at(&mut conn, "s002@G3", 55);
        insert_row_at(&mut conn, "s001@D4", 62);
        insert_adopted_take(&mut conn, "s001@G3");
    }

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let subbanks = l.subbanks().expect("読めること");
    assert_eq!(
        subbanks.iter().map(|s| s.tone).collect::<Vec<_>>(),
        [55, 62, 69]
    );
    for s in &subbanks {
        assert_eq!(s.voice_color, None, "声色なし");
        assert_eq!(
            s.method,
            AliasMethod::Sequential,
            "多音階連続音は音階ごとに連続音"
        );
        assert_eq!(s.preset_id.as_deref(), Some("sequential"));
        assert_eq!(s.edited_range, None, "担当音域は既定のまま");
    }
    assert_eq!(
        subbanks.iter().map(|s| s.rows).collect::<Vec<_>>(),
        [2, 1, 1]
    );
    assert_eq!(
        subbanks
            .iter()
            .map(|s| (s.takes, s.recorded_rows))
            .collect::<Vec<_>>(),
        [(1, 1), (0, 0), (0, 0)],
        "テイクは自分の音階の枠に残る"
    );
    for (row, at) in [
        ("s001@G3", 0),
        ("s002@G3", 0),
        ("s001@D4", 1),
        ("s001@A4", 2),
    ] {
        assert_eq!(
            l.row_subbank(row).expect("属する"),
            subbanks[at].id,
            "{row}"
        );
    }
    assert_eq!(l.recording_tones().expect("引ける"), [55, 62, 69]);
}

/// 単音階の project は枠を1つ持って開く。 プリセットを持たない古い manifest なら、枠も持たない。
#[test]
fn 単音階の既存の台帳は枠を1つ持って開く() {
    use koeru_core::alias::Method as AliasMethod;

    let dir = new_project("subbanks-single");
    {
        let mut conn = pending_before_subbanks(&dir.db_path());
        insert_row(&mut conn, "s001");
        insert_row(&mut conn, "s002");
    }
    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let subbanks = l.subbanks().expect("読めること");
    assert_eq!(subbanks.len(), 1);
    assert_eq!(subbanks[0].tone, 60);
    assert_eq!(subbanks[0].method, AliasMethod::Single);
    assert_eq!(subbanks[0].preset_id, None);
    assert_eq!(subbanks[0].rows, 2);
}

/// manifest が読めなければ、`project.db` を変えずに止め、失敗を記録する（`DEC-PLT-041`）。
/// 方式を推し量って枠を作らない。
#[test]
fn manifest_が読めなければ台帳を変えずに失敗を記録する() {
    let dir = new_project("subbanks-bad-manifest");
    {
        let mut conn = pending_before_subbanks(&dir.db_path());
        insert_row(&mut conn, "s001");
    }
    std::fs::write(dir.manifest_path(), "method = 'no-such-method'\n").expect("書けること");

    let before = std::fs::read(dir.db_path()).expect("読めること");
    let err = dir.migrate_ledger().expect_err("落ちること");
    let after = std::fs::read(dir.db_path()).expect("読めること");
    assert_eq!(before, after, "稼働中の台帳は変わらないこと");
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending
    );

    let failure = dir.migration_failure().expect("記録が残ること");
    assert_eq!(failure.code, koeru_failure::Failure::code(&err));
    assert!(!dir.root().join("project.db.migrating").exists());
    assert!(!dir.root().join("migration-backup.part").exists());
}

/// 移行が二度走っても枠は増えない。 前回が写しに当て終えたところで落ち、枠を持つ写しが
/// 残っていても、写しを捨てて `project.db` から作り直す。
#[test]
fn 移行が二度走っても枠は増えない() {
    let dir = new_project("subbanks-twice");
    {
        let mut conn = pending_before_subbanks(&dir.db_path());
        insert_row_at(&mut conn, "s001@G3", 55);
        insert_row_at(&mut conn, "s001@D4", 62);
    }
    let leftover = dir.root().join("project.db.migrating");
    std::fs::copy(dir.db_path(), &leftover).expect("写せること");
    db::migrate_and_validate(
        &leftover,
        &db::MigrationManifest {
            method: koeru_core::alias::Method::Single,
            preset_id: None,
        },
    )
    .expect("写しに当てられること");

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );
    assert_eq!(
        dir.migrate_ledger().expect("二度目は何もしない"),
        MigrationOutcome::AlreadyCurrent
    );
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    assert_eq!(l.subbanks().expect("読めること").len(), 2);
}

/// manifest を置かずに当てると、行のある台帳では落ちる。 方式を推し量って枠を埋めない。
/// 落ちても、その接続で書いたものは残らない。
#[test]
fn manifest_を置かずに当てると行のある台帳は落ちる() {
    let dir = new_project("subbanks-unseeded");
    {
        let mut conn = pending_before_subbanks(&dir.db_path());
        insert_row(&mut conn, "s001");
    }
    {
        let mut conn = raw_connection(&dir.db_path());
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut conn)
            .expect("効かせられること");
        assert!(conn.run_pending_migrations(MIGRATIONS).is_err());
    }
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending
    );
    assert_eq!(count_rows(&dir.db_path()), 1, "行は残ること");
    let mut conn = raw_connection(&dir.db_path());
    assert!(!business_tables(&mut conn).contains(&"subbanks".to_owned()));
}

/// 64字でないものは台帳が断り、64字の16進でないものは置いてあっても読めないと言う。
/// 計算し直して隠さない。
#[test]
fn 壊れた指紋は置けず_置いてあれば読めないと言う() {
    let dir = new_project("master-digest-bad");
    let mut conn = raw_connection(&dir.db_path());
    conn.run_pending_migrations(MIGRATIONS)
        .expect("全部当てられること");
    diesel::sql_query(
        "INSERT INTO sessions (started_at, device_id, sample_rate_hz, channels, effects_state, \
         route) VALUES ('t', 'd', 48000, 1, 'clean', 'r')",
    )
    .execute(&mut conn)
    .expect("セッションを入れられること");
    insert_row(&mut conn, "r1");
    diesel::sql_query(
        "INSERT INTO takes (row_id, session_id, rel_path, frames, recorded_at, generation) \
         VALUES ('r1', 1, 'masters/x.wav', 1, 't', 1)",
    )
    .execute(&mut conn)
    .expect("テイクを入れられること");

    assert!(
        diesel::sql_query("UPDATE takes SET master_sha256 = 'abc'")
            .execute(&mut conn)
            .is_err(),
        "長さが違うものは台帳が断る"
    );
    diesel::sql_query(format!(
        "UPDATE takes SET master_sha256 = '{}'",
        "Z".repeat(64)
    ))
    .execute(&mut conn)
    .expect("長さが合えば入る（字までは SQL が見ない）");
    drop(conn);

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    assert!(matches!(
        l.master_digest_of(1),
        Err(LedgerError::MalformedMasterDigest)
    ));
    assert!(matches!(
        l.record_master_digest(1, &koeru_core::fingerprint::Digest::of(b"x")),
        Err(LedgerError::MalformedMasterDigest)
    ));
}

/// テイクの名前の列を足す前の台帳を移行すると、テイクは名前を持たないまま残り
/// （画面は既定の表示を出す）、移行のあとで付けられる。 戻して当て直せる。
#[test]
fn 旧い台帳のテイクは名前を持たずに残り_移行のあとで名前を付けられる() {
    use koeru_core::db::{FinalizedTake, SessionSnapshot};
    use koeru_core::inventory::UnitSet;
    use koeru_core::reclist::generate_single;

    let dir = new_project("take-labels");
    let take_id = {
        let mut l = Ledger::open(dir.db_path()).expect("開けること");
        let list = generate_single(UnitSet::Core, 3).expect("生成できること");
        l.install_reclist(&list, 60).expect("書けること");
        let session = l
            .start_session(&SessionSnapshot {
                started_at: "2026-11-01T00:00:00Z".into(),
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
            .expect("始められること");
        l.commit_take(&FinalizedTake {
            row_id: list[0].id.clone(),
            session_id: session,
            rel_path: "masters/m0.wav".into(),
            frames: 44_100,
            recorded_at: "2026-11-01T00:00:01Z".into(),
        })
        .expect("載せられること")
    };
    {
        let mut conn = raw_connection(&dir.db_path());
        revert_named(&mut conn, "take_labels");
        assert!(!has_column(&mut conn, "takes", "label"));
    }
    assert_eq!(
        db::migration_state(&dir.db_path()).expect("読めること"),
        MigrationState::Pending
    );

    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let kept = l
        .take(take_id)
        .expect("読めること")
        .expect("テイクが残っていること");
    assert_eq!(kept.label, None, "移行は名前を付けない");
    let named = l
        .set_take_label(take_id, Some("テイク 1"))
        .expect("書けること")
        .expect("テイクがあること");
    assert_eq!(named.label.as_deref(), Some("テイク 1"));
    drop(l);

    let mut conn = raw_connection(&dir.db_path());
    revert_named(&mut conn, "take_labels");
    assert!(!has_column(&mut conn, "takes", "label"), "戻せること");
    conn.run_pending_migrations(MIGRATIONS)
        .expect("当て直せること");
    assert!(has_column(&mut conn, "takes", "label"));
}

/// 配布物（`DEC-PKG-018`）を持つ前の版の台帳を作る。 後ろに別の移行が足されても、この移行だけを戻す。
fn pending_before_distributions(db_path: &Path) -> SqliteConnection {
    let mut conn = raw_connection(db_path);
    conn.run_pending_migrations(MIGRATIONS)
        .expect("全部当てられること");
    revert_named(&mut conn, "_distributions");
    assert!(
        !business_tables(&mut conn).contains(&"distributions".to_owned()),
        "前提: 配布物が無い"
    );
    assert!(
        !has_column(&mut conn, "releases", "distribution_id"),
        "前提: 書き出しの記録は配布物を持たない"
    );
    conn
}

/// 枠を2つ入れる。 音階の昇順。
fn two_subbanks(conn: &mut SqliteConnection) {
    for (tone, ordinal) in [(55, 0), (62, 1)] {
        diesel::sql_query(
            "INSERT INTO subbanks (voice_color, method, preset_id, tone, ordinal) \
             VALUES (NULL, 'single', 'single', ?, ?)",
        )
        .bind::<diesel::sql_types::Integer, _>(tone)
        .bind::<diesel::sql_types::Integer, _>(ordinal)
        .execute(conn)
        .expect("枠を入れられること");
    }
}

/// いまの配布の設定（フォルダ名・版・規約・名義）を書く。
fn legacy_settings(conn: &mut SqliteConnection, name: &str, version: Option<&str>) {
    diesel::sql_query(
        "INSERT INTO distribution (id, distribution_name, profile, author, version, terms) \
         VALUES (1, ?, 'classic', 'おれ', ?, '音源の規約')",
    )
    .bind::<diesel::sql_types::Text, _>(name)
    .bind::<diesel::sql_types::Nullable<diesel::sql_types::Text>, _>(version)
    .execute(conn)
    .expect("設定を入れられること");
}

fn legacy_release(conn: &mut SqliteConnection, seq: i32, version: &str) {
    diesel::sql_query(
        "INSERT INTO releases (seq, version, method, alias_count, validation, oto_hash, \
         terms_hash, archive_name, released_at) \
         VALUES (?, ?, 'single', 10, 'passed', 'h', 't', ? || '.zip', '2026-10-01T00:00:00Z')",
    )
    .bind::<diesel::sql_types::Integer, _>(seq)
    .bind::<diesel::sql_types::Text, _>(version)
    .bind::<diesel::sql_types::Text, _>(format!("{seq:06}-{version}"))
    .execute(conn)
    .expect("記録を入れられること");
}

/// 既存のプロジェクトの配布の設定は「すべて」に残る（`DEC-PKG-018`）。 フォルダ名と版は配布物へ移り、
/// 音源で1つの値は `distribution` に残る。 書き出した記録は「すべて」のものになる。
#[test]
fn 既存の配布の設定は_すべて_に残る() {
    let dir = new_project("distributions-existing");
    {
        let mut conn = pending_before_distributions(&dir.db_path());
        two_subbanks(&mut conn);
        legacy_settings(&mut conn, "Koeru-Old", Some("v2"));
        legacy_release(&mut conn, 1, "v1");
    }
    assert_eq!(
        dir.migrate_ledger().expect("移行できること"),
        MigrationOutcome::Migrated
    );

    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let all = l.distributions().expect("読めること");
    assert_eq!(all.len(), 1, "配布物は「すべて」だけ");
    let d = &all[0];
    assert_eq!(d.name, "すべて");
    assert_eq!(
        d.archive_name.as_deref(),
        Some("Koeru-Old"),
        "フォルダ名が残る"
    );
    assert_eq!(
        d.draft_version.as_deref(),
        Some("v2"),
        "版が下書きとして残る"
    );
    assert_eq!(d.terms, None, "配布物の規約は上書きしていない");
    let subbanks: Vec<_> = l
        .subbanks()
        .expect("読めること")
        .iter()
        .map(|s| s.id)
        .collect();
    assert_eq!(
        d.subbanks.iter().map(|m| m.subbank).collect::<Vec<_>>(),
        subbanks,
        "いまある枠をすべて入れる"
    );

    // 音源で1つの値は、配布物へ移さない。
    let settings = l
        .distribution()
        .expect("読めること")
        .expect("残っていること");
    assert_eq!(settings.author.as_deref(), Some("おれ"));
    assert_eq!(settings.terms.as_deref(), Some("音源の規約"));

    // 書き出しの記録は「すべて」のもの。 基準を残していない古い記録は数えない。
    let releases = l.releases().expect("読めること");
    assert_eq!(releases.len(), 1);
    assert_eq!(releases[0].distribution, d.id);
    assert_eq!(
        l.rows_changed_since_release(d.id).expect("読めること"),
        None
    );
}

/// 版が書き出し済みの番号と重なるなら、下書きにしない（`TR-PKG-60`）。 何も書いていなければ、
/// 書き出していないときだけ最初の番号を出しておく。
#[test]
fn 移行は書き出した番号と重なる版を下書きにしない() {
    // 書いてある版が、書き出した番号と同じ。
    let dir = new_project("distributions-version-taken");
    {
        let mut conn = pending_before_distributions(&dir.db_path());
        two_subbanks(&mut conn);
        legacy_settings(&mut conn, "Koeru", Some("v1"));
        legacy_release(&mut conn, 1, "v1");
    }
    dir.migrate_ledger().expect("移行できること");
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    assert_eq!(
        l.distributions().expect("読めること")[0].draft_version,
        None
    );

    // 何も書いておらず、書き出してもいない。
    let dir = new_project("distributions-fresh");
    {
        let mut conn = pending_before_distributions(&dir.db_path());
        two_subbanks(&mut conn);
    }
    dir.migrate_ledger().expect("移行できること");
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let all = l.distributions().expect("読めること");
    assert_eq!(all[0].archive_name, None, "既定の名前は開くときに入れる");
    assert_eq!(all[0].draft_version.as_deref(), Some("1.0.0"));

    // 何も書いておらず、書き出している。
    let dir = new_project("distributions-released");
    {
        let mut conn = pending_before_distributions(&dir.db_path());
        two_subbanks(&mut conn);
        legacy_release(&mut conn, 1, "v1");
    }
    dir.migrate_ledger().expect("移行できること");
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    assert_eq!(
        l.distributions().expect("読めること")[0].draft_version,
        None
    );
}

/// 新規の台帳も「すべて」を持つ。 配布物は少なくとも1つ（`TR-PKG-58`）。
#[test]
fn 新規の台帳も_すべて_を持つ() {
    let dir = new_project("distributions-new");
    let mut l = Ledger::open(dir.db_path()).expect("開けること");
    let all = l.distributions().expect("読めること");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].name, "すべて");
    assert!(all[0].subbanks.is_empty(), "枠はまだ無い");
}

/// この移行を戻して、当て直せる。 戻すとフォルダ名と下書きが `distribution` へ帰る。
#[test]
fn 配布物の移行は戻して当て直せる() {
    let dir = new_project("distributions-revert");
    {
        let mut conn = pending_before_distributions(&dir.db_path());
        two_subbanks(&mut conn);
        legacy_settings(&mut conn, "Koeru-Old", Some("v2"));
        legacy_release(&mut conn, 1, "v1");
    }
    dir.migrate_ledger().expect("移行できること");
    {
        let mut l = Ledger::open(dir.db_path()).expect("開けること");
        let id = l.distributions().expect("読めること")[0].id;
        l.revise_distribution(
            id,
            &koeru_core::db::shelf::DistributionRevision {
                archive_name: Some("Koeru-New"),
                draft_version: Some("v9"),
                ..Default::default()
            },
        )
        .expect("書けること")
        .expect("直せること");
    }

    let mut conn = raw_connection(&dir.db_path());
    revert_named(&mut conn, "_distributions");
    assert!(!business_tables(&mut conn).contains(&"distributions".to_owned()));
    assert!(!has_column(&mut conn, "releases", "distribution_id"));
    #[derive(diesel::QueryableByName)]
    struct Legacy {
        #[diesel(sql_type = diesel::sql_types::Text)]
        distribution_name: String,
        #[diesel(sql_type = diesel::sql_types::Nullable<diesel::sql_types::Text>)]
        version: Option<String>,
    }
    let legacy: Legacy = diesel::sql_query("SELECT distribution_name, version FROM distribution")
        .get_result(&mut conn)
        .expect("読めること");
    assert_eq!(legacy.distribution_name, "Koeru-New");
    assert_eq!(legacy.version.as_deref(), Some("v9"));

    conn.run_pending_migrations(MIGRATIONS)
        .expect("当て直せること");
    assert!(business_tables(&mut conn).contains(&"distributions".to_owned()));
}
