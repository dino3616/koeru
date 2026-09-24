//! プロジェクトの永続化（`TR-REC-27`, `TR-REC-29`, `TR-REC-31`, `TR-RCL-23`）。
//!
//! 8分から3時間までを同じ機構で扱う（`TR-REC-29`）。長さで経路を分けない——
//! 分けると、短いほうでしか試されない経路ができる。
//!
//! 行単位で中断・再開でき、再開点と経過をここから復元する（`TR-RCL-23`）。
//!
//! 状態の単一の真実（`TR-REC-31`）。`masters/` のファイル存在をスキャンして
//! 導出しない。試唱の可否・再開位置・書き出しの可否をすべてここから決める。
//!
//! ## テイクを確定させる順序
//!
//! ファイル確定 → DB コミット（`DEC-REC-004`、`project-storage.fsl` で `proved`）。
//! 逆にすると、ファイルの無い行が DB に残る。テイクは録音という
//! 「やり直しが高い操作」の成果物なので、行だけが残って音が無い状態は復旧できない。
//!
//! この向きは [`Ledger::commit_take`] が構造的に保証する。確定済みのパスを
//! 受け取ってからしか呼べない。
//!
//! ## 孤児
//!
//! rename が済んでコミット前に落ちると、確定済みの WAV があるのに行が無い状態が残る
//! （`DEC-REC-004` が決めた順序の帰結）。[`Ledger::find_orphans`] が見つけて
//! 復旧候補として提示する。本人が採るか捨てるまで消さない。

use crate::analysis::{TakeAnalysis, TakeMetrics, bytes_to_f64s, f64s_to_bytes};
use crate::calibration::Calibration;
use crate::frq::Frq;
use crate::inventory::{Unit, UnitSet, units};
use crate::oto::Boundary;
use crate::project::Method;
use crate::reclist::Row as ReclistRow;
use crate::release::{NewRelease, Release, Validation, archive_name};
use crate::schema::{
    adopted_takes, calibrations, distribution, oto_values, presamp_snapshot, recording_order,
    releases, review_state, row_aliases, row_units, rows, sessions, song_notes, songs,
    take_analysis, take_boundaries, take_fingerprints, take_metrics, takes,
};
use crate::song::{Note, Provenance, Song};
use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use diesel_migrations::{EmbeddedMigrations, MigrationHarness, embed_migrations};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

/// 無音のピークを表す値（dBFS）。
///
/// SQLite に `-inf` は入らない。 往復させるための番人。
const SILENT_PEAK_DBFS: f64 = -1000.0;

/// マイグレーションを実行ファイルへ埋め込む。外部ファイルに依存しない（`TR-PLT-20`）。
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
    /// 送信層へ載せてよい固定文字列。`Display` を送らない（パスが入りうる）。
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

/// 行を1つの収録音高へ入れる。 [`Ledger::install_rows`] と
/// [`Ledger::replace_untaken_rows`] が同じトランザクションの中で呼ぶ。
///
/// 並び順は台帳の末尾から続ける。 一度は呼ぶたびに 0 から振っていたので、
/// 詰め直した行がフルリストの行と同じ番号を持ち、並びが同点で決まらなかった。
fn insert_rows(
    c: &mut SqliteConnection,
    list: &[ReclistRow],
    rules: &crate::presamp::Rules,
    method: crate::alias::Method,
    tone: i32,
    suffixed: bool,
    origin: RowOrigin,
) -> QueryResult<usize> {
    let base = rows::table
        .select(diesel::dsl::max(rows::ordinal))
        .first::<Option<i32>>(c)?
        .map_or(0, |m| m + 1);
    let mut inserted = 0_usize;
    for (ordinal, r) in (base..).zip(list) {
        let id = if suffixed {
            format!("{}@{}", r.id, crate::tone::name(tone))
        } else {
            r.id.clone()
        };
        let added = diesel::insert_into(rows::table)
            .values((
                rows::id.eq(&id),
                rows::text.eq(&r.text),
                rows::file_stem.eq(&r.file_stem),
                rows::tone.eq(tone),
                rows::state.eq(RowState::Unrecorded.as_str()),
                rows::ordinal.eq(ordinal),
                rows::origin.eq(origin.as_str()),
            ))
            .on_conflict(rows::id)
            .do_nothing()
            .execute(c)?;
        // 既にある行は、単位もエイリアスも入っている。
        if added == 0 {
            continue;
        }
        inserted += 1;
        // 単位は集合として入れる。 連続音の行は同じ仮名を2度持つが、
        // 集合としては変わらない。語順は `rows.text` が持っている。
        let mut seen = std::collections::BTreeSet::new();
        for u in &r.units {
            if !seen.insert(u.kana) {
                continue;
            }
            diesel::insert_into(row_units::table)
                .values((
                    row_units::row_id.eq(&id),
                    row_units::kana.eq(u.kana),
                    row_units::consonant.eq(u.consonant),
                    row_units::vowel.eq(u.vowel),
                ))
                .execute(c)?;
        }
        // 行が生むエイリアス（`TR-PKG-22` の判定の正本）。 **全部入れる。**
        //
        // 同じ音高の中で同じ綴りを生む行があっても、ここでは潰さない。 誰が
        // 持つかは録った順で決まり、台帳のテイクから導く（[`Ledger::alias_owners`]、
        // `DEC-RCL-016`）。**入れる順で先に名乗った行に持たせていた**ので、
        // フルリストのあとに足した詰め直しの行は綴りを1つも持たず、録っても
        // 原音設定も被覆も1つも増えなかった。
        for (n, alias) in crate::reclist::row_aliases(rules, method, &r.units)
            .into_iter()
            .enumerate()
        {
            diesel::insert_into(row_aliases::table)
                .values((
                    row_aliases::row_id.eq(&id),
                    row_aliases::alias.eq(&alias),
                    row_aliases::ordinal.eq(i32::try_from(n).unwrap_or(i32::MAX)),
                ))
                .on_conflict((row_aliases::row_id, row_aliases::alias))
                .do_nothing()
                .execute(c)?;
        }
    }
    Ok(inserted)
}

fn db(op: &'static str) -> impl FnOnce(diesel::result::Error) -> LedgerError {
    move |source| LedgerError::Db { op, source }
}

/// 行の出どころ（`TR-RCL-18` (g)）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowOrigin {
    /// プリセットから生成したフルリスト。
    Preset,
    /// 選択から詰め直した行（`TR-RCL-16`, `DEC-RCL-011`）。
    Repack,
}

impl RowOrigin {
    /// 台帳での表記。送信してよい固定語彙。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Preset => "preset",
            Self::Repack => "repack",
        }
    }

    /// 台帳から戻す。 知らない値はフルリストに倒す——列の既定と同じ。
    fn parse(s: &str) -> Self {
        if s == "repack" {
            Self::Repack
        } else {
            Self::Preset
        }
    }

    /// もう片方の出どころ（`DEC-RCL-016`）。 片方を録ったら、こちらを組み直す。
    #[must_use]
    pub const fn other(self) -> Self {
        match self {
            Self::Preset => Self::Repack,
            Self::Repack => Self::Preset,
        }
    }
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
    /// 台帳での表記。送信してよい固定語彙。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
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
    /// 永続識別子。表示名は保存しない（`TR-REC-03`）。
    pub device_id: String,
    pub sample_rate_hz: i32,
    pub channels: i32,
    /// `clean` / `some_remain` / `unknown`。
    pub effects_state: String,
    /// 実際に接続した経路（`TR-REC-12`）。
    pub route: String,
    /// モノラルの元にしたチャンネル（`TR-REC-06`）。-1 は混ぜた。
    pub source_channel: i32,
    /// マスターとして保存したレート。常に 44100（`TR-REC-01`, `TR-REC-02`）。
    ///
    /// [`Self::sample_rate_hz`] はネイティブレート。**両方持って初めて、
    /// 一致／不一致が後から分かる**（`TR-REC-02` が記録を要求している）。
    pub master_rate_hz: i32,
    /// 使用したリサンプラの識別子と版（`TR-REC-02`）。
    pub resampler: String,
    /// 上流（ドライバ・APO）の変換の有無。
    ///
    /// `unknown` と明記する（`TR-REC-02`）。`MATCH_FORMAT` は
    /// ドライバと APO が対応する場合にのみ有効で、アプリからは確かめられない。
    pub upstream_conversion: String,
}

/// 確定したテイクの記録。
///
/// `rel_path` は既に確定済み（fsync + rename が済んでいる）。
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
///
/// `Eq` を持たない。 [`Self::peak`] が浮動小数で、全順序を持たないため。
#[derive(Debug, Clone, PartialEq)]
pub struct Take {
    pub id: i32,
    pub row_id: String,
    pub rel_path: String,
    pub frames: i64,
    pub invalid: bool,
    pub generation: i32,
    /// 波形のピーク（0.0〜1.0）。解析がまだなら `None`。
    ///
    /// 割れているかを画面で言うのに要る（`koeru_core::analysis::CLIP_THRESHOLD`）。
    /// 波形を読み直して測らない——解析は録音停止時に済んでいる（`TR-PKG-42`）。
    pub peak: Option<f32>,
    /// 録った時刻（RFC 3339）。
    ///
    /// **事実だけを置く。** 集計しない——「何回に跨るか」「何日空いたか」を
    /// 出すと、声質の推測材料を置いたことになる（`DEC-RCL-015`）。
    /// 1テイクに1つ、どう読むかは本人が決める。
    pub recorded_at: String,
}

/// 行と、その行に積んだテイク（`TR-REC-21`, `TR-RCL-25`）。
///
/// 録り直しは上書きしない。 世代として積み、採用テイクだけが
/// 配布パッケージのファイル名（＝行テキスト）を持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct RowTakes {
    pub row_id: String,
    /// 読み上げる文字列。
    pub text: String,
    pub state: RowState,
    /// この行から取れる収録単位の数。
    ///
    /// 読み上げ文字列から数えない。 空白の数で割ると、方式が増えたときに
    /// 合わなくなる（`TR-RCL-05` の CVVC は1行から CV・VC・語尾を同時に回収する）。
    pub units: u32,
    /// 世代順。非採用も含む——いつでも採用を戻せる（`TR-REC-21`）。
    pub takes: Vec<Take>,
    /// いま採用しているテイク。無ければ未収録。
    pub adopted: Option<i32>,
}

/// 問い合わせの行を [`Take`] へ組む。
///
/// 3箇所で同じ組み立てをしていた。 列を1つ足すたびに3箇所を直すことになり、
/// 1つ直し忘れても型は通る——`peak` を足したときに実際そうなりかけた。
#[allow(
    clippy::cast_possible_truncation,
    reason = "peak は 0.0..=1.0 付近。f32 で保つ"
)]
fn build_take(row: (i32, String, String, i64, i32, i32, Option<f64>, String)) -> Take {
    let (id, row_id, rel_path, frames, invalid, generation, peak, recorded_at) = row;
    Take {
        id,
        row_id,
        rel_path,
        frames,
        invalid: invalid != 0,
        generation,
        peak: peak.map(|p| p as f32),
        recorded_at,
    }
}

/// プロジェクトの台帳。
pub struct Ledger {
    conn: SqliteConnection,
}

// `SqliteConnection` は Debug を実装しない。接続の中身は出さない
// （パスやクエリが入りうる）ので、型名だけを出す。
impl std::fmt::Debug for Ledger {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Ledger { .. }")
    }
}

impl Ledger {
    /// 開いてスキーマを適用する。
    ///
    /// WAL モードにする（`TR-REC-27`）。書き込み中に読めるようにして、
    /// 収録とバックグラウンドの解析が互いを待たないようにする。
    #[tracing::instrument(skip(path), err)]
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let url = path.as_ref().to_string_lossy().into_owned();
        let mut conn =
            SqliteConnection::establish(&url).map_err(|source| LedgerError::Open { source })?;
        diesel::sql_query("PRAGMA journal_mode = WAL")
            .execute(&mut conn)
            .map_err(db("wal"))?;
        // 外部キーを効かせる。 SQLite は既定で無効。
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

    /// 録音リストを台帳へ書き込む（`TR-RCL-18`）。 出どころはフルリスト。
    ///
    /// 生成が決定的なので、並び順もそのまま持つ（`TR-RCL-27`）。
    /// 単音階なら `tones` は1つ。多音階は同じリストを音高の数だけ入れる
    /// （`TR-RCL-26`。音高ごとに独立した行集合を持つ）。
    ///
    /// 行 ID は音高ごとに分ける。 同じ ID を共有すると、どの音高の行を
    /// 録ったのかが台帳から分からなくなる。
    ///
    /// 返すのは実際に入った行の数（[`install_rows`](Self::install_rows)）。
    #[tracing::instrument(skip(self, list, rules), fields(rows = list.len(), tones = tones.len()), err)]
    pub fn install_reclist_for_tones(
        &mut self,
        list: &[ReclistRow],
        rules: &crate::presamp::Rules,
        method: crate::alias::Method,
        tones: &[i32],
    ) -> Result<usize> {
        let suffixed = tones.len() > 1;
        let mut inserted = 0;
        for tone in tones {
            inserted +=
                self.install_rows(list, rules, method, *tone, suffixed, RowOrigin::Preset)?;
        }
        Ok(inserted)
    }

