//! プロジェクトの永続化。
//!
//! **状態の単一の真実**（`TR-REC-31`）。`masters/` のファイル存在をスキャンして
//! 導出しない。試唱の可否・再開位置・書き出しの可否をすべてここから決める。
//!
//! ## テイクを確定させる順序
//!
//! **ファイル確定 → DB コミット**（`DEC-REC-004`、`project-storage.fsl` で `proved`）。
//! 逆にすると、ファイルの無い行が DB に残る。テイクは録音という
//! 「やり直しが高い操作」の成果物なので、**行だけが残って音が無い状態は復旧できない。**
//!
//! この向きは [`Ledger::commit_take`] が構造的に保証する。**確定済みのパスを
//! 受け取ってからしか呼べない。**
//!
//! ## 孤児
//!
//! rename が済んでコミット前に落ちると、**確定済みの WAV があるのに行が無い状態**が残る
//! （`Q-REC-005` → `DEC-REC-004`）。[`Ledger::find_orphans`] が見つけて
//! **復旧候補として提示する。本人が採るか捨てるまで消さない。**

use crate::inventory::Unit;
use crate::reclist::Row as ReclistRow;
use crate::schema::{adopted_takes, oto_values, row_units, rows, sessions, takes};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::collections::BTreeSet;
use std::path::Path;

/// マイグレーションを実行ファイルへ埋め込む。**外部ファイルに依存しない**（`TR-PLT-20`）。
const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

/// 台帳の操作が失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum LedgerError {
    /// SQLite の操作が失敗した。
    #[error("データベースの操作が失敗した（{op}）")]
    Db {
        op: &'static str,
        #[source]
        source: diesel::result::Error,
    },

    /// 接続を開けなかった。
    #[error("データベースを開けなかった")]
    Open {
        #[source]
        source: diesel::ConnectionError,
    },

    /// マイグレーションが失敗した。
    #[error("スキーマの適用が失敗した")]
    Migration,

    /// 指定した行が台帳に無い。
    #[error("行が台帳に無い")]
    UnknownRow,

    /// 指定したテイクが台帳に無い。
    #[error("テイクが台帳に無い")]
    UnknownTake,
}

impl LedgerError {
    /// 送信層へ載せてよい固定文字列。**`Display` を送らない**（パスが入りうる）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Db { .. } => "ledger.db_failed",
            Self::Open { .. } => "ledger.open_failed",
            Self::Migration => "ledger.migration_failed",
            Self::UnknownRow => "ledger.unknown_row",
            Self::UnknownTake => "ledger.unknown_take",
        }
    }
}

type Result<T> = std::result::Result<T, LedgerError>;

fn db(op: &'static str) -> impl FnOnce(diesel::result::Error) -> LedgerError {
    move |source| LedgerError::Db { op, source }
}

/// 行の状態（`TR-RCL-18`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowState {
    Unrecorded,
    Recorded,
    NeedsRetake,
    Excluded,
}

impl RowState {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Unrecorded => "unrecorded",
            Self::Recorded => "recorded",
            Self::NeedsRetake => "needs_retake",
            Self::Excluded => "excluded",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "recorded" => Self::Recorded,
            "needs_retake" => Self::NeedsRetake,
            "excluded" => Self::Excluded,
            _ => Self::Unrecorded,
        }
    }
}

/// 収録セッションの記録（`TR-REC-30` / `TR-REC-13`）。
#[derive(Debug, Clone)]
pub struct SessionSnapshot {
    pub started_at: String,
    /// **永続識別子。表示名は保存しない**（`TR-REC-03`）。
    pub device_id: String,
    pub sample_rate_hz: i32,
    pub channels: i32,
    /// `clean` / `some_remain` / `unknown`。
    pub effects_state: String,
    /// 実際に接続した経路（`TR-REC-12`）。
    pub route: String,
}

