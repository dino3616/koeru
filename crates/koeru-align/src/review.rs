//! 確認キューと、人の編集の扱い（`TR-ALN-25`〜`27`, `TR-ALN-30`）。
//!
//! **[`specs/requirements/align-review.fsl`] の写し。** 状態・遷移・不変条件は
//! あちらが正本で、ここはその Rust 実装。**片方だけ直さない。**
//!
//! # 何を守っているか
//!
//! - **人が直した値を、自動が上書きしない**（`INV-ALN-001`）。固定は
//!   エントリ単位ではなく**値単位**（`TR-ALN-30`）——「オフセットだけ直した」が表せる
//! - **固定されている値は、人が入れた値である**（`INV-ALN-002`）
//! - **確認が残っているうちは書き出せない**（`INV-ALN-003`, `REQ-PKG-003`）
//! - **個別確認をやめるのは、上限を超えたときだけ**（`INV-ALN-004`）
//!
//! # 上限は件数ではなく時間
//!
//! `TR-ALN-25` が「切り方を件数から**確認の合計所要時間の上限**に変える」と定め、
//! `DEC-ALN-003` が通常モードを合計5分とした。**方式ごとに件数を持たずに済み、
//! 多音階で上限が膨らむ問題が消える。**
//!
//! 1件あたりの確認時間は未実測（`TGT-ALN-007` の note）。[`ReviewQueue::new`] に
//! 見積もりを渡す形にして、**実測が出たら呼び出し側だけ直せばよい**ようにしてある。
//!
//! [`specs/requirements/align-review.fsl`]: https://github.com/dino3616/koeru/blob/main/specs/requirements/align-review.fsl

use std::collections::BTreeMap;
use std::time::Duration;

use koeru_core::oto::Oto;

use crate::confidence::{Cause, Confidence};

/// 通常モードで人に確認させる合計時間の上限（`DEC-ALN-003`, `TGT-ALN-008`）。
pub const REVIEW_BUDGET: Duration = Duration::from_secs(5 * 60);

/// 5値のどれか。**固定は値単位で持つ**（`TR-ALN-30`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    /// 左ブランク。
    Offset,
    /// 子音部（固定範囲）。
    Consonant,
    /// 右ブランク。
    Cutoff,
    /// 先行発声。
    Preutterance,
    /// オーバーラップ。
    Overlap,
}

impl Slot {
    /// 5値すべて。**並びは常に同じ**（`TR-ALN-29` の決定性）。
    pub const ALL: [Self; 5] = [
        Self::Offset,
        Self::Consonant,
        Self::Cutoff,
        Self::Preutterance,
        Self::Overlap,
    ];

    /// 送信してよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Offset => "slot.offset",
            Self::Consonant => "slot.consonant",
            Self::Cutoff => "slot.cutoff",
            Self::Preutterance => "slot.preutterance",
            Self::Overlap => "slot.overlap",
        }
    }

    /// `oto` からこのスロットの値を取る。
    #[must_use]
    pub const fn get(self, oto: &Oto) -> f64 {
        match self {
            Self::Offset => oto.offset_ms,
            Self::Consonant => oto.consonant_ms,
            Self::Cutoff => oto.cutoff_ms,
            Self::Preutterance => oto.preutterance_ms,
            Self::Overlap => oto.overlap_ms,
        }
    }

    /// `oto` のこのスロットへ書く。
    pub const fn set(self, oto: &mut Oto, v: f64) {
        match self {
            Self::Offset => oto.offset_ms = v,
            Self::Consonant => oto.consonant_ms = v,
            Self::Cutoff => oto.cutoff_ms = v,
            Self::Preutterance => oto.preutterance_ms = v,
            Self::Overlap => oto.overlap_ms = v,
        }
    }
}

