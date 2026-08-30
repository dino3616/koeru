//! プロジェクトの永続化（`TR-REC-27`, `TR-REC-29`, `TR-REC-31`, `TR-RCL-23`）。
//!
//! **8分から3時間までを同じ機構で扱う**（`TR-REC-29`）。長さで経路を分けない——
//! 分けると、短いほうでしか試されない経路ができる。
//!
//! **行単位で中断・再開でき、再開点と経過をここから復元する**（`TR-RCL-23`）。
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

use crate::analysis::{TakeAnalysis, TakeMetrics, bytes_to_f64s, f64s_to_bytes};
use crate::calibration::Calibration;
use crate::frq::Frq;
use crate::inventory::Unit;
use crate::project::Method;
use crate::reclist::Row as ReclistRow;
use crate::release::{NewRelease, Release, Validation, archive_name};
use crate::schema::{
    adopted_takes, calibrations, oto_values, releases, row_units, rows, sessions, song_notes,
    songs, take_analysis, take_metrics, takes,
};
use crate::song::{Note, Provenance, Song};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::collections::BTreeSet;
use std::path::Path;

/// 無音のピークを表す値（dBFS）。
///
/// **SQLite に `-inf` は入らない。** 往復させるための番人。
const SILENT_PEAK_DBFS: f64 = -1000.0;

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
    /// モノラルの元にしたチャンネル（`TR-REC-06`）。**-1 は混ぜた。**
    pub source_channel: i32,
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

    /// 録音停止時の解析値を保存する（`TR-PKG-05`, `TR-PKG-42`）。
    ///
    /// **ここで入れたものを書き出し時に使う。WAV を読み直さない。**
    #[tracing::instrument(skip(self, a), fields(take_id), err)]
    pub fn put_analysis(&mut self, take_id: i32, a: &TakeAnalysis) -> Result<()> {
        let f0 = f64s_to_bytes(&a.frq.f0);
        let amp = f64s_to_bytes(&a.frq.amp);
        #[allow(clippy::cast_possible_wrap, reason = "HOP_SIZE は 256 の定数")]
        let hop = a.hop_size() as i32;
        diesel::insert_into(take_analysis::table)
            .values((
                take_analysis::take_id.eq(take_id),
                take_analysis::peak.eq(f64::from(a.peak)),
                take_analysis::hop_size.eq(hop),
                take_analysis::f0.eq(&f0),
                take_analysis::amp.eq(&amp),
                take_analysis::thumbnail.eq(&a.thumbnail),
            ))
            .on_conflict(take_analysis::take_id)
            .do_update()
            .set((
                take_analysis::peak.eq(f64::from(a.peak)),
                take_analysis::hop_size.eq(hop),
                take_analysis::f0.eq(&f0),
                take_analysis::amp.eq(&amp),
                take_analysis::thumbnail.eq(&a.thumbnail),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_analysis"))?;
        Ok(())
    }

    /// 解析値を引く。**無ければ `None`。** 解析が無いことは失敗ではない
    /// （古いプロジェクトや、まだ解析が終わっていないテイク）。
    #[tracing::instrument(skip(self), fields(take_id), err)]
    pub fn analysis_of(&mut self, take_id: i32) -> Result<Option<TakeAnalysis>> {
        let row = take_analysis::table
            .filter(take_analysis::take_id.eq(take_id))
            .select((
                take_analysis::peak,
                take_analysis::f0,
                take_analysis::amp,
                take_analysis::thumbnail,
            ))
            .first::<(f64, Vec<u8>, Vec<u8>, Vec<u8>)>(&mut self.conn)
            .optional()
            .map_err(db("analysis_of"))?;

        #[allow(
            clippy::cast_possible_truncation,
            reason = "peak は 0.0..=1.0 付近。f32 で保つ"
        )]
        Ok(row.map(|(peak, f0, amp, thumbnail)| TakeAnalysis {
            peak: peak as f32,
            frq: Frq {
                f0: bytes_to_f64s(&f0),
                amp: bytes_to_f64s(&amp),
            },
            thumbnail,
        }))
    }

    /// 書き出しを1件記録する（`TR-PKG-44`）。
    ///
    /// **連番と書き出し先の名前はここが決める。** 呼び出し側に採番させると、
    /// 同じ番号のリリースが2つできる。返るのは確定したレコード。
    ///
    /// **書き出し先の名前は過去のものと衝突しない**（連番が先頭に付く）。
    #[tracing::instrument(skip(self, r), err)]
    pub fn record_release(&mut self, r: &NewRelease, ext: &str) -> Result<Release> {
        let next = releases::table
            .select(diesel::dsl::max(releases::seq))
            .first::<Option<i32>>(&mut self.conn)
            .map_err(db("record_release"))?
            .unwrap_or(0)
            + 1;
        let name = archive_name(next, &r.version, ext);

        diesel::insert_into(releases::table)
            .values((
                releases::seq.eq(next),
                releases::version.eq(&r.version),
                releases::method.eq(r.method.as_str()),
                releases::alias_count.eq(r.alias_count),
                releases::validation.eq(r.validation.as_str()),
                releases::oto_hash.eq(&r.oto_hash),
                releases::terms_hash.eq(&r.terms_hash),
                releases::archive_name.eq(&name),
                releases::released_at.eq(&r.released_at),
            ))
            .execute(&mut self.conn)
            .map_err(db("record_release"))?;

        Ok(Release {
            seq: next,
            version: r.version.clone(),
            method: r.method,
            alias_count: r.alias_count,
            validation: r.validation,
            oto_hash: r.oto_hash.clone(),
            terms_hash: r.terms_hash.clone(),
            archive_name: name,
            released_at: r.released_at.clone(),
        })
    }

    /// 書き出しの履歴を古い順に引く（`TR-PKG-44`）。
    ///
    /// **過去のリリースはここからだけ取り出せる**（`TR-PKG-46`）。
    #[tracing::instrument(skip(self), err)]
    pub fn releases(&mut self) -> Result<Vec<Release>> {
        let rows = releases::table
            .order(releases::seq.asc())
            .select((
                releases::seq,
                releases::version,
                releases::method,
                releases::alias_count,
                releases::validation,
                releases::oto_hash,
                releases::terms_hash,
                releases::archive_name,
                releases::released_at,
            ))
            .load::<(
                i32,
                String,
                String,
                i32,
                String,
                String,
                String,
                String,
                String,
            )>(&mut self.conn)
            .map_err(db("releases"))?;

        Ok(rows
            .into_iter()
            .map(
                |(
                    seq,
                    version,
                    method,
                    alias_count,
                    validation,
                    oto_hash,
                    terms_hash,
                    archive_name,
                    released_at,
                )| Release {
                    seq,
                    version,
                    // **保存した方式名が読めなくても履歴は出す。**
                    // 読めないものを落とすと、その回に何を配ったかが辿れなくなる。
                    method: parse_method_or_single(&method),
                    alias_count,
                    validation: Validation::parse(&validation),
                    oto_hash,
                    terms_hash,
                    archive_name,
                    released_at,
                },
            )
            .collect())
    }

    /// 一番新しい書き出し。**外部編集の検出はここと突き合わせる**（`TR-PKG-48`）。
    #[tracing::instrument(skip(self), err)]
    pub fn latest_release(&mut self) -> Result<Option<Release>> {
        Ok(self.releases()?.pop())
    }

    /// 書き出し履歴があるか（`TR-PKG-33` の `handoff_state`）。
    ///
    /// **完成判定はこれを参照しない。**
    #[tracing::instrument(skip(self), err)]
    pub fn has_been_exported(&mut self) -> Result<bool> {
        let n: i64 = releases::table
            .count()
            .get_result(&mut self.conn)
            .map_err(db("has_been_exported"))?;
        Ok(n > 0)
    }

    /// まだ採用テイクが無い行の数。**残量の見積もりに使う**（`REQ-REC-110`）。
    #[tracing::instrument(skip(self), err)]
    pub fn remaining_rows(&mut self) -> Result<u64> {
        let n: i64 = rows::table
            .filter(rows::id.ne_all(adopted_takes::table.select(adopted_takes::row_id)))
            .count()
            .get_result(&mut self.conn)
            .map_err(db("remaining_rows"))?;
        Ok(u64::try_from(n).unwrap_or(0))
    }

    /// テイクの計測値を保存する（`TR-REC-16`, `TR-REC-07`, `TR-REC-19`, `TR-REC-38`）。
    ///
    /// **測った値で自動的に無効化しない。** 自動無効化は取りこぼし（`TR-REC-07`）と
    /// デバイス消失（`TR-REC-04`）の2つだけで、それは呼び出し側が
    /// [`Ledger::invalidate_take`] を明示的に呼ぶ。
    #[tracing::instrument(skip(self, m), fields(take_id), err)]
    pub fn put_metrics(
        &mut self,
        take_id: i32,
        m: &TakeMetrics,
        discontinuities: usize,
        preroll_frames: usize,
        guide_offset_frames: Option<i64>,
    ) -> Result<()> {
        // **SQLite に -inf は入らない。** 無音のピークを -1000 dBFS で表す。
        let peak = if m.peak_dbfs.is_finite() {
            m.peak_dbfs
        } else {
            SILENT_PEAK_DBFS
        };
        let values = (
            take_metrics::peak_dbfs.eq(peak),
            take_metrics::rms.eq(m.rms),
            take_metrics::full_scale_runs.eq(i32::try_from(m.full_scale_runs).unwrap_or(i32::MAX)),
            take_metrics::dc_offset.eq(m.dc_offset),
            take_metrics::noise_floor_rms.eq(m.noise_floor_rms),
            take_metrics::leading_margin_ms.eq(m.leading_margin_ms),
            take_metrics::trailing_margin_ms.eq(m.trailing_margin_ms),
            take_metrics::discontinuities.eq(i32::try_from(discontinuities).unwrap_or(i32::MAX)),
            take_metrics::preroll_frames.eq(i32::try_from(preroll_frames).unwrap_or(i32::MAX)),
            // **参考値**（TR-REC-26）。切り出しの根拠にしない。
            take_metrics::guide_offset_frames.eq(guide_offset_frames),
        );
        diesel::insert_into(take_metrics::table)
            .values((take_metrics::take_id.eq(take_id), values))
            .on_conflict(take_metrics::take_id)
            .do_update()
            .set(values)
            .execute(&mut self.conn)
            .map_err(db("put_metrics"))?;
        Ok(())
    }

    /// テイクの計測値を引く。
    #[tracing::instrument(skip(self), fields(take_id), err)]
    pub fn metrics_of(&mut self, take_id: i32) -> Result<Option<TakeMetrics>> {
        let row = take_metrics::table
            .filter(take_metrics::take_id.eq(take_id))
            .select((
                take_metrics::peak_dbfs,
                take_metrics::rms,
                take_metrics::full_scale_runs,
                take_metrics::dc_offset,
                take_metrics::noise_floor_rms,
                take_metrics::leading_margin_ms,
                take_metrics::trailing_margin_ms,
            ))
            .first::<(f64, f64, i32, f64, f64, f64, f64)>(&mut self.conn)
            .optional()
            .map_err(db("metrics_of"))?;

        Ok(row.map(
            |(peak_dbfs, rms, runs, dc_offset, noise_floor_rms, lead, trail)| TakeMetrics {
                peak_dbfs: if peak_dbfs <= SILENT_PEAK_DBFS {
                    f64::NEG_INFINITY
                } else {
                    peak_dbfs
                },
                rms,
                full_scale_runs: u32::try_from(runs).unwrap_or(0),
                dc_offset,
                noise_floor_rms,
                leading_margin_ms: lead,
                trailing_margin_ms: trail,
            },
        ))
    }

    /// 採用テイクのうち、フルスケールに達しているものを挙げる（`TR-REC-16`）。
    ///
    /// **書き出しの直前に一度だけ呼ぶ。** 収録中には呼ばない
    /// ——リアルタイムの判定はスコープ外で、ここは「壊れた成果物が完成に
    /// 到達する経路を塞ぐ」ためだけの関門。
    #[tracing::instrument(skip(self), err)]
    pub fn clipped_adopted_takes(&mut self) -> Result<Vec<(String, i32, u32)>> {
        let rows = adopted_takes::table
            .inner_join(take_metrics::table.on(take_metrics::take_id.eq(adopted_takes::take_id)))
            .filter(take_metrics::full_scale_runs.gt(0))
            .order(adopted_takes::row_id.asc())
            .select((
                adopted_takes::row_id,
                adopted_takes::take_id,
                take_metrics::full_scale_runs,
            ))
            .load::<(String, i32, i32)>(&mut self.conn)
            .map_err(db("clipped_adopted_takes"))?;
        Ok(rows
            .into_iter()
            .map(|(row_id, take_id, runs)| (row_id, take_id, u32::try_from(runs).unwrap_or(0)))
            .collect())
    }

    /// 校正の結果を保存する（`TR-REC-15`）。
    ///
    /// **デバイスごとに1つ。** 同じプロジェクトを別のマイクで続けることがあり、
    /// そのときに前のマイクの値を当てはめても意味が無い。
    #[tracing::instrument(skip(self, c), err)]
    pub fn put_calibration(&mut self, c: &Calibration, measured_at: &str) -> Result<()> {
        let values = (
            calibrations::gain.eq(c.gain),
            calibrations::control.eq(&c.control),
            calibrations::peak_dbfs.eq(if c.peak_dbfs.is_finite() {
                c.peak_dbfs
            } else {
                SILENT_PEAK_DBFS
            }),
            calibrations::settled.eq(i32::from(c.settled)),
            calibrations::measured_at.eq(measured_at),
            calibrations::source_channel.eq(c.source_channel),
        );
        diesel::insert_into(calibrations::table)
            .values((calibrations::device_id.eq(&c.device_id), values))
            .on_conflict(calibrations::device_id)
            .do_update()
            .set(values)
            .execute(&mut self.conn)
            .map_err(db("put_calibration"))?;
        Ok(())
    }

    /// そのデバイスの校正結果を引く。**まだ校正していなければ `None`。**
    #[tracing::instrument(skip(self), err)]
    pub fn calibration_of(&mut self, device_id: &str) -> Result<Option<Calibration>> {
        let row = calibrations::table
            .filter(calibrations::device_id.eq(device_id))
            .select((
                calibrations::gain,
                calibrations::control,
                calibrations::peak_dbfs,
                calibrations::settled,
                calibrations::source_channel,
            ))
            .first::<(Option<f32>, String, f64, i32, i32)>(&mut self.conn)
            .optional()
            .map_err(db("calibration_of"))?;

        Ok(row.map(
            |(gain, control, peak_dbfs, settled, source_channel)| Calibration {
                gain,
                control,
                peak_dbfs: if peak_dbfs <= SILENT_PEAK_DBFS {
                    f64::NEG_INFINITY
                } else {
                    peak_dbfs
                },
                settled: settled != 0,
                device_id: device_id.to_owned(),
                source_channel,
            },
        ))
    }

    /// 課題曲を入れる（`TR-RCL-12`）。
    ///
    /// **同じ id なら差し替える。** 同梱曲を毎回入れ直せるようにしておく。
    #[tracing::instrument(skip(self, song), fields(id, notes = song.notes.len()), err)]
    pub fn put_song(&mut self, id: &str, song: &Song, bundled: bool, added_at: &str) -> Result<()> {
        self.conn
            .transaction::<_, diesel::result::Error, _>(|conn| {
                let values = (
                    songs::title.eq(&song.title),
                    songs::source.eq(&song.provenance.source),
                    songs::license.eq(&song.provenance.license),
                    songs::bundled.eq(i32::from(bundled)),
                    songs::in_bank.eq(1),
                    songs::added_at.eq(added_at),
                );
                diesel::insert_into(songs::table)
                    .values((songs::id.eq(id), values))
                    .on_conflict(songs::id)
                    .do_update()
                    .set(values)
                    .execute(conn)?;

                diesel::delete(song_notes::table.filter(song_notes::song_id.eq(id)))
                    .execute(conn)?;
                for (i, n) in song.notes.iter().enumerate() {
                    diesel::insert_into(song_notes::table)
                        .values((
                            song_notes::song_id.eq(id),
                            song_notes::ordinal.eq(i32::try_from(i).unwrap_or(i32::MAX)),
                            song_notes::lyric.eq(&n.lyric),
                            song_notes::midi.eq(n.midi),
                            song_notes::ticks.eq(i32::try_from(n.ticks).unwrap_or(i32::MAX)),
                        ))
                        .execute(conn)?;
                }
                Ok(())
            })
            .map_err(db("put_song"))?;
        Ok(())
    }

    /// 曲バンクの中身（`TR-RCL-12`）。
    ///
    /// **バンクが空でも成立する。** そのとき進捗はカバレッジだけで読む。
    #[tracing::instrument(skip(self), err)]
    pub fn songs_in_bank(&mut self) -> Result<Vec<(String, Song)>> {
        let heads = songs::table
            .filter(songs::in_bank.eq(1))
            .order(songs::added_at.asc())
            .select((songs::id, songs::title, songs::source, songs::license))
            .load::<(String, String, String, String)>(&mut self.conn)
            .map_err(db("songs_in_bank"))?;

        let mut out = Vec::with_capacity(heads.len());
        for (id, title, source, license) in heads {
            let notes = song_notes::table
                .filter(song_notes::song_id.eq(&id))
                .order(song_notes::ordinal.asc())
                .select((song_notes::lyric, song_notes::midi, song_notes::ticks))
                .load::<(String, i32, i32)>(&mut self.conn)
                .map_err(db("songs_in_bank"))?;
            out.push((
                id,
                Song {
                    title,
                    notes: notes
                        .into_iter()
                        .map(|(lyric, midi, ticks)| Note {
                            lyric,
                            midi,
                            ticks: u32::try_from(ticks).unwrap_or(0),
                        })
                        .collect(),
                    provenance: Provenance { source, license },
                },
            ));
        }
        Ok(out)
    }

    /// 曲をバンクから外す／戻す（`TR-RCL-12`）。
    ///
    /// **曲そのものは消さない。** 同梱分も本人が外せる。
    #[tracing::instrument(skip(self), err)]
    pub fn set_song_in_bank(&mut self, id: &str, in_bank: bool) -> Result<()> {
        diesel::update(songs::table.filter(songs::id.eq(id)))
            .set(songs::in_bank.eq(i32::from(in_bank)))
            .execute(&mut self.conn)
            .map_err(db("set_song_in_bank"))?;
        Ok(())
    }

    /// その収録単位を鳴らすためのテイク（`TR-SYN-12`, `TR-RCL-18`）。
    ///
    /// **採用テイクだけ。** 無効にしたテイク（取りこぼし、`TR-REC-07`）は入らない。
    /// 同じ単位を複数の行が持つときは、**先に来る行のものを使う**（決定的にする）。
    #[tracing::instrument(skip(self), err)]
    pub fn take_for_unit(&mut self, kana: &str) -> Result<Option<Take>> {
        let row = row_units::table
            .inner_join(rows::table.on(rows::id.eq(row_units::row_id)))
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(rows::id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(row_units::kana.eq(kana))
            .filter(takes::invalid.eq(0))
            .order(rows::ordinal.asc())
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
            ))
            .first::<(i32, String, String, i64, i32, i32)>(&mut self.conn)
            .optional()
            .map_err(db("take_for_unit"))?;

        Ok(
            row.map(|(id, row_id, rel_path, frames, invalid, generation)| Take {
                id,
                row_id,
                rel_path,
                frames,
                invalid: invalid != 0,
                generation,
            }),
        )
    }

    /// テイクを1件引く。**無ければ `None`。**
    #[tracing::instrument(skip(self), fields(take_id), err)]
    pub fn take(&mut self, take_id: i32) -> Result<Option<Take>> {
        takes::table
            .filter(takes::id.eq(take_id))
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
            ))
            .first::<(i32, String, String, i64, i32, i32)>(&mut self.conn)
            .optional()
            .map_err(db("take"))
            .map(|o| {
                o.map(|(id, row_id, rel_path, frames, invalid, generation)| Take {
                    id,
                    row_id,
                    rel_path,
                    frames,
                    invalid: invalid != 0,
                    generation,
                })
            })
    }

    /// テイクに紐づく oto の5値を引く。**まだ無ければ `None`。**
    #[tracing::instrument(skip(self), fields(take_id), err)]
    pub fn oto_of(&mut self, take_id: i32) -> Result<Option<koeru_oto::Oto>> {
        oto_values::table
            .filter(oto_values::take_id.eq(take_id))
            .select((
                oto_values::offset_ms,
                oto_values::consonant_ms,
                oto_values::cutoff_ms,
                oto_values::preutterance_ms,
                oto_values::overlap_ms,
            ))
            .first::<(f64, f64, f64, f64, f64)>(&mut self.conn)
            .optional()
            .map_err(db("oto_of"))
            .map(|o| {
                o.map(
                    |(offset_ms, consonant_ms, cutoff_ms, preutterance_ms, overlap_ms)| {
                        koeru_oto::Oto {
                            offset_ms,
                            consonant_ms,
                            cutoff_ms,
                            preutterance_ms,
                            overlap_ms,
                        }
                    },
                )
            })
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

/// 保存されていた方式名を戻す。
///
/// **知らない名前でも履歴を落とさない。** その回に何を配ったかが辿れなくなるほうが痛い。
fn parse_method_or_single(s: &str) -> Method {
    match s {
        "sequential" => Method::Sequential,
        "cvvc" => Method::Cvvc,
        "multi_pitch_sequential" => Method::MultiPitchSequential,
        _ => Method::Single,
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
            source_channel: 0,
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
    /// **解析値を録音時に確定させ、書き出しと再開で WAV を読み直さない**
    /// （TR-PKG-05, TR-PKG-42）。
    #[test]
    fn 解析値が往復する() {
        let (mut l, sid, list) = ready();
        let id = l
            .commit_take(&take(&list[0].id, sid, 1))
            .expect("確定できる");

        let samples: Vec<f32> = (0..44_100)
            .map(|i| ((i as f32) / 100.0).sin() * 0.5)
            .collect();
        let a = crate::analysis::TakeAnalysis::compute(&samples, 44_100, &[220.0; 200], 0.005);
        l.put_analysis(id, &a).expect("保存できる");

        let got = l.analysis_of(id).expect("引ける").expect("ある");
        assert!((got.peak - a.peak).abs() < 1e-6);
        assert_eq!(got.frq, a.frq, "F0 と振幅がそのまま戻ること");
        assert_eq!(got.thumbnail, a.thumbnail);
    }

    /// **解析がまだ無いことは失敗ではない。**
    #[test]
    fn 解析が無いテイクは無しを返す() {
        let (mut l, sid, list) = ready();
        let id = l
            .commit_take(&take(&list[0].id, sid, 1))
            .expect("確定できる");
        assert!(l.analysis_of(id).expect("引ける").is_none());
    }

    #[test]
    fn 解析は上書きできる() {
        let (mut l, sid, list) = ready();
        let id = l
            .commit_take(&take(&list[0].id, sid, 1))
            .expect("確定できる");

        let quiet =
            crate::analysis::TakeAnalysis::compute(&[0.1_f32; 512], 44_100, &[220.0; 4], 0.005);
        let loud =
            crate::analysis::TakeAnalysis::compute(&[0.9_f32; 512], 44_100, &[220.0; 4], 0.005);
        l.put_analysis(id, &quiet).expect("保存できる");
        l.put_analysis(id, &loud).expect("上書きできる");

        assert!((l.analysis_of(id).expect("引ける").expect("ある").peak - 0.9).abs() < 1e-6);
    }

    fn new_release(version: &str) -> NewRelease {
        NewRelease {
            version: version.into(),
            method: Method::Single,
            alias_count: 102,
            validation: Validation::Passed,
            oto_hash: crate::release::content_hash(b"[a.wav]"),
            terms_hash: crate::release::content_hash(b"terms"),
            released_at: "2026-08-30T12:00:00Z".into(),
        }
    }

    #[test]
    fn 書き出しの連番は台帳が採る() {
        let (mut l, _sid, _list) = ready();
        assert_eq!(
            l.record_release(&new_release("v1"), "zip")
                .expect("記録できる")
                .seq,
            1
        );
        assert_eq!(
            l.record_release(&new_release("v2"), "zip")
                .expect("記録できる")
                .seq,
            2
        );
    }

    /// **過去のバージョンの ZIP を上書きしない**（TR-PKG-44）。
    /// 同じバージョン文字列で二度書き出しても、名前が別になる。
    #[test]
    fn 同じバージョン文字列でも書き出し先が衝突しない() {
        let (mut l, _sid, _list) = ready();
        let a = l
            .record_release(&new_release("v1.0"), "zip")
            .expect("記録できる");
        let b = l
            .record_release(&new_release("v1.0"), "zip")
            .expect("記録できる");
        assert_ne!(a.archive_name, b.archive_name);
        assert_eq!(a.archive_name, "000001-v1.0.zip");
        assert_eq!(b.archive_name, "000002-v1.0.zip");
    }

    /// **リリースレコードは不変**（TR-PKG-44）。規律ではなくトリガが止める。
    #[test]
    fn リリースレコードは書き換えられない() {
        let (mut l, _sid, _list) = ready();
        l.record_release(&new_release("v1"), "zip")
            .expect("記録できる");

        let updated = diesel::update(releases::table.filter(releases::seq.eq(1)))
            .set(releases::version.eq("すり替え"))
            .execute(&mut l.conn);
        assert!(updated.is_err(), "UPDATE が拒まれること");

        let deleted =
            diesel::delete(releases::table.filter(releases::seq.eq(1))).execute(&mut l.conn);
        assert!(deleted.is_err(), "DELETE が拒まれること");

        assert_eq!(
            l.releases().expect("引ける")[0].version,
            "v1",
            "元のまま残ること"
        );
    }

    #[test]
    fn 履歴は古い順に並ぶ() {
        let (mut l, _sid, _list) = ready();
        for v in ["v1", "v2", "v3"] {
            l.record_release(&new_release(v), "zip")
                .expect("記録できる");
        }
        let seqs: Vec<i32> = l
            .releases()
            .expect("引ける")
            .iter()
            .map(|r| r.seq)
            .collect();
        assert_eq!(seqs, [1, 2, 3]);
        assert_eq!(
            l.latest_release().expect("引ける").expect("ある").version,
            "v3"
        );
    }

    /// **書き出し履歴は完成判定と直交する**（TR-PKG-33, TR-PKG-36）。
    #[test]
    fn 書き出し履歴の有無だけが手渡し状態を決める() {
        let (mut l, _sid, _list) = ready();
        assert!(!l.has_been_exported().expect("引ける"));
        l.record_release(&new_release("v1"), "zip")
            .expect("記録できる");
        assert!(l.has_been_exported().expect("引ける"));
    }

    /// **知らない方式名でも履歴を落とさない。**
    #[test]
    fn 読めない方式名でも履歴が残る() {
        let (mut l, _sid, _list) = ready();
        diesel::insert_into(releases::table)
            .values((
                releases::seq.eq(1),
                releases::version.eq("v1"),
                releases::method.eq("未来の方式"),
                releases::alias_count.eq(1),
                releases::validation.eq("passed"),
                releases::oto_hash.eq("x"),
                releases::terms_hash.eq("y"),
                releases::archive_name.eq("000001-v1.zip"),
                releases::released_at.eq("2026-08-30T12:00:00Z"),
            ))
            .execute(&mut l.conn)
            .expect("入る");
        assert_eq!(l.releases().expect("引ける").len(), 1);
    }
}
