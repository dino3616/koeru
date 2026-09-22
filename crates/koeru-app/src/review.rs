//! 確認キューを台帳と繋ぐ（`TR-ALN-25`〜`27`, `TR-ALN-30`）。
//!
//! 状態機械そのものは [`koeru_align::review::ReviewQueue`] が持ち、
//! その正本は `specs/requirements/align-review.fsl`。ここがするのは、
//! 台帳に置いた状態からキューを組み直すことと、遷移の結果を台帳へ書き戻すことだけ。
//!
//! # 正本は台帳
//!
//! oto はプロジェクトのデータで、DB を正とする（`TR-PKG-40`）。
//! キューは開いている間だけ持つ写しで、遷移の可否を判定する係。
//! **判定をキューに任せ、値の保存を台帳に任せる。** 両方に判定を書くと、
//! 片方だけが `INV-ALN-001`〜`004` を守る形になる。
//!
//! # 採用テイクだけを見る
//!
//! 非採用の世代は書き出しに出ないので、確認キューにも入れない。
//! 入れると、録り直すたびにキューが伸びて、誰も見ないものが上限を食う。

use std::collections::HashMap;
use std::time::Duration;

use koeru_align::confidence::Confidence;
use koeru_align::review::{Entry, EntryState, ReviewMode, ReviewQueue, Slot};
use koeru_core::db::{Ledger, ReviewStateRow};

use crate::error::Result;

/// 1件を確認するのにかかると見込む時間（`TR-ALN-25`）。
///
/// **実測していない**（`TGT-ALN-007` の note）。`DEC-ALN-003` が上限を合計5分と
/// 決めているので、この値が「個別確認で何件まで見るか」を決めてしまう。
///
/// 直す場所はここ1箇所。 `ReviewQueue` が見積もりを外から受け取る形にしてあるのは、
/// 実測が出たときに呼び出し側だけ直せばよいようにするため。
pub const PER_ITEM: Duration = Duration::from_secs(10);

/// 確認キューのエントリを指す鍵（`TR-ALN-22`, `TR-ALN-25`）。
///
/// **綴りだけでは足りない。** 多音階は音高ごとにフォルダと `oto.ini` を分ける
/// （`TR-RCL-26`）ので、同じ綴りが音高の数だけ並ぶ。綴りだけを鍵にすると
/// キューは最後に読んだ1件しか残さず、落ちたほうは確認もされないまま
/// 配布物へ入る（`INV-ALN-003`）。**`あ` を2音階で録っただけで起きた。**
///
/// 配布物の側では綴りが一意に戻る。 区画の接頭辞・接尾辞が音高を綴りへ
/// 織り込むので（`koeru_package::tree` の `decorate`）、`TR-PKG-19` の
/// 「音源全体で一意」はそちらで満たされる。台帳の中だけが（音高, 綴り）。
///
/// 画面へは [`handle`](Self::handle) の文字列で渡す。 画面は中身を読まずに
/// そのまま返すだけ——分解して組み直させると、綴りに区切り文字が入った
/// ときに画面と台帳で別のエントリを指す。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntryKey {
    tone: i32,
    alias: String,
}

/// 鍵の中で綴りと音高を分ける文字。
///
/// 制御文字を使う。 エイリアスに入りうる文字と重ならないもので、
/// `TR-PKG-18` の綴りの形は制御文字を許していない。
const SEP: char = '\u{1f}';

impl EntryKey {
    /// 音高と綴りから作る。
    pub fn new(tone: i32, alias: impl Into<String>) -> Self {
        Self {
            tone,
            alias: alias.into(),
        }
    }

    /// `oto.ini` に出る綴り。
    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// 収録音高（MIDI）。
    #[must_use]
    pub const fn tone(&self) -> i32 {
        self.tone
    }