    /// 1つの収録音高へ行を足す（`TR-RCL-18`）。
    ///
    /// `suffixed` は行 ID に音高名を付けるか。 多音階では付ける——付けないと、
    /// 同じ中身の行が音高を跨いで同じ ID になる。
    ///
    /// **既にある行は飛ばす。** 選択から詰め直した行（`TR-RCL-16`）は
    /// 中身の指紋を ID にするので、選び直した範囲が前と重なれば同じ ID で
    /// 戻ってくる。素の `INSERT` だと主鍵に当たって**台帳ごと落ちた。**
    ///
    /// 上書きもしない。 同じ ID は同じ中身で、既にテイクが付いているかも
    /// しれない。`ordinal` を書き換えると、録った行の並びが動く。
    ///
    /// 返すのは実際に入った行の数。 **渡した数ではない**——既にある行を
    /// 飛ばすので、同じ範囲を2度詰め直すと 0 になる。渡した数を
    /// 「足しました」と出すと、台帳が増えていないのに増えたと言うことになる。
    #[tracing::instrument(
        skip(self, list, rules, suffixed, origin),
        fields(rows = list.len(), tone),
        err
    )]
    pub fn install_rows(
        &mut self,
        list: &[ReclistRow],
        rules: &crate::presamp::Rules,
        method: crate::alias::Method,
        tone: i32,
        suffixed: bool,
        origin: RowOrigin,
    ) -> Result<usize> {
        self.conn
            .transaction(|c| insert_rows(c, list, rules, method, tone, suffixed, origin))
            .map_err(db("install_rows"))
    }

    /// その音高・出どころの、テイクを1本も持たない行（`DEC-RCL-016`）。
    ///
    /// 返すのは `(行 ID, その行が生むエイリアス)`。 除外した行は入らない——
    /// 本人が外したものを組み直しで戻さない。
    pub fn untaken_rows(
        &mut self,
        tone: i32,
        origin: RowOrigin,
    ) -> Result<Vec<(String, BTreeSet<String>)>> {
        let ids: Vec<String> = rows::table
            .filter(rows::tone.eq(tone))
            .filter(rows::origin.eq(origin.as_str()))
            .filter(rows::state.eq(RowState::Unrecorded.as_str()))
            .filter(diesel::dsl::not(diesel::dsl::exists(
                takes::table.filter(takes::row_id.eq(rows::id)),
            )))
            .order(rows::ordinal.asc())
            .select(rows::id)
            .load(&mut self.conn)
            .map_err(db("untaken_rows"))?;
        let mut out = Vec::with_capacity(ids.len());
        for id in ids {
            let aliases = self.aliases_of_row(&id)?;
            out.push((id, aliases));
        }
        Ok(out)
    }

    /// その音高・出どころの、テイクを1本も持たない行を入れ替える（`DEC-RCL-016`）。
    ///
    /// 消すのは [`untaken_rows`](Self::untaken_rows) が返す行だけ。 **録った行は
    /// 消さない**——テイクを1本でも持つ行は、無効のテイクしか無くても残す
    /// （`INV-REC-003` の「未収録の項目は、確定したテイクも無効テイクも持たない」）。
    /// 消す行は何も持っていないので、失われるものが無い。
    ///
    /// 消すのと足すのを1つのトランザクションで行う。 途中で落ちると、
    /// 録る行が消えたまま足されない。返すのは足した行の数。
    #[tracing::instrument(
        skip(self, list, rules, suffixed, origin),
        fields(rows = list.len(), tone),
        err
    )]
    pub fn replace_untaken_rows(
        &mut self,
        list: &[ReclistRow],
        rules: &crate::presamp::Rules,
        method: crate::alias::Method,
        tone: i32,
        suffixed: bool,
        origin: RowOrigin,
    ) -> Result<usize> {
        let gone: Vec<String> = self
            .untaken_rows(tone, origin)?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        self.conn
            .transaction(|c| {
                diesel::delete(row_aliases::table.filter(row_aliases::row_id.eq_any(&gone)))
                    .execute(c)?;
                diesel::delete(row_units::table.filter(row_units::row_id.eq_any(&gone)))
                    .execute(c)?;
                diesel::delete(rows::table.filter(rows::id.eq_any(&gone))).execute(c)?;
                insert_rows(c, list, rules, method, tone, suffixed, origin)
            })
            .map_err(db("replace_untaken_rows"))
    }

    /// 台帳の行を、録音リストの行の形で（`TR-SYN-19`, `DEC-RCL-016`）。 返すのは
    /// `(収録音高, 行)` で、並びは正準順。除外した行は入らない。
    ///
    /// 提示順はこれを並べる。 **プリセットから生成し直したリストを並べていた**ので、
    /// 詰め直した行も組み直した行も一度も並ばなかった。
    ///
    /// 単位は読み上げる文字列から引く（[`row_units_of`](Self::row_units_of) と同じ）。
    /// 行ごとに問い合わせない——提示順はテイクのたびに作り直す。
    pub fn listed_rows(&mut self, set: UnitSet) -> Result<Vec<(i32, ReclistRow)>> {
        let table = units(set);
        Ok(rows::table
            .filter(rows::state.ne(RowState::Excluded.as_str()))
            .order(rows::ordinal.asc())
            .select((rows::id, rows::text, rows::file_stem, rows::tone))
            .load::<(String, String, String, i32)>(&mut self.conn)
            .map_err(db("listed_rows"))?
            .into_iter()
            .map(|(id, text, file_stem, tone)| {
                let units = text
                    .split_whitespace()
                    .filter_map(|k| table.iter().find(|u| u.kana == k).cloned())
                    .collect();
                (
                    tone,
                    ReclistRow {
                        id,
                        text,
                        units,
                        file_stem,
                    },
                )
            })
            .collect())
    }

    /// 行の出どころ（`TR-RCL-18` (g)）。
    pub fn row_origin(&mut self, row_id: &str) -> Result<RowOrigin> {
        rows::table
            .find(row_id)
            .select(rows::origin)
            .first::<String>(&mut self.conn)
            .map(|s| RowOrigin::parse(&s))
            .map_err(db("row_origin"))
    }

    /// 単独音・単音階の入口。 既存の呼び出しを壊さないために残す。
    #[tracing::instrument(skip(self, list), fields(rows = list.len(), tone), err)]
    pub fn install_reclist(&mut self, list: &[ReclistRow], tone: i32) -> Result<()> {
        self.install_reclist_for_tones(
            list,
            &crate::presamp::Rules::builtin(crate::inventory::UnitSet::Core),
            crate::alias::Method::Single,
            &[tone],
        )
        .map(|_| ())
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
                        sessions::source_channel.eq(s.source_channel),
                        // 変換の記録（`TR-REC-02`）。
                        sessions::master_rate_hz.eq(s.master_rate_hz),
                        sessions::resampler.eq(&s.resampler),
                        sessions::upstream_conversion.eq(&s.upstream_conversion),
                    ))
                    .execute(c)?;
                sessions::table
                    .select(sessions::id)
                    .order(sessions::id.desc())
                    .first::<i32>(c)
            })
            .map_err(db("start_session"))
    }

    /// 最後に使ったマイク。まだ一度も開いていなければ `None`。
    ///
    /// マイクは音源に固定される（`TR-REC-03`）。 起動し直しても選び直させないため、
    /// セッションの記録から引く——専用の欄を足さない。セッションは録音条件の
    /// スナップショット（`TR-REC-30`）で、そこに既に device_id が入っている。
    pub fn last_device(&mut self) -> Result<Option<String>> {
        sessions::table
            .select(sessions::device_id)
            .order(sessions::id.desc())
            .first::<String>(&mut self.conn)
            .optional()
            .map_err(db("last_device"))
    }

    /// 確定済みのテイクを台帳へ載せる。
    ///
    /// 呼べるのは fsync と rename が済んだあとだけ（`DEC-REC-004`）。
    /// 世代は行ごとに単調に増える。採用テイクを新しい方へ切り替える（`TR-REC-21`）。
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

                // 採用を新しい方へ切り替える。過去のテイクは残る（`TR-REC-21`）。
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
    /// ファイルは消さない。 過去のテイクは残す（`TR-REC-21`）。
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
    /// カバレッジは変わらない。 行が生む単位は行が持っていて、テイクに依らない。
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
            .left_join(take_analysis::table.on(take_analysis::take_id.eq(takes::id)))
            .filter(takes::row_id.eq(row_id))
            .order(takes::generation.asc())
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
                take_analysis::peak.nullable(),
                takes::recorded_at,
            ))
            .load::<(i32, String, String, i64, i32, i32, Option<f64>, String)>(&mut self.conn)
            .map(|v| v.into_iter().map(build_take).collect())
            .map_err(db("takes_of"))
    }

    /// 収録済みの単位集合。 設定した音高のすべてで録れているものだけ。
    ///
    /// 音高の中では、採用テイクを持つ行の単位の和集合として導出する
    /// （`TR-RCL-18`。二重に保持しない）。 音高を跨いでは積を取る——
    /// 被覆は（エイリアス, 収録音高）の組で持つ（`TR-RCL-26`）。
    ///
    /// **音高を跨いだ和集合を返していた。** 多音階で1音高だけ録り終えると
    /// 全部揃ったように見え、見出しの被覆も環も満ち、完成の判定まで進んだ
    /// ——残りの音高は1本も録っていないのに。単音階では和と積が同じなので、
    /// 単音階の試験では見えなかった。
    pub fn covered_units(&mut self) -> Result<BTreeSet<String>> {
        let rows: Vec<(i32, String)> = row_units::table
            .inner_join(rows::table.on(rows::id.eq(row_units::row_id)))
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(row_units::row_id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select((rows::tone, row_units::kana))
            .load(&mut self.conn)
            .map_err(db("covered_units"))?;
        let mut by_tone: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
        for (tone, kana) in rows {
            by_tone.entry(tone).or_default().insert(kana);
        }
        // 1テイクも録っていない音高は空集合として数える。 台帳に現れないので、
        // `by_tone` の値だけで積を取ると、その音高が判定から漏れる。
        let mut tones = self.recording_tones()?.into_iter();
        let Some(first) = tones.next() else {
            return Ok(BTreeSet::new());
        };
        let mut out = by_tone.remove(&first).unwrap_or_default();
        for t in tones {
            let here = by_tone.get(&t);
            out.retain(|k| here.is_some_and(|s| s.contains(k)));
        }
        Ok(out)
    }

    /// 収録済みのエイリアス集合（`TR-RCL-18`, `TR-PKG-22`）。
    ///
    /// 採用テイクを持つ行が生むエイリアスの和集合。 書き出せる方式の判定は
    /// これで決まる（`TR-PKG-23`）。[`covered_units`](Self::covered_units) の
    /// 仮名では、連続音と CVVC の被覆を表せない。
    pub fn covered_aliases(&mut self) -> Result<BTreeSet<String>> {
        row_aliases::table
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(row_aliases::row_id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select(row_aliases::alias)
            .load::<String>(&mut self.conn)
            .map(|v| v.into_iter().collect())
            .map_err(db("covered_aliases"))
    }

    /// その行を歌うと録れるエイリアス（`TR-RCL-18`）。 持ち主かどうかは見ない。
    ///
    /// 5値を置くのは [`owned_aliases_of_row`](Self::owned_aliases_of_row) のほう。
    pub fn aliases_of_row(&mut self, row_id: &str) -> Result<BTreeSet<String>> {
        row_aliases::table
            .filter(row_aliases::row_id.eq(row_id))
            .select(row_aliases::alias)
            .load::<String>(&mut self.conn)
            .map(|v| v.into_iter().collect())
            .map_err(db("aliases_of_row"))
    }

    /// その行が持ち主になっているエイリアス（`DEC-RCL-016`）。 5値を置くのはこれだけ。
    ///
    /// **綴りから作り直さない。** 同じ音高で同じ綴りを2つの行が生むとき、
    /// 持つのは先に録った1つだけ（[`alias_owners`](Self::alias_owners)）。
    /// 作り直すと、持たない行にも5値が生え、確認キューが片方を落とす。
    pub fn owned_aliases_of_row(&mut self, row_id: &str) -> Result<BTreeSet<String>> {
        Ok(self
            .alias_owners()?
            .into_iter()
            .filter_map(|((_, alias), owner)| (owner == row_id).then_some(alias))
            .collect())
    }

    /// （収録音高, 綴り）ごとの持ち主の行（`DEC-RCL-016`）。
    ///
    /// 同じ音高で同じ綴りを生む行のうち、有効な採用テイクを持ち、最初の有効な
    /// テイクがいちばん早いもの。**先に録った行が持ち、あとから別の行を録っても
    /// 入れ替わらない**——確認済みの5値と手で直した値が、本人の知らないうちに
    /// 別の素材へ移らない。持ち主の採用テイクが無効になれば、次に録った行へ移る。
    ///
    /// 欄に持たず、テイクから導く。 欄に持つとテイクを無効にするたびに
    /// 書き換えることになり、書き換え忘れた欄が別の行を指す。
    ///
    /// **入れる順で先に名乗った行に持たせていた**（`DEC-ALN-017`）。 フルリストの
    /// あとに足した詰め直しの行は綴りを1つも持たず、録っても何も増えなかった。
    pub fn alias_owners(&mut self) -> Result<BTreeMap<(i32, String), String>> {
        let valid: BTreeSet<String> = adopted_takes::table
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select(adopted_takes::row_id)
            .load::<String>(&mut self.conn)
            .map_err(db("alias_owners.valid"))?
            .into_iter()
            .collect();
        let mut first: BTreeMap<String, i32> = BTreeMap::new();
        for (row, id) in takes::table
            .filter(takes::invalid.eq(0))
            .select((takes::row_id, takes::id))
            .load::<(String, i32)>(&mut self.conn)
            .map_err(db("alias_owners.first"))?
        {
            let e = first.entry(row).or_insert(id);
            *e = (*e).min(id);
        }
        let produced: Vec<(String, i32, String)> = row_aliases::table
            .inner_join(rows::table.on(rows::id.eq(row_aliases::row_id)))
            .select((rows::id, rows::tone, row_aliases::alias))
            .load(&mut self.conn)
            .map_err(db("alias_owners.aliases"))?;
        let mut best: BTreeMap<(i32, String), (i32, String)> = BTreeMap::new();
        for (row, tone, alias) in produced {
            if !valid.contains(&row) {
                continue;
            }
            let Some(at) = first.get(&row).copied() else {
                continue;
            };
            let e = best.entry((tone, alias)).or_insert((at, row.clone()));
            // 同じテイクの番号は2行に付かないが、並びを決定的にしておく。
            if (at, &row) < (e.0, &e.1) {
                *e = (at, row);
            }
        }
        Ok(best.into_iter().map(|(k, (_, row))| (k, row)).collect())
    }

    /// 録音リストが要求するエイリアスの全体（`TR-RCL-18`）。
    ///
    /// 録ったかどうかは見ない。 分母になるほう。
    pub fn all_aliases(&mut self) -> Result<BTreeSet<String>> {
        row_aliases::table
            .select(row_aliases::alias)
            .load::<String>(&mut self.conn)
            .map(|v| v.into_iter().collect())
            .map_err(db("all_aliases"))
    }

    /// 音高ごとの収録済みエイリアス（`TR-RCL-26`）。
    ///
    /// > 収録済み単位は (エイリアス, 収録音高) の組で管理する
    ///
    /// 音高を跨いで混ぜない。 1音高だけ録り終えても、音域の広い曲は歌えない。
    pub fn covered_aliases_by_tone(&mut self) -> Result<BTreeMap<i32, BTreeSet<String>>> {
        let rows: Vec<(i32, String)> = row_aliases::table
            .inner_join(rows::table.on(rows::id.eq(row_aliases::row_id)))
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(rows::id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select((rows::tone, row_aliases::alias))
            .load(&mut self.conn)
            .map_err(db("covered_aliases_by_tone"))?;
        let mut out: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
        for (tone, alias) in rows {
            out.entry(tone).or_default().insert(alias);
        }
        Ok(out)
    }

    /// いまの録る順（`TR-SYN-19`）。
    ///
    /// 返すのは `(モード, 本人が明示的に選んだか)`。 立っていれば自動では戻さない。
    pub fn recording_order(&mut self) -> Result<(crate::order::Mode, bool)> {
        recording_order::table
            .find(1)
            .select((recording_order::mode, recording_order::pinned))
            .first::<(String, i32)>(&mut self.conn)
            .map(|(m, p)| {
                let mode = if m == "coverage_efficiency" {
                    crate::order::Mode::CoverageEfficiency
                } else {
                    crate::order::Mode::SongBankFirst
                };
                (mode, p != 0)
            })
            .map_err(db("recording_order"))
    }

    /// 録る順を切り替える（`TR-SYN-19`）。
    ///
    /// `pinned` は本人の明示的な操作かどうか。 自動の移行（曲バンクが完全に
    /// なったとき）では立てない——立てると、そのあと本人が選んだことになる。
    pub fn set_recording_order(&mut self, mode: crate::order::Mode, pinned: bool) -> Result<()> {
        let name = match mode {
            crate::order::Mode::SongBankFirst => "song_bank_first",
            crate::order::Mode::CoverageEfficiency => "coverage_efficiency",
        };
        diesel::update(recording_order::table.find(1))
            .set((
                recording_order::mode.eq(name),
                recording_order::pinned.eq(i32::from(pinned)),
            ))
            .execute(&mut self.conn)
            .map(|_| ())
            .map_err(db("set_recording_order"))
    }

    /// その行の収録音高（`TR-REC-25`）。
    pub fn row_tone(&mut self, row_id: &str) -> Result<i32> {
        rows::table
            .find(row_id)
            .select(rows::tone)
            .first::<i32>(&mut self.conn)
            .map_err(db("row_tone"))
    }

    /// 行 ID から収録音高を引く表（`TR-ALN-22`）。
    ///
    /// 一貫性補正の集団を音階内に閉じるために要る。 1件ずつ問い合わせると、
    /// テイクの数だけ往復する。
    pub fn row_tones(&mut self) -> Result<BTreeMap<String, i32>> {
        rows::table
            .select((rows::id, rows::tone))
            .load::<(String, i32)>(&mut self.conn)
            .map(|v| v.into_iter().collect())
            .map_err(db("row_tones"))
    }

    /// その行のテイク数（`TR-RCL-25`）。世代番号の採番に使う。
    ///
    /// 無効にしたものも数える。 世代は録った順の通し番号なので、
    /// 失敗したテイクを飛ばすと番号が詰まって、後から並べ直せなくなる。
    pub fn take_count(&mut self, row_id: &str) -> Result<u32> {
        takes::table
            .filter(takes::row_id.eq(row_id))
            .count()
            .get_result::<i64>(&mut self.conn)
            .map(|n| u32::try_from(n).unwrap_or(u32::MAX))
            .map_err(db("take_count"))
    }

    /// 音高ごとの消化率（`TR-RCL-26`）。
    ///
    /// > 進捗表示では「いま歌える曲の数」を音高を跨いだ実際の判定結果で出し、
    /// > 音高ごとの消化率は詳細表示に置く
    ///
    /// 返すのは音高ごとの `(録り終えた行, その音高の行の総数)`。
    /// **跨いで足さない。** 3音高のうち1本だけ録り終えても「33%」にはならない
    /// ——音域の広い曲は依然として歌えない。
    pub fn progress_by_tone(&mut self) -> Result<BTreeMap<i32, (usize, usize)>> {
        let all: Vec<(i32, String)> = rows::table
            .select((rows::tone, rows::id))
            .load(&mut self.conn)
            .map_err(db("progress_by_tone"))?;
        let done: BTreeSet<String> = adopted_takes::table
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select(adopted_takes::row_id)
            .load::<String>(&mut self.conn)
            .map_err(db("progress_by_tone"))?
            .into_iter()
            .collect();
        let mut out: BTreeMap<i32, (usize, usize)> = BTreeMap::new();
        for (tone, id) in all {
            let e = out.entry(tone).or_insert((0, 0));
            e.1 += 1;
            if done.contains(&id) {
                e.0 += 1;
            }
        }
        Ok(out)
    }

    /// 方式が要求するエイリアスのうち、まだ無いもの（`TR-PKG-23`）。
    ///
    /// 不足を全件返す。 「何件足りない」だけだと、部分的なパッケージを
    /// 出したくなる誘惑が残る。
    pub fn missing_aliases(&mut self, required: &BTreeSet<String>) -> Result<Vec<String>> {
        let have = self.covered_aliases()?;
        Ok(required.difference(&have).cloned().collect())
    }

    /// アライメントが出した境界を保存する（`TR-ALN-34`）。
    ///
    /// 同じテイクの同じエイリアスは差し替える。 再アライメントしたときに
    /// 古い境界が残ると、5値を作り直したときだけ値が飛ぶ。
    pub fn put_boundaries(&mut self, take_id: i32, entries: &[(String, Boundary)]) -> Result<()> {
        self.conn
            .transaction(|c| {
                for (alias, b) in entries {
                    diesel::insert_into(take_boundaries::table)
                        .values((
                            take_boundaries::take_id.eq(take_id),
                            take_boundaries::alias.eq(alias),
                            take_boundaries::voice_start_ms.eq(b.voice_start_ms),
                            take_boundaries::vowel_start_ms.eq(b.vowel_start_ms),
                            take_boundaries::vowel_end_ms.eq(b.vowel_end_ms),
                        ))
                        .on_conflict((take_boundaries::take_id, take_boundaries::alias))
                        .do_update()
                        .set((
                            take_boundaries::voice_start_ms.eq(b.voice_start_ms),
                            take_boundaries::vowel_start_ms.eq(b.vowel_start_ms),
                            take_boundaries::vowel_end_ms.eq(b.vowel_end_ms),
                        ))
                        .execute(c)?;
                }
                Ok(())
            })
            .map_err(db("put_boundaries"))
    }

    /// そのテイクの境界（`TR-ALN-34`）。
    ///
    /// 空なら、この要件より前に推定したテイク。 再導出の対象外で、
    /// 求められたらアライメントからやり直す。
    pub fn boundaries_for_take(&mut self, take_id: i32) -> Result<Vec<(String, Boundary)>> {
        take_boundaries::table
            .filter(take_boundaries::take_id.eq(take_id))
            .order(take_boundaries::alias.asc())
            .select((
                take_boundaries::alias,
                take_boundaries::voice_start_ms,
                take_boundaries::vowel_start_ms,
                take_boundaries::vowel_end_ms,
            ))
            .load::<(String, f64, f64, f64)>(&mut self.conn)
            .map(|v| {
                v.into_iter()
                    .map(|(alias, voice_start_ms, vowel_start_ms, vowel_end_ms)| {
                        (
                            alias,
                            Boundary {
                                voice_start_ms,
                                vowel_start_ms,
                                vowel_end_ms,
                            },
                        )
                    })
                    .collect()
            })
            .map_err(db("boundaries_for_take"))
    }

    /// このプロジェクトの収録音高（MIDI、`TR-REC-25`）。
    ///
    /// 行が名乗っている音高の集合。 プロジェクト作成時に確定し、途中で増減しない
    /// （`TR-REC-25` の「1プロジェクトで収録する音高の集合はプロジェクト作成時に
    /// 確定させ、収録途中に増減させない」）。単音階なら1つ。
    pub fn recording_tones(&mut self) -> Result<Vec<i32>> {
        let mut v = rows::table
            .select(rows::tone)
            .distinct()
            .load::<i32>(&mut self.conn)
            .map_err(db("recording_tones"))?;
        v.sort_unstable();
        Ok(v)
    }

    /// 五十音の行ごとの被覆（`DEC-PLT-025` の環）。
    ///
    /// 並びは五十音順（`crate::inventory::KANA_ROWS`）。 録音リストの並びで返さない
    /// ——あれは presamp から機械的に導いた順で、内側から 母音・ち・ぎ・つ・ぴ……
    /// となり、**どの環がどの行かを人が数えられない。**
    ///
    /// **音素ではなく行で畳む。** 音素だと 28 本になり、同心の線がその密度では
    /// 閉じ具合を読めない（`crate::inventory::kana_row`）。
    ///
    /// 行の名前は返さない。 環に要るのは順番と数だけで、行の名前は
    /// 画面に出す文字列ではない（`TR-REC-18`）。
    ///
    /// 五十音の行に属さない単位は、最後にまとめて1本の環になる。 拡張セットの
    /// ヴだけが当たる。落とさないのは、分母が合わなくなるため。
    ///
    /// # Errors
    ///
    /// 台帳を読めないとき。
    #[tracing::instrument(skip(self), err)]
    pub fn coverage_by_kana_row(&mut self) -> Result<Vec<(u32, u32)>> {
        let all = row_units::table
            .select((row_units::consonant, row_units::kana))
            .load::<(String, String)>(&mut self.conn)
            .map_err(db("coverage_by_kana_row"))?;
        let covered = self.covered_units()?;

        let mut counts: std::collections::HashMap<String, (u32, u32)> =
            std::collections::HashMap::new();
        // 同じ仮名を二度数えない。 行が2つ同じ単位を生むことはありうるが、
        // 被覆は単位の集合なので（`TR-RCL-19`）、環の分母も集合で数える。
        let mut seen: BTreeSet<String> = BTreeSet::new();
        // 五十音の行に入らないものの並び。初出の順で足す。
        let mut orphans: Vec<String> = Vec::new();

        for (consonant, kana) in all {
            if !seen.insert(kana.clone()) {
                continue;
            }
            let key = crate::inventory::kana_row(&consonant).map_or_else(
                || {
                    if !orphans.contains(&consonant) {
                        orphans.push(consonant.clone());
                    }
                    consonant.clone()
                },
                ToOwned::to_owned,
            );
            let slot = counts.entry(key).or_insert((0, 0));
            slot.1 += 1;
            if covered.contains(&kana) {
                slot.0 += 1;
            }
        }

        Ok(crate::inventory::KANA_ROWS
            .iter()
            .map(|r| (*r).to_owned())
            .chain(orphans)
            .filter_map(|k| counts.get(&k).copied())
            .collect())
    }

    /// 採用テイクの観測（`DEC-PLT-027` の声の色）。
    ///
    /// 採用しているものだけを見る。 非採用まで混ぜると、切り替えても色が
    /// 変わらない——採用の切り替えは「声の表情を選ぶ」操作なので
    /// （`DEC-PLT-025`）、色が動かないと選んだことにならない。
    ///
    /// `rate_hz` は F0 のフレーム長を出すためだけに要る。
    ///
    /// # Errors
    ///
    /// 台帳を読めないとき。
    #[tracing::instrument(skip(self), err)]
    pub fn adopted_voice(&mut self, rate_hz: u32) -> Result<Vec<crate::voice::TakeVoice>> {
        let rows = take_analysis::table
            .inner_join(adopted_takes::table.on(adopted_takes::take_id.eq(take_analysis::take_id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .filter(takes::invalid.eq(0))
            .select((
                take_analysis::f0,
                take_analysis::centroid_hz,
                take_analysis::hop_size,
            ))
            .load::<(Vec<u8>, Option<f64>, i32)>(&mut self.conn)
            .map_err(db("adopted_voice"))?;

        Ok(rows
            .into_iter()
            .map(|(f0, centroid_hz, hop)| crate::voice::TakeVoice {
                f0: bytes_to_f64s(&f0),
                centroid_hz,
                frame_ms: f64::from(hop.max(1)) * 1000.0 / f64::from(rate_hz.max(1)),
            })
            .collect())
    }

    /// その行が未収録なら、読み上げるテキストを返す（`TR-REC-18`）。
    ///
    /// 提示順（`TR-SYN-19`）は行 ID の並びしか持たないので、先頭の行の
    /// 本文を引くのに要る。状態もここで見る。提示順が古くても、
    /// 収録済みや除外済みの行を次の収録に戻さない。
    pub fn unrecorded_row_text(&mut self, row_id: &str) -> Result<Option<String>> {
        rows::table
            .find(row_id)
            .filter(rows::state.eq(RowState::Unrecorded.as_str()))
            .select(rows::text)
            .first::<String>(&mut self.conn)
            .optional()
            .map_err(db("unrecorded_row_text"))
    }

    /// 次に録る行（`TR-REC-18`）。未録音のうち並び順が最も早いもの。
    ///
    /// **提示順（`TR-SYN-19`）は見ない。** ここは正準順の退避経路で、
    /// モードを反映した並びは `Studio::next_presented_row` が持つ。
    pub fn next_row(&mut self) -> Result<Option<(String, String)>> {
        rows::table
            .filter(rows::state.eq(RowState::Unrecorded.as_str()))
            .order(rows::ordinal.asc())
            .select((rows::id, rows::text))
            .first::<(String, String)>(&mut self.conn)
            .optional()
            .map_err(db("next_row"))
    }

    /// 全部の行と、それぞれのテイク（`TR-REC-21`, `TR-RCL-25`）。
    ///
    /// 録り直しの入口。 `next_row` は未収録しか返さないので、
    /// これが無いと一度録った行を二度と選べない。
    ///
    /// # Errors
    ///
    /// 台帳を読めないとき。
    #[tracing::instrument(skip(self), err)]
    pub fn rows_with_takes(&mut self) -> Result<Vec<RowTakes>> {
        // 3クエリで済ませる。 行ごとに引くと、行数ぶん往復する。
        let rows = rows::table
            .order(rows::ordinal.asc())
            .select((rows::id, rows::text, rows::state))
            .load::<(String, String, String)>(&mut self.conn)
            .map_err(db("rows_with_takes.rows"))?;

        let takes = takes::table
            .left_join(take_analysis::table.on(take_analysis::take_id.eq(takes::id)))
            .order((takes::row_id.asc(), takes::generation.asc()))
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
                take_analysis::peak.nullable(),
                takes::recorded_at,
            ))
            .load::<(i32, String, String, i64, i32, i32, Option<f64>, String)>(&mut self.conn)
            .map_err(db("rows_with_takes.takes"))?;

        let adopted = adopted_takes::table
            .select((adopted_takes::row_id, adopted_takes::take_id))
            .load::<(String, i32)>(&mut self.conn)
            .map_err(db("rows_with_takes.adopted"))?;

        let units = row_units::table
            .select(row_units::row_id)
            .load::<String>(&mut self.conn)
            .map_err(db("rows_with_takes.units"))?;

        let mut by_row: std::collections::HashMap<String, Vec<Take>> =
            std::collections::HashMap::new();
        for raw in takes {
            let take = build_take(raw);
            by_row.entry(take.row_id.clone()).or_default().push(take);
        }
        let adopted: std::collections::HashMap<String, i32> = adopted.into_iter().collect();

        let mut unit_counts: std::collections::HashMap<String, u32> =
            std::collections::HashMap::new();
        for row_id in units {
            *unit_counts.entry(row_id).or_default() += 1;
        }

        Ok(rows
            .into_iter()
            .map(|(row_id, text, state)| RowTakes {
                takes: by_row.remove(&row_id).unwrap_or_default(),
                adopted: adopted.get(&row_id).copied(),
                state: RowState::parse(&state),
                units: unit_counts.get(&row_id).copied().unwrap_or(0),
                row_id,
                text,
            })
            .collect())
    }

    /// 台帳が知らない確定済みファイルを見つける（`DEC-REC-004` の孤児）。
    ///
    /// 提示するだけ。DB へ自動で書き戻さない（`TR-REC-31` の「自動修復しない」）。
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

    /// oto の5値を保存する（`TR-ALN-13`）。
    ///
    /// エイリアス単位。 単独音でも1ファイルに複数モーラが入るので
    /// （`TR-RCL-03`、`DEC-ALN-013`）、1テイクに複数のエントリがぶら下がる。
    /// 同じ WAV を複数のエイリアスが別の位置で指す。
    /// oto を1件書く。
    ///
    /// `parts` は確信度の成分（`TR-ALN-24` の「成分ごとの値も保持する」）。
    /// 合成スコアだけを持つと、開き直したあとに `TR-ALN-26` (3) の主因が出せない
    /// ——合成は積なので、成分を作り直そうとすると値が歪む。
    pub fn put_oto(
        &mut self,
        take_id: i32,
        alias: &str,
        o: &koeru_oto::Oto,
        confidence: f64,
        parts: Option<&ConfidenceParts>,
        hand_edited: bool,
    ) -> Result<()> {
        diesel::insert_into(oto_values::table)
            .values((
                oto_values::take_id.eq(take_id),
                oto_values::alias.eq(alias),
                oto_values::offset_ms.eq(o.offset_ms),
                oto_values::consonant_ms.eq(o.consonant_ms),
                oto_values::cutoff_ms.eq(o.cutoff_ms),
                oto_values::preutterance_ms.eq(o.preutterance_ms),
                oto_values::overlap_ms.eq(o.overlap_ms),
                oto_values::confidence.eq(confidence),
                oto_values::conf_path.eq(parts.and_then(|p| p.path)),
                oto_values::conf_sharpness.eq(parts.map(|p| p.sharpness)),
                oto_values::conf_prior.eq(parts.map(|p| p.prior)),
                oto_values::conf_acoustic.eq(parts.map(|p| p.acoustic)),
                oto_values::hand_edited.eq(i32::from(hand_edited)),
            ))
            .on_conflict((oto_values::take_id, oto_values::alias))
            .do_update()
            .set((
                oto_values::offset_ms.eq(o.offset_ms),
                oto_values::consonant_ms.eq(o.consonant_ms),
                oto_values::cutoff_ms.eq(o.cutoff_ms),
                oto_values::preutterance_ms.eq(o.preutterance_ms),
                oto_values::overlap_ms.eq(o.overlap_ms),
                oto_values::confidence.eq(confidence),
                oto_values::conf_path.eq(parts.and_then(|p| p.path)),
                oto_values::conf_sharpness.eq(parts.map(|p| p.sharpness)),
                oto_values::conf_prior.eq(parts.map(|p| p.prior)),
                oto_values::conf_acoustic.eq(parts.map(|p| p.acoustic)),
                oto_values::hand_edited.eq(i32::from(hand_edited)),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_oto"))?;
        Ok(())
    }

    /// 録音停止時の解析値を保存する（`TR-PKG-05`, `TR-PKG-42`）。
    ///
    /// ここで入れたものを書き出し時に使う。WAV を読み直さない。
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
                take_analysis::centroid_hz.eq(a.centroid_hz),
            ))
            .on_conflict(take_analysis::take_id)
            .do_update()
            .set((
                take_analysis::peak.eq(f64::from(a.peak)),
                take_analysis::hop_size.eq(hop),
                take_analysis::f0.eq(&f0),
                take_analysis::amp.eq(&amp),
                take_analysis::thumbnail.eq(&a.thumbnail),
                take_analysis::centroid_hz.eq(a.centroid_hz),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_analysis"))?;
        Ok(())
    }

    /// 解析値を引く。無ければ `None`。 解析が無いことは失敗ではない
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
                take_analysis::centroid_hz,
            ))
            .first::<(f64, Vec<u8>, Vec<u8>, Vec<u8>, Option<f64>)>(&mut self.conn)
            .optional()
            .map_err(db("analysis_of"))?;

        #[allow(
            clippy::cast_possible_truncation,
            reason = "peak は 0.0..=1.0 付近。f32 で保つ"
        )]
        Ok(
            row.map(|(peak, f0, amp, thumbnail, centroid_hz)| TakeAnalysis {
                peak: peak as f32,
                frq: Frq {
                    f0: bytes_to_f64s(&f0),
                    amp: bytes_to_f64s(&amp),
                },
                thumbnail,
                centroid_hz,
            }),
        )
    }

    /// 書き出しを1件記録する（`TR-PKG-44`）。
    ///
    /// 連番と書き出し先の名前はここが決める。 呼び出し側に採番させると、
    /// 同じ番号のリリースが2つできる。返るのは確定したレコード。
    ///
    /// 書き出し先の名前は過去のものと衝突しない（連番が先頭に付く）。
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

    /// 次に採る書き出しの連番（`TR-PKG-44`）。
    ///
    /// 台帳を変えない。 名前を先に決めてから包み、包み終えてから
    /// [`Self::record_release`] で確定させるために要る——逆にすると、
    /// 包むのに失敗した回の記録だけが残る。
    #[tracing::instrument(skip(self), err)]
    pub fn next_release_seq(&mut self) -> Result<i32> {
        Ok(releases::table
            .select(diesel::dsl::max(releases::seq))
            .first::<Option<i32>>(&mut self.conn)
            .map_err(db("next_release_seq"))?
            .unwrap_or(0)
            + 1)
    }

    /// 書き出しの履歴を古い順に引く（`TR-PKG-44`）。
    ///
    /// 過去のリリースはここからだけ取り出せる（`TR-PKG-46`）。
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
                    // 保存した方式名が読めなくても履歴は出す。
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

    /// 一番新しい書き出し。外部編集の検出はここと突き合わせる（`TR-PKG-48`）。
    #[tracing::instrument(skip(self), err)]
    pub fn latest_release(&mut self) -> Result<Option<Release>> {
        Ok(self.releases()?.pop())
    }

    /// 書き出し履歴があるか（`TR-PKG-33` の `handoff_state`）。
    ///
    /// 完成判定はこれを参照しない。
    #[tracing::instrument(skip(self), err)]
    pub fn has_been_exported(&mut self) -> Result<bool> {
        let n: i64 = releases::table
            .count()
            .get_result(&mut self.conn)
            .map_err(db("has_been_exported"))?;
        Ok(n > 0)
    }

    /// まだ採用テイクが無い行の数。残量の見積もりに使う（`REQ-REC-110`）。
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
    /// 測った値で自動的に無効化しない。 自動無効化は取りこぼし（`TR-REC-07`）と
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
        // SQLite に -inf は入らない。 無音のピークを -1000 dBFS で表す。
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
            // 参考値（`TR-REC-26`）。切り出しの根拠にしない。
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
    /// 書き出しの直前に一度だけ呼ぶ。 収録中には呼ばない
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
    /// デバイスごとに1つ。 同じプロジェクトを別のマイクで続けることがあり、
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

    /// そのデバイスの校正結果を引く。まだ校正していなければ `None`。
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
    /// 同じ id なら差し替える。 同梱曲を毎回入れ直せるようにしておく。
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
                    // 内部形式の一部（`TR-SYN-30`）。読み込んだ UST の
                    // テンポを落とすと、試唱の速さが復元できない。
                    songs::tempo_bpm.eq(song.tempo_bpm),
                    songs::default_portamento_ms.eq(song.default_portamento_ms),
                    songs::transpose.eq(song.transpose),
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
                            song_notes::rest_ticks
                                .eq(i32::try_from(n.rest_ticks).unwrap_or(i32::MAX)),
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
    /// バンクが空でも成立する。 そのとき進捗はカバレッジだけで読む。
    #[tracing::instrument(skip(self), err)]
    pub fn songs_in_bank(&mut self) -> Result<Vec<(String, Song)>> {
        Ok(self
            .songs(true)?
            .into_iter()
            .map(|(id, song, _)| (id, song))
            .collect())
    }

    /// 取り込んだ曲すべて（`TR-RCL-12`）。バンクに入っているかを添える。
    ///
    /// **外した曲も返す。** バンクの中だけを返すと、一度外した曲が
    /// どこからも見えなくなり、戻す道が無くなる。
    #[tracing::instrument(skip(self), err)]
    pub fn all_songs(&mut self) -> Result<Vec<(String, Song, bool)>> {
        self.songs(false)
    }

    /// 曲を読む。 `in_bank_only` ならバンクの中だけ。
    fn songs(&mut self, in_bank_only: bool) -> Result<Vec<(String, Song, bool)>> {
        let mut query = songs::table.into_boxed();
        if in_bank_only {
            query = query.filter(songs::in_bank.eq(1));
        }
        let heads = query
            .order(songs::added_at.asc())
            .select((
                songs::id,
                songs::title,
                songs::source,
                songs::license,
                songs::tempo_bpm,
                songs::default_portamento_ms,
                songs::in_bank,
                songs::transpose,
            ))
            .load::<(String, String, String, String, f64, f64, i32, i32)>(&mut self.conn)
            .map_err(db("songs"))?;

        let mut out = Vec::with_capacity(heads.len());
        for (id, title, source, license, tempo_bpm, default_portamento_ms, in_bank, transpose) in
            heads
        {
            let notes = song_notes::table
                .filter(song_notes::song_id.eq(&id))
                .order(song_notes::ordinal.asc())
                .select((
                    song_notes::lyric,
                    song_notes::midi,
                    song_notes::ticks,
                    song_notes::rest_ticks,
                ))
                .load::<(String, i32, i32, i32)>(&mut self.conn)
                .map_err(db("songs"))?;
            out.push((
                id,
                Song {
                    title,
                    notes: notes
                        .into_iter()
                        .map(|(lyric, midi, ticks, rest_ticks)| Note {
                            lyric,
                            midi,
                            ticks: u32::try_from(ticks).unwrap_or(0),
                            rest_ticks: u32::try_from(rest_ticks).unwrap_or(0),
                        })
                        .collect(),
                    provenance: Provenance { source, license },
                    tempo_bpm,
                    default_portamento_ms,
                    transpose,
                },
                in_bank == 1,
            ));
        }
        Ok(out)
    }

    /// 曲をバンクから外す／戻す（`TR-RCL-12`）。
    ///
    /// 曲そのものは消さない。 同梱分も本人が外せる。
    #[tracing::instrument(skip(self), err)]
    pub fn set_song_in_bank(&mut self, id: &str, in_bank: bool) -> Result<()> {
        diesel::update(songs::table.filter(songs::id.eq(id)))
            .set(songs::in_bank.eq(i32::from(in_bank)))
            .execute(&mut self.conn)
            .map_err(db("set_song_in_bank"))?;
        Ok(())
    }

    /// 曲の題を変える（`TR-RCL-12`）。
    ///
    /// 題はファイル名から採るので、そのままでは一覧に並べられないことがある。
    /// 曲そのものは触らない。
    ///
    /// **題をトレースに載せない。** 持ち込んだファイルの名前がそのまま題になる
    /// ので、ここを流すとファイル名が外へ出る（`TR-PKG-56` の「音源名・ファイルパス・
    /// 歌詞・プロジェクト名・波形を送らない」）。
    #[tracing::instrument(skip(self, id, title), err)]
    pub fn rename_song(&mut self, id: &str, title: &str) -> Result<()> {
        diesel::update(songs::table.filter(songs::id.eq(id)))
            .set(songs::title.eq(title))
            .execute(&mut self.conn)
            .map_err(db("rename_song"))?;
        Ok(())
    }

    /// 曲のキーを決める（`TR-SYN-15`）。
    ///
    /// 曲そのものは触らない。 ノートの音高は元のまま持ち、
    /// 鳴らすときに一律で足す——戻せなくなる形で書き換えない。
    #[tracing::instrument(skip(self, id), err)]
    pub fn set_song_transpose(&mut self, id: &str, semitones: i32) -> Result<()> {
        diesel::update(songs::table.filter(songs::id.eq(id)))
            .set(songs::transpose.eq(semitones))
            .execute(&mut self.conn)
            .map_err(db("set_song_transpose"))?;
        Ok(())
    }

    /// その綴りを鳴らすためのテイク（`TR-SYN-12`, `TR-RCL-18`）。
    ///
    /// 採用テイクだけ。 無効にしたテイク（取りこぼし、`TR-REC-07`）は入らない。
    /// 同じ綴りを複数の行が生むときは、先に来る行のものを使う（決定的にする）。
    #[tracing::instrument(skip(self, alias), err)]
    pub fn take_for_alias(&mut self, alias: &str) -> Result<Option<Take>> {
        self.take_for_alias_at(alias, None)
    }

    /// その収録音高で、その綴りを鳴らすためのテイク（`TR-SYN-16`）。
    ///
    /// `tone` が `None` なら音高を問わない（単音階）。 多音階では
    /// **音高を跨いで拾わない**——高音階の素材を低音階の音符に当てると、
    /// 1音だけ別人の声になる。
    ///
    /// **仮名で引いていた。** 原音設定は行が生む綴りで置かれている
    /// （`TR-RCL-18`）ので、連続音では `か` を引いても何も出ず、
    /// 試唱の素材が1つも載らなかった。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。
    #[tracing::instrument(skip(self, alias), err)]
    pub fn take_for_alias_at(&mut self, alias: &str, tone: Option<i32>) -> Result<Option<Take>> {
        let mut q = row_aliases::table
            .inner_join(rows::table.on(rows::id.eq(row_aliases::row_id)))
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(rows::id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .left_join(take_analysis::table.on(take_analysis::take_id.eq(takes::id)))
            .filter(row_aliases::alias.eq(alias))
            .filter(takes::invalid.eq(0))
            .into_boxed();
        if let Some(t) = tone {
            q = q.filter(rows::tone.eq(t));
        }
        let row = q
            .order(rows::ordinal.asc())
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
                take_analysis::peak.nullable(),
                takes::recorded_at,
            ))
            .first::<(i32, String, String, i64, i32, i32, Option<f64>, String)>(&mut self.conn)
            .optional()
            .map_err(db("take_for_alias"))?;

        Ok(row.map(build_take))
    }

    /// テイクを1件引く。無ければ `None`。
    #[tracing::instrument(skip(self), fields(take_id), err)]
    pub fn take(&mut self, take_id: i32) -> Result<Option<Take>> {
        takes::table
            .left_join(take_analysis::table.on(take_analysis::take_id.eq(takes::id)))
            .filter(takes::id.eq(take_id))
            .select((
                takes::id,
                takes::row_id,
                takes::rel_path,
                takes::frames,
                takes::invalid,
                takes::generation,
                take_analysis::peak.nullable(),
                takes::recorded_at,
            ))
            .first::<(i32, String, String, i64, i32, i32, Option<f64>, String)>(&mut self.conn)
            .optional()
            .map_err(db("take"))
            .map(|o| o.map(build_take))
    }

    /// テイクに紐づく oto の5値を引く。まだ無ければ `None`。
    #[tracing::instrument(skip(self, alias), fields(take_id), err)]
    /// そのテイクのエイリアスに対する oto（`DEC-ALN-013`）。
    pub fn oto_of(&mut self, take_id: i32, alias: &str) -> Result<Option<koeru_oto::Oto>> {
        oto_values::table
            .filter(oto_values::take_id.eq(take_id))
            .filter(oto_values::alias.eq(alias))
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

    /// そのテイクに紐づく oto を全部（`DEC-ALN-013`）。
    ///
    /// 並びはエイリアス順で常に同じ（`TR-ALN-29` の決定性）。
    pub fn otos_of(&mut self, take_id: i32) -> Result<Vec<(String, koeru_oto::Oto)>> {
        oto_values::table
            .filter(oto_values::take_id.eq(take_id))
            .order(oto_values::alias.asc())
            .select((
                oto_values::alias,
                oto_values::offset_ms,
                oto_values::consonant_ms,
                oto_values::cutoff_ms,
                oto_values::preutterance_ms,
                oto_values::overlap_ms,
            ))
            .load::<(String, f64, f64, f64, f64, f64)>(&mut self.conn)
            .map_err(db("otos_of"))
            .map(|v| {
                v.into_iter()
                    .map(
                        |(
                            alias,
                            offset_ms,
                            consonant_ms,
                            cutoff_ms,
                            preutterance_ms,
                            overlap_ms,
                        )| {
                            (
                                alias,
                                koeru_oto::Oto {
                                    offset_ms,
                                    consonant_ms,
                                    cutoff_ms,
                                    preutterance_ms,
                                    overlap_ms,
                                },
                            )
                        },
                    )
                    .collect()
            })
    }

    /// 行のモーラ列を、読み上げる順で（`TR-RCL-18`）。
    ///
    /// 並びは `rows.text` が持つ。 `row_units` は集合で入っている
    /// ——同じ仮名を2度持つ行があるので、そこからは順も重複も出ない。
    /// **`row_units` から引いていた。** 仮名が重なる行では、モーラごとの
    /// 境界と1つずつずれた組で oto を導くことになる。
    pub fn units_of(&mut self, row_id: &str) -> Result<Vec<String>> {
        let text: String = rows::table
            .find(row_id)
            .select(rows::text)
            .first(&mut self.conn)
            .map_err(db("units_of"))?;
        Ok(text.split_whitespace().map(str::to_owned).collect())
    }

    /// 行の収録単位を、読み上げる順で。
    ///
    /// 綴りを作るのに子音と母音クラスが要る（[`crate::reclist::row_entries`]）。
    /// インベントリに無い仮名は落とす——行はインベントリから作られるので、
    /// 落ちるのは単位集合を切り替えたときだけ。
    pub fn row_units_of(&mut self, row_id: &str, set: UnitSet) -> Result<Vec<Unit>> {
        let table = units(set);
        Ok(self
            .units_of(row_id)?
            .into_iter()
            .filter_map(|k| table.iter().find(|u| u.kana == k).cloned())
            .collect())
    }

    /// 配布に出す値を読む（`PROFILE-M4`）。まだ決めていなければ `None`。
    #[tracing::instrument(skip(self), err)]
    pub fn distribution(&mut self) -> Result<Option<Distribution>> {
        distribution::table
            .find(1)
            .select((
                distribution::distribution_name,
                distribution::profile,
                distribution::author,
                distribution::voice,
                distribution::sample,
                distribution::web,
                distribution::version,
                distribution::icon,
                distribution::portrait,
                distribution::portrait_opacity,
                distribution::portrait_height,
                distribution::tone_range_note,
                distribution::terms,
                distribution::credit_example,
                distribution::contact,
                distribution::disclaimer,
                distribution::character_note,
            ))
            .first::<DistributionRow>(&mut self.conn)
            .optional()
            .map_err(db("distribution"))
            .map(|r| r.map(Distribution::from))
    }

    /// 配布に出す値を保存する（`PROFILE-M4`）。
    ///
    /// 1行しか無いので、常に差し替える。
    #[tracing::instrument(skip(self, d), fields(profile = %d.profile), err)]
    pub fn set_distribution(&mut self, d: &Distribution) -> Result<()> {
        let values = (
            distribution::distribution_name.eq(&d.distribution_name),
            distribution::profile.eq(&d.profile),
            distribution::author.eq(&d.author),
            distribution::voice.eq(&d.voice),
            distribution::sample.eq(&d.sample),
            distribution::web.eq(&d.web),
            distribution::version.eq(&d.version),
            distribution::icon.eq(&d.icon),
            distribution::portrait.eq(&d.portrait),
            distribution::portrait_opacity.eq(d.portrait_opacity),
            distribution::portrait_height.eq(d.portrait_height),
            distribution::tone_range_note.eq(&d.tone_range_note),
            distribution::terms.eq(&d.terms),
            distribution::credit_example.eq(&d.credit_example),
            distribution::contact.eq(&d.contact),
            distribution::disclaimer.eq(&d.disclaimer),
            distribution::character_note.eq(&d.character_note),
        );
        diesel::insert_into(distribution::table)
            .values((distribution::id.eq(1), values))
            .on_conflict(distribution::id)
            .do_update()
            .set(values)
            .execute(&mut self.conn)
            .map_err(db("set_distribution"))?;
        Ok(())
    }

    /// 配布に出す素材を、行ごとにまとめて引く（`PROFILE-M4`）。
    ///
    /// 採用テイクを持つ行だけ。 WAV 名は行の `file_stem` から作る
    /// （`TR-RCL-08` が ASCII を保証している）ので、テイクの相対パスは
    /// 配布物の名前に出ない。
    ///
    /// エイリアスの並びは決めておく。 同じ音源からは同じ `oto.ini` が出る。
    #[tracing::instrument(skip(self), err)]
    pub fn distribution_samples(&mut self) -> Result<Vec<DistributionSample>> {
        let takes: Vec<(String, String, String, i32, i32, i64)> = rows::table
            .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(rows::id)))
            .inner_join(takes::table.on(takes::id.eq(adopted_takes::take_id)))
            .order(rows::ordinal.asc())
            .select((
                rows::id,
                rows::file_stem,
                takes::rel_path,
                adopted_takes::take_id,
                rows::tone,
                takes::frames,
            ))
            .load(&mut self.conn)
            .map_err(db("distribution_samples"))?;

        let mut out = Vec::with_capacity(takes.len());
        for (row_id, file_stem, rel_path, take_id, tone, frames) in takes {
            let mut otos = self.otos_of(take_id)?;
            otos.sort_by(|a, b| a.0.cmp(&b.0));
            let frq = self.analysis_of(take_id)?.map(|a| a.frq);
            out.push(DistributionSample {
                row_id,
                file_stem,
                rel_path,
                take_id,
                otos,
                frq,
                tone,
                frames,
            });
        }
        Ok(out)
    }

    /// 採用テイクに紐づく oto を、確認の状態ごと全部（`TR-ALN-25`）。
    ///
    /// 採用していないテイクは出さない。 書き出しに出るのは採用したものだけで、
    /// 非採用の世代まで確認キューへ入れると、録り直すたびにキューが伸びる。
    ///
    /// 並びはエイリアス順で常に同じ（`TR-ALN-29` の決定性）。
    pub fn adopted_otos(&mut self) -> Result<Vec<OtoEntry>> {
        oto_values::table
            .inner_join(adopted_takes::table.on(adopted_takes::take_id.eq(oto_values::take_id)))
            .inner_join(takes::table.on(takes::id.eq(oto_values::take_id)))
            .order(oto_values::alias.asc())
            .select((
                oto_values::take_id,
                oto_values::alias,
                adopted_takes::row_id,
                takes::frames,
                oto_values::offset_ms,
                oto_values::consonant_ms,
                oto_values::cutoff_ms,
                oto_values::preutterance_ms,
                oto_values::overlap_ms,
                oto_values::confidence,
                oto_values::state,
                oto_values::pinned_offset,
                oto_values::pinned_consonant,
                oto_values::pinned_cutoff,
                oto_values::pinned_preutterance,
                oto_values::pinned_overlap,
                oto_values::conf_path,
                oto_values::conf_sharpness,
                oto_values::conf_prior,
                oto_values::conf_acoustic,
                oto_values::branch_mismatch,
            ))
            .load::<OtoEntryRow>(&mut self.conn)
            .map_err(db("adopted_otos"))
            .map(|v| v.into_iter().map(OtoEntry::from).collect())
    }

    /// 無声破裂音の分岐不一致の印を書く（`TR-ALN-16`, `DEC-ALN-018`）。
    ///
    /// 推定するたびに書く。 立てるときだけ書くと、推定し直して閉鎖が
    /// 見つかったあとも前の印が残る。
    pub fn set_branch_mismatch(&mut self, take_id: i32, alias: &str, mismatch: bool) -> Result<()> {
        diesel::update(
            oto_values::table
                .filter(oto_values::take_id.eq(take_id))
                .filter(oto_values::alias.eq(alias)),
        )
        .set(oto_values::branch_mismatch.eq(i32::from(mismatch)))
        .execute(&mut self.conn)
        .map_err(db("set_branch_mismatch"))?;
        Ok(())
    }

    /// エントリの状態を書く（`align-review.fsl` の `EntryState`）。
    pub fn set_oto_state(&mut self, take_id: i32, alias: &str, state: &str) -> Result<()> {
        diesel::update(
            oto_values::table
                .filter(oto_values::take_id.eq(take_id))
                .filter(oto_values::alias.eq(alias)),
        )
        // `confirmed` も一緒に動かす。 この列を読む経路がまだ残っているので、
        // 片方だけ進むと「確認済みだが確認待ち」という行ができる。
        .set((
            oto_values::state.eq(state),
            oto_values::confirmed.eq(i32::from(state == "auto_confirmed")),
        ))
        .execute(&mut self.conn)
        .map_err(db("set_oto_state"))?;
        Ok(())
    }

    /// 値ごとの固定を書く（`TR-ALN-30`）。並びは `Slot::ALL` と同じ。
    pub fn set_oto_pins(&mut self, take_id: i32, alias: &str, pins: [bool; 5]) -> Result<()> {
        diesel::update(
            oto_values::table
                .filter(oto_values::take_id.eq(take_id))
                .filter(oto_values::alias.eq(alias)),
        )
        // `hand_edited` は「1つでも固定がある」に畳む（`TR-EDT-46`）。
        .set((
            oto_values::pinned_offset.eq(i32::from(pins[0])),
            oto_values::pinned_consonant.eq(i32::from(pins[1])),
            oto_values::pinned_cutoff.eq(i32::from(pins[2])),
            oto_values::pinned_preutterance.eq(i32::from(pins[3])),
            oto_values::pinned_overlap.eq(i32::from(pins[4])),
            oto_values::hand_edited.eq(i32::from(pins.iter().any(|p| *p))),
        ))
        .execute(&mut self.conn)
        .map_err(db("set_oto_pins"))?;
        Ok(())
    }

    /// 5値だけを書き換える。確信度も固定も状態も動かさない。
    ///
    /// 受けるのは [`crate::oto::Oto`]。 この表には同じ形の型が2つあり
    /// （[`koeru_oto::Oto`] は台帳の中だけで使う写し）、確認キューと検証と
    /// `oto.ini` はどれもドメイン側の型で動く。境界をここで1度だけ揃える。
    pub fn set_oto_value(&mut self, take_id: i32, alias: &str, o: &crate::oto::Oto) -> Result<()> {
        diesel::update(
            oto_values::table
                .filter(oto_values::take_id.eq(take_id))
                .filter(oto_values::alias.eq(alias)),
        )
        .set((
            oto_values::offset_ms.eq(o.offset_ms),
            oto_values::consonant_ms.eq(o.consonant_ms),
            oto_values::cutoff_ms.eq(o.cutoff_ms),
            oto_values::preutterance_ms.eq(o.preutterance_ms),
            oto_values::overlap_ms.eq(o.overlap_ms),
        ))
        .execute(&mut self.conn)
        .map_err(db("set_oto_value"))?;
        Ok(())
    }

    /// 5値・状態・固定を一度に書く（`TR-ALN-30`）。
    ///
    /// **1つのトランザクションにまとめる。** 別々に流すと、途中で失敗したときに
    /// 「人が直した値なのに固定が付いていない」行が残る。その行は次の再推定で
    /// 上書きされ、`INV-ALN-001`（人が直した値を自動が上書きしない）が破れる。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。そのときは3つとも書かれていない。
    pub fn put_review_entry(
        &mut self,
        take_id: i32,
        alias: &str,
        o: &crate::oto::Oto,
        state: &str,
        pins: [bool; 5],
    ) -> Result<()> {
        self.conn
            .transaction(|conn| {
                let target = oto_values::table
                    .filter(oto_values::take_id.eq(take_id))
                    .filter(oto_values::alias.eq(alias));
                diesel::update(target)
                    .set((
                        oto_values::offset_ms.eq(o.offset_ms),
                        oto_values::consonant_ms.eq(o.consonant_ms),
                        oto_values::cutoff_ms.eq(o.cutoff_ms),
                        oto_values::preutterance_ms.eq(o.preutterance_ms),
                        oto_values::overlap_ms.eq(o.overlap_ms),
                        oto_values::state.eq(state),
                        // `confirmed` も一緒に動かす。 この列を読む経路がまだ
                        // 残っているので、片方だけ進むと食い違う。
                        oto_values::confirmed.eq(i32::from(state == "auto_confirmed")),
                        oto_values::pinned_offset.eq(i32::from(pins[0])),
                        oto_values::pinned_consonant.eq(i32::from(pins[1])),
                        oto_values::pinned_cutoff.eq(i32::from(pins[2])),
                        oto_values::pinned_preutterance.eq(i32::from(pins[3])),
                        oto_values::pinned_overlap.eq(i32::from(pins[4])),
                        // 1つでも固定があれば手が入ったとみなす（`TR-EDT-46`）。
                        oto_values::hand_edited.eq(i32::from(pins.iter().any(|p| *p))),
                    ))
                    .execute(conn)
            })
            .map_err(db("put_review_entry"))?;
        Ok(())
    }

    /// 複数のエントリを一度に書く。 途中で失敗したら1件も書かれていない。
    ///
    /// まとめて確認（`REQ-ALN-010`）は全件の状態を動かす。1件ずつ流すと、
    /// 途中で落ちたときに「半分だけ確認済み」の台帳が残る。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。
    pub fn put_review_entries(&mut self, rows: &[ReviewEntryRow]) -> Result<()> {
        self.conn
            .transaction(|conn| {
                for r in rows {
                    let target = oto_values::table
                        .filter(oto_values::take_id.eq(r.take_id))
                        .filter(oto_values::alias.eq(&r.alias));
                    diesel::update(target)
                        .set((
                            oto_values::offset_ms.eq(r.oto.offset_ms),
                            oto_values::consonant_ms.eq(r.oto.consonant_ms),
                            oto_values::cutoff_ms.eq(r.oto.cutoff_ms),
                            oto_values::preutterance_ms.eq(r.oto.preutterance_ms),
                            oto_values::overlap_ms.eq(r.oto.overlap_ms),
                            oto_values::state.eq(&r.state),
                            oto_values::confirmed.eq(i32::from(r.state == "auto_confirmed")),
                            oto_values::pinned_offset.eq(i32::from(r.pinned[0])),
                            oto_values::pinned_consonant.eq(i32::from(r.pinned[1])),
                            oto_values::pinned_cutoff.eq(i32::from(r.pinned[2])),
                            oto_values::pinned_preutterance.eq(i32::from(r.pinned[3])),
                            oto_values::pinned_overlap.eq(i32::from(r.pinned[4])),
                            oto_values::hand_edited.eq(i32::from(r.pinned.iter().any(|p| *p))),
                        ))
                        .execute(conn)?;
                }
                Ok::<_, diesel::result::Error>(())
            })
            .map_err(db("put_review_entries"))?;
        Ok(())
    }

    /// 採用しているのに、名乗った綴りの oto が揃っていない行（`TR-ALN-20`, `INV-ALN-003`）。
    ///
    /// 発声が見つからなかったテイクも採用される（取りこぼしが無ければ）。
    /// そのテイクは `oto_values` に1行も書かないので、**確認キューにも現れず、
    /// 書き出しからも黙って落ちる。** 関門がこれを見て止める。
    ///
    /// **数える分母は `row_aliases`。仮名の数ではない。** 行が生む綴りの数は
    /// 仮名の数と一致しない——CVVC の行は渡りと語尾を足すぶん多く、連続音の
    /// 第2段は名乗らない語頭 CV があるぶん少ない（`TR-ALN-22`）。仮名で
    /// 数えると、**名乗っていない綴りを「取れていない切り出し」として
    /// 数え、録り終えた連続音が書き出しの関門で止まる。**
    ///
    /// 返るのは行 ID。エイリアスではなく行で返すのは、画面が行で開くため。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。
    pub fn adopted_rows_without_oto(&mut self) -> Result<Vec<String>> {
        let adopted: Vec<(String, i32)> = adopted_takes::table
            .select((adopted_takes::row_id, adopted_takes::take_id))
            .load(&mut self.conn)
            .map_err(db("adopted_rows_without_oto.adopted"))?;
        // 数えるのは持ち主になっている綴りだけ（`DEC-RCL-016`）。 持たない綴りには
        // 5値を置かないので、行が生む綴りで数えると、重なりを持つ行が全部
        // 「足りない」ことになる。
        let mut owned: BTreeMap<String, i64> = BTreeMap::new();
        for owner in self.alias_owners()?.into_values() {
            *owned.entry(owner).or_default() += 1;
        }
        let mut out = Vec::new();
        for (row_id, take_id) in adopted {
            let want = owned.get(&row_id).copied().unwrap_or(0);
            let otos: i64 = oto_values::table
                .filter(oto_values::take_id.eq(take_id))
                .count()
                .get_result(&mut self.conn)
                .map_err(db("adopted_rows_without_oto.otos"))?;
            if otos < want {
                out.push(row_id);
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    /// 同じ収録音高の中で、別の採用テイクにまたがって重複しているエイリアス。
    ///
    /// エントリの識別子は（収録音高, 綴り）（`docs/reports/ux/ooui-model.md`）。
    /// **重なると確認キューが片方を落とす**——あとに読んだ行が前の行を
    /// 置き換え、落ちたほうは確認もされず `oto.ini` にも出ない。
    ///
    /// **音高を跨ぐ重なりは重なりではない**（`TR-ALN-22`）。 多音階は音高ごとに
    /// フォルダと `oto.ini` を分け、配布物では区画の接頭辞・接尾辞が綴りを
    /// 分ける（`koeru_package::tree` の `decorate`）。音高を見ずに数えると、
    /// **`あ` を2音階で録っただけで全部の綴りが衝突になり、書き出しが止まる。**
    ///
    /// `TR-ALN-20` (6) の同一 WAV 内の重複とは別。 あちらは1つの WAV の中の話で、
    /// [`validate`] 側が WAV ごとに見る。ここが見るのは WAV をまたぐ重なり。
    ///
    /// **発火しないのが正常。** 綴りの持ち主は録った順で1つに決まり
    /// （[`alias_owners`](Self::alias_owners)）、5値は持ち主にしか置かない。
    /// ここはその取り決めが破れたときに気づくための後備で、破れた状態で
    /// 配らないことだけを保証する。
    ///
    /// # Errors
    ///
    /// SQLite の操作が失敗した。
    ///
    /// [`validate`]: https://docs.rs/koeru-align
    pub fn adopted_conflicting_aliases(&mut self) -> Result<Vec<String>> {
        let rows: Vec<(i32, String, i32)> = oto_values::table
            .inner_join(adopted_takes::table.on(adopted_takes::take_id.eq(oto_values::take_id)))
            .inner_join(rows::table.on(rows::id.eq(adopted_takes::row_id)))
            .select((rows::tone, oto_values::alias, oto_values::take_id))
            .load(&mut self.conn)
            .map_err(db("adopted_conflicting_aliases"))?;
        let mut seen: std::collections::BTreeMap<(i32, String), i32> =
            std::collections::BTreeMap::new();
        let mut out = Vec::new();
        for (tone, alias, take_id) in rows {
            let key = (tone, alias.clone());
            match seen.get(&key) {
                Some(first) if *first != take_id => out.push(alias),
                Some(_) => {}
                None => {
                    seen.insert(key, take_id);
                }
            }
        }
        out.sort_unstable();
        out.dedup();
        Ok(out)
    }

    /// 確認の進み方（`TR-ALN-25`）。
    pub fn review_state(&mut self) -> Result<ReviewStateRow> {
        review_state::table
            .filter(review_state::id.eq(1))
            .select((
                review_state::mode,
                review_state::over_budget,
                review_state::exported,
            ))
            .first::<(String, i32, i32)>(&mut self.conn)
            .map_err(db("review_state"))
            .map(|(mode, over_budget, exported)| ReviewStateRow {
                mode,
                over_budget: over_budget != 0,
                exported: exported != 0,
            })
    }

    /// 綴りの表の写し（`TR-SYN-36`, `DEC-SYN-013`）。 作ったときに書いたもの。
    ///
    /// 無ければ `None`。 写しを持つ前に作ったプロジェクトで、台帳の綴りは
    /// 同梱の既定の表で書かれている。
    pub fn presamp_snapshot(&mut self) -> Result<Option<String>> {
        presamp_snapshot::table
            .filter(presamp_snapshot::id.eq(1))
            .select(presamp_snapshot::text)
            .first::<String>(&mut self.conn)
            .optional()
            .map_err(db("presamp_snapshot"))
    }

    /// 綴りの表の写しを書く（`DEC-SYN-013`）。
    ///
    /// 作るときと、写しを持たない古いプロジェクトを初めて開くときだけ呼ぶ。
    /// **録り始めたあとに書き換えない。** 台帳の綴りはこの表で書かれている。
    pub fn put_presamp_snapshot(&mut self, text: &str) -> Result<()> {
        diesel::insert_into(presamp_snapshot::table)
            .values((presamp_snapshot::id.eq(1), presamp_snapshot::text.eq(text)))
            .on_conflict(presamp_snapshot::id)
            .do_update()
            .set(presamp_snapshot::text.eq(text))
            .execute(&mut self.conn)
            .map_err(db("put_presamp_snapshot"))?;
        Ok(())
    }

    /// 確認の進み方を書く。
    pub fn put_review_state(&mut self, s: &ReviewStateRow) -> Result<()> {
        diesel::update(review_state::table.filter(review_state::id.eq(1)))
            .set((
                review_state::mode.eq(&s.mode),
                review_state::over_budget.eq(i32::from(s.over_budget)),
                review_state::exported.eq(i32::from(s.exported)),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_review_state"))?;
        Ok(())
    }

    /// 推定を作った入力の指紋を保存する（`TR-ALN-29`）。
    pub fn put_fingerprint(&mut self, take_id: i32, f: &FingerprintRow) -> Result<()> {
        diesel::insert_into(take_fingerprints::table)
            .values((
                take_fingerprints::take_id.eq(take_id),
                take_fingerprints::audio.eq(&f.audio),
                take_fingerprints::reading.eq(&f.reading),
                take_fingerprints::preset.eq(&f.preset),
                take_fingerprints::aligner.eq(&f.aligner),
            ))
            .on_conflict(take_fingerprints::take_id)
            .do_update()
            .set((
                take_fingerprints::audio.eq(&f.audio),
                take_fingerprints::reading.eq(&f.reading),
                take_fingerprints::preset.eq(&f.preset),
                take_fingerprints::aligner.eq(&f.aligner),
            ))
            .execute(&mut self.conn)
            .map_err(db("put_fingerprint"))?;
        Ok(())
    }

    /// 保存してある指紋（`TR-ALN-29`）。まだ無ければ `None`。
    pub fn fingerprint_of(&mut self, take_id: i32) -> Result<Option<FingerprintRow>> {
        take_fingerprints::table
            .filter(take_fingerprints::take_id.eq(take_id))
            .select((
                take_fingerprints::audio,
                take_fingerprints::reading,
                take_fingerprints::preset,
                take_fingerprints::aligner,
            ))
            .first::<(String, String, String, String)>(&mut self.conn)
            .optional()
            .map_err(db("fingerprint_of"))
            .map(|o| {
                o.map(|(audio, reading, preset, aligner)| FingerprintRow {
                    audio,
                    reading,
                    preset,
                    aligner,
                })
            })
    }
}

/// 配布に出す値（`PROFILE-M4`）。
///
/// 表示名は持たない。 manifest.toml が正本で、ここに写すと2箇所になる。
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Distribution {
    /// 音源ルートフォルダ名。ASCII 固定（`DEC-PKG-008`）。
    pub distribution_name: String,
    /// 書き出しプロファイル（`TR-PKG-12`）。名前は `koeru-package` が決める。
    pub profile: String,
    pub author: Option<String>,
    pub voice: Option<String>,
    pub sample: Option<String>,
    pub web: Option<String>,
    pub version: Option<String>,
    /// 音源アイコンの元画像（`DEC-PKG-012`）。変換後ではない。
    pub icon: Option<Vec<u8>>,
    /// 立ち絵の PNG（`TR-PKG-07`）。
    pub portrait: Option<Vec<u8>>,
    pub portrait_opacity: f64,
    pub portrait_height: i32,
    pub tone_range_note: Option<String>,
    /// 利用規約の本文（`DEC-PKG-011`）。
    pub terms: Option<String>,
    pub credit_example: Option<String>,
    pub contact: Option<String>,
    pub disclaimer: Option<String>,
    pub character_note: Option<String>,
}

/// `distribution` から読んだ1行。欄が多いので、組み立ては
/// [`Distribution::from`] が持つ。
type DistributionRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<Vec<u8>>,
    Option<Vec<u8>>,
    f64,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
    Option<String>,
);

impl From<DistributionRow> for Distribution {
    fn from(r: DistributionRow) -> Self {
        let (
            distribution_name,
            profile,
            author,
            voice,
            sample,
            web,
            version,
            icon,
            portrait,
            portrait_opacity,
            portrait_height,
            tone_range_note,
            terms,
            credit_example,
            contact,
            disclaimer,
            character_note,
        ) = r;
        Self {
            distribution_name,
            profile,
            author,
            voice,
            sample,
            web,
            version,
            icon,
            portrait,
            portrait_opacity,
            portrait_height,
            tone_range_note,
            terms,
            credit_example,
            contact,
            disclaimer,
            character_note,
        }
    }
}

/// 配布に出す素材1本（`PROFILE-M4`）。
#[derive(Debug, Clone, PartialEq)]
pub struct DistributionSample {
    /// 行 ID。検証結果から行へ戻るのに要る（`TR-PKG-51`）。
    pub row_id: String,
    /// 配布 WAV の名前のもと。拡張子を含まない（`TR-RCL-08`）。
    pub file_stem: String,
    /// プロジェクト直下からのマスターの相対パス。
    pub rel_path: String,
    /// 採用しているテイク。
    pub take_id: i32,
    /// この WAV が持つエイリアスと5値。
    pub otos: Vec<(String, koeru_oto::Oto)>,
    /// 周波数表（`TR-PKG-05`）。録音時に作ったもの。
    pub frq: Option<Frq>,
    /// 収録音高（MIDI、`TR-REC-25`）。多音階では区画を分ける軸になる。
    pub tone: i32,
    /// マスターの標本数。 5値を作り直すのに素材の長さが要る（`TR-ALN-34`）。
    pub frames: i64,
}

/// `adopted_otos` が読む行。列の並びは `select` と揃える。
type OtoEntryRow = (
    i32,
    String,
    String,
    i64,
    f64,
    f64,
    f64,
    f64,
    f64,
    f64,
    String,
    i32,
    i32,
    i32,
    i32,
    i32,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    Option<f64>,
    i32,
);

/// 確信度の4成分（`TR-ALN-24`）。
///
/// `koeru-align` の型を使わない。 あちらがここを引いているので、依存が逆になる。
/// 合成スコアは [`OtoEntry::confidence`] が別に持つ——こちらから計算し直さない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ConfidenceParts {
    /// (1) 経路確信度。退避経路は出せないので `None`（`DEC-ALN-006`）。
    pub path: Option<f64>,
    /// (2) 境界鋭さ。
    pub sharpness: f64,
    /// (3) 事前分布逸脱の裏返し。
    pub prior: f64,
    /// (4) 音響異常度の裏返し。
    pub acoustic: f64,
}

/// 確認の状態まで含んだ oto の1件（`TR-ALN-26`）。
///
/// 状態と固定を文字列と真偽で持つ。 `koeru-align` の型を使わないのは、
/// 依存が逆向きになるため——あちらがここを引いている。
#[derive(Debug, Clone, PartialEq)]
pub struct OtoEntry {
    pub take_id: i32,
    /// エイリアス。行の収録音高と組で1つのエントリを指す（`TR-ALN-22`）。
    ///
    /// これだけでは一意にならない。 多音階は音高ごとに `oto.ini` を分けるので、
    /// 同じ綴りが音高の数だけある（`DEC-ALN-017`）。音高は [`row_id`](Self::row_id)
    /// の行が持つ。配布物で綴りが一意に戻るのは、区画の接頭辞・接尾辞が
    /// 音高を織り込んだあと（`TR-PKG-19`）。
    pub alias: String,
    pub row_id: String,
    /// その WAV のフレーム数。長さを引くための問い合わせを1件ずつ出さないために持つ。
    pub frames: i64,
    pub oto: crate::oto::Oto,
    pub confidence: f64,
    /// `align-review.fsl` の `EntryState` を写した文字列。
    pub state: String,
    /// 値ごとの固定（`TR-ALN-30`）。並びは `Slot::ALL` と同じ。
    pub pinned: [bool; 5],
    /// 確信度の成分（`TR-ALN-24`）。この列より前に録ったものは持たない。
    pub parts: Option<ConfidenceParts>,
    /// 無声破裂音の分岐不一致（`TR-ALN-16`, `DEC-ALN-018`）。
    pub branch_mismatch: bool,
}

impl From<OtoEntryRow> for OtoEntry {
    fn from(r: OtoEntryRow) -> Self {
        let (
            take_id,
            alias,
            row_id,
            frames,
            offset_ms,
            consonant_ms,
            cutoff_ms,
            preutterance_ms,
            overlap_ms,
            confidence,
            state,
            p0,
            p1,
            p2,
            p3,
            p4,
            c_path,
            c_sharp,
            c_prior,
            c_acoustic,
            mismatch,
        ) = r;
        // 3つ揃っていて初めて成分として読む。 欠けたものを 0 で埋めない。
        let parts = match (c_sharp, c_prior, c_acoustic) {
            (Some(sharpness), Some(prior), Some(acoustic)) => Some(ConfidenceParts {
                path: c_path,
                sharpness,
                prior,
                acoustic,
            }),
            _ => None,
        };
        Self {
            take_id,
            alias,
            row_id,
            frames,
            oto: crate::oto::Oto {
                offset_ms,
                consonant_ms,
                cutoff_ms,
                preutterance_ms,
                overlap_ms,
            },
            confidence,
            state,
            pinned: [p0 != 0, p1 != 0, p2 != 0, p3 != 0, p4 != 0],
            parts,
            branch_mismatch: mismatch != 0,
        }
    }
}

/// 確認の進み方（`TR-ALN-25`）。音源ごとに1つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewStateRow {
    /// `align-review.fsl` の `ReviewMode` を写した文字列。
    pub mode: String,
    /// 個別確認をやめたか。やめられるのは上限を超えたときだけ（`INV-ALN-004`）。
    pub over_budget: bool,
    pub exported: bool,
}

/// 台帳へ書き戻す確認エントリ1件。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewEntryRow {
    pub take_id: i32,
    pub alias: String,
    pub oto: crate::oto::Oto,
    /// `align-review.fsl` の `EntryState` を写した文字列。
    pub state: String,
    /// 値ごとの固定（`TR-ALN-30`）。並びは `Slot::ALL` と同じ。
    pub pinned: [bool; 5],
}