/// エントリの状態（`align-review.fsl` の `EntryState`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryState {
    /// まだ推定していない。**録り直した直後もここへ戻る。**
    NotEstimated,
    /// 自動で確定した。
    AutoConfirmed,
    /// 確認キューに入っている。
    InQueue,
    /// 書き出しを塞いでいる。**検証で修復できない違反があった**（`TR-ALN-20`）。
    Blocked,
}

/// 確認のさせ方（`align-review.fsl` の `ReviewMode`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReviewMode {
    /// 1件ずつ確認させる。**既定。**
    Individual,
    /// まとめて確認させる。
    Batch,
    /// 録り直しを提案する。
    SuggestRerecord,
}

/// 1エントリ。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    /// 状態。
    pub state: EntryState,
    /// いまの5値。
    pub oto: Oto,
    /// 値ごとの固定（`TR-ALN-30`）。**`Slot::ALL` と同じ並び。**
    pinned: [bool; 5],
    /// 確信度。推定していなければ `None`。
    pub confidence: Option<Confidence>,
}

impl Entry {
    /// 未推定のエントリ。
    #[must_use]
    pub const fn new(oto: Oto) -> Self {
        Self {
            state: EntryState::NotEstimated,
            oto,
            pinned: [false; 5],
            confidence: None,
        }
    }

    /// その値が固定されているか（`TR-ALN-30`）。
    #[must_use]
    pub fn is_pinned(&self, s: Slot) -> bool {
        self.pinned[Self::index(s)]
    }

    /// 固定されている値の数。
    #[must_use]
    pub fn pinned_count(&self) -> usize {
        self.pinned.iter().filter(|p| **p).count()
    }

    fn index(s: Slot) -> usize {
        Slot::ALL.iter().position(|x| *x == s).unwrap_or(0)
    }
}

/// キューへの操作が通らなかった理由。
///
/// **状態機械の遷移条件を満たしていないだけで、異常ではない。**
/// 画面は押せないボタンとして出せばよい。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ReviewError {
    /// そのエントリが無い。
    #[error("そのエントリが無い")]
    NoSuchEntry,

    /// 書き出し済みなので、もう触れない。
    #[error("書き出し済み")]
    AlreadyExported,

    /// いまの状態からは行えない遷移。
    #[error("いまの状態からは行えない")]
    WrongState,

    /// 個別確認モードでないのに、1件ずつ確認しようとした（`REQ-ALN-008`）。
    #[error("個別確認モードではない")]
    NotIndividualMode,

    /// **上限を超えていないのに、個別確認をやめようとした**（`INV-ALN-004`）。
    #[error("確認の上限を超えていない")]
    BudgetNotExceeded,

    /// 確認が残っているのに書き出そうとした（`INV-ALN-003`）。
    #[error("確認が残っている")]
    ReviewPending,

    /// 固定されていない値の固定を解こうとした（`REQ-ALN-006`）。
    #[error("固定されていない")]
    NotPinned,
}

impl ReviewError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NoSuchEntry => "review.no_such_entry",
            Self::AlreadyExported => "review.already_exported",
            Self::WrongState => "review.wrong_state",
            Self::NotIndividualMode => "review.not_individual_mode",
            Self::BudgetNotExceeded => "review.budget_not_exceeded",
            Self::ReviewPending => "review.review_pending",
            Self::NotPinned => "review.not_pinned",
        }
    }
}

type Result<T> = std::result::Result<T, ReviewError>;

/// 1プロジェクトの確認キュー。
///
/// **`align-review.fsl` の状態そのもの。** 追加した状態は無い。
#[derive(Debug, Clone)]
pub struct ReviewQueue {
    entries: BTreeMap<String, Entry>,
    mode: ReviewMode,
    over_budget: bool,
    exported: bool,
    /// 1件あたりの確認にかかる見積もり時間。**未実測**（`TGT-ALN-007`）。
    per_item: Duration,
    /// 合計時間の上限（`DEC-ALN-003`）。
    budget: Duration,
}

