//! サブバンクの枠（`DEC-RCL-018`）。 行はちょうど1つの枠に属する。
//!
//! 枠を足す・付け替える・並べ替える・担当音域を直す・消す手がここにある。 FSL の手との対応は
//! `specs/requirements/project-lifecycle.fsl` の `add_subbank` / `remove_subbank`。
//!
//! 予期できる断り（同じ組の枠がある、録ってある枠の方式を変えようとした、など）は失敗にせず
//! [`SubbankRefusal`] の値で返す（`DEC-PLT-038`）。 断るときは何も書いていない。

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use koeru_model::id::SubbankId;
use koeru_model::subbank::{ToneRange, VoiceColor};
use std::collections::{BTreeMap, BTreeSet};

use super::intent::OPEN_STATES;
use super::{Ledger, LedgerError, Result, RowOrigin, RowState, db, insert_rows};
use crate::alias::Method;
use crate::inventory::{UnitSet, units};
use crate::reclist::Row as ReclistRow;
use crate::schema::{
    adopted_takes, capture_intents, commit_receipts, row_aliases, row_units, rows, subbanks, takes,
};

/// 台帳での方式の表記。 送信してよい固定語彙。
pub(super) const fn method_str(m: Method) -> &'static str {
    match m {
        Method::Single => "single",
        Method::Sequential => "sequential",
        Method::Cvvc => "cvvc",
    }
}

fn parse_method(s: &str) -> Result<Method> {
    match s {
        "single" => Ok(Method::Single),
        "sequential" => Ok(Method::Sequential),
        "cvvc" => Ok(Method::Cvvc),
        _ => Err(LedgerError::UnknownSubbankMethod),
    }
}

/// 台帳にある枠1つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubbankRecord {
    pub id: SubbankId,
    /// `None` は声色なし。
    pub voice_color: Option<VoiceColor>,
    pub method: Method,
    /// 作ったときの方式プリセット。 移行前に作った project は持たないことがある。
    pub preset_id: Option<String>,
    /// 収録音階（MIDI）。
    pub tone: i32,
    /// 本人が直した担当音域。 `None` なら既定（[`koeru_model::subbank::default_ranges`]）。
    pub edited_range: Option<ToneRange>,
    /// 除外していない行の数。
    pub rows: usize,
    /// 採用テイクのある行の数。
    pub recorded_rows: usize,
    /// 確定したテイクの数。 無効のテイクも数える——枠を消すと、それも消える。
    pub takes: usize,
}

/// 足す枠（`TR-RCL-28`）。 方式はプリセットが決める。
#[derive(Debug, Clone, Copy)]
pub struct NewSubbank<'a> {
    pub voice_color: Option<&'a VoiceColor>,
    pub method: Method,
    pub preset_id: &'a str,
    pub tone: i32,
    /// この枠の後ろに置く。 `None` なら末尾。
    pub after: Option<SubbankId>,
    pub list: &'a [ReclistRow],
    pub rules: &'a crate::presamp::Rules,
}

/// 付け替え（`TR-RCL-30`）。 `None` の欄は変えない。
#[derive(Debug, Clone, Copy, Default)]
pub struct SubbankRevision<'a> {
    /// `Some(None)` で声色なしへ戻す。
    pub voice_color: Option<Option<&'a VoiceColor>>,
    pub tone: Option<i32>,
    pub preset: Option<PresetChange<'a>>,
}

/// 方式プリセットの付け替え。 未収録の枠では録音リストを作り直す。
#[derive(Debug, Clone, Copy)]
pub struct PresetChange<'a> {
    pub preset_id: &'a str,
    pub method: Method,
    pub list: &'a [ReclistRow],
    pub rules: &'a crate::presamp::Rules,
}

/// 枠の手を断った理由。 断ったときは何も書いていない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubbankRefusal {
    /// 同じ声色・方式・収録音階の枠がもうある（`TR-RCL-28`）。
    Conflict { existing: SubbankId },
    /// 指した枠が無い。
    UnknownSubbank,
    /// 録ってある枠の方式は変えない（`TR-RCL-30`）。
    MethodLocked,
    /// その枠の行を録っている途中。
    Recording,
    /// その枠の行に、本人がまだ採るか捨てるか決めていない復旧候補がある（`REQ-REC-007`）。
    RecoveryPending,
    /// 本人に示した、消えるテイクの数が今の数と違う（`TR-RCL-29`）。
    TakesChanged { actual: usize },
    /// 並びが、すべての枠をちょうど1回ずつ持っていない。
    BadOrder,
}

/// 枠の手の答え。 外側の `Result` は台帳の失敗、内側は断り。
pub type SubbankAnswer<T> = std::result::Result<T, SubbankRefusal>;