/// 推定を作った入力の指紋（`TR-ALN-29`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FingerprintRow {
    pub audio: String,
    pub reading: String,
    pub preset: String,
    pub aligner: String,
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
/// 知らない名前でも履歴を落とさない。 その回に何を配ったかが辿れなくなるほうが痛い。
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
            master_rate_hz: 44_100,
            resampler: "test".into(),
            upstream_conversion: "unknown".into(),
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

    /// 確定済みのテイクだけが台帳に載る（`DEC-REC-004`）。
    #[test]
    fn テイクを確定させると行が録音済みになる() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        assert_eq!(l.row_state(row).expect("引ける"), RowState::Unrecorded);
        assert!(
            l.unrecorded_row_text(row).expect("引ける").is_some(),
            "未収録の間は提示できる"
        );
        l.commit_take(&take(row, sid, 1)).expect("確定できる");
        assert_eq!(l.row_state(row).expect("引ける"), RowState::Recorded);
        assert_eq!(
            l.unrecorded_row_text(row).expect("引ける"),
            None,
            "古い提示順に残っても録音済みは返さない"
        );
    }

    /// 配布に出す値を往復できる（`PROFILE-M4`）。
    ///
    /// スキーマとマイグレーションの食い違いは、実際に SQL を投げないと出ない。
    #[test]
    fn 配布に出す値を往復できる() {
        let (mut l, _sid, _) = ready();
        assert_eq!(l.distribution().expect("引ける"), None);

        let d = Distribution {
            distribution_name: "koeru".into(),
            profile: "both".into(),
            author: Some("しお".into()),
            version: Some("1.0".into()),
            icon: Some(vec![1, 2, 3]),
            portrait_opacity: 0.8,
            portrait_height: 800,
            terms: Some("自由に使えます".into()),
            ..Distribution::default()
        };
        l.set_distribution(&d).expect("書ける");
        assert_eq!(l.distribution().expect("引ける"), Some(d.clone()));

        // 差し替えても行は増えない。
        let d2 = Distribution {
            distribution_name: "koeru2".into(),
            ..d
        };
        l.set_distribution(&d2).expect("書ける");
        assert_eq!(l.distribution().expect("引ける"), Some(d2));
    }

    /// 未記入と空文字を分ける（`DEC-PKG-011`）。
    #[test]
    fn 未記入は_none_のまま戻る() {
        let (mut l, _sid, _) = ready();
        l.set_distribution(&Distribution {
            distribution_name: "koeru".into(),
            profile: "both".into(),
            terms: None,
            ..Distribution::default()
        })
        .expect("書ける");
        assert_eq!(l.distribution().expect("引ける").expect("ある").terms, None);
    }

    /// 採用テイクを持つ行だけが配布に出る（`PROFILE-M4`）。
    #[test]
    fn 配布に出す素材は採用テイクの分だけ() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        assert!(
            l.distribution_samples().expect("引ける").is_empty(),
            "録っていない行は出ない"
        );

        // 確定すると、その回が採用になる（`TR-REC-21`）。
        let take_id = l.commit_take(&take(row, sid, 1)).expect("確定できる");
        l.put_oto(
            take_id,
            "あ",
            &koeru_oto::Oto {
                offset_ms: 10.0,
                consonant_ms: 20.0,
                cutoff_ms: -50.0,
                preutterance_ms: 15.0,
                overlap_ms: 5.0,
            },
            1.0,
            None,
            false,
        )
        .expect("書ける");

        let samples = l.distribution_samples().expect("引ける");
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].file_stem, list[0].file_stem);
        assert_eq!(samples[0].otos.len(), 1);
        assert_eq!(samples[0].otos[0].0, "あ");
    }

    /// 確認の状態と、値ごとの固定が往復する（`TR-ALN-30`）。
    ///
    /// 固定は5値それぞれに付く。 1つの真偽に畳むと、
    /// 「オフセットだけ直した」が表せない（`INV-ALN-001`）。
    #[test]
    fn 確認の状態と値ごとの固定が往復する() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let id = l.commit_take(&take(row, sid, 1)).expect("確定できる");
        l.adopt_take(row, id).expect("採用できる");
        let oto = koeru_oto::Oto {
            offset_ms: 10.0,
            consonant_ms: 20.0,
            cutoff_ms: -30.0,
            preutterance_ms: 15.0,
            overlap_ms: 5.0,
        };
        l.put_oto(id, "か", &oto, 0.4, None, false).expect("書ける");

        // 既定は確認待ち。 誰も見ていない推定値を自動確定にしない。
        let got = l.adopted_otos().expect("引ける");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].state, "in_queue");
        assert_eq!(got[0].pinned, [false; 5]);
        assert_eq!(got[0].row_id, *row);

        l.set_oto_state(id, "か", "auto_confirmed").expect("書ける");
        l.set_oto_pins(id, "か", [true, false, false, false, false])
            .expect("書ける");
        let got = l.adopted_otos().expect("引ける");
        assert_eq!(got[0].state, "auto_confirmed");
        assert_eq!(got[0].pinned, [true, false, false, false, false]);
    }

    /// 分岐不一致の印が往復し、推定し直せば下ろせる（`DEC-ALN-018`）。
    #[test]
    fn 分岐不一致の印が往復する() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let id = l.commit_take(&take(row, sid, 1)).expect("確定できる");
        l.adopt_take(row, id).expect("採用できる");
        let oto = koeru_oto::Oto {
            offset_ms: 10.0,
            consonant_ms: 20.0,
            cutoff_ms: -30.0,
            preutterance_ms: 15.0,
            overlap_ms: 5.0,
        };
        l.put_oto(id, "か", &oto, 0.9, None, false).expect("書ける");
        assert!(
            !l.adopted_otos().expect("引ける")[0].branch_mismatch,
            "既定は無印"
        );

        l.set_branch_mismatch(id, "か", true).expect("書ける");
        assert!(l.adopted_otos().expect("引ける")[0].branch_mismatch);
        l.set_branch_mismatch(id, "か", false).expect("書ける");
        assert!(!l.adopted_otos().expect("引ける")[0].branch_mismatch);
    }

    /// 採用していないテイクのエントリは確認キューに出ない。
    ///
    /// 出すと、録り直すたびにキューが伸びる。
    #[test]
    fn 採用していないテイクのotoは出ない() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let old = l.commit_take(&take(row, sid, 1)).expect("確定できる");
        let new = l.commit_take(&take(row, sid, 2)).expect("確定できる");
        let oto = koeru_oto::Oto {
            offset_ms: 1.0,
            consonant_ms: 2.0,
            cutoff_ms: -3.0,
            preutterance_ms: 1.5,
            overlap_ms: 0.5,
        };
        l.put_oto(old, "か", &oto, 0.4, None, false)
            .expect("書ける");
        l.put_oto(new, "か", &oto, 0.9, None, false)
            .expect("書ける");
        l.adopt_take(row, new).expect("採用できる");

        let got = l.adopted_otos().expect("引ける");
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].take_id, new, "採用しているほう");
    }

    /// 確認の進め方が往復する（`TR-ALN-25`）。
    #[test]
    fn 確認の進め方が往復する() {
        let (mut l, _sid, _) = ready();
        // 既定は個別確認。 そこから離れるには上限超過が要る（`INV-ALN-004`）。
        let s = l.review_state().expect("引ける");
        assert_eq!(s.mode, "individual");
        assert!(!s.over_budget);
        assert!(!s.exported);

        l.put_review_state(&ReviewStateRow {
            mode: "batch".into(),
            over_budget: true,
            exported: false,
        })
        .expect("書ける");
        let s = l.review_state().expect("引ける");
        assert_eq!(s.mode, "batch");
        assert!(s.over_budget);
    }

    /// 推定を作った入力の指紋が往復する（`TR-ALN-29`）。
    #[test]
    fn 指紋が往復する() {
        let (mut l, sid, list) = ready();
        let id = l
            .commit_take(&take(&list[0].id, sid, 1))
            .expect("確定できる");
        assert!(l.fingerprint_of(id).expect("引ける").is_none());

        let f = FingerprintRow {
            audio: "abc".into(),
            reading: "か".into(),
            preset: "single@1".into(),
            aligner: "mfa-japanese@3.0.0".into(),
        };
        l.put_fingerprint(id, &f).expect("書ける");
        assert_eq!(l.fingerprint_of(id).expect("引ける"), Some(f));
    }

    /// 台帳に無い行のテイクは受け付けない。
    #[test]
    fn 知らない行のテイクは弾く() {
        let (mut l, sid, _) = ready();
        assert!(matches!(
            l.commit_take(&take("存在しない行", sid, 1)),
            Err(LedgerError::UnknownRow)
        ));
    }

    /// 録り直しは上書きせず、世代として積む（`TR-REC-21`）。
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

    /// 一度録った行も一覧に出る（`TR-REC-21`, `TR-RCL-25`）。
    ///
    /// `next_row` は未収録しか返さないので、これが無いと
    /// 録り直しの入口が存在しない。 実際に無くて困った。
    #[test]
    fn 録り直しの一覧に全部の行が出る() {
        let (mut l, sid, list) = ready();
        let row = &list[0].id;
        let first = l.commit_take(&take(row, sid, 1)).expect("1回目");
        let second = l.commit_take(&take(row, sid, 2)).expect("2回目");

        let rows = l.rows_with_takes().expect("引ける");
        assert_eq!(rows.len(), list.len(), "未収録の行も含めて全部出る");

        let r = rows.iter().find(|r| &r.row_id == row).expect("ある");
        assert_eq!(r.state, RowState::Recorded);
        assert_eq!(r.takes.len(), 2, "過去のテイクも出る");
        assert_eq!(r.takes[0].generation, 1);
        assert_eq!(r.takes[1].generation, 2);
        assert_eq!(r.adopted, Some(second), "採用は新しい方（TR-REC-21）");

        // 採用を戻せる。
        l.adopt_take(row, first).expect("戻せる");
        let rows = l.rows_with_takes().expect("引ける");
        let r = rows.iter().find(|r| &r.row_id == row).expect("ある");
        assert_eq!(r.adopted, Some(first));

        // 未収録の行は、テイクも採用も無い。
        let other = rows.iter().find(|r| &r.row_id != row).expect("ある");
        assert_eq!(other.state, RowState::Unrecorded);
        assert!(other.takes.is_empty());
        assert_eq!(other.adopted, None);
    }

    /// 一覧は並び順で返す。 画面が並べ直さなくてよい。
    #[test]
    fn 録り直しの一覧は並び順() {
        let (mut l, _, list) = ready();
        let got = l.rows_with_takes().expect("引ける");
        let want: Vec<&String> = list.iter().map(|r| &r.id).collect();
        let ids: Vec<&String> = got.iter().map(|r| &r.row_id).collect();
        assert_eq!(ids, want);
    }

    /// 採用テイクを切り替えてもカバレッジは変わらない（`TR-RCL-25`）。
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

    /// 収録済み単位は採用テイクを持つ行から導出する（`TR-RCL-18`）。
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

    /// 無効にしたテイクはカバレッジから外れる（`TR-REC-07`）。
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

    /// 孤児を見つけて提示する。消さない（`DEC-REC-004`）。
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

    /// 次の行は未録音のうち並び順が最も早いもの（`TR-REC-18`）。
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
        l.put_oto(id, "か", &o, 0.9, None, false)
            .expect("保存できる");
        l.put_oto(id, "か", &o, 0.5, None, true)
            .expect("上書きできる");

        // 1テイクに複数のエントリを持てる（`DEC-ALN-013`）。
        // 単独音でも1ファイルに複数モーラが入る（`TR-RCL-03`）。
        let mut o2 = o;
        o2.offset_ms = 500.0;
        l.put_oto(id, "き", &o2, 0.8, None, false)
            .expect("2つ目も保存できる");

        let all = l.otos_of(id).expect("引ける");
        assert_eq!(all.len(), 2, "1テイクに2つ入っている");
        // 並びはエイリアス順で常に同じ（`TR-ALN-29`）。
        assert_eq!(all[0].0, "か");
        assert_eq!(all[1].0, "き");
        assert!((all[1].1.offset_ms - 500.0).abs() < 1e-9);

        // 名前で引ける。
        assert!(
            (l.oto_of(id, "き").expect("引ける").expect("ある").offset_ms - 500.0).abs() < 1e-9
        );
        assert!(l.oto_of(id, "く").expect("引ける").is_none());
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

    /// 行が生む単位は行が持ち、テイクに依らない。
    #[test]
    fn 行の単位はテイクと独立している() {
        let (mut l, _sid, list) = ready();
        let before = l.units_of(&list[0].id).expect("引ける");
        assert_eq!(before.len(), list[0].units.len());
    }
    /// 解析値を録音時に確定させ、書き出しと再開で WAV を読み直さない
    /// （`TR-PKG-05`, `TR-PKG-42`）。
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

    /// 解析がまだ無いことは失敗ではない。
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

    /// 過去のバージョンの ZIP を上書きしない（`TR-PKG-44`）。
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

    /// リリースレコードは不変（`TR-PKG-44`）。規律ではなくトリガが止める。
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

    /// 書き出し履歴は完成判定と直交する（`TR-PKG-33`, `TR-PKG-36`）。
    #[test]
    fn 書き出し履歴の有無だけが手渡し状態を決める() {
        let (mut l, _sid, _list) = ready();
        assert!(!l.has_been_exported().expect("引ける"));
        l.record_release(&new_release("v1"), "zip")
            .expect("記録できる");
        assert!(l.has_been_exported().expect("引ける"));
    }

    /// 知らない方式名でも履歴を落とさない。
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