impl ReviewQueue {
    /// 空のキュー。
    ///
    /// `per_item` は1件あたりの確認時間の見積もり。**実測に置き換わるまでは仮の値。**
    #[must_use]
    pub fn new(per_item: Duration) -> Self {
        Self {
            entries: BTreeMap::new(),
            mode: ReviewMode::Individual,
            over_budget: false,
            exported: false,
            per_item,
            budget: REVIEW_BUDGET,
        }
    }

    /// エントリを足す。**同じ鍵なら差し替える。**
    pub fn insert(&mut self, id: impl Into<String>, entry: Entry) {
        self.entries.insert(id.into(), entry);
    }

    /// エントリを引く。
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&Entry> {
        self.entries.get(id)
    }

    /// いまのモード。
    #[must_use]
    pub const fn mode(&self) -> ReviewMode {
        self.mode
    }

    /// 書き出し済みか。
    #[must_use]
    pub const fn is_exported(&self) -> bool {
        self.exported
    }

    /// 確認待ちの件数。
    #[must_use]
    pub fn pending_count(&self) -> usize {
        self.entries
            .values()
            .filter(|e| matches!(e.state, EntryState::InQueue | EntryState::Blocked))
            .count()
    }

    /// 確認にかかる見積もりの合計（`TR-ALN-25`）。
    #[must_use]
    pub fn estimated_review_time(&self) -> Duration {
        self.per_item * u32::try_from(self.pending_count()).unwrap_or(u32::MAX)
    }

    /// 上限を超えているか（`TR-ALN-25`, `DEC-ALN-003`）。
    #[must_use]
    pub fn exceeds_budget(&self) -> bool {
        self.estimated_review_time() > self.budget
    }

    /// 確認が残っているか（`align-review.fsl` の `needs_review`）。
    #[must_use]
    pub fn needs_review(&self) -> bool {
        self.pending_count() > 0
    }

    /// 確認キューの中身を、手が届く順に返す（`TR-ALN-26`）。
    ///
    /// **確信度の低い順。** 同点なら鍵の順で、**並びは常に同じ**（`TR-ALN-29`）。
    #[must_use]
    pub fn queued(&self) -> Vec<(&str, &Entry)> {
        let mut v: Vec<(&str, &Entry)> = self
            .entries
            .iter()
            .filter(|(_, e)| matches!(e.state, EntryState::InQueue | EntryState::Blocked))
            .map(|(k, e)| (k.as_str(), e))
            .collect();
        v.sort_by(|(ka, a), (kb, b)| {
            let sa = a.confidence.map_or(0.0, |c| c.score());
            let sb = b.confidence.map_or(0.0, |c| c.score());
            sa.total_cmp(&sb).then_with(|| ka.cmp(kb))
        });
        v
    }

    /// 確信度が足りていたので、自動で確定させる（`REQ-ALN-001`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、未推定でない。
    pub fn estimate_confident(&mut self, id: &str, c: Confidence) -> Result<()> {
        self.estimate(id, c, EntryState::AutoConfirmed)
    }

    /// 確信度が足りないので、確認キューへ回す（`REQ-ALN-002`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、未推定でない。
    pub fn estimate_low_confidence(&mut self, id: &str, c: Confidence) -> Result<()> {
        self.estimate(id, c, EntryState::InQueue)
    }

    /// テキスト逸脱なので、oto を自動確定させず確認キューへ回す（`REQ-ALN-003`, `TR-ALN-09`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、未推定でない。
    pub fn text_deviation(&mut self, id: &str) -> Result<()> {
        let e = self.editable(id)?;
        if e.state != EntryState::NotEstimated {
            return Err(ReviewError::WrongState);
        }
        e.state = EntryState::InQueue;
        Ok(())
    }

    fn estimate(&mut self, id: &str, c: Confidence, to: EntryState) -> Result<()> {
        let e = self.editable(id)?;
        if e.state != EntryState::NotEstimated {
            return Err(ReviewError::WrongState);
        }
        e.confidence = Some(c);
        e.state = to;
        Ok(())
    }

