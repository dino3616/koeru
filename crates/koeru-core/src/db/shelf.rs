//! 名前を付けて残す配布物（`DEC-PKG-018`、`TR-PKG-58`〜`TR-PKG-61`）。
//!
//! 1つの音源が配布物を1つ以上持ち、配布物ごとに版を数える。 配布物が持つのは、入れる枠・名前・
//! 音源ルートフォルダ名・利用規約の上書き・下書きの番号。 画像・名義・声の提供者・web・
//! サンプル音声は音源で1つで、`distribution` 表（[`super::Distribution`]）に残る。
//!
//! 予期できる断り（フォルダ名が別の配布物と重なった、最後の1つを消そうとした、など）は失敗に
//! せず [`DistributionRefusal`] の値で返す（`DEC-PLT-038`）。 断るときは何も書いていない。

use diesel::prelude::*;
use diesel::sql_types::{BigInt, Integer};
use diesel::sqlite::SqliteConnection;
use koeru_model::distribution::{FIRST_VERSION, copy_archive_name, copy_name, next_version};
use koeru_model::id::{DistributionId, SubbankId};
use koeru_model::subbank::ToneRange;

use super::{Ledger, Result, RowState, db};
use crate::schema::{distribution_subbanks, distributions, release_adoptions, releases, subbanks};

/// 配布物が入れる枠1つ（`TR-PKG-58`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PackagedSubbankRecord {
    pub subbank: SubbankId,
    /// この配布物で直した担当音域（`TR-PKG-59`）。 `None` なら枠の担当音域を書く。
    pub edited_range: Option<ToneRange>,
}

/// 台帳にある配布物1つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DistributionRecord {
    pub id: DistributionId,
    pub name: String,
    /// 音源ルートフォルダ名。 `None` は、まだ決めていない（`fill_default_archive_name` が埋める）。
    pub archive_name: Option<String>,
    /// この配布物だけの利用規約。 `None` なら音源の規約。
    pub terms: Option<String>,
    /// 次に書き出す版の番号。 `None` は下書きが無いこと（`TR-PKG-60`）。
    pub draft_version: Option<String>,
    /// 入れる枠。 枠の並びの順。
    pub subbanks: Vec<PackagedSubbankRecord>,
}

/// 直す欄。 `None` の欄は変えない（`reviseDistribution`）。
#[derive(Debug, Clone, Copy, Default)]
pub struct DistributionRevision<'a> {
    pub name: Option<&'a str>,
    pub archive_name: Option<&'a str>,
    /// 渡すと入れる枠をこれに置き換える。
    pub subbanks: Option<&'a [PackagedSubbankRecord]>,
    pub terms: Option<&'a str>,
    /// この配布物の規約を消し、音源の規約に戻す。
    pub clear_terms: bool,
    pub draft_version: Option<&'a str>,
    /// 下書きを消す。
    pub clear_draft_version: bool,
}

/// 配布物の手を断った理由。 断ったときは何も書いていない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistributionRefusal {
    /// 指した配布物が無い。
    UnknownDistribution,
    /// 入れる枠に、無い枠を指した。
    UnknownSubbank,
    /// フォルダ名が別の配布物と重なった（`TR-PKG-58`）。
    Conflict { existing: DistributionId },
    /// 最後の1つは消せない（`TR-PKG-58`）。
    LastOne,
    /// 下書きの番号が、この配布物で書き出した番号と重なった（`TR-PKG-60`）。
    VersionTaken,
}

/// 配布物の手の答え。 外側の `Result` は台帳の失敗、内側は断り。
pub type DistributionAnswer<T> = std::result::Result<T, DistributionRefusal>;

type StoredDistribution = (i32, String, Option<String>, Option<String>, Option<String>);

