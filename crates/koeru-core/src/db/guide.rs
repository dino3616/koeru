//! ガイドのテンポ（`TR-REC-23`, `DEC-REC-012`）。
//!
//! 音源に1つ持ち、収録セッションは始めたときの値を写して持つ（`TR-REC-30`）。
//! テンポを変えたらセッションを切るので、1つのセッションの中でテンポは変わらない。
//! 録ったテイクがどのテンポで録られたかは、属するセッションから読む。

use diesel::prelude::*;
use diesel::sqlite::SqliteConnection;
use koeru_model::guide::GuideTempo;

use super::{Ledger, Result, db};
use crate::schema::{guide_settings, sessions, takes};

/// 台帳にあるテンポ。 行が無ければ既定。
///
/// 範囲の外の値は、範囲を広げた版のアプリが書いたもの。 この版では既定として扱い、
/// 書き換えない——その版へ戻ったときに本人の値が残る。
pub(super) fn current_tempo(c: &mut SqliteConnection) -> QueryResult<GuideTempo> {
    let stored = guide_settings::table
        .find(1)
        .select(guide_settings::tempo_bpm)
        .first::<i32>(c)
        .optional()?;
    Ok(stored.and_then(GuideTempo::new).unwrap_or_default())
}

/// 前のセッションと入力の条件が同じで、テンポだけ違うセッションを始める。
///
/// **`sessions` に列を足したら、ここにも足す。** 写し忘れた条件は、切ったあとの
/// セッションから黙って消える。 試験（`切ったセッションはテンポと開始時刻のほかは前と同じ`）が
/// 全列を突き合わせる。
fn restart_session(c: &mut SqliteConnection, prev: i32, at: &str, bpm: i32) -> QueryResult<i32> {
    let (
        device_id,
        sample_rate_hz,
        channels,
        effects_state,
        route,
        source_channel,
        master_rate_hz,
        resampler,
        upstream_conversion,
    ) = sessions::table
        .find(prev)
        .select((
            sessions::device_id,
            sessions::sample_rate_hz,
            sessions::channels,
            sessions::effects_state,
            sessions::route,
            sessions::source_channel,
            sessions::master_rate_hz,
            sessions::resampler,
            sessions::upstream_conversion,
        ))
        .first::<(String, i32, i32, String, String, i32, i32, String, String)>(c)?;
    diesel::insert_into(sessions::table)
        .values((
            sessions::started_at.eq(at),
            sessions::device_id.eq(device_id),
            sessions::sample_rate_hz.eq(sample_rate_hz),
            sessions::channels.eq(channels),
            sessions::effects_state.eq(effects_state),
            sessions::route.eq(route),
            sessions::source_channel.eq(source_channel),
            sessions::master_rate_hz.eq(master_rate_hz),
            sessions::resampler.eq(resampler),
            sessions::upstream_conversion.eq(upstream_conversion),
            sessions::guide_tempo_bpm.eq(Some(bpm)),
        ))
        .execute(c)?;
    sessions::table
        .select(sessions::id)
        .order(sessions::id.desc())
        .first::<i32>(c)
}

impl Ledger {
    /// いまのガイドのテンポ。 本人が変えていなければ既定。
    pub fn guide_tempo(&mut self) -> Result<GuideTempo> {
        current_tempo(&mut self.conn).map_err(db("guide_tempo"))
    }

    /// ガイドのテンポを変える。
    ///
    /// `session` に開いている収録セッションを渡すと、テンポが変わったときだけそれを切り、
    /// 新しく始めたセッションの ID を返す（`TR-REC-30`）。 値が今と同じなら何も書かず、
    /// セッションも切らない。 テンポの保存と切り替えは1つのトランザクションで行う。
    pub fn set_guide_tempo(
        &mut self,
        tempo: GuideTempo,
        session: Option<i32>,
        at: &str,
    ) -> Result<Option<i32>> {
        self.conn
            .transaction(|c| {
                if current_tempo(c)? == tempo {
                    return Ok(None);
                }
                let bpm = i32::from(tempo.bpm());
                diesel::insert_into(guide_settings::table)
                    .values((guide_settings::id.eq(1), guide_settings::tempo_bpm.eq(bpm)))
                    .on_conflict(guide_settings::id)
                    .do_update()
                    .set(guide_settings::tempo_bpm.eq(bpm))
                    .execute(c)?;
                match session {
                    Some(prev) => restart_session(c, prev, at, bpm).map(Some),
                    None => Ok(None),
                }
            })
            .map_err(db("set_guide_tempo"))
    }

    /// 収録セッションが録ったときのガイドのテンポ（BPM）。
    ///
    /// テンポを記録する前に始めたセッションは `None`。 既定の値で埋めない
    /// （`DEC-REC-012`）。
    pub fn session_guide_tempo(&mut self, session: i32) -> Result<Option<i32>> {
        sessions::table
            .find(session)
            .select(sessions::guide_tempo_bpm)
            .first::<Option<i32>>(&mut self.conn)
            .map_err(db("session_guide_tempo"))
    }