    /// キューと画面が持ち回す文字列。
    ///
    /// 綴りを先に置く。 キューは鍵の順に並べるので（`TR-ALN-29`）、
    /// **単音階では並びが綴り順のまま変わらない**——音高を先に置くと、
    /// 音階を増やしただけで `oto.ini` の行順が入れ替わる。
    ///
    /// 音高は3桁に揃える。 MIDI の音高は 0〜127 なので、桁を揃えれば
    /// 文字列の順が数の順と一致する。
    #[must_use]
    pub fn handle(&self) -> String {
        format!("{}{SEP}{:03}", self.alias, self.tone)
    }

    /// [`handle`](Self::handle) の裏。形が違えば `None`。
    ///
    /// 既定へ倒さない。 倒すと、画面から来た壊れた鍵が別のエントリを書き換える。
    #[must_use]
    pub fn parse(handle: &str) -> Option<Self> {
        let (alias, tone) = handle.rsplit_once(SEP)?;
        Some(Self::new(tone.parse().ok()?, alias))
    }
}

/// 台帳からキューを組み直す。
///
/// 返るのは `(キュー, 鍵 → 書き戻す先のテイク)`。
/// 書き戻す先を別に持つのは、キューが鍵にしているのが（音高, 綴り）だけで、
/// どのテイクの行に書くかを知らないため。
pub fn load(ledger: &mut Ledger) -> Result<(ReviewQueue, HashMap<String, i32>)> {
    let s = ledger.review_state()?;
    let mut q = ReviewQueue::restored(
        PER_ITEM,
        ReviewMode::parse(&s.mode),
        s.over_budget,
        s.exported,
    );
    let mut takes = HashMap::new();
    // 鍵に音高を織り込む（`TR-ALN-22`）。 行ごとに1度ずつ引かない——
    // エントリは音源1つで数千あり、1件ずつ問い合わせると開くのが遅くなる。
    let tones = ledger.row_tones()?;
    for e in ledger.adopted_otos()? {
        let state = EntryState::parse(&e.state);
        // 成分を持っているものだけ確信度を載せる。
        //
        // **合成スコアから成分を作り直さない。** 合成は積なので、同じ値を
        // 3つ置くとスコアが3乗になり、主因も常に同じ成分を指す
        // ——保存した 0.4 が 0.064 になり、理由が何であれ「音の変わり目が
        // はっきりしません」と出ていた。
        //
        // 未推定に確信度は無い。 持たせると、録り直した直後に
        // 前の推定の点数が残って見える。
        let confidence = match state {
            EntryState::NotEstimated => None,
            _ => e.parts.map(|p| Confidence {
                path: p.path,
                sharpness: p.sharpness,
                prior: p.prior,
                acoustic: p.acoustic,
            }),
        };
        // 行の音高が引けないものは 0 として置く。 落とすと、そのエントリは
        // 確認もされず、書き出しの関門にも現れないまま配布物へ入る。
        let tone = tones.get(&e.row_id).copied().unwrap_or_default();
        let key = EntryKey::new(tone, e.alias).handle();
        takes.insert(key.clone(), e.take_id);
        q.insert(key, Entry::restored(e.oto, state, confidence, e.pinned));
    }
    Ok((q, takes))
}

/// エントリ1件の状態と固定を台帳へ書く。
///
/// 3つを1つのトランザクションで書く（[`Ledger::put_review_entry`]）。
/// 途中で失敗すると、固定の無い人の値が残って次の再推定に消される。
pub fn save_entry(ledger: &mut Ledger, take_id: i32, alias: &str, entry: &Entry) -> Result<()> {
    ledger.put_review_entry(
        take_id,
        alias,
        &entry.oto,
        entry.state.as_str(),
        entry.pins(),
    )?;
    Ok(())
}

/// キュー全体の進み方を台帳へ書く。
pub fn save_mode(ledger: &mut Ledger, q: &ReviewQueue, over_budget: bool) -> Result<()> {
    ledger.put_review_state(&ReviewStateRow {
        mode: q.mode().as_str().to_owned(),
        over_budget,
        exported: q.is_exported(),
    })?;
    Ok(())
}