    /// 検証で修復できない違反があったので、書き出しを止める（`REQ-ALN-004`, `TR-ALN-20`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、自動確定していない。
    pub fn validation_unrepairable(&mut self, id: &str) -> Result<()> {
        let e = self.editable(id)?;
        if e.state != EntryState::AutoConfirmed {
            return Err(ReviewError::WrongState);
        }
        e.state = EntryState::Blocked;
        Ok(())
    }

    /// 人が値を編集した。**その値だけを固定する**（`REQ-ALN-005`, `TR-ALN-30`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、まだ推定していない。
    pub fn human_edit(&mut self, id: &str, slot: Slot, value: f64) -> Result<()> {
        let e = self.editable(id)?;
        if e.state == EntryState::NotEstimated {
            return Err(ReviewError::WrongState);
        }
        slot.set(&mut e.oto, value);
        e.pinned[Entry::index(slot)] = true;
        Ok(())
    }

    /// 外部ツールで変わった値を、固定として取り込む（`REQ-ALN-005`, `TR-ALN-30`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み。
    pub fn import_external_edit(&mut self, id: &str, slot: Slot, value: f64) -> Result<()> {
        let e = self.editable(id)?;
        slot.set(&mut e.oto, value);
        e.pinned[Entry::index(slot)] = true;
        Ok(())
    }

    /// 固定を解く。**本人が明示的に「自動に戻す」を選んだときだけ**（`REQ-ALN-006`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、固定されていない。
    pub fn revert_to_auto(&mut self, id: &str, slot: Slot) -> Result<()> {
        let e = self.editable(id)?;
        if !e.pinned[Entry::index(slot)] {
            return Err(ReviewError::NotPinned);
        }
        e.pinned[Entry::index(slot)] = false;
        Ok(())
    }

    /// 再推定する。**固定されていない値だけを書き換える**（`REQ-ALN-007`, `INV-ALN-001`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、まだ推定していない。
    pub fn re_estimate(&mut self, id: &str, fresh: Oto, c: Confidence) -> Result<()> {
        let e = self.editable(id)?;
        if e.state == EntryState::NotEstimated {
            return Err(ReviewError::WrongState);
        }
        for s in Slot::ALL {
            if !e.pinned[Entry::index(s)] {
                s.set(&mut e.oto, s.get(&fresh));
            }
        }
        e.confidence = Some(c);
        Ok(())
    }

    /// 1件ずつ確認して確定させる（`REQ-ALN-008`）。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、個別確認モードでない、キューに入っていない。
    pub fn confirm(&mut self, id: &str) -> Result<()> {
        if self.mode != ReviewMode::Individual {
            return Err(ReviewError::NotIndividualMode);
        }
        let e = self.editable(id)?;
        if e.state != EntryState::InQueue {
            return Err(ReviewError::WrongState);
        }
        e.state = EntryState::AutoConfirmed;
        Ok(())
    }

    /// まとめて確認する。**個別確認をやめたあとだけ**（`REQ-ALN-010`）。
    ///
    /// # Errors
    ///
    /// 書き出し済み、まとめて確認モードでない。
    pub fn confirm_all(&mut self) -> Result<usize> {
        if self.exported {
            return Err(ReviewError::AlreadyExported);
        }
        if self.mode != ReviewMode::Batch {
            return Err(ReviewError::WrongState);
        }
        let mut n = 0;
        for e in self.entries.values_mut() {
            if e.state == EntryState::InQueue {
                e.state = EntryState::AutoConfirmed;
                n += 1;
            }
        }
        Ok(n)
    }

    /// oto を直すのではなく録り直す（`REQ-ALN-009`, `TR-ALN-27`）。
    ///
    /// **固定した値は残る**（`REQ-ALN-007`）。録り直しても、人が決めた値は人のもの。
    ///
    /// # Errors
    ///
    /// エントリが無い、書き出し済み、確認待ちでない。
    pub fn rerecord(&mut self, id: &str) -> Result<()> {
        let e = self.editable(id)?;
        if !matches!(e.state, EntryState::InQueue | EntryState::Blocked) {
            return Err(ReviewError::WrongState);
        }
        e.state = EntryState::NotEstimated;
        e.confidence = None;
        Ok(())
    }