/// 消した枠の後始末に要るもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedSubbank {
    /// 消したテイクの数。
    pub takes: usize,
    /// 消したテイクの WAV の場所（プロジェクトの根からの相対）。 台帳から消したあとで
    /// 呼び出し側が消す——先に消すと、台帳が無いファイルを指す。
    pub files: Vec<String>,
}

/// 声色なしの枠を引く。 無ければ末尾に作る。 作成と移行の入口（[`Ledger::install_reclist_for_tones`]）。
pub(super) fn ensure_plain(
    c: &mut SqliteConnection,
    method: Method,
    preset_id: Option<&str>,
    tone: i32,
) -> QueryResult<SubbankId> {
    if let Some(id) = find_same(c, None, method, tone, None)? {
        return Ok(id);
    }
    let ordinal = next_ordinal(c)?;
    insert_subbank(c, None, method, preset_id, tone, ordinal)
}

fn next_ordinal(c: &mut SqliteConnection) -> QueryResult<i32> {
    Ok(subbanks::table
        .select(diesel::dsl::max(subbanks::ordinal))
        .first::<Option<i32>>(c)?
        .map_or(0, |m| m + 1))
}

fn insert_subbank(
    c: &mut SqliteConnection,
    voice_color: Option<&VoiceColor>,
    method: Method,
    preset_id: Option<&str>,
    tone: i32,
    ordinal: i32,
) -> QueryResult<SubbankId> {
    diesel::insert_into(subbanks::table)
        .values((
            subbanks::voice_color.eq(voice_color.map(VoiceColor::as_str)),
            subbanks::method.eq(method_str(method)),
            subbanks::preset_id.eq(preset_id),
            subbanks::tone.eq(tone),
            subbanks::ordinal.eq(ordinal),
        ))
        .execute(c)?;
    let id = subbanks::table
        .select(subbanks::id)
        .order(subbanks::id.desc())
        .first(c)
        .map(SubbankId::new)?;
    // 枠をすべて入れている配布物は、増えた枠も入れる（`TR-PKG-58`）。
    super::shelf::join_covering(c, id)?;
    Ok(id)
}

/// 同じ声色・方式・収録音階の枠（`TR-RCL-28`）。 `except` は自分を除くため。
fn find_same(
    c: &mut SqliteConnection,
    voice_color: Option<&VoiceColor>,
    method: Method,
    tone: i32,
    except: Option<SubbankId>,
) -> QueryResult<Option<SubbankId>> {
    let mut q = subbanks::table
        .filter(subbanks::method.eq(method_str(method)))
        .filter(subbanks::tone.eq(tone))
        .filter(subbanks::id.ne(except.map_or(0, SubbankId::get)))
        .select(subbanks::id)
        .into_boxed();
    q = match voice_color {
        Some(v) => q.filter(subbanks::voice_color.eq(v.as_str())),
        None => q.filter(subbanks::voice_color.is_null()),
    };
    q.first(c).optional().map(|o| o.map(SubbankId::new))
}

type StoredSubbank = (
    i32,
    Option<String>,
    String,
    Option<String>,
    i32,
    Option<i32>,
    Option<i32>,
);

fn stored(c: &mut SqliteConnection, id: SubbankId) -> QueryResult<Option<StoredSubbank>> {
    subbanks::table
        .find(id.get())
        .select((
            subbanks::id,
            subbanks::voice_color,
            subbanks::method,
            subbanks::preset_id,
            subbanks::tone,
            subbanks::range_low,
            subbanks::range_high,
        ))
        .first(c)
        .optional()
}

/// 枠の番号ごとの件数。
type CountBySubbank = BTreeMap<i32, i64>;

/// 枠ごとの数（行・採用のある行・テイク）。
fn counts(
    c: &mut SqliteConnection,
) -> QueryResult<(CountBySubbank, CountBySubbank, CountBySubbank)> {
    let listed = rows::table
        .filter(rows::state.ne(RowState::Excluded.as_str()))
        .group_by(rows::subbank_id)
        .select((rows::subbank_id, diesel::dsl::count_star()))
        .load::<(i32, i64)>(c)?
        .into_iter()
        .collect();
    let recorded = rows::table
        .inner_join(adopted_takes::table.on(adopted_takes::row_id.eq(rows::id)))
        .filter(rows::state.ne(RowState::Excluded.as_str()))
        .group_by(rows::subbank_id)
        .select((rows::subbank_id, diesel::dsl::count_star()))
        .load::<(i32, i64)>(c)?
        .into_iter()
        .collect();
    let taken = takes::table
        .inner_join(rows::table.on(rows::id.eq(takes::row_id)))
        .group_by(rows::subbank_id)
        .select((rows::subbank_id, diesel::dsl::count_star()))
        .load::<(i32, i64)>(c)?
        .into_iter()
        .collect();
    Ok((listed, recorded, taken))
}