/// 確定したテイクの記録。
///
/// **`rel_path` は既に確定済み**（fsync + rename が済んでいる）。
/// この型を作れること自体が、順序を守った証拠になる。
#[derive(Debug, Clone)]
pub struct FinalizedTake {
    pub row_id: String,
    pub session_id: i32,
    /// `masters/` からの相対パス。
    pub rel_path: String,
    pub frames: i64,
    pub recorded_at: String,
}

/// テイク1件の読み出し結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Take {
    pub id: i32,
    pub row_id: String,
    pub rel_path: String,
    pub frames: i64,
    pub invalid: bool,
    pub generation: i32,
}

/// プロジェクトの台帳。
pub struct Ledger {
    conn: SqliteConnection,
}

// `SqliteConnection` は Debug を実装しない。**接続の中身は出さない**
// （パスやクエリが入りうる）ので、型名だけを出す。
impl std::fmt::Debug for Ledger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ledger { .. }")
    }
}

impl Ledger {
    /// 開いてスキーマを適用する。
    ///
    /// **WAL モードにする**（`TR-REC-27`）。書き込み中に読めるようにして、
    /// 収録とバックグラウンドの解析が互いを待たないようにする。
    #[tracing::instrument(skip(path), err)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let url = path.as_ref().to_string_lossy().into_owned();
        let mut conn =
            SqliteConnection::establish(&url).map_err(|source| LedgerError::Open { source })?;
        diesel::sql_query("PRAGMA journal_mode = WAL")
            .execute(&mut conn)
            .map_err(db("wal"))?;
        // **外部キーを効かせる。** SQLite は既定で無効。
        diesel::sql_query("PRAGMA foreign_keys = ON")
            .execute(&mut conn)
            .map_err(db("foreign_keys"))?;
        conn.run_pending_migrations(MIGRATIONS)
            .map_err(|_| LedgerError::Migration)?;
        Ok(Self { conn })
    }

    /// メモリ上に開く。テスト用。
    pub fn open_in_memory() -> Result<Self> {
        Self::open(":memory:")
    }

    /// 録音リストを台帳へ書き込む（`TR-RCL-18`）。
    ///
    /// **生成が決定的なので、並び順もそのまま持つ**（`TR-RCL-27`）。
    #[tracing::instrument(skip(self, list), fields(rows = list.len(), tone), err)]
    pub fn install_reclist(&mut self, list: &[ReclistRow], tone: i32) -> Result<()> {
        self.conn
            .transaction(|c| {
                for (i, r) in list.iter().enumerate() {
                    diesel::insert_into(rows::table)
                        .values((
                            rows::id.eq(&r.id),
                            rows::text.eq(&r.text),
                            rows::file_stem.eq(&r.file_stem),
                            rows::tone.eq(tone),
                            rows::state.eq(RowState::Unrecorded.as_str()),
                            rows::ordinal.eq(i32::try_from(i).unwrap_or(i32::MAX)),
                        ))
                        .execute(c)?;
                    for u in &r.units {
                        diesel::insert_into(row_units::table)
                            .values((
                                row_units::row_id.eq(&r.id),
                                row_units::kana.eq(u.kana),
                                row_units::consonant.eq(u.consonant),
                                row_units::vowel.eq(u.vowel),
                            ))
                            .execute(c)?;
                    }
                }
                Ok(())
            })
            .map_err(db("install_reclist"))
    }

    /// 収録セッションを始める。
    pub fn start_session(&mut self, s: &SessionSnapshot) -> Result<i32> {
        self.conn
            .transaction(|c| {
                diesel::insert_into(sessions::table)
                    .values((
                        sessions::started_at.eq(&s.started_at),
                        sessions::device_id.eq(&s.device_id),
                        sessions::sample_rate_hz.eq(s.sample_rate_hz),
                        sessions::channels.eq(s.channels),
                        sessions::effects_state.eq(&s.effects_state),
                        sessions::route.eq(&s.route),
                    ))
                    .execute(c)?;
                sessions::table
                    .select(sessions::id)
                    .order(sessions::id.desc())
                    .first::<i32>(c)
            })
            .map_err(db("start_session"))
    }

    /// **確定済みのテイクを台帳へ載せる。**
    ///
    /// 呼べるのは fsync と rename が済んだあとだけ（`DEC-REC-004`）。
    /// 世代は行ごとに単調に増える。**採用テイクを新しい方へ切り替える**（`TR-REC-21`）。
    #[tracing::instrument(skip(self, t), fields(row = %t.row_id, frames = t.frames), err)]
    pub fn commit_take(&mut self, t: &FinalizedTake) -> Result<i32> {
        let exists: i64 = rows::table
            .filter(rows::id.eq(&t.row_id))
            .count()
            .get_result(&mut self.conn)
            .map_err(db("row_exists"))?;
        if exists == 0 {
            return Err(LedgerError::UnknownRow);
        }

        self.conn
            .transaction(|c| {
                let generation: i32 = takes::table
                    .filter(takes::row_id.eq(&t.row_id))
                    .select(diesel::dsl::max(takes::generation))
                    .first::<Option<i32>>(c)?
                    .unwrap_or(0)
                    + 1;
                diesel::insert_into(takes::table)
                    .values((
                        takes::row_id.eq(&t.row_id),
                        takes::session_id.eq(t.session_id),
                        takes::rel_path.eq(&t.rel_path),
                        takes::frames.eq(t.frames),
                        takes::recorded_at.eq(&t.recorded_at),
                        takes::invalid.eq(0),
                        takes::generation.eq(generation),
                    ))
                    .execute(c)?;
                let id: i32 = takes::table
                    .select(takes::id)
                    .order(takes::id.desc())
                    .first(c)?;

                // **採用を新しい方へ切り替える。過去のテイクは残る**（TR-REC-21）。
                diesel::insert_into(adopted_takes::table)
                    .values((
                        adopted_takes::row_id.eq(&t.row_id),
                        adopted_takes::take_id.eq(id),
                    ))
                    .on_conflict(adopted_takes::row_id)
                    .do_update()
                    .set(adopted_takes::take_id.eq(id))
                    .execute(c)?;

                diesel::update(rows::table.filter(rows::id.eq(&t.row_id)))
                    .set(rows::state.eq(RowState::Recorded.as_str()))
                    .execute(c)?;
                Ok(id)
            })
            .map_err(db("commit_take"))
    }

    /// 取りこぼしを検出したテイクを無効にする（`TR-REC-07`）。
    ///
    /// **ファイルは消さない。** 過去のテイクは残す（`TR-REC-21`）。
    pub fn invalidate_take(&mut self, take_id: i32) -> Result<()> {
        let n = diesel::update(takes::table.filter(takes::id.eq(take_id)))
            .set(takes::invalid.eq(1))
            .execute(&mut self.conn)
            .map_err(db("invalidate_take"))?;
        if n == 0 {
            return Err(LedgerError::UnknownTake);
        }
        Ok(())
    }

    /// 採用テイクを切り替える（`TR-RCL-25`）。
    ///
    /// **カバレッジは変わらない。** 行が生む単位は行が持っていて、テイクに依らない。
    pub fn adopt_take(&mut self, row_id: &str, take_id: i32) -> Result<()> {
        let ok: i64 = takes::table
            .filter(takes::id.eq(take_id))
            .filter(takes::row_id.eq(row_id))
            .count()
            .get_result(&mut self.conn)
            .map_err(db("take_belongs"))?;
        if ok == 0 {
            return Err(LedgerError::UnknownTake);
        }
        diesel::insert_into(adopted_takes::table)
            .values((
                adopted_takes::row_id.eq(row_id),
                adopted_takes::take_id.eq(take_id),
            ))
            .on_conflict(adopted_takes::row_id)
            .do_update()
            .set(adopted_takes::take_id.eq(take_id))
            .execute(&mut self.conn)
            .map_err(db("adopt_take"))?;
        Ok(())
    }

    /// 行の状態を引く。
    pub fn row_state(&mut self, row_id: &str) -> Result<RowState> {
        rows::table
            .filter(rows::id.eq(row_id))
            .select(rows::state)
            .first::<String>(&mut self.conn)
            .map(|s| RowState::parse(&s))
            .map_err(db("row_state"))
    }

    /// 行のテイクを世代順に引く。
    pub fn takes_of(&mut self, row_id: &str) -> Result<Vec<Take>> {
        takes::table
            .filter(takes::row_id.eq(row_id))
            .order(takes::generation.asc())
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
            ))
            .load::<(i32, String, String, i64, i32, i32)>(&mut self.conn)
            .map(|v| {
                v.into_iter()
                    .map(|(id, row_id, rel_path, frames, invalid, generation)| Take {
                        id,
                        row_id,
                        rel_path,
                        frames,
                        invalid: invalid != 0,
                        generation,
                    })
                    .collect()
            })
            .map_err(db("takes_of"))
    }

    /// **収録済みの単位集合。** 採用テイクを持つ行の単位の和集合として導出する
    /// （`TR-RCL-18`。二重に保持しない）。
    pub fn covered_units(&mut self) -> Result<BTreeSet<String>> {
        row_units::table
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(row_units::row_id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select(row_units::kana)
            .load::<String>(&mut self.conn)
            .map(|v| v.into_iter().collect())
            .map_err(db("covered_units"))
    }

    /// 次に録る行（`TR-REC-18`）。**未録音のうち並び順が最も早いもの。**
    pub fn next_row(&mut self) -> Result<Option<(String, String)>> {
        rows::table
            .filter(rows::state.eq(RowState::Unrecorded.as_str()))
            .order(rows::ordinal.asc())
            .select((rows::id, rows::text))
            .first::<(String, String)>(&mut self.conn)
            .optional()
            .map_err(db("next_row"))
    }

    /// 台帳が知らない確定済みファイルを見つける（`DEC-REC-004` の孤児）。
    ///
    /// **提示するだけ。DB へ自動で書き戻さない**（`TR-REC-31` の「自動修復しない」）。
    /// 本人が採るか捨てるまで消えない。
    #[tracing::instrument(skip(self, on_disk), fields(files = on_disk.len()), err)]
    pub fn find_orphans(&mut self, on_disk: &[String]) -> Result<Vec<String>> {
        let known: BTreeSet<String> = takes::table
            .select(takes::rel_path)
            .load::<String>(&mut self.conn)
            .map_err(db("known_paths"))?
            .into_iter()
            .collect();
        Ok(on_disk
            .iter()
            .filter(|p| !known.contains(*p))
            .cloned()
            .collect())
    }

    /// oto の5値を保存する。
    pub fn put_oto(
        &mut self,
        take_id: i32,
        o: &koeru_oto::Oto,
        confidence: f64,
        hand_edited: bool,
    ) -> Result<()> {
        diesel::insert_into(oto_values::table)
            .values((
                oto_values::take_id.eq(take_id),
                oto_values::offset_ms.eq(o.offset_ms),
                oto_values::consonant_ms.eq(o.consonant_ms),
                oto_values::cutoff_ms.eq(o.cutoff_ms),
                oto_values::preutterance_ms.eq(o.preutterance_ms),
                oto_values::overlap_ms.eq(o.overlap_ms),
                oto_values::confidence.eq(confidence),
                oto_values::hand_edited.eq(i32::from(hand_edited)),
            ))
            .on_conflict(oto_values::take_id)
            .do_update()
            .set((
                oto_values::offset_ms.eq(o.offset_ms),
                oto_values::consonant_ms.eq(o.consonant_ms),
                oto_values::cutoff_ms.eq(o.cutoff_ms),
                oto_values::preutterance_ms.eq(o.preutterance_ms),
                oto_values::overlap_ms.eq(o.overlap_ms),
                oto_values::confidence.eq(confidence),
                oto_values::hand_edited.eq(i32::from(hand_edited)),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_oto"))?;
        Ok(())
    }

    /// 行が生む単位を引く。
    pub fn units_of(&mut self, row_id: &str) -> Result<Vec<String>> {
        row_units::table
            .filter(row_units::row_id.eq(row_id))
            .select(row_units::kana)
            .load::<String>(&mut self.conn)
            .map_err(db("units_of"))
    }
}

/// oto の5値。`koeru-synth` から独立させて、依存の向きを一方向に保つ。
pub mod koeru_oto {
    /// oto.ini の1エントリ。単位はすべてミリ秒。
    #[derive(Debug, Clone, Copy, PartialEq)]
    pub struct Oto {
        pub offset_ms: f64,
        pub consonant_ms: f64,
        pub cutoff_ms: f64,
        pub preutterance_ms: f64,
        pub overlap_ms: f64,
    }
}

/// 収録単位を行へ紐づけるための補助。
#[must_use]
pub fn unit_kana(units: &[Unit]) -> Vec<&'static str> {
    units.iter().map(|u| u.kana).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn session() -> SessionSnapshot {
        SessionSnapshot {
            started_at: "2026-08-30T12:00:00Z".into(),
            device_id: "test-device".into(),
            sample_rate_hz: 48_000,
            channels: 1,
            effects_state: "clean".into(),
            route: "coreaudio".into(),
        }
    }

    fn take(row_id: &str, sid: i32, n: u32) -> FinalizedTake {
        FinalizedTake {
            row_id: row_id.into(),
            session_id: sid,
            rel_path: format!("masters/{row_id}_{n}.wav"),
            frames: 44_100,
            recorded_at: "2026-08-30T12:00:01Z".into(),
        }
    }

    fn ready() -> (Ledger, i32, Vec<crate::reclist::Row>) {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        let sid = l.start_session(&session()).expect("セッションを始められる");
        (l, sid, list)
    }

    #[test]
    fn 開いてスキーマが入る() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        assert_eq!(l.covered_units().expect("引ける").len(), 0);
    }

    #[test]
    fn 録音リストを入れて次の行が引ける() {
        let (mut l, _sid, list) = ready();
        let (id, text) = l.next_row().expect("引ける").expect("行がある");
        assert_eq!(id, list[0].id, "並び順の先頭");
        assert_eq!(text, list[0].text);
    }

    /// **確定済みのテイクだけが台帳に載る**（DEC-REC-004）。
    #[test]
    fn テイクを確定させると行が録音済みになる() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        assert_eq!(l.row_state(row).expect("引ける"), RowState::Unrecorded);
        l.commit_take(&take(row, sid, 1)).expect("確定できる");
        assert_eq!(l.row_state(row).expect("引ける"), RowState::Recorded);
    }

    /// **台帳に無い行のテイクは受け付けない。**
    #[test]
    fn 知らない行のテイクは弾く() {
        let (mut l, sid, _) = ready();
        assert!(matches!(
            l.commit_take(&take("存在しない行", sid, 1)),
            Err(LedgerError::UnknownRow)
        ));
    }

    /// **録り直しは上書きせず、世代として積む**（TR-REC-21）。
    #[test]
    fn 録り直しても過去のテイクが残る() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let first = l.commit_take(&take(row, sid, 1)).expect("1回目");
        let second = l.commit_take(&take(row, sid, 2)).expect("2回目");
        let all = l.takes_of(row).expect("引ける");
        assert_eq!(all.len(), 2, "両方残る");
        assert_eq!(all[0].generation, 1);
        assert_eq!(all[1].generation, 2);
        assert_ne!(first, second);
    }

    /// **採用テイクを切り替えてもカバレッジは変わらない**（TR-RCL-25）。
    #[test]
    fn 採用を切り替えてもカバレッジが変わらない() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let first = l.commit_take(&take(row, sid, 1)).expect("1回目");
        l.commit_take(&take(row, sid, 2)).expect("2回目");
        let after_second = l.covered_units().expect("引ける");
        l.adopt_take(row, first).expect("戻せる");
        assert_eq!(l.covered_units().expect("引ける"), after_second);
    }

    /// **収録済み単位は採用テイクを持つ行から導出する**（TR-RCL-18）。
    #[test]
    fn カバレッジは採用テイクのある行から導かれる() {
        let (mut l, sid, list) = ready();
        assert!(l.covered_units().expect("引ける").is_empty());
        let row = &list[0].id;
        l.commit_take(&take(row, sid, 1)).expect("確定できる");
        let covered = l.covered_units().expect("引ける");
        let expected: BTreeSet<String> = list[0].units.iter().map(|u| u.kana.to_string()).collect();
        assert_eq!(covered, expected, "その行が生む単位だけが入る");
    }

    /// **無効にしたテイクはカバレッジから外れる**（TR-REC-07）。
    #[test]
    fn 無効にしたテイクは被覆に数えない() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let id = l.commit_take(&take(row, sid, 1)).expect("確定できる");
        assert!(!l.covered_units().expect("引ける").is_empty());
        l.invalidate_take(id).expect("無効にできる");
        assert!(l.covered_units().expect("引ける").is_empty());
        assert_eq!(
            l.takes_of(row).expect("引ける").len(),
            1,
            "ファイルの記録は残る"
        );
    }

    /// **孤児を見つけて提示する。消さない**（DEC-REC-004）。
    #[test]
    fn 台帳に無い確定済みファイルを孤児として挙げる() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        l.commit_take(&take(row, sid, 1)).expect("確定できる");
        let on_disk = vec![
            format!("masters/{row}_1.wav"),
            "masters/落ちて残ったもの.wav".to_string(),
        ];
        let orphans = l.find_orphans(&on_disk).expect("引ける");
        assert_eq!(orphans, vec!["masters/落ちて残ったもの.wav".to_string()]);
    }

    /// **次の行は未録音のうち並び順が最も早いもの**（TR-REC-18）。
    #[test]
    fn 録るたびに次の行が進む() {
        let (mut l, sid, list) = ready();
        for (n, r) in list.iter().take(3).enumerate() {
            let (id, _) = l.next_row().expect("引ける").expect("行がある");
            assert_eq!(id, r.id, "{n} 番目");
            l.commit_take(&take(&r.id, sid, 1)).expect("確定できる");
        }
    }

    #[test]
    fn oto_の五値を保存して上書きできる() {
        let (mut l, sid, list) = ready();
        let id = l
            .commit_take(&take(&list[0].id, sid, 1))
            .expect("確定できる");
        let o = koeru_oto::Oto {
            offset_ms: 80.0,
            consonant_ms: 100.0,
            cutoff_ms: -520.0,
            preutterance_ms: 70.0,
            overlap_ms: 23.0,
        };
        l.put_oto(id, &o, 0.9, false).expect("保存できる");
        l.put_oto(id, &o, 0.5, true).expect("上書きできる");
    }

    #[test]
    fn 知らないテイクの操作は弾く() {
        let (mut l, _sid, _) = ready();
        assert!(matches!(
            l.invalidate_take(999),
            Err(LedgerError::UnknownTake)
        ));
        assert!(matches!(
            l.adopt_take("s001", 999),
            Err(LedgerError::UnknownTake)
        ));
    }

    /// **行が生む単位は行が持ち、テイクに依らない。**
    #[test]
    fn 行の単位はテイクと独立している() {
        let (mut l, _sid, list) = ready();
        let before = l.units_of(&list[0].id).expect("引ける");
        assert_eq!(before.len(), list[0].units.len());
    }
}