    /// 上限を超えたので、個別確認をやめてまとめて確認へ切り替える（`REQ-ALN-010`）。
    ///
    /// # Errors
    ///
    /// 書き出し済み、既に切り替え済み、上限を超えていない、確認が残っていない。
    pub fn switch_to_batch(&mut self) -> Result<()> {
        self.switch(ReviewMode::Batch)
    }

    /// 上限を超えたので、録り直し提案へ切り替える（`REQ-ALN-010`）。
    ///
    /// # Errors
    ///
    /// 書き出し済み、既に切り替え済み、上限を超えていない、確認が残っていない。
    pub fn switch_to_rerecord(&mut self) -> Result<()> {
        self.switch(ReviewMode::SuggestRerecord)
    }

    fn switch(&mut self, to: ReviewMode) -> Result<()> {
        if self.exported {
            return Err(ReviewError::AlreadyExported);
        }
        if self.over_budget || !self.needs_review() {
            return Err(ReviewError::WrongState);
        }
        // **個別確認をやめるのは、上限を超えたときだけ**（INV-ALN-004）。
        if !self.exceeds_budget() {
            return Err(ReviewError::BudgetNotExceeded);
        }
        self.over_budget = true;
        self.mode = to;
        Ok(())
    }

    /// 書き出す。**確認が残っている間は通らない**（`REQ-PKG-003`, `INV-ALN-003`）。
    ///
    /// # Errors
    ///
    /// 書き出し済み、確認が残っている、自動確定していないエントリがある。
    pub fn export(&mut self) -> Result<()> {
        if self.exported {
            return Err(ReviewError::AlreadyExported);
        }
        if self
            .entries
            .values()
            .any(|e| e.state != EntryState::AutoConfirmed)
        {
            return Err(ReviewError::ReviewPending);
        }
        self.exported = true;
        Ok(())
    }

    /// 低確信度の主因（`TR-ALN-26` (3)）。
    #[must_use]
    pub fn cause(&self, id: &str, threshold: f64) -> Option<Cause> {
        self.entries.get(id)?.confidence?.cause(threshold)
    }

    fn editable(&mut self, id: &str) -> Result<&mut Entry> {
        if self.exported {
            return Err(ReviewError::AlreadyExported);
        }
        self.entries.get_mut(id).ok_or(ReviewError::NoSuchEntry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ITEM: Duration = Duration::from_secs(30);

    fn oto(v: f64) -> Oto {
        Oto {
            offset_ms: v,
            consonant_ms: v,
            cutoff_ms: -v,
            preutterance_ms: v,
            overlap_ms: v,
        }
    }

    fn queue(n: usize) -> ReviewQueue {
        let mut q = ReviewQueue::new(ITEM);
        for i in 0..n {
            q.insert(format!("e{i:03}"), Entry::new(oto(1.0)));
        }
        q
    }

    #[test]
    fn 確信度が足りていれば自動で確定する() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        assert_eq!(q.get("e000").unwrap().state, EntryState::AutoConfirmed);
        assert!(!q.needs_review());
    }

    #[test]
    fn 確信度が足りなければ確認キューへ回る() {
        let mut q = queue(1);
        let low = Confidence {
            sharpness: 0.1,
            ..Confidence::full()
        };
        q.estimate_low_confidence("e000", low).unwrap();
        assert_eq!(q.get("e000").unwrap().state, EntryState::InQueue);
        assert!(q.needs_review());
    }

    /// **テキスト逸脱は oto を自動確定させない**（`REQ-ALN-003`, `TR-ALN-09`）。
    #[test]
    fn テキスト逸脱は確認キューへ回る() {
        let mut q = queue(1);
        q.text_deviation("e000").unwrap();
        assert_eq!(q.get("e000").unwrap().state, EntryState::InQueue);
    }

    /// **INV-ALN-001。** ここが破れると、人が直した値が自動で消える。
    #[test]
    fn 再推定は固定した値に触れない() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.human_edit("e000", Slot::Offset, 42.0).unwrap();

        q.re_estimate("e000", oto(9.0), Confidence::full()).unwrap();

        let e = q.get("e000").unwrap();
        assert_eq!(e.oto.offset_ms, 42.0, "固定した値は残る");
        assert_eq!(e.oto.consonant_ms, 9.0, "固定していない値は書き換わる");
    }