fn count_of(m: &CountBySubbank, id: i32) -> usize {
    m.get(&id)
        .copied()
        .map_or(0, |n| usize::try_from(n).unwrap_or(0))
}

/// 枠の並びの順で読む。
fn load(c: &mut SqliteConnection) -> Result<Vec<SubbankRecord>> {
    let stored: Vec<StoredSubbank> = subbanks::table
        .order((subbanks::ordinal.asc(), subbanks::id.asc()))
        .select((
            subbanks::id,
            subbanks::voice_color,
            subbanks::method,
            subbanks::preset_id,
            subbanks::tone,
            subbanks::range_low,
            subbanks::range_high,
        ))
        .load(c)
        .map_err(db("subbanks"))?;
    let (listed, recorded, taken) = counts(c).map_err(db("subbanks.counts"))?;
    stored
        .into_iter()
        .map(|(id, voice_color, method, preset_id, tone, low, high)| {
            Ok(SubbankRecord {
                id: SubbankId::new(id),
                voice_color: voice_color.as_deref().and_then(VoiceColor::parse),
                method: parse_method(&method)?,
                preset_id,
                tone,
                edited_range: low.zip(high).map(|(low, high)| ToneRange { low, high }),
                rows: count_of(&listed, id),
                recorded_rows: count_of(&recorded, id),
                takes: count_of(&taken, id),
            })
        })
        .collect()
}

fn takes_in(c: &mut SqliteConnection, id: SubbankId) -> QueryResult<i64> {
    takes::table
        .inner_join(rows::table.on(rows::id.eq(takes::row_id)))
        .filter(rows::subbank_id.eq(id.get()))
        .count()
        .get_result(c)
}

/// その枠の行が持つ、閉じていない予定から断る理由を引く。
fn open_capture_refusal(
    c: &mut SqliteConnection,
    id: SubbankId,
) -> QueryResult<Option<SubbankRefusal>> {
    let states: Vec<String> = capture_intents::table
        .inner_join(rows::table.on(rows::id.eq(capture_intents::row_id)))
        .filter(rows::subbank_id.eq(id.get()))
        .filter(capture_intents::state.eq_any(OPEN_STATES))
        .select(capture_intents::state)
        .load(c)?;
    Ok(if states.iter().any(|s| s == "open") {
        Some(SubbankRefusal::Recording)
    } else if states.is_empty() {
        None
    } else {
        Some(SubbankRefusal::RecoveryPending)
    })
}

/// テイクを持たない枠の行を全部消す。 呼ぶ側が、テイクも閉じていない予定も無いことを確かめてある。
fn delete_untaken_rows_of(c: &mut SqliteConnection, id: SubbankId) -> QueryResult<()> {
    let ids: Vec<String> = rows::table
        .filter(rows::subbank_id.eq(id.get()))
        .select(rows::id)
        .load(c)?;
    diesel::delete(row_aliases::table.filter(row_aliases::row_id.eq_any(&ids))).execute(c)?;
    diesel::delete(row_units::table.filter(row_units::row_id.eq_any(&ids))).execute(c)?;
    diesel::delete(rows::table.filter(rows::id.eq_any(&ids))).execute(c)?;
    Ok(())
}

/// [`Ledger::revise_subbank`] のトランザクションの中身。
fn revise(
    c: &mut SqliteConnection,
    id: SubbankId,
    change: &SubbankRevision<'_>,
) -> QueryResult<Result<SubbankAnswer<()>>> {
    let Some((_, color, method, preset_id, tone, _, _)) = stored(c, id)? else {
        return Ok(Ok(Err(SubbankRefusal::UnknownSubbank)));
    };
    let method = match parse_method(&method) {
        Ok(m) => m,
        Err(e) => return Ok(Err(e)),
    };
    let color = color.as_deref().and_then(VoiceColor::parse);
    let new_color = change.voice_color.unwrap_or(color.as_ref());
    let new_tone = change.tone.unwrap_or(tone);

    let mut rebuild = None;
    let mut new_method = method;
    let mut new_preset = preset_id.clone();
    if let Some(p) = change.preset {
        let same_preset = preset_id.as_deref() == Some(p.preset_id);
        // 古い project の枠はプリセットを持たない。 方式が同じなら、名乗りを埋めるだけ。
        let fills_in = preset_id.is_none() && p.method == method;
        if !same_preset && !fills_in {
            if takes_in(c, id)? > 0 {
                return Ok(Ok(Err(SubbankRefusal::MethodLocked)));
            }
            if let Some(r) = open_capture_refusal(c, id)? {
                return Ok(Ok(Err(r)));
            }
            rebuild = Some(p);
        }
        new_method = p.method;
        new_preset = Some(p.preset_id.to_owned());
    }

    if let Some(existing) = find_same(c, new_color, new_method, new_tone, Some(id))? {
        return Ok(Ok(Err(SubbankRefusal::Conflict { existing })));
    }

    diesel::update(subbanks::table.find(id.get()))
        .set((
            subbanks::voice_color.eq(new_color.map(VoiceColor::as_str)),
            subbanks::method.eq(method_str(new_method)),
            subbanks::preset_id.eq(new_preset.as_deref()),
            subbanks::tone.eq(new_tone),
        ))
        .execute(c)?;
    if new_tone != tone {
        diesel::update(rows::table.filter(rows::subbank_id.eq(id.get())))
            .set(rows::tone.eq(new_tone))
            .execute(c)?;
    }
    if let Some(p) = rebuild {
        delete_untaken_rows_of(c, id)?;
        insert_rows(c, p.list, p.rules, p.method, id, true, RowOrigin::Preset)?;
    }
    Ok(Ok(Ok(())))
}

