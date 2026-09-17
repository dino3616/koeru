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

/// 確信度を4成分ごと持たない台帳から、合成スコアだけで確信度を組み直す。
///
/// 成分の内訳は保存していない。 台帳が持っているのは合成スコア1つで、
/// `TR-ALN-26` (3) の主因ラベルは開き直したあとには出せない。
/// **成分を 0 で埋めない**——0 は「測ってその値だった」であって「持っていない」ではない。
/// 全成分へ同じ値を置き、`is_complete` が偽になる形（経路確信度なし）にしてある。
fn confidence_from_score(score: f64) -> Confidence {
    Confidence {
        path: None,
        sharpness: score,
        prior: score,
        acoustic: score,
    }
}

/// 台帳からキューを組み直す。
///
/// 返るのは `(キュー, エイリアス → 書き戻す先のテイク)`。
/// 書き戻す先を別に持つのは、キューが鍵にしているのがエイリアスだけで、
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
    for e in ledger.adopted_otos()? {
        let state = EntryState::parse(&e.state);
        let confidence = match state {
            // 未推定に確信度は無い。 持たせると、録り直した直後に
            // 前の推定の点数が残って見える。
            EntryState::NotEstimated => None,
            _ => Some(confidence_from_score(e.confidence)),
        };
        takes.insert(e.alias.clone(), e.take_id);
        q.insert(e.alias, Entry::restored(e.oto, state, confidence, e.pinned));
    }
    Ok((q, takes))
}

/// エントリ1件の状態と固定を台帳へ書く。
pub fn save_entry(ledger: &mut Ledger, take_id: i32, alias: &str, entry: &Entry) -> Result<()> {
    ledger.set_oto_value(take_id, alias, &entry.oto)?;
    ledger.set_oto_state(take_id, alias, entry.state.as_str())?;
    ledger.set_oto_pins(take_id, alias, entry.pins())?;
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

    /// 台帳へ書いた状態が、開き直したキューにそのまま載る。
    ///
    /// 遷移をやり直さない。 やり直すと、確認し終えたものが未推定へ戻る。
    #[test]
    fn 台帳から確認キューを組み直せる() {
        use koeru_core::db::koeru_oto;
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
            false,
        )
        .expect("書ける");
        l.set_oto_pins(take, "か", [true, false, false, false, false])
            .expect("書ける");

        let (q, takes) = load(&mut l).expect("組み直せる");
        assert_eq!(takes.get("か"), Some(&take), "書き戻す先を持っている");
        let e = q.get("か").expect("エントリがある");
        assert_eq!(e.state, EntryState::InQueue);
        assert!(e.is_pinned(Slot::Offset));
        assert!(!e.is_pinned(Slot::Cutoff));
        assert_eq!(q.pending_count(), 1);
        // 確認が残っているうちは書き出せない（`INV-ALN-003`）。
        assert!(q.needs_review());
    }

    /// 開き直したときに経路確信度を持たない。
    ///
    /// 持っていると、退避経路で録ったものと MFA で録ったものが
    /// 台帳から読んだ瞬間に区別できなくなる。
    #[test]
    fn 組み直した確信度は成分を名乗らない() {
        let c = confidence_from_score(0.5);
        assert!(!c.is_complete());
        assert!((c.score() - 0.125).abs() < 1e-9);
    }
}