/// 画面から来る5値の名前を [`Slot`] へ。
///
/// 知らない名前は `None`。 既定へ倒さない——倒すと、打ち間違えた名前が
/// オフセットを書き換える。
#[must_use]
pub fn slot_of(name: &str) -> Option<Slot> {
    match name {
        "offset" => Some(Slot::Offset),
        "consonant" => Some(Slot::Consonant),
        "cutoff" => Some(Slot::Cutoff),
        "preutterance" => Some(Slot::Preutterance),
        "overlap" => Some(Slot::Overlap),
        _ => None,
    }
}

/// [`Slot`] を画面へ渡す名前へ。[`slot_of`] の裏。
#[must_use]
pub const fn slot_name(s: Slot) -> &'static str {
    match s {
        Slot::Offset => "offset",
        Slot::Consonant => "consonant",
        Slot::Cutoff => "cutoff",
        Slot::Preutterance => "preutterance",
        Slot::Overlap => "overlap",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 名前と_slot_が往復する() {
        for s in Slot::ALL {
            assert_eq!(slot_of(slot_name(s)), Some(s));
        }
    }

    /// 知らない名前は倒さない。
    #[test]
    fn 知らない名前は弾く() {
        assert_eq!(slot_of("offsett"), None);
        assert_eq!(slot_of(""), None);
    }

    /// 鍵は往復する（`DEC-ALN-017`）。
    #[test]
    fn 鍵が往復する() {
        for (tone, alias) in [(60, "か"), (55, "- あ"), (127, "a k"), (0, "aー")] {
            let k = EntryKey::new(tone, alias);
            let back = EntryKey::parse(&k.handle()).expect("読み戻せる");
            assert_eq!(back, k);
            assert_eq!(back.alias(), alias);
            assert_eq!(back.tone(), tone);
        }
    }

    /// 壊れた鍵は既定へ倒さない。
    ///
    /// 倒すと、画面から来た鍵が別のエントリを書き換える。
    #[test]
    fn 形の違う鍵は読まない() {
        assert_eq!(EntryKey::parse("か"), None, "区切りが無い");
        assert_eq!(EntryKey::parse("か\u{1f}C4"), None, "音高が数でない");
        assert_eq!(EntryKey::parse(""), None);
    }

    /// 単音階の並びは綴り順のまま（`TR-ALN-29`）。
    ///
    /// 音高を先に置くと、音階を増やしただけで `oto.ini` の行順が入れ替わる。
    #[test]
    fn 単音階の鍵は綴り順に並ぶ() {
        let mut keys: Vec<String> = ["さ", "あ", "か"]
            .into_iter()
            .map(|a| EntryKey::new(60, a).handle())
            .collect();
        keys.sort();
        let aliases: Vec<String> = keys
            .iter()
            .map(|k| EntryKey::parse(k).expect("読める").alias)
            .collect();
        assert_eq!(aliases, ["あ", "か", "さ"]);
    }

    /// 同じ綴りでも音高が違えば別の鍵（`DEC-ALN-017`）。
    ///
    /// 数の順に並ぶ。 桁を揃えていないと `110` が `62` より前に来る。
    #[test]
    fn 音高が違えば別の鍵() {
        let mut keys: Vec<String> = [62, 110, 55]
            .into_iter()
            .map(|t| EntryKey::new(t, "あ").handle())
            .collect();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), 3, "潰れない");
        let tones: Vec<i32> = keys
            .iter()
            .map(|k| EntryKey::parse(k).expect("読める").tone)
            .collect();
        assert_eq!(tones, [55, 62, 110]);
    }

    /// 採用テイクが1つだけある台帳を作る。返るのは `(台帳, テイク, 行)`。
    fn 一件だけ入れた台帳() -> (Ledger, i32, String) {
        use koeru_core::inventory::UnitSet;
        use koeru_core::reclist::generate_single;

        let mut l = Ledger::open_in_memory().expect("開ける");
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        l.install_reclist(&list, 60).expect("書き込める");
        let sid = l
            .start_session(&koeru_core::db::SessionSnapshot {
                started_at: "2026-09-18T00:00:00Z".into(),
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
        let row = &list[0].id;
        let take = l
            .commit_take(&koeru_core::db::FinalizedTake {
                row_id: row.clone(),
                session_id: sid,
                rel_path: "masters/a_1.wav".into(),
                frames: 44_100,
                recorded_at: "2026-09-18T00:00:01Z".into(),
            })
            .expect("確定できる");
        l.adopt_take(row, take).expect("採用できる");
        (l, take, row.clone())
    }

    /// 台帳へ書いた状態が、開き直したキューにそのまま載る。
    ///
    /// 遷移をやり直さない。 やり直すと、確認し終えたものが未推定へ戻る。
    #[test]
    fn 台帳から確認キューを組み直せる() {
        use koeru_core::db::koeru_oto;

        let (mut l, take, _) = 一件だけ入れた台帳();
        l.put_oto(
            take,
            "か",
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
        .expect("書ける");
        l.set_oto_pins(take, "か", [true, false, false, false, false])
            .expect("書ける");

        let (q, takes) = load(&mut l).expect("組み直せる");
        // 鍵は（音高, 綴り）（`DEC-ALN-017`）。綴りでは引けない。
        let key = EntryKey::new(60, "か").handle();
        assert_eq!(takes.get(&key), Some(&take), "書き戻す先を持っている");
        assert!(q.get("か").is_none(), "綴りだけでは指せない");
        let e = q.get(&key).expect("エントリがある");
        assert_eq!(e.state, EntryState::InQueue);
        assert!(e.is_pinned(Slot::Offset));
        assert!(!e.is_pinned(Slot::Cutoff));
        assert_eq!(q.pending_count(), 1);
        // 確認が残っているうちは書き出せない（`INV-ALN-003`）。
        assert!(q.needs_review());
    }

    /// 成分を持たないエントリには確信度を載せない。
    ///
    /// 合成スコアから成分は作り直せない。 積で畳んであるので、
    /// 同じ値を3つ置くとスコアが3乗になる。
    #[test]
    fn 成分が無ければ確信度を載せない() {
        use koeru_core::db::koeru_oto;

        let (mut l, take, row) = 一件だけ入れた台帳();
        let _ = row;
        // `put_oto` に成分を渡していないので、読み戻しても成分は無い。
        l.put_oto(
            take,
            "き",
            &koeru_oto::Oto {
                offset_ms: 1.0,
                consonant_ms: 2.0,
                cutoff_ms: -3.0,
                preutterance_ms: 1.5,
                overlap_ms: 0.5,
            },
            0.4,
            None,
            false,
        )
        .expect("書ける");

        let (q, _) = load(&mut l).expect("組み直せる");
        let key = EntryKey::new(60, "き").handle();
        assert!(q.get(&key).expect("ある").confidence.is_none());
    }

    /// 成分を渡したものは、合成スコアが往復する。
    #[test]
    fn 成分を持つものは点数が歪まない() {
        use koeru_core::db::{ConfidenceParts, koeru_oto};

        let (mut l, take, _) = 一件だけ入れた台帳();
        let parts = ConfidenceParts {
            path: Some(0.8),
            sharpness: 0.5,
            prior: 1.0,
            acoustic: 0.9,
        };
        let score = 0.8 * 0.5 * 1.0 * 0.9;
        l.put_oto(
            take,
            "き",
            &koeru_oto::Oto {
                offset_ms: 1.0,
                consonant_ms: 2.0,
                cutoff_ms: -3.0,
                preutterance_ms: 1.5,
                overlap_ms: 0.5,
            },
            score,
            Some(&parts),
            false,
        )
        .expect("書ける");

        let (q, _) = load(&mut l).expect("組み直せる");
        let key = EntryKey::new(60, "き").handle();
        let c = q.get(&key).expect("ある").confidence.expect("成分がある");
        assert!(
            (c.score() - score).abs() < 1e-9,
            "点数が歪んでいる: {}",
            c.score()
        );
        // 主因は落ちた成分を指す（`TR-ALN-26` (3)）。
        assert_eq!(
            c.cause(0.6),
            Some(koeru_align::confidence::Cause::Sharpness)
        );
    }
}