impl Ledger {
    /// 枠を並びの順で（`TR-RCL-28`）。
    ///
    /// # Errors
    ///
    /// 台帳を読めない、方式の表記を知らない（[`LedgerError::UnknownSubbankMethod`]）。
    pub fn subbanks(&mut self) -> Result<Vec<SubbankRecord>> {
        load(&mut self.conn)
    }

    /// 行が属する枠。
    ///
    /// # Errors
    ///
    /// 行が無い（[`LedgerError::UnknownRow`]）、台帳を読めない。
    pub fn row_subbank(&mut self, row_id: &str) -> Result<SubbankId> {
        rows::table
            .find(row_id)
            .select(rows::subbank_id)
            .first::<i32>(&mut self.conn)
            .optional()
            .map_err(db("row_subbank"))?
            .map(SubbankId::new)
            .ok_or(LedgerError::UnknownRow)
    }

    /// その音階を持つ枠のうち、並びの先頭のもの。 無ければ `None`。
    ///
    /// 被覆を音階で数える手（詰め直し、`DEC-RCL-016`）が、行を入れる枠を選ぶのに使う。
    /// 声色の違う枠が同じ音階を持つと、後ろの枠には入らない（被覆の鍵を枠へ移すまで）。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn subbank_for_tone(&mut self, tone: i32) -> Result<Option<SubbankId>> {
        subbanks::table
            .filter(subbanks::tone.eq(tone))
            .order((subbanks::ordinal.asc(), subbanks::id.asc()))
            .select(subbanks::id)
            .first::<i32>(&mut self.conn)
            .optional()
            .map(|o| o.map(SubbankId::new))
            .map_err(db("subbank_for_tone"))
    }

    /// 枠の、まだ採用テイクの無い行を、録音リストの行の形で（`TR-RCL-09` の残り）。
    /// 除外した行は入らない。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn unrecorded_rows_of(&mut self, id: SubbankId, set: UnitSet) -> Result<Vec<ReclistRow>> {
        let table = units(set);
        Ok(rows::table
            .filter(rows::subbank_id.eq(id.get()))
            .filter(rows::state.ne(RowState::Excluded.as_str()))
            .filter(diesel::dsl::not(diesel::dsl::exists(
                adopted_takes::table.filter(adopted_takes::row_id.eq(rows::id)),
            )))
            .order(rows::ordinal.asc())
            .select((rows::id, rows::text, rows::file_stem))
            .load::<(String, String, String)>(&mut self.conn)
            .map_err(db("unrecorded_rows_of"))?
            .into_iter()
            .map(|(id, text, file_stem)| {
                let units = text
                    .split_whitespace()
                    .filter_map(|k| table.iter().find(|u| u.kana == k).cloned())
                    .collect();
                ReclistRow {
                    id,
                    text,
                    units,
                    file_stem,
                }
            })
            .collect())
    }

    /// 枠を足す（`TR-RCL-28`、FSL の `add_subbank`）。 足した枠は未収録から始まる。
    ///
    /// 枠と、その枠の録音リストの行を1つのトランザクションで入れる。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self, new), fields(rows = new.list.len()))]
    pub fn add_subbank(&mut self, new: &NewSubbank<'_>) -> Result<SubbankAnswer<SubbankId>> {
        self.conn
            .transaction(|c| {
                if let Some(existing) = find_same(c, new.voice_color, new.method, new.tone, None)? {
                    return Ok(Err(SubbankRefusal::Conflict { existing }));
                }
                let ordinal = match new.after {
                    None => next_ordinal(c)?,
                    Some(after) => {
                        let Some(at) = subbanks::table
                            .find(after.get())
                            .select(subbanks::ordinal)
                            .first::<i32>(c)
                            .optional()?
                        else {
                            return Ok(Err(SubbankRefusal::UnknownSubbank));
                        };
                        diesel::update(subbanks::table.filter(subbanks::ordinal.gt(at)))
                            .set(subbanks::ordinal.eq(subbanks::ordinal + 1))
                            .execute(c)?;
                        at + 1
                    }
                };
                let id = insert_subbank(
                    c,
                    new.voice_color,
                    new.method,
                    Some(new.preset_id),
                    new.tone,
                    ordinal,
                )?;
                insert_rows(
                    c,
                    new.list,
                    new.rules,
                    new.method,
                    id,
                    true,
                    RowOrigin::Preset,
                )?;
                Ok(Ok(id))
            })
            .map_err(db("add_subbank"))
    }

    /// 枠の声色・収録音階・方式プリセットを付け替える（`TR-RCL-30`）。
    ///
    /// 収録音階を付け替えると、枠の行の音高も同じトランザクションで書き換える。 テイクも
    /// 音声も変えない。 方式プリセットを変えた未収録の枠は、録音リストを作り直す。
    ///
    /// # Errors
    ///
    /// 台帳を書けない、方式の表記を知らない。
    #[tracing::instrument(skip(self, change))]
    pub fn revise_subbank(
        &mut self,
        id: SubbankId,
        change: &SubbankRevision<'_>,
    ) -> Result<SubbankAnswer<()>> {
        // 内側の `Result` は書く前に見つかる台帳の失敗（方式の表記を知らない）。 まだ何も書いていない。
        self.conn
            .transaction(|c| revise(c, id, change))
            .map_err(db("revise_subbank"))?
    }

    /// 枠を並べ替える（`TR-RCL-28`）。 `order` はすべての枠を、ちょうど1回ずつ。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self, order), fields(subbanks = order.len()))]
    pub fn reorder_subbanks(&mut self, order: &[SubbankId]) -> Result<SubbankAnswer<()>> {
        self.conn
            .transaction(|c| {
                let all: BTreeSet<i32> = subbanks::table
                    .select(subbanks::id)
                    .load::<i32>(c)?
                    .into_iter()
                    .collect();
                let given: BTreeSet<i32> = order.iter().map(|s| s.get()).collect();
                if given.len() != order.len() || given != all {
                    return Ok(Err(SubbankRefusal::BadOrder));
                }
                for (ordinal, s) in (0..).zip(order) {
                    diesel::update(subbanks::table.find(s.get()))
                        .set(subbanks::ordinal.eq(ordinal))
                        .execute(c)?;
                }
                Ok(Ok(()))
            })
            .map_err(db("reorder_subbanks"))
    }

    /// 枠の担当音域を直す（`TR-RCL-31`）。 `None` で既定へ戻す。
    ///
    /// 重なりも隙間も弾かない。 どの枠が歌うかの規則がまだ無い（`Q-RCL-005`）。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self, range))]
    pub fn set_subbank_tone_range(
        &mut self,
        id: SubbankId,
        range: Option<ToneRange>,
    ) -> Result<SubbankAnswer<()>> {
        self.conn
            .transaction(|c| {
                let changed = diesel::update(subbanks::table.find(id.get()))
                    .set((
                        subbanks::range_low.eq(range.map(|r| r.low)),
                        subbanks::range_high.eq(range.map(|r| r.high)),
                    ))
                    .execute(c)?;
                Ok(if changed == 0 {
                    Err(SubbankRefusal::UnknownSubbank)
                } else {
                    Ok(())
                })
            })
            .map_err(db("set_subbank_tone_range"))
    }

    /// 枠を消す（`TR-RCL-29`、FSL の `remove_subbank`）。 その枠の行とテイクも消え、
    /// ほかの枠は変わらない（`INV-RCL-102`）。
    ///
    /// `acknowledged_takes` は本人に示した、消えるテイクの数。 今の数と違えば断る。 その枠の
    /// 行を録っている途中か、復旧候補が残っていれば断る——候補を消したことにすると、
    /// 本人が選んでいない候補が消える（`REQ-REC-007`）。
    ///
    /// WAV は消さない。 [`RemovedSubbank::files`] を呼び出し側が消す。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self, acknowledged_takes))]
    pub fn remove_subbank(
        &mut self,
        id: SubbankId,
        acknowledged_takes: usize,
    ) -> Result<SubbankAnswer<RemovedSubbank>> {
        self.conn
            .transaction(|c| {
                if stored(c, id)?.is_none() {
                    return Ok(Err(SubbankRefusal::UnknownSubbank));
                }
                if let Some(r) = open_capture_refusal(c, id)? {
                    return Ok(Err(r));
                }
                let row_ids: Vec<String> = rows::table
                    .filter(rows::subbank_id.eq(id.get()))
                    .select(rows::id)
                    .load(c)?;
                let gone: Vec<(i32, String)> = takes::table
                    .filter(takes::row_id.eq_any(&row_ids))
                    .order(takes::id.asc())
                    .select((takes::id, takes::rel_path))
                    .load(c)?;
                if gone.len() != acknowledged_takes {
                    return Ok(Err(SubbankRefusal::TakesChanged { actual: gone.len() }));
                }
                let take_ids: Vec<i32> = gone.iter().map(|(t, _)| *t).collect();
                let captures: Vec<String> = capture_intents::table
                    .filter(capture_intents::row_id.eq_any(&row_ids))
                    .select(capture_intents::capture_id)
                    .load(c)?;

                diesel::delete(
                    commit_receipts::table.filter(
                        commit_receipts::take_id
                            .eq_any(&take_ids)
                            .or(commit_receipts::capture_id.eq_any(&captures)),
                    ),
                )
                .execute(c)?;
                diesel::delete(
                    capture_intents::table.filter(capture_intents::row_id.eq_any(&row_ids)),
                )
                .execute(c)?;
                diesel::delete(adopted_takes::table.filter(adopted_takes::row_id.eq_any(&row_ids)))
                    .execute(c)?;
                diesel::delete(takes::table.filter(takes::id.eq_any(&take_ids))).execute(c)?;
                delete_untaken_rows_of(c, id)?;
                diesel::delete(subbanks::table.find(id.get())).execute(c)?;
                Ok(Ok(RemovedSubbank {
                    takes: gone.len(),
                    files: gone.into_iter().map(|(_, p)| p).collect(),
                }))
            })
            .map_err(db("remove_subbank"))
    }
}

