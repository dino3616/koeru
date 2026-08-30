//! 試唱レイテンシ（`TR-SYN-33`）。
//!
//! **押してから鳴るまでを3つに分ける。** 同じ目標でひとくくりにすると、
//! 初回の重さと2回目以降の軽さのどちらかが説明できなくなる。
//!
//! | 場面 | 中央値 | p95 |
//! |---|---|---|
//! | 初回（直前の録音の前処理を含む） | 6秒 | 10秒 |
//! | 2回目以降・前処理済み・キャッシュ無し | 1.5秒 | 5秒 |
//! | 追加録音後の差分再試唱 | 0.5秒 | — |
//! | 同一条件の再再生 | 0.1秒 | — |
//!
//! # 無言の待ち時間にしない
//!
//! **初回に限り「録音終了 → 試唱ボタン活性化」の間に、
//! 前処理の完了を待つ明示的な進捗状態を置く**（`TR-SYN-33`）。
//! 何も出ないまま6秒待たされると、壊れたと思われる。

use std::time::Duration;

/// どの場面か（`TR-SYN-33`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Case {
    /// 初回試唱。**直前の録音の前処理を含む。**
    First,
    /// 2回目以降。前処理は済んでいて、キャッシュは無い。
    Warm,
    /// 追加録音後の差分再試唱。
    Incremental,
    /// 同一条件の再再生。
    Replay,
}

/// 目標（`TR-SYN-33`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Budget {
    /// 中央値の目標。
    pub median: Duration,
    /// p95 の目標。**置いていない場面もある。**
    pub p95: Option<Duration>,
}

/// その場面の目標を引く。
#[must_use]
pub const fn budget(case: Case) -> Budget {
    match case {
        Case::First => Budget {
            median: Duration::from_secs(6),
            p95: Some(Duration::from_secs(10)),
        },
        Case::Warm => Budget {
            median: Duration::from_millis(1500),
            p95: Some(Duration::from_secs(5)),
        },
        Case::Incremental => Budget {
            median: Duration::from_millis(500),
            p95: None,
        },
        Case::Replay => Budget {
            median: Duration::from_millis(100),
            p95: None,
        },
    }
}

/// 測った時間を溜める。
///
/// **中央値と p95 を出すために持つ。** 1回だけ測っても、目標と比べられない。
#[derive(Debug, Default)]
pub struct Observed {
    samples: Vec<Duration>,
}

impl Observed {
    /// 空。
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 1回ぶん足す。
    pub fn record(&mut self, elapsed: Duration) {
        self.samples.push(elapsed);
    }

    /// 測った回数。
    #[must_use]
    pub fn count(&self) -> usize {
        self.samples.len()
    }

    /// 中央値。**1回も測っていなければ `None`。**
    #[must_use]
    pub fn median(&self) -> Option<Duration> {
        self.percentile(0.5)
    }

    /// p95。
    #[must_use]
    pub fn p95(&self) -> Option<Duration> {
        self.percentile(0.95)
    }

    /// 目標に収まっているか。
    ///
    /// **回数が少ないうちは判定しない。** 3回では中央値も p95 も意味が無い。
    #[must_use]
    pub fn meets(&self, case: Case, min_samples: usize) -> Option<bool> {
        if self.samples.len() < min_samples {
            return None;
        }
        let b = budget(case);
        let median_ok = self.median().is_some_and(|m| m <= b.median);
        let p95_ok = b
            .p95
            .is_none_or(|want| self.p95().is_some_and(|got| got <= want));
        Some(median_ok && p95_ok)
    }

    fn percentile(&self, q: f64) -> Option<Duration> {
        if self.samples.is_empty() {
            return None;
        }
        let mut sorted = self.samples.clone();
        sorted.sort_unstable();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "位置は 0..len に収める"
        )]
        let at = ((sorted.len() - 1) as f64 * q).round() as usize;
        sorted.get(at).copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **目標は要件どおり**（TR-SYN-33）。
    #[test]
    fn 目標が要件どおり() {
        assert_eq!(budget(Case::First).median, Duration::from_secs(6));
        assert_eq!(budget(Case::First).p95, Some(Duration::from_secs(10)));
        assert_eq!(budget(Case::Warm).median, Duration::from_millis(1500));
        assert_eq!(budget(Case::Warm).p95, Some(Duration::from_secs(5)));
        assert_eq!(budget(Case::Incremental).median, Duration::from_millis(500));
        assert_eq!(budget(Case::Replay).median, Duration::from_millis(100));
    }

    /// **初回だけ p95 が緩い。** 前処理を含むから。
    #[test]
    fn 初回は他より緩い() {
        assert!(budget(Case::First).median > budget(Case::Warm).median);
        assert!(budget(Case::Warm).median > budget(Case::Incremental).median);
        assert!(budget(Case::Incremental).median > budget(Case::Replay).median);
    }

    #[test]
    fn 中央値とp95を出す() {
        let mut o = Observed::new();
        for ms in [100, 200, 300, 400, 5000] {
            o.record(Duration::from_millis(ms));
        }
        assert_eq!(o.median(), Some(Duration::from_millis(300)));
        assert_eq!(o.p95(), Some(Duration::from_millis(5000)));
    }

    /// **回数が少ないうちは判定しない。**
    #[test]
    fn 回数が少なければ判定しない() {
        let mut o = Observed::new();
        o.record(Duration::from_millis(50));
        assert_eq!(o.meets(Case::Replay, 10), None);
    }

    #[test]
    fn 目標に収まっているかを言える() {
        let mut fast = Observed::new();
        let mut slow = Observed::new();
        for _ in 0..20 {
            fast.record(Duration::from_millis(50));
            slow.record(Duration::from_millis(5000));
        }
        assert_eq!(fast.meets(Case::Replay, 10), Some(true));
        assert_eq!(slow.meets(Case::Replay, 10), Some(false));
    }

    #[test]
    fn 空でも落ちない() {
        let o = Observed::new();
        assert_eq!(o.median(), None);
        assert_eq!(o.p95(), None);
        assert_eq!(o.count(), 0);
    }

    /// **p95 を置いていない場面では中央値だけで判定する。**
    #[test]
    fn p95のない場面は中央値だけ見る() {
        assert_eq!(budget(Case::Incremental).p95, None);
        let mut o = Observed::new();
        for _ in 0..20 {
            o.record(Duration::from_millis(400));
        }
        assert_eq!(o.meets(Case::Incremental, 10), Some(true));
    }
}