    /// テイクが録られたときのガイドのテンポ（BPM、`TR-REC-30`）。 属する収録セッションから読む。
    pub fn take_guide_tempo(&mut self, take: i32) -> Result<Option<i32>> {
        takes::table
            .inner_join(sessions::table)
            .filter(takes::id.eq(take))
            .select(sessions::guide_tempo_bpm)
            .first::<Option<i32>>(&mut self.conn)
            .map_err(db("take_guide_tempo"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{FinalizedTake, SessionSnapshot};
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn snapshot() -> SessionSnapshot {
        SessionSnapshot {
            started_at: "2026-11-01T00:00:00Z".into(),
            device_id: "test".into(),
            sample_rate_hz: 48_000,
            channels: 2,
            effects_state: "clean".into(),
            route: "test".into(),
            source_channel: 1,
            master_rate_hz: 44_100,
            resampler: "test".into(),
            upstream_conversion: "unknown".into(),
        }
    }

    fn ledger() -> (Ledger, Vec<String>) {
        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 3).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        (l, list.into_iter().map(|r| r.id).collect())
    }

    fn take(l: &mut Ledger, row: &str, session: i32, n: u32) -> i32 {
        l.commit_take(&FinalizedTake {
            row_id: row.into(),
            session_id: session,
            rel_path: format!("masters/{row}_{n}.wav"),
            frames: 44_100,
            recorded_at: "2026-11-01T00:00:01Z".into(),
        })
        .expect("テイクを書ける")
    }

    fn tempo(bpm: i32) -> GuideTempo {
        GuideTempo::new(bpm).expect("範囲の中")
    }

    #[test]
    fn 変えていなければ既定の_120_で_セッションにもそれが残る() {
        let (mut l, _) = ledger();
        assert_eq!(l.guide_tempo().expect("読める"), GuideTempo::DEFAULT);
        let s = l.start_session(&snapshot()).expect("始められる");
        assert_eq!(l.session_guide_tempo(s).expect("読める"), Some(120));
    }

    #[test]
    fn 変えた値は残り_次に始めるセッションに写る() {
        let (mut l, _) = ledger();
        assert_eq!(
            l.set_guide_tempo(tempo(90), None, "2026-11-01T00:00:00Z")
                .expect("書ける"),
            None,
            "開いているセッションが無ければ切らない"
        );
        assert_eq!(l.guide_tempo().expect("読める"), tempo(90));
        let s = l.start_session(&snapshot()).expect("始められる");
        assert_eq!(l.session_guide_tempo(s).expect("読める"), Some(90));
    }

    /// 録ったテイクは、録ったときのテンポを持ち続ける（`DEC-REC-012`）。
    #[test]
    fn 変えたらセッションを切り_テイクは録ったときのテンポを持つ() {
        let (mut l, rows) = ledger();
        let before = l.start_session(&snapshot()).expect("始められる");
        let first = take(&mut l, &rows[0], before, 1);

        let after = l
            .set_guide_tempo(tempo(90), Some(before), "2026-11-01T00:10:00Z")
            .expect("書ける")
            .expect("テンポが変わったので切る");
        assert_ne!(after, before);
        let second = take(&mut l, &rows[1], after, 1);

        assert_eq!(l.take_guide_tempo(first).expect("読める"), Some(120));
        assert_eq!(l.take_guide_tempo(second).expect("読める"), Some(90));
        assert_eq!(l.session_guide_tempo(before).expect("読める"), Some(120));
    }

    #[test]
    fn 同じ値なら何も書かずセッションも切らない() {
        let (mut l, _) = ledger();
        let s = l.start_session(&snapshot()).expect("始められる");
        let r0 = l.revision().expect("読める");
        assert_eq!(
            l.set_guide_tempo(GuideTempo::DEFAULT, Some(s), "2026-11-01T00:10:00Z")
                .expect("書ける"),
            None
        );
        assert_eq!(l.revision().expect("読める"), r0, "版も進めない");
    }

    #[derive(QueryableByName)]
    struct Column {
        #[diesel(sql_type = diesel::sql_types::Text)]
        name: String,
    }

    #[derive(QueryableByName)]
    struct Same {
        #[diesel(sql_type = diesel::sql_types::Integer)]
        same: i32,
    }

    /// 入力の条件を写し忘れると、切ったあとのセッションから黙って消える（`restart_session`）。
    #[test]
    fn 切ったセッションはテンポと開始時刻のほかは前と同じ() {
        let (mut l, _) = ledger();
        let before = l.start_session(&snapshot()).expect("始められる");
        let after = l
            .set_guide_tempo(tempo(150), Some(before), "2026-11-01T00:10:00Z")
            .expect("書ける")
            .expect("切る");

        let columns = diesel::sql_query("SELECT name FROM pragma_table_info('sessions')")
            .load::<Column>(&mut l.conn)
            .expect("列を読める");
        let compared: Vec<String> = columns
            .into_iter()
            .map(|c| c.name)
            .filter(|n| !["id", "started_at", "guide_tempo_bpm"].contains(&n.as_str()))
            .collect();
        assert!(compared.len() >= 9, "比べる列 {compared:?}");
        for name in compared {
            let same = diesel::sql_query(format!(
                "SELECT (a.{name} IS b.{name}) AS same FROM sessions a, sessions b \
                 WHERE a.id = {before} AND b.id = {after}"
            ))
            .get_result::<Same>(&mut l.conn)
            .expect("比べられる");
            assert_eq!(same.same, 1, "{name} を写していない");
        }
    }
}