impl super::LedgerSnapshot {
    /// 枠を並びの順で。 [`Ledger::subbanks`] と同じ形で、このスナップショットの版から読む。
    ///
    /// # Errors
    ///
    /// 台帳を読めない、方式の表記を知らない（[`LedgerError::UnknownSubbankMethod`]）。
    pub fn subbanks(&mut self) -> Result<Vec<SubbankRecord>> {
        load(self.conn())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::intent::{CaptureId, Leftover, NewIntent};
    use crate::db::{FinalizedTake, SessionSnapshot};
    use crate::reclist::{generate_sequential, generate_single};

    fn rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }

    fn session() -> SessionSnapshot {
        SessionSnapshot {
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
        }
    }

    /// 単独音の枠が 55 と 62 に1つずつある台帳。
    fn ledger() -> (Ledger, Vec<ReclistRow>, Vec<SubbankId>) {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist_for_tones(&list, &rules(), Method::Single, Some("single"), &[55, 62])
            .expect("書き込める");
        let ids = l.subbanks().expect("読める").iter().map(|s| s.id).collect();
        (l, list, ids)
    }

    fn take(l: &mut Ledger, row_id: &str, path: &str) {
        let sid = l.start_session(&session()).expect("始められる");
        l.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id: sid,
            rel_path: path.to_owned(),
            frames: 44_100,
            recorded_at: "2026-11-01T00:00:01Z".into(),
        })
        .expect("確定できる");
    }

    fn rows_of(l: &mut Ledger, id: SubbankId) -> Vec<(String, i32)> {
        rows::table
            .filter(rows::subbank_id.eq(id.get()))
            .order(rows::ordinal.asc())
            .select((rows::id, rows::tone))
            .load(&mut l.conn)
            .expect("読める")
    }

    fn soft() -> VoiceColor {
        VoiceColor::parse("soft").expect("空でない")
    }

    fn new_subbank<'a>(
        color: Option<&'a VoiceColor>,
        tone: i32,
        list: &'a [ReclistRow],
        rules: &'a crate::presamp::Rules,
    ) -> NewSubbank<'a> {
        NewSubbank {
            voice_color: color,
            method: Method::Single,
            preset_id: "single",
            tone,
            after: None,
            list,
            rules,
        }
    }

    #[test]
    fn 作るときの枠は音階ごとに1つで_二度入れても増えない() {
        let (mut l, list, ids) = ledger();
        assert_eq!(ids.len(), 2);
        l.install_reclist_for_tones(&list, &rules(), Method::Single, Some("single"), &[55, 62])
            .expect("書き込める");
        let subbanks = l.subbanks().expect("読める");
        assert_eq!(subbanks.len(), 2);
        assert_eq!(subbanks[0].tone, 55);
        assert_eq!(subbanks[0].preset_id.as_deref(), Some("single"));
        assert_eq!(subbanks[0].rows, list.len());
    }

    /// 声色が違えば同じ音階・方式でも別の枠（`TR-RCL-28`）。 同じ組は断り、もうある枠を返す。
    #[test]
    fn 同じ組の枠は足せず_声色が違えば足せる() {
        let (mut l, list, ids) = ledger();
        let r = rules();
        assert_eq!(
            l.add_subbank(&new_subbank(None, 55, &list, &r))
                .expect("台帳は書ける"),
            Err(SubbankRefusal::Conflict { existing: ids[0] })
        );
        let soft = soft();
        let added = l
            .add_subbank(&new_subbank(Some(&soft), 55, &list, &r))
            .expect("台帳は書ける")
            .expect("足せる");
        let rows = rows_of(&mut l, added);
        assert_eq!(rows.len(), list.len(), "足した枠は未収録の行を持つ");
        assert!(rows.iter().all(|(_, t)| *t == 55));
        let before: BTreeSet<String> = rows_of(&mut l, ids[0])
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        assert!(
            rows.iter().all(|(id, _)| !before.contains(id)),
            "同じ音階の行と ID を分ける"
        );
        let subbanks = l.subbanks().expect("読める");
        assert_eq!(subbanks.last().map(|s| s.id), Some(added), "省けば末尾");
        assert_eq!(
            subbanks.last().and_then(|s| s.voice_color.clone()),
            Some(soft)
        );
    }

    #[test]
    fn 足す枠は指した枠の後ろに入る() {
        let (mut l, list, ids) = ledger();
        let r = rules();
        let added = l
            .add_subbank(&NewSubbank {
                after: Some(ids[0]),
                ..new_subbank(None, 69, &list, &r)
            })
            .expect("台帳は書ける")
            .expect("足せる");
        let order: Vec<SubbankId> = l.subbanks().expect("読める").iter().map(|s| s.id).collect();
        assert_eq!(order, [ids[0], added, ids[1]]);
        assert_eq!(
            l.add_subbank(&NewSubbank {
                after: Some(SubbankId::new(999)),
                ..new_subbank(None, 70, &list, &r)
            })
            .expect("台帳は書ける"),
            Err(SubbankRefusal::UnknownSubbank)
        );
    }

    /// 収録音階の付け替えは行の音高も一緒に変える。 テイクは残る（`TR-RCL-30`）。
    #[test]
    fn 音階を付け替えると行の音高も同じく変わる() {
        let (mut l, _, ids) = ledger();
        let row = rows_of(&mut l, ids[0])[0].0.clone();
        take(&mut l, &row, "audio/G3/a_1.wav");
        let change = SubbankRevision {
            tone: Some(57),
            ..SubbankRevision::default()
        };
        assert_eq!(
            l.revise_subbank(ids[0], &change).expect("台帳は書ける"),
            Ok(())
        );
        assert!(rows_of(&mut l, ids[0]).iter().all(|(_, t)| *t == 57));
        assert!(
            rows_of(&mut l, ids[1]).iter().all(|(_, t)| *t == 62),
            "ほかの枠は変わらない"
        );
        let s = &l.subbanks().expect("読める")[0];
        assert_eq!((s.tone, s.takes), (57, 1));
        assert_eq!(l.recording_tones().expect("引ける"), [57, 62]);

        let clash = SubbankRevision {
            tone: Some(62),
            ..SubbankRevision::default()
        };
        assert_eq!(
            l.revise_subbank(ids[0], &clash).expect("台帳は書ける"),
            Err(SubbankRefusal::Conflict { existing: ids[1] })
        );
    }

    /// 録ってある枠の方式は変えない。 未収録の枠は変えられ、録音リストを作り直す（`TR-RCL-30`）。
    #[test]
    fn 録ってある枠の方式は変えられず_未収録なら作り直す() {
        let (mut l, _, ids) = ledger();
        let r = rules();
        let sequential = generate_sequential(UnitSet::Core, 8).expect("生成できる");
        let change = SubbankRevision {
            preset: Some(PresetChange {
                preset_id: "sequential",
                method: Method::Sequential,
                list: &sequential,
                rules: &r,
            }),
            ..SubbankRevision::default()
        };
        let row = rows_of(&mut l, ids[0])[0].0.clone();
        take(&mut l, &row, "audio/a_1.wav");
        assert_eq!(
            l.revise_subbank(ids[0], &change).expect("台帳は書ける"),
            Err(SubbankRefusal::MethodLocked)
        );

        assert_eq!(
            l.revise_subbank(ids[1], &change).expect("台帳は書ける"),
            Ok(())
        );
        let s = &l.subbanks().expect("読める")[1];
        assert_eq!(s.method, Method::Sequential);
        assert_eq!(s.preset_id.as_deref(), Some("sequential"));
        assert_eq!(s.rows, sequential.len(), "連続音のリストで作り直す");
    }

    #[test]
    fn 並べ替えはすべての枠をちょうど1回ずつ受け取る() {
        let (mut l, _, ids) = ledger();
        assert_eq!(
            l.reorder_subbanks(&[ids[0]]).expect("台帳は書ける"),
            Err(SubbankRefusal::BadOrder),
            "欠け"
        );
        assert_eq!(
            l.reorder_subbanks(&[ids[0], ids[1], ids[1]])
                .expect("台帳は書ける"),
            Err(SubbankRefusal::BadOrder),
            "重複"
        );
        assert_eq!(
            l.reorder_subbanks(&[ids[1], ids[0]]).expect("台帳は書ける"),
            Ok(())
        );
        let order: Vec<SubbankId> = l.subbanks().expect("読める").iter().map(|s| s.id).collect();
        assert_eq!(order, [ids[1], ids[0]]);
    }

    #[test]
    fn 担当音域は直したものだけを持ち_既定へ戻せる() {
        let (mut l, _, ids) = ledger();
        let range = ToneRange::edited(50, 60).expect("範囲の中");
        assert_eq!(
            l.set_subbank_tone_range(ids[0], Some(range))
                .expect("台帳は書ける"),
            Ok(())
        );
        let subbanks = l.subbanks().expect("読める");
        assert_eq!(subbanks[0].edited_range, Some(range));
        assert_eq!(subbanks[1].edited_range, None);
        assert_eq!(
            l.set_subbank_tone_range(ids[0], None)
                .expect("台帳は書ける"),
            Ok(())
        );
        assert_eq!(l.subbanks().expect("読める")[0].edited_range, None);
        assert_eq!(
            l.set_subbank_tone_range(SubbankId::new(999), None)
                .expect("台帳は書ける"),
            Err(SubbankRefusal::UnknownSubbank)
        );
    }

    /// 消すと、その枠の行とテイクが消え、ほかの枠は変わらない（`INV-RCL-102`）。
    /// 示したテイクの数が違えば断る（`TR-RCL-29`）。
    #[test]
    fn 枠を消すとその枠の行とテイクだけが消える() {
        let (mut l, _, ids) = ledger();
        let mine = rows_of(&mut l, ids[0]);
        let other = rows_of(&mut l, ids[1]);
        take(&mut l, &mine[0].0, "audio/G3/a_1.wav");
        take(&mut l, &mine[0].0, "audio/G3/a_2.wav");
        take(&mut l, &other[0].0, "audio/D4/a_1.wav");

        assert_eq!(
            l.remove_subbank(ids[0], 1).expect("台帳は書ける"),
            Err(SubbankRefusal::TakesChanged { actual: 2 })
        );
        let removed = l
            .remove_subbank(ids[0], 2)
            .expect("台帳は書ける")
            .expect("消せる");
        assert_eq!(removed.takes, 2);
        assert_eq!(removed.files, ["audio/G3/a_1.wav", "audio/G3/a_2.wav"]);

        let left = l.subbanks().expect("読める");
        assert_eq!(left.iter().map(|s| s.id).collect::<Vec<_>>(), [ids[1]]);
        assert_eq!((left[0].takes, left[0].rows), (1, other.len()));
        assert_eq!(rows_of(&mut l, ids[1]), other);
        assert!(rows_of(&mut l, ids[0]).is_empty());
        assert_eq!(
            l.remove_subbank(ids[0], 0).expect("台帳は書ける"),
            Err(SubbankRefusal::UnknownSubbank)
        );
    }

    /// 録っている途中の枠と、復旧候補が残る枠は消せない（`REQ-REC-007`）。
    #[test]
    fn 録音中と復旧候補の残る枠は消せない() {
        let (mut l, _, ids) = ledger();
        let row = rows_of(&mut l, ids[0])[0].0.clone();
        let sid = l.start_session(&session()).expect("始められる");
        let capture = CaptureId::generate();
        l.declare_capture(&NewIntent {
            capture: &capture,
            row_id: &row,
            session_id: sid,
            rel_path: "audio/a_1.wav",
            declared_at: "2026-11-01T00:00:01Z",
        })
        .expect("予定を書ける");
        assert_eq!(
            l.remove_subbank(ids[0], 0).expect("台帳は書ける"),
            Err(SubbankRefusal::Recording)
        );
        l.mark_leftover(&capture, Leftover::Orphaned, "2026-11-01T00:00:02Z")
            .expect("印を付けられる");
        assert_eq!(
            l.remove_subbank(ids[0], 0).expect("台帳は書ける"),
            Err(SubbankRefusal::RecoveryPending)
        );
        assert_eq!(
            l.remove_subbank(ids[1], 0)
                .expect("台帳は書ける")
                .map(|r| r.takes),
            Ok(0),
            "ほかの枠は消せる"
        );
    }
}