fn load(c: &mut SqliteConnection) -> QueryResult<Vec<DistributionRecord>> {
    let stored: Vec<StoredDistribution> = distributions::table
        .order((distributions::ordinal.asc(), distributions::id.asc()))
        .select((
            distributions::id,
            distributions::name,
            distributions::archive_name,
            distributions::terms,
            distributions::draft_version,
        ))
        .load(c)?;
    // 枠の並びの順（`RecordingFacet.subbanks` と同じ）。
    let members: Vec<(i32, i32, Option<i32>, Option<i32>)> = distribution_subbanks::table
        .inner_join(subbanks::table.on(subbanks::id.eq(distribution_subbanks::subbank_id)))
        .order((subbanks::ordinal.asc(), subbanks::id.asc()))
        .select((
            distribution_subbanks::distribution_id,
            distribution_subbanks::subbank_id,
            distribution_subbanks::range_low,
            distribution_subbanks::range_high,
        ))
        .load(c)?;
    Ok(stored
        .into_iter()
        .map(|(id, name, archive_name, terms, draft_version)| DistributionRecord {
            id: DistributionId::new(id),
            name,
            archive_name,
            terms,
            draft_version,
            subbanks: members
                .iter()
                .filter(|(d, ..)| *d == id)
                .map(|(_, s, low, high)| PackagedSubbankRecord {
                    subbank: SubbankId::new(*s),
                    edited_range: low.zip(*high).map(|(low, high)| ToneRange { low, high }),
                })
                .collect(),
        })
        .collect())
}

/// 足した枠を、いまある枠をすべて入れている配布物へ入れる（`TR-PKG-58`）。
///
/// 「すべて」は枠が増えても「すべて」でいる。 枠を外した配布物は、増えた枠を入れない
/// ——本人が外したものを、黙って戻さない。 枠を足す入口は [`super::subbank`] の `insert_subbank`
/// だけで、そこが呼ぶ。
pub(super) fn join_covering(c: &mut SqliteConnection, added: SubbankId) -> QueryResult<()> {
    diesel::sql_query(
        "INSERT INTO distribution_subbanks (distribution_id, subbank_id) \
         SELECT d.id, ? FROM distributions AS d \
         WHERE NOT EXISTS ( \
             SELECT 1 FROM subbanks AS s \
             WHERE s.id <> ? AND NOT EXISTS ( \
                 SELECT 1 FROM distribution_subbanks AS m \
                 WHERE m.distribution_id = d.id AND m.subbank_id = s.id))",
    )
    .bind::<Integer, _>(added.get())
    .bind::<Integer, _>(added.get())
    .execute(c)
    .map(|_| ())
}

fn versions_of(c: &mut SqliteConnection, id: DistributionId) -> QueryResult<Vec<String>> {
    releases::table
        .filter(releases::distribution_id.eq(id.get()))
        .order(releases::seq.asc())
        .select(releases::version)
        .load(c)
}

/// 書き出した記録を1件足したあとの下書き（`TR-PKG-60`）。 下書きの番号で書き出したなら、
/// 次の番号の下書きを出しておく。 別の番号で書き出したなら、下書きは変えない。
pub(super) fn after_release(
    c: &mut SqliteConnection,
    id: DistributionId,
    version: &str,
) -> QueryResult<()> {
    let draft = distributions::table
        .find(id.get())
        .select(distributions::draft_version)
        .first::<Option<String>>(c)
        .optional()?
        .flatten();
    if draft.as_deref() != Some(version) {
        return Ok(());
    }
    let taken = versions_of(c, id)?;
    let taken: Vec<&str> = taken.iter().map(String::as_str).collect();
    let next = next_version(Some(version), &taken);
    diesel::update(distributions::table.find(id.get()))
        .set(distributions::draft_version.eq(Some(next)))
        .execute(c)
        .map(|_| ())
}

/// 書き出した時点の採用テイクを残す（`TR-PKG-61`）。
pub(super) fn snapshot_adoptions(c: &mut SqliteConnection, release_seq: i32) -> QueryResult<()> {
    diesel::sql_query(
        "INSERT INTO release_adoptions (release_seq, row_id, take_id) \
         SELECT ?, row_id, take_id FROM adopted_takes",
    )
    .bind::<Integer, _>(release_seq)
    .execute(c)
    .map(|_| ())
}

fn exists(c: &mut SqliteConnection, id: DistributionId) -> QueryResult<bool> {
    diesel::select(diesel::dsl::exists(distributions::table.find(id.get()))).get_result(c)
}

