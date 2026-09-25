//! 同じ対象を作れる行のうち、どれが持つかを選ぶ（`DEC-RCL-016`）。
//!
//! 同じ音高で同じ対象を生む行のうち、有効な採用テイクを持ち、最初の有効なテイクが
//! いちばん早いものが持つ。 **先に録った行が持ち、あとから別の行を録っても入れ替わらない**
//! ——確認済みの5値と手で直した値が、本人の知らないうちに別の素材へ移らない。
//! 持ち主の採用テイクが無効になれば、次に録った行へ移る。
//!
//! 欄に持たず、テイクの事実から毎回導く。 欄に持つとテイクを無効にするたびに
//! 書き換えることになり、書き換え忘れた欄が別の行を指す。 書き出しと試唱が同じ答えを
//! 使えるよう、台帳の外で純粋に決める。

use std::collections::{BTreeMap, BTreeSet};

use crate::id::{RowId, TakeId};

/// 対象ごとの持ち主の行。
///
/// - `produced`: 行がどの対象を生むか。 同じ行が同じ対象を2度名乗ってもよい
/// - `adopted_valid`: 有効な採用テイクを持つ行
/// - `first_valid`: 行ごとの、最初に確定した有効なテイク（[`TakeId`] は確定した順に増える）
///
/// 対象の鍵は呼び出し側が決める。 移行中の台帳は（音高, 綴り）を、移行の先では
/// 同値の組の代表（`target::Catalog::canonical`）を渡す（`DEC-RCL-017`）。
///
/// 並びに依らない。 同じ事実を別の順で渡しても同じ持ち主を返す。
#[must_use]
pub fn owners<K: Ord>(
    produced: impl IntoIterator<Item = (K, RowId)>,
    adopted_valid: &BTreeSet<RowId>,
    first_valid: &BTreeMap<RowId, TakeId>,
) -> BTreeMap<K, RowId> {
    let mut best: BTreeMap<K, (TakeId, RowId)> = BTreeMap::new();
    for (key, row) in produced {
        if !adopted_valid.contains(&row) {
            continue;
        }
        let Some(at) = first_valid.get(&row).copied() else {
            continue;
        };
        match best.get_mut(&key) {
            // 同じテイクの番号は2行に付かないが、行の ID で並びを決定的にしておく。
            Some(held) if (at, &row) < (held.0, &held.1) => *held = (at, row),
            Some(_) => {}
            None => {
                best.insert(key, (at, row));
            }
        }
    }
    best.into_iter().map(|(k, (_, row))| (k, row)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(s: &str) -> RowId {
        RowId::new(s)
    }

    struct Facts {
        produced: Vec<(&'static str, RowId)>,
        adopted_valid: BTreeSet<RowId>,
        first_valid: BTreeMap<RowId, TakeId>,
    }

    /// `さ` をフルリストの行（s001、テイク 7）と詰め直した行（p1、テイク 3）が生む。
    /// `し` はフルリストの行だけ。 まだ録っていない行（s002）も `さ` を名乗る。
    fn facts() -> Facts {
        Facts {
            produced: vec![
                ("さ", row("s001")),
                ("し", row("s001")),
                ("さ", row("p1")),
                ("さ", row("s002")),
            ],
            adopted_valid: [row("s001"), row("p1")].into(),
            first_valid: [(row("s001"), TakeId::new(7)), (row("p1"), TakeId::new(3))].into(),
        }
    }

    fn run(f: &Facts) -> BTreeMap<&'static str, RowId> {
        owners(f.produced.iter().cloned(), &f.adopted_valid, &f.first_valid)
    }

    #[test]
    fn 先に録った行が持つ() {
        let got = run(&facts());
        assert_eq!(got.get("さ"), Some(&row("p1")));
        assert_eq!(got.get("し"), Some(&row("s001")));
    }

    /// あとから録った行は、先に録った行から持ち主を奪わない。
    #[test]
    fn あとから録っても入れ替わらない() {
        let mut f = facts();
        f.adopted_valid.insert(row("s002"));
        f.first_valid.insert(row("s002"), TakeId::new(9));
        assert_eq!(run(&f).get("さ"), Some(&row("p1")));
    }

    /// 持ち主の採用テイクが無効になれば、次に録った行へ移る。
    #[test]
    fn 採用が無効になれば次に録った行へ移る() {
        let mut f = facts();
        f.adopted_valid.remove(&row("p1"));
        assert_eq!(run(&f).get("さ"), Some(&row("s001")));
    }

    /// 有効な採用テイクを持つ行が無ければ、誰も持たない。
    #[test]
    fn 録った行が無ければ持ち主も無い() {
        let mut f = facts();
        f.adopted_valid.clear();
        assert!(run(&f).is_empty());
    }

    /// 持ち主は、その対象を生む行のどれか。
    #[test]
    fn 持ち主はその対象を生む行() {
        let f = facts();
        for (key, owner) in run(&f) {
            assert!(
                f.produced.iter().any(|(k, r)| *k == key && *r == owner),
                "{key} の持ち主 {owner} はその対象を生まない"
            );
        }
    }

    /// 並びを入れ替えても同じ持ち主を返す。 台帳から読む順は決まっていない。
    #[test]
    fn 並びに依らない() {
        let f = facts();
        let want = run(&f);
        let n = f.produced.len();
        // 4 件の並べ方をすべて試す（24 通り）。
        let mut order: Vec<usize> = (0..n).collect();
        let mut seen = 0_usize;
        loop {
            let shuffled = order.iter().map(|&i| f.produced[i].clone());
            assert_eq!(
                owners(shuffled, &f.adopted_valid, &f.first_valid),
                want,
                "{order:?}"
            );
            seen += 1;
            if !next_permutation(&mut order) {
                break;
            }
        }
        assert_eq!(seen, 24);
    }

    /// 辞書順で次の並べ方へ。 最後の並べ方なら偽。
    fn next_permutation(v: &mut [usize]) -> bool {
        let Some(i) = (1..v.len()).rev().find(|&i| v[i - 1] < v[i]) else {
            return false;
        };
        let j = (i..v.len()).rev().find(|&j| v[j] > v[i - 1]).unwrap_or(i);
        v.swap(i - 1, j);
        v[i..].reverse();
        true
    }
}