    /// **固定は値単位**（`TR-ALN-30`）。エントリ単位だと、
    /// オフセットを直しただけで他の4値も自動から外れてしまう。
    #[test]
    fn 固定は値ごとに持つ() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.human_edit("e000", Slot::Overlap, -3.0).unwrap();

        let e = q.get("e000").unwrap();
        assert!(e.is_pinned(Slot::Overlap));
        assert!(!e.is_pinned(Slot::Offset));
        assert_eq!(e.pinned_count(), 1);
    }

    /// **REQ-ALN-006。** 固定が解けるのは明示的な操作だけ。
    #[test]
    fn 自動に戻すまで固定は解けない() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.human_edit("e000", Slot::Offset, 42.0).unwrap();

        assert_eq!(
            q.revert_to_auto("e000", Slot::Consonant),
            Err(ReviewError::NotPinned),
            "固定していない値は解けない"
        );

        q.revert_to_auto("e000", Slot::Offset).unwrap();
        q.re_estimate("e000", oto(9.0), Confidence::full()).unwrap();
        assert_eq!(q.get("e000").unwrap().oto.offset_ms, 9.0);
    }

    /// **外部ツールの変更は固定として取り込む**（`TR-ALN-30`）。
    #[test]
    fn 外部の編集も固定になる() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.import_external_edit("e000", Slot::Cutoff, -700.0)
            .unwrap();

        q.re_estimate("e000", oto(9.0), Confidence::full()).unwrap();
        assert_eq!(q.get("e000").unwrap().oto.cutoff_ms, -700.0);
    }

    /// **録り直しても固定した値は残る**（`REQ-ALN-007`, `REQ-ALN-009`）。
    #[test]
    fn 録り直しても固定は残る() {
        let mut q = queue(1);
        q.estimate_low_confidence("e000", Confidence::full())
            .unwrap();
        q.human_edit("e000", Slot::Offset, 42.0).unwrap();
        q.rerecord("e000").unwrap();

        let e = q.get("e000").unwrap();
        assert_eq!(e.state, EntryState::NotEstimated);
        assert!(e.is_pinned(Slot::Offset));
        assert_eq!(e.oto.offset_ms, 42.0);
    }

    /// **INV-ALN-004。** 上限を超えていないのに個別確認をやめられない。
    #[test]
    fn 上限を超えるまで個別確認をやめられない() {
        let mut q = queue(2);
        for i in 0..2 {
            q.estimate_low_confidence(&format!("e{i:03}"), Confidence::full())
                .unwrap();
        }
        // 30秒 × 2件 = 1分。5分の上限内。
        assert!(!q.exceeds_budget());
        assert_eq!(q.switch_to_batch(), Err(ReviewError::BudgetNotExceeded));
        assert_eq!(q.mode(), ReviewMode::Individual);
    }

    /// **上限を超えたら、まとめて確認か録り直し提案へ切り替えられる**（`TR-ALN-25`）。
    #[test]
    fn 上限を超えたらまとめて確認へ切り替わる() {
        let mut q = queue(11); // 30秒 × 11件 = 5分30秒 > 5分
        for i in 0..11 {
            q.estimate_low_confidence(&format!("e{i:03}"), Confidence::full())
                .unwrap();
        }
        assert!(q.exceeds_budget());

        // **個別確認は、切り替える前しかできない**（REQ-ALN-008）。
        q.switch_to_batch().unwrap();
        assert_eq!(q.mode(), ReviewMode::Batch);
        assert_eq!(q.confirm("e000"), Err(ReviewError::NotIndividualMode));

        assert_eq!(q.confirm_all().unwrap(), 11);
        assert!(!q.needs_review());
    }

    #[test]
    fn 上限を超えたら録り直し提案へも切り替えられる() {
        let mut q = queue(11);
        for i in 0..11 {
            q.estimate_low_confidence(&format!("e{i:03}"), Confidence::full())
                .unwrap();
        }
        q.switch_to_rerecord().unwrap();
        assert_eq!(q.mode(), ReviewMode::SuggestRerecord);
    }

    /// **INV-ALN-003 / REQ-PKG-003。** 確認が残っている間は書き出せない。
    #[test]
    fn 確認が残っているうちは書き出せない() {
        let mut q = queue(2);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.estimate_low_confidence("e001", Confidence::full())
            .unwrap();

        assert_eq!(q.export(), Err(ReviewError::ReviewPending));

        q.confirm("e001").unwrap();
        q.export().unwrap();
        assert!(q.is_exported());
    }

    /// **修復できない違反は書き出しを塞ぐ**（`REQ-ALN-004`, `TR-ALN-20`）。
    #[test]
    fn 修復できない違反は書き出しを塞ぐ() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.validation_unrepairable("e000").unwrap();

        assert_eq!(q.get("e000").unwrap().state, EntryState::Blocked);
        assert_eq!(q.export(), Err(ReviewError::ReviewPending));
        // **Blocked は個別確認では抜けられない。** 直すか録り直すか。
        assert_eq!(q.confirm("e000"), Err(ReviewError::WrongState));
        q.rerecord("e000").unwrap();
    }

    /// 書き出したら、もう触れない。
    #[test]
    fn 書き出したあとは触れない() {
        let mut q = queue(1);
        q.estimate_confident("e000", Confidence::full()).unwrap();
        q.export().unwrap();

        assert_eq!(
            q.human_edit("e000", Slot::Offset, 1.0),
            Err(ReviewError::AlreadyExported)
        );
        assert_eq!(q.export(), Err(ReviewError::AlreadyExported));
    }

    /// **並びは常に同じ**（`TR-ALN-29`）。確信度の低い順、同点なら鍵の順。
    #[test]
    fn 確認キューは確信度の低い順に並ぶ() {
        let mut q = queue(3);
        let c = |v: f64| Confidence {
            sharpness: v,
            ..Confidence::full()
        };
        q.estimate_low_confidence("e000", c(0.8)).unwrap();
        q.estimate_low_confidence("e001", c(0.2)).unwrap();
        q.estimate_low_confidence("e002", c(0.5)).unwrap();

        let ids: Vec<&str> = q.queued().iter().map(|(k, _)| *k).collect();
        assert_eq!(ids, ["e001", "e002", "e000"]);
    }

    #[test]
    fn 見積もり時間は件数に比例する() {
        let mut q = queue(3);
        for i in 0..3 {
            q.estimate_low_confidence(&format!("e{i:03}"), Confidence::full())
                .unwrap();
        }
        assert_eq!(q.estimated_review_time(), Duration::from_secs(90));
    }

    #[test]
    fn 主因を引ける() {
        let mut q = queue(1);
        q.estimate_low_confidence(
            "e000",
            Confidence {
                acoustic: 0.1,
                ..Confidence::full()
            },
        )
        .unwrap();
        assert_eq!(q.cause("e000", 0.6), Some(Cause::Acoustic));
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            ReviewError::NoSuchEntry,
            ReviewError::AlreadyExported,
            ReviewError::WrongState,
            ReviewError::NotIndividualMode,
            ReviewError::BudgetNotExceeded,
            ReviewError::ReviewPending,
            ReviewError::NotPinned,
        ] {
            assert!(e.kind().starts_with("review."), "{}", e.kind());
        }
    }
}