fn archive_owner(
    c: &mut SqliteConnection,
    archive_name: &str,
    except: Option<DistributionId>,
) -> QueryResult<Option<DistributionId>> {
    distributions::table
        .filter(distributions::archive_name.eq(archive_name))
        .filter(distributions::id.ne(except.map_or(0, DistributionId::get)))
        .select(distributions::id)
        .first::<i32>(c)
        .optional()
        .map(|o| o.map(DistributionId::new))
}

#[derive(QueryableByName)]
struct Count {
    #[diesel(sql_type = BigInt)]
    n: i64,
}

fn revise(
    c: &mut SqliteConnection,
    id: DistributionId,
    change: &DistributionRevision<'_>,
) -> QueryResult<DistributionAnswer<()>> {
    if !exists(c, id)? {
        return Ok(Err(DistributionRefusal::UnknownDistribution));
    }
    if let Some(archive_name) = change.archive_name
        && let Some(existing) = archive_owner(c, archive_name, Some(id))?
    {
        return Ok(Err(DistributionRefusal::Conflict { existing }));
    }
    if let Some(members) = change.subbanks {
        for m in members {
            let known: bool = diesel::select(diesel::dsl::exists(
                subbanks::table.find(m.subbank.get()),
            ))
            .get_result(c)?;
            if !known {
                return Ok(Err(DistributionRefusal::UnknownSubbank));
            }
        }
    }
    if let Some(version) = change.draft_version
        && versions_of(c, id)?.iter().any(|v| v == version)
    {
        return Ok(Err(DistributionRefusal::VersionTaken));
    }

    if let Some(name) = change.name {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::name.eq(name))
            .execute(c)?;
    }
    if let Some(archive_name) = change.archive_name {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::archive_name.eq(Some(archive_name)))
            .execute(c)?;
    }
    if let Some(terms) = change.terms {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::terms.eq(Some(terms)))
            .execute(c)?;
    } else if change.clear_terms {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::terms.eq(None::<String>))
            .execute(c)?;
    }
    if let Some(version) = change.draft_version {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::draft_version.eq(Some(version)))
            .execute(c)?;
    } else if change.clear_draft_version {
        diesel::update(distributions::table.find(id.get()))
            .set(distributions::draft_version.eq(None::<String>))
            .execute(c)?;
    }
    if let Some(members) = change.subbanks {
        diesel::delete(
            distribution_subbanks::table.filter(distribution_subbanks::distribution_id.eq(id.get())),
        )
        .execute(c)?;
        for m in members {
            diesel::insert_into(distribution_subbanks::table)
                .values((
                    distribution_subbanks::distribution_id.eq(id.get()),
                    distribution_subbanks::subbank_id.eq(m.subbank.get()),
                    distribution_subbanks::range_low.eq(m.edited_range.map(|r| r.low)),
                    distribution_subbanks::range_high.eq(m.edited_range.map(|r| r.high)),
                ))
                .execute(c)?;
        }
    }
    Ok(Ok(()))
}