#[cfg(test)]
mod m5_tests {
    use super::*;
    use crate::alias::Method as AliasMethod;
    use crate::inventory::UnitSet;
    use crate::reclist::{generate_cvvc, generate_sequential, generate_single};

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }

    fn session() -> SessionSnapshot {
        SessionSnapshot {
            started_at: "2026-09-20T12:00:00Z".into(),
            device_id: "test-device".into(),
            sample_rate_hz: 48_000,
            channels: 1,
            effects_state: "clean".into(),
            route: "coreaudio".into(),
            source_channel: 0,
            master_rate_hz: 44_100,
            resampler: "test".into(),
            upstream_conversion: "unknown".into(),
        }
    }

    fn adopt(l: &mut Ledger, sid: i32, row_id: &str) -> i32 {
        let t = l
            .commit_take(&FinalizedTake {
                row_id: row_id.into(),
                session_id: sid,
                rel_path: format!("masters/{row_id}.wav"),
                frames: 44_100,
                recorded_at: "2026-09-20T12:00:01Z".into(),
            })
            .expect("確定できる");
        l.adopt_take(row_id, t).expect("採用できる");
        t
    }

    /// その行が持つ綴りの5値を置く。値は突合に使わないので同じものでよい。
    fn put_otos_of_row(l: &mut Ledger, take_id: i32, row_id: &str) {
        let o = koeru_oto::Oto {
            offset_ms: 0.0,
            consonant_ms: 10.0,
            cutoff_ms: -100.0,
            preutterance_ms: 5.0,
            overlap_ms: 2.0,
        };
        for a in l.aliases_of_row(row_id).expect("引ける") {
            l.put_oto(take_id, &a, &o, 0.9, None, false)
                .expect("書ける");
        }
    }

    /// 音高を跨ぐ同じ綴りは衝突ではない（`TR-ALN-22`）。
    ///
    /// **音高を見ずに数えていた。** 多音階は音高ごとにフォルダと `oto.ini` を
    /// 分けるので `あ` が音階の数だけ並ぶのが正常なのに、全部が衝突として
    /// 数えられ、**2音階で録っただけで書き出しの関門が開かなくなっていた。**
    #[test]
    fn 音高を跨ぐ同じ綴りは衝突にしない() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Single, &[60, 62])
            .expect("書き込める");
        let sid = l.start_session(&session()).expect("始められる");

        for suffix in ["@C4", "@D4"] {
            let row_id = format!("{}{suffix}", list[0].id);
            let t = adopt(&mut l, sid, &row_id);
            put_otos_of_row(&mut l, t, &row_id);
        }
        assert!(
            l.adopted_conflicting_aliases().expect("引ける").is_empty(),
            "音高が違えば同じ綴りでも衝突しない"
        );
    }

    /// 同じ音高で同じ綴りを生む行は、先に録ったほうが持つ（`DEC-RCL-016`）。
    ///
    /// **入れる順で先に名乗った行に持たせていた**（`DEC-ALN-017`）。 連続音の
    /// 第2段の行頭は語頭 CV を重ねて生む（`Core` で `- い` `- え` `- ん`）ので、
    /// 第2段の行を先に録ると、その `- い` は持ち主のいないまま捨てられていた。
    #[test]
    fn 重なった綴りは先に録った行が持つ() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_sequential(UnitSet::Core, 8).expect("生成できる");
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Sequential, &[60])
            .expect("書き込める");
        let sid = l.start_session(&session()).expect("始められる");

        // 生成器は重複を出したままでよい。 誰が持つかは録った順で決まる。
        let producers: Vec<String> = row_aliases::table
            .filter(row_aliases::alias.eq("- い"))
            .select(row_aliases::row_id)
            .load(&mut l.conn)
            .expect("引ける");
        assert!(
            producers.len() > 1,
            "第2段が語頭 CV を重ねて生む前提が崩れている"
        );
        let (early, late) = (&producers[producers.len() - 1], &producers[0]);

        // 録っていなければ誰も持たない。
        assert!(l.owned_aliases_of_row(early).expect("引ける").is_empty());

        // 並びの後ろの行を先に録る。 持つのはそちら。
        let first = adopt(&mut l, sid, early);
        adopt(&mut l, sid, late);
        assert!(
            l.owned_aliases_of_row(early)
                .expect("引ける")
                .contains("- い")
        );
        assert!(
            !l.owned_aliases_of_row(late)
                .expect("引ける")
                .contains("- い"),
            "あとから録った行へ移らない"
        );

        // 先に録った行のテイクが無効になれば、録ってある次の行へ移る。
        l.invalidate_take(first).expect("無効にできる");
        assert!(
            l.owned_aliases_of_row(late)
                .expect("引ける")
                .contains("- い")
        );
    }

    /// 連続音の行が生むのは仮名ではなくエイリアス（`TR-PKG-22`）。
    #[test]
    fn 連続音の台帳はエイリアスで被覆を持つ() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_sequential(UnitSet::Core, 8).expect("生成できる");
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Sequential, &[60])
            .expect("書き込める");
        let sid = l.start_session(&session()).expect("始められる");

        assert!(l.covered_aliases().expect("引ける").is_empty());
        adopt(&mut l, sid, &list[0].id);

        let covered = l.covered_aliases().expect("引ける");
        let want =
            crate::reclist::row_aliases(&builtin_rules(), AliasMethod::Sequential, &list[0].units);
        assert_eq!(covered, want.into_iter().collect::<BTreeSet<_>>());
        // 仮名の集合では足りない。`- あ` のような綴りは仮名には無い。
        assert!(covered.iter().any(|a| a.starts_with("- ")), "{covered:?}");
    }

    /// 同じ仮名が2度出る行でも入る（`row_units` は集合として持つ）。
    #[test]
    fn 同じ仮名が二度出る行も書き込める() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let repeated = list
            .iter()
            .find(|r| {
                let mut seen = BTreeSet::new();
                r.units.iter().any(|u| !seen.insert(u.kana))
            })
            .expect("同じ仮名を2度持つ行がある");
        l.install_reclist_for_tones(
            std::slice::from_ref(repeated),
            &builtin_rules(),
            AliasMethod::Sequential,
            &[60],
        )
        .expect("書き込める");
    }

    /// 音高ごとに独立した行集合を持つ（`TR-RCL-26`）。
    #[test]
    fn 多音階は音高ごとに行を持つ() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let tones = [55, 62, 69];
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Single, &tones)
            .expect("書き込める");
        assert_eq!(l.recording_tones().expect("引ける"), tones);

        let sid = l.start_session(&session()).expect("始められる");
        // G3 の1行だけ録る。
        let g3 = format!("{}@G3", list[0].id);
        adopt(&mut l, sid, &g3);

        let by_tone = l.covered_aliases_by_tone().expect("引ける");
        assert_eq!(by_tone.len(), 1, "録った音高だけが出る");
        assert!(by_tone.contains_key(&55));
        // 音高を跨いで混ぜない。1音高だけ録り終えても他は空のまま。
        assert!(!by_tone.contains_key(&62));
    }

    /// 見出しの被覆は、設定した音高のすべてで録れた単位だけ（`TR-RCL-26`）。
    ///
    /// **音高を跨いだ和集合で数えていた。** 1音高だけ録り終えると
    /// 見出しも環も満ち、完成の判定まで進んでいた。
    #[test]
    fn 一音高だけ録り終えても被覆は満ちない() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Single, &[55, 62])
            .expect("書き込める");
        let sid = l.start_session(&session()).expect("始められる");

        // G3 を全行録る。 D4 は1行も録らない。
        for r in &list {
            adopt(&mut l, sid, &format!("{}@G3", r.id));
        }
        assert!(
            l.covered_units().expect("引ける").is_empty(),
            "D4 が空なので、どの単位も揃っていない"
        );
        let (done, total) = l
            .coverage_by_kana_row()
            .expect("引ける")
            .into_iter()
            .fold((0, 0), |(d, t), (c, n)| (d + c, t + n));
        assert!(total > 0);
        assert_eq!(done, 0, "環も満ちない");

        // D4 で1行録ると、その行の単位だけが両方で揃う。
        adopt(&mut l, sid, &format!("{}@D4", list[0].id));
        let covered = l.covered_units().expect("引ける");
        let want: BTreeSet<String> = list[0].units.iter().map(|u| u.kana.to_owned()).collect();
        assert_eq!(covered, want);
    }

    /// 不足は全件返す（`TR-PKG-23`）。件数だけにしない。
    #[test]
    fn 不足エイリアスは全件返る() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_cvvc(&builtin_rules(), UnitSet::Core, 8).expect("生成できる");
        l.install_reclist_for_tones(&list, &builtin_rules(), AliasMethod::Cvvc, &[60])
            .expect("書き込める");
        let required: BTreeSet<String> = ["- か".to_owned(), "a k".to_owned()].into();
        let missing = l.missing_aliases(&required).expect("引ける");
        assert_eq!(missing.len(), 2, "何も録っていないので全部足りない");
    }

    /// 境界を保存して読み戻せる（`TR-ALN-34`）。
    #[test]
    fn 境界を保存して読み戻せる() {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        let sid = l.start_session(&session()).expect("始められる");
        let t = l
            .commit_take(&FinalizedTake {
                row_id: list[0].id.clone(),
                session_id: sid,
                rel_path: "masters/a.wav".into(),
                frames: 44_100,
                recorded_at: "2026-09-20T12:00:01Z".into(),
            })
            .expect("確定できる");

        assert!(l.boundaries_for_take(t).expect("引ける").is_empty());
        let b = Boundary {
            voice_start_ms: 100.0,
            vowel_start_ms: 150.0,
            vowel_end_ms: 600.0,
        };
        l.put_boundaries(t, &[("あ".to_owned(), b)])
            .expect("書き込める");
        assert_eq!(
            l.boundaries_for_take(t).expect("引ける"),
            [("あ".to_owned(), b)]
        );

        // 同じ鍵は差し替える。古い境界が残ると、5値を作り直したときだけ値が飛ぶ。
        let b2 = Boundary {
            vowel_end_ms: 700.0,
            ..b
        };
        l.put_boundaries(t, &[("あ".to_owned(), b2)])
            .expect("書き込める");
        assert_eq!(
            l.boundaries_for_take(t).expect("引ける"),
            [("あ".to_owned(), b2)]
        );
    }
}