fn duplicate(
    c: &mut SqliteConnection,
    source: DistributionId,
) -> QueryResult<DistributionAnswer<DistributionId>> {
    let all = load(c)?;
    let Some(from) = all.iter().find(|d| d.id == source) else {
        return Ok(Err(DistributionRefusal::UnknownDistribution));
    };
    let names: Vec<&str> = all.iter().map(|d| d.name.as_str()).collect();
    let archives: Vec<&str> = all.iter().filter_map(|d| d.archive_name.as_deref()).collect();
    let name = copy_name(&from.name, &names);
    // 元の名前が決まっていなければ、既定の名前に番号を付けるしかない。 開くときに埋めるので、通常は起きない。
    let archive_name = copy_archive_name(from.archive_name.as_deref().unwrap_or("voice"), &archives);
    let ordinal = distributions::table
        .select(diesel::dsl::max(distributions::ordinal))
        .first::<Option<i32>>(c)?
        .map_or(0, |m| m + 1);

    diesel::insert_into(distributions::table)
        .values((
            distributions::name.eq(&name),
            distributions::archive_name.eq(Some(&archive_name)),
            distributions::terms.eq(&from.terms),
            // 写した配布物はまだ書き出していない。 最初の版から数える（`TR-PKG-60`）。
            distributions::draft_version.eq(Some(FIRST_VERSION)),
            distributions::ordinal.eq(ordinal),
        ))
        .execute(c)?;
    let id = distributions::table
        .select(distributions::id)
        .order(distributions::id.desc())
        .first::<i32>(c)?;
    for m in &from.subbanks {
        diesel::insert_into(distribution_subbanks::table)
            .values((
                distribution_subbanks::distribution_id.eq(id),
                distribution_subbanks::subbank_id.eq(m.subbank.get()),
                distribution_subbanks::range_low.eq(m.edited_range.map(|r| r.low)),
                distribution_subbanks::range_high.eq(m.edited_range.map(|r| r.high)),
            ))
            .execute(c)?;
    }
    Ok(Ok(DistributionId::new(id)))
}

impl Ledger {
    /// 配布物を足した順で（`TR-PKG-58`）。 少なくとも1つ。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn distributions(&mut self) -> Result<Vec<DistributionRecord>> {
        load(&mut self.conn).map_err(db("distributions"))
    }

    /// 音源ルートフォルダ名がまだ決まっていない配布物へ、既定の名前を入れる。
    ///
    /// 既定は表示名から作る（`DEC-PKG-008`）ので、SQL の移行では作れない。 決まっている
    /// 配布物には触れない。 何も無ければ何も書かない。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    pub fn fill_default_archive_name(&mut self, archive_name: &str) -> Result<()> {
        self.conn
            .transaction(|c| {
                let undecided: Vec<i32> = distributions::table
                    .filter(distributions::archive_name.is_null())
                    .order((distributions::ordinal.asc(), distributions::id.asc()))
                    .select(distributions::id)
                    .load(c)?;
                let mut taken: Vec<String> = distributions::table
                    .filter(distributions::archive_name.is_not_null())
                    .select(distributions::archive_name)
                    .load::<Option<String>>(c)?
                    .into_iter()
                    .flatten()
                    .collect();
                for id in undecided {
                    // 2つ目以降が未決になることは無いが、重なる名前を入れて開けなくなるよりは番号を付ける。
                    let borrowed: Vec<&str> = taken.iter().map(String::as_str).collect();
                    let name = if borrowed.contains(&archive_name) {
                        copy_archive_name(archive_name, &borrowed)
                    } else {
                        archive_name.to_owned()
                    };
                    diesel::update(distributions::table.find(id))
                        .set(distributions::archive_name.eq(Some(&name)))
                        .execute(c)?;
                    taken.push(name);
                }
                Ok(())
            })
            .map_err(db("fill_default_archive_name"))
    }

    /// 配布物を写して足す（`TR-PKG-58`）。 入れる枠・担当音域・規約の上書きを引き継ぐ。
    /// 名前とフォルダ名は、元に番号を付けたもの。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self))]
    pub fn duplicate_distribution(
        &mut self,
        source: DistributionId,
    ) -> Result<DistributionAnswer<DistributionId>> {
        self.conn
            .transaction(|c| duplicate(c, source))
            .map_err(db("duplicate_distribution"))
    }

    /// 配布物の欄を直す（`TR-PKG-58`〜`TR-PKG-60`）。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self, change))]
    pub fn revise_distribution(
        &mut self,
        id: DistributionId,
        change: &DistributionRevision<'_>,
    ) -> Result<DistributionAnswer<()>> {
        self.conn
            .transaction(|c| revise(c, id, change))
            .map_err(db("revise_distribution"))
    }

    /// 配布物を消す（`TR-PKG-58`）。 書き出しの記録は残る（`TR-PKG-44`）。 最後の1つは消せない。
    ///
    /// # Errors
    ///
    /// 台帳を書けない。
    #[tracing::instrument(skip(self))]
    pub fn remove_distribution(&mut self, id: DistributionId) -> Result<DistributionAnswer<()>> {
        self.conn
            .transaction(|c| {
                if !exists(c, id)? {
                    return Ok(Err(DistributionRefusal::UnknownDistribution));
                }
                let n: i64 = distributions::table.count().get_result(c)?;
                if n <= 1 {
                    return Ok(Err(DistributionRefusal::LastOne));
                }
                diesel::delete(distributions::table.find(id.get())).execute(c)?;
                Ok(Ok(()))
            })
            .map_err(db("remove_distribution"))
    }

    /// その配布物で書き出した番号。 書き出した順（`TR-PKG-60`）。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn released_versions(&mut self, id: DistributionId) -> Result<Vec<String>> {
        versions_of(&mut self.conn, id).map_err(db("released_versions"))
    }

    /// 最後に書き出したあとで採用テイクが変わった行の数（`TR-PKG-61`）。
    ///
    /// 基準は、その配布物の最後の書き出しが残した採用テイク（`release_adoptions`）。
    /// 数えるのは、いま入れている枠の、除外していない行だけ。 採用テイクが基準と違う
    /// （新しく録った、選び直した、採用が無くなった）行を数える。
    ///
    /// まだ書き出していない配布物、基準を残していない古い記録（この移行より前の書き出し）の
    /// ときは `None`。 0 件と「分からない」を取り違えない。
    ///
    /// # Errors
    ///
    /// 台帳を読めない。
    pub fn rows_changed_since_release(&mut self, id: DistributionId) -> Result<Option<usize>> {
        let c = &mut self.conn;
        (|| -> QueryResult<Option<usize>> {
            let Some(seq) = releases::table
                .filter(releases::distribution_id.eq(id.get()))
                .select(diesel::dsl::max(releases::seq))
                .first::<Option<i32>>(c)?
            else {
                return Ok(None);
            };
            let has_baseline: bool = diesel::select(diesel::dsl::exists(
                release_adoptions::table.filter(release_adoptions::release_seq.eq(seq)),
            ))
            .get_result(c)?;
            if !has_baseline {
                return Ok(None);
            }
            let counted: Count = diesel::sql_query(
                "SELECT COUNT(*) AS n FROM rows AS r \
                 JOIN distribution_subbanks AS m \
                   ON m.subbank_id = r.subbank_id AND m.distribution_id = ? \
                 LEFT JOIN adopted_takes AS a ON a.row_id = r.id \
                 LEFT JOIN release_adoptions AS s ON s.release_seq = ? AND s.row_id = r.id \
                 WHERE r.state <> ? AND a.take_id IS NOT s.take_id",
            )
            .bind::<Integer, _>(id.get())
            .bind::<Integer, _>(seq)
            .bind::<diesel::sql_types::Text, _>(RowState::Excluded.as_str())
            .get_result(c)?;
            Ok(Some(usize::try_from(counted.n).unwrap_or(0)))
        })()
        .map_err(db("rows_changed_since_release"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::Method;
    use crate::db::{FinalizedTake, SessionSnapshot};
    use crate::inventory::UnitSet;
    use crate::release::{NewRelease, Validation, content_hash};
    use crate::reclist::{Row as ReclistRow, generate_single};
    use koeru_model::subbank::VoiceColor;

    fn rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }

    /// 単独音の枠が 55 / 62 / 69 にある台帳。
    fn ledger() -> (Ledger, Vec<ReclistRow>, Vec<SubbankId>) {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist_for_tones(
            &list,
            &rules(),
            Method::Single,
            Some("single"),
            &[55, 62, 69],
        )
        .expect("書き込める");
        let ids = l.subbanks().expect("読める").iter().map(|s| s.id).collect();
        (l, list, ids)
    }

    fn only(l: &mut Ledger) -> DistributionRecord {
        let mut all = l.distributions().expect("読める");
        assert_eq!(all.len(), 1);
        all.remove(0)
    }

    fn members(d: &DistributionRecord) -> Vec<SubbankId> {
        d.subbanks.iter().map(|m| m.subbank).collect()
    }

    fn release(distribution: DistributionId, version: &str) -> NewRelease {
        NewRelease {
            distribution,
            version: version.into(),
            method: crate::project::Method::Single,
            alias_count: 1,
            validation: Validation::Passed,
            oto_hash: content_hash(b"oto"),
            terms_hash: content_hash(b"terms"),
            released_at: "2026-11-01T00:00:00Z".into(),
        }
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

    /// `TR-PKG-58`。新しい台帳は「すべて」を1つ持ち、いまある枠をすべて入れている。
    #[test]
    fn 新しい台帳は_すべて_を1つ持ち枠をすべて入れている() {
        let (mut l, _, ids) = ledger();
        let d = only(&mut l);
        assert_eq!(d.name, "すべて");
        assert_eq!(members(&d), ids);
        assert!(d.subbanks.iter().all(|m| m.edited_range.is_none()));
        assert_eq!(d.terms, None);
        // 書き出していないので、最初の版から数える（`TR-PKG-60`）。
        assert_eq!(d.draft_version.as_deref(), Some("1.0.0"));
        assert_eq!(d.archive_name, None, "既定の名前は開くときに入れる");

        l.fill_default_archive_name("Koeru").expect("入れられる");
        assert_eq!(only(&mut l).archive_name.as_deref(), Some("Koeru"));
        l.fill_default_archive_name("Other").expect("入れられる");
        assert_eq!(
            only(&mut l).archive_name.as_deref(),
            Some("Koeru"),
            "決まっている名前は変えない"
        );
    }

    /// 枠をすべて入れている配布物だけが、増えた枠を入れる。 外したものを黙って戻さない。
    #[test]
    fn 枠を足すと_すべてだけが増え_外した配布物は増えない() {
        let (mut l, list, ids) = ledger();
        let all = only(&mut l).id;
        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");
        let trimmed = [PackagedSubbankRecord {
            subbank: ids[0],
            edited_range: None,
        }];
        l.revise_distribution(
            copy,
            &DistributionRevision {
                subbanks: Some(&trimmed),
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("直せる");

        let soft = VoiceColor::parse("soft").expect("空でない");
        let added = l
            .add_subbank(&crate::db::subbank::NewSubbank {
                voice_color: Some(&soft),
                method: Method::Single,
                preset_id: "single",
                tone: 60,
                after: None,
                list: &list,
                rules: &rules(),
            })
            .expect("書ける")
            .expect("足せる");

        let all = l.distributions().expect("読める");
        assert_eq!(all[0].subbanks.len(), 4);
        assert!(members(&all[0]).contains(&added));
        assert_eq!(members(&all[1]), [ids[0]]);
    }

    /// `TR-PKG-58`。写すと、枠・担当音域・規約の上書きを引き継ぎ、名前とフォルダ名は重ならない。
    #[test]
    fn 写すと中身を引き継ぎ_名前とフォルダ名は重ならない() {
        let (mut l, _, ids) = ledger();
        l.fill_default_archive_name("Koeru").expect("入れられる");
        let all = only(&mut l).id;
        let kept = [
            PackagedSubbankRecord {
                subbank: ids[0],
                edited_range: Some(ToneRange { low: 0, high: 70 }),
            },
            PackagedSubbankRecord {
                subbank: ids[2],
                edited_range: None,
            },
        ];
        l.revise_distribution(
            all,
            &DistributionRevision {
                subbanks: Some(&kept),
                terms: Some("この配布物の規約"),
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("直せる");

        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");
        let again = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");

        let d = l.distributions().expect("読める");
        assert_eq!(d.len(), 3, "足した順");
        assert_eq!(d[1].id, copy);
        assert_eq!(d[1].name, "すべて のコピー");
        assert_eq!(d[1].archive_name.as_deref(), Some("Koeru-2"));
        assert_eq!(d[1].subbanks, d[0].subbanks, "枠と担当音域を引き継ぐ");
        assert_eq!(d[1].terms.as_deref(), Some("この配布物の規約"));
        assert_eq!(d[1].draft_version.as_deref(), Some("1.0.0"));
        assert_eq!(d[2].id, again);
        assert_eq!(d[2].name, "すべて のコピー 2");
        assert_eq!(d[2].archive_name.as_deref(), Some("Koeru-3"));
    }

    /// `TR-PKG-58`。フォルダ名は配布物のあいだで重ならない。 断るときは何も書かない。
    #[test]
    fn フォルダ名が重なる直しは断り何も書かない() {
        let (mut l, _, _) = ledger();
        l.fill_default_archive_name("Koeru").expect("入れられる");
        let all = only(&mut l).id;
        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");

        let refused = l
            .revise_distribution(
                copy,
                &DistributionRevision {
                    name: Some("変えた名前"),
                    archive_name: Some("Koeru"),
                    ..DistributionRevision::default()
                },
            )
            .expect("書ける");
        assert_eq!(
            refused,
            Err(DistributionRefusal::Conflict { existing: all })
        );
        assert_eq!(l.distributions().expect("読める")[1].name, "すべて のコピー");

        // 自分の名前へ書き直すのは重なりではない。
        l.revise_distribution(
            copy,
            &DistributionRevision {
                archive_name: Some("Koeru-2"),
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("直せる");
    }

    /// `TR-PKG-58`、`TR-PKG-44`。最後の1つは消せない。 消しても書き出しの記録は残る。
    #[test]
    fn 最後の1つは消せず_消しても記録は残る() {
        let (mut l, _, _) = ledger();
        let all = only(&mut l).id;
        assert_eq!(
            l.remove_distribution(all).expect("書ける"),
            Err(DistributionRefusal::LastOne)
        );

        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");
        l.record_release(&release(copy, "1.0.0"), "zip")
            .expect("記録できる");
        l.remove_distribution(copy)
            .expect("書ける")
            .expect("消せる");
        assert_eq!(l.distributions().expect("読める").len(), 1);

        let kept = l.releases().expect("読める");
        assert_eq!(kept.len(), 1, "記録は残る");
        assert_eq!(kept[0].distribution, copy, "消した配布物の番号を持ったまま");
        assert_eq!(
            l.remove_distribution(copy).expect("書ける"),
            Err(DistributionRefusal::UnknownDistribution)
        );
    }

    /// `TR-PKG-60`。版は配布物ごとに数え、下書きの番号で書き出すと次の番号を出しておく。
    #[test]
    fn 下書きで書き出すと次の番号を出しておく() {
        let (mut l, _, _) = ledger();
        let all = only(&mut l).id;
        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");

        l.record_release(&release(all, "1.0.0"), "zip")
            .expect("記録できる");
        let d = l.distributions().expect("読める");
        assert_eq!(d[0].draft_version.as_deref(), Some("1.0.1"));
        assert_eq!(
            d[1].draft_version.as_deref(),
            Some("1.0.0"),
            "別の配布物の版は動かない"
        );

        // 書き出し済みの番号を選んで出し直すと、同じ番号で記録が1件増え、下書きは動かない。
        l.record_release(&release(all, "1.0.0"), "zip")
            .expect("記録できる");
        assert_eq!(
            l.released_versions(all).expect("読める"),
            ["1.0.0", "1.0.0"]
        );
        assert_eq!(
            l.distributions().expect("読める")[0].draft_version.as_deref(),
            Some("1.0.1")
        );
        assert_eq!(l.released_versions(copy).expect("読める"), Vec::<String>::new());
    }

    /// `TR-PKG-60`。下書きの番号は、書き出した番号と重ねず、消せる。
    #[test]
    fn 下書きの番号は書き出した番号と重ねず_消せる() {
        let (mut l, _, _) = ledger();
        let all = only(&mut l).id;
        l.record_release(&release(all, "2.0"), "zip")
            .expect("記録できる");

        let refused = l
            .revise_distribution(
                all,
                &DistributionRevision {
                    draft_version: Some("2.0"),
                    ..DistributionRevision::default()
                },
            )
            .expect("書ける");
        assert_eq!(refused, Err(DistributionRefusal::VersionTaken));

        l.revise_distribution(
            all,
            &DistributionRevision {
                draft_version: Some("3.0"),
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("直せる");
        assert_eq!(only(&mut l).draft_version.as_deref(), Some("3.0"));

        l.revise_distribution(
            all,
            &DistributionRevision {
                clear_draft_version: true,
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("消せる");
        assert_eq!(only(&mut l).draft_version, None);
        assert_eq!(l.released_versions(all).expect("読める"), ["2.0"], "書き出し済みは消えない");
    }

    /// `TR-PKG-61`。基準は最後の書き出しが残した採用テイク。 入れた枠の行だけを数える。
    #[test]
    fn 書き出したあとに採用が変わった行を数える() {
        let (mut l, list, ids) = ledger();
        let all = only(&mut l).id;
        assert_eq!(
            l.rows_changed_since_release(all).expect("読める"),
            None,
            "まだ書き出していない"
        );

        let rows_of = |l: &mut Ledger, id: SubbankId| -> Vec<String> {
            use crate::schema::rows;
            rows::table
                .filter(rows::subbank_id.eq(id.get()))
                .order(rows::ordinal.asc())
                .select(rows::id)
                .load(&mut l.conn)
                .expect("読める")
        };
        let low = rows_of(&mut l, ids[0]);
        let mid = rows_of(&mut l, ids[1]);
        assert!(low.len() >= 2 && !mid.is_empty() && !list.is_empty());
        take(&mut l, &low[0], "audio/a1.wav");
        take(&mut l, &mid[0], "audio/b1.wav");

        // 外した枠の行は数えない配布物を用意する。
        let copy = l
            .duplicate_distribution(all)
            .expect("書ける")
            .expect("写せる");
        let only_low = [PackagedSubbankRecord {
            subbank: ids[0],
            edited_range: None,
        }];
        l.revise_distribution(
            copy,
            &DistributionRevision {
                subbanks: Some(&only_low),
                ..DistributionRevision::default()
            },
        )
        .expect("書ける")
        .expect("直せる");

        l.record_release(&release(all, "1.0.0"), "zip")
            .expect("記録できる");
        l.record_release(&release(copy, "1.0.0"), "zip")
            .expect("記録できる");
        assert_eq!(l.rows_changed_since_release(all).expect("読める"), Some(0));
        assert_eq!(l.rows_changed_since_release(copy).expect("読める"), Some(0));

        // 新しく録った（低い枠）、選び直した（低い枠）、新しく録った（中の枠）。
        take(&mut l, &low[1], "audio/a2.wav");
        take(&mut l, &low[0], "audio/a1b.wav");
        take(&mut l, &mid[0], "audio/b1b.wav");
        assert_eq!(l.rows_changed_since_release(all).expect("読める"), Some(3));
        assert_eq!(
            l.rows_changed_since_release(copy).expect("読める"),
            Some(2),
            "入れていない枠の行は数えない"
        );

        // 選び直しを戻せば、渡した ZIP と同じ採用に戻る（数えるのは「違う行」で、手数ではない）。
        let original: i32 = crate::schema::takes::table
            .filter(crate::schema::takes::rel_path.eq("audio/a1.wav"))
            .select(crate::schema::takes::id)
            .first(&mut l.conn)
            .expect("ある");
        l.adopt_take(&low[0], original).expect("採用できる");
        assert_eq!(
            l.rows_changed_since_release(copy).expect("読める"),
            Some(1),
            "新しく録った行だけが残る"
        );
    }

    /// 基準を残していない古い記録では「分からない」。 0 件と取り違えない。
    #[test]
    fn 基準を持たない古い記録は数えない() {
        let (mut l, _, _) = ledger();
        let all = only(&mut l).id;
        l.record_release(&release(all, "1.0.0"), "zip")
            .expect("記録できる");
        diesel::sql_query("DROP TRIGGER release_adoptions_reject_delete")
            .execute(&mut l.conn)
            .expect("外せる");
        diesel::sql_query("DELETE FROM release_adoptions")
            .execute(&mut l.conn)
            .expect("消せる");
        assert_eq!(l.rows_changed_since_release(all).expect("読める"), None);
    }
}
