//! 録る順（`TR-SYN-19`）。
//!
//! 提示順を2つ持つ。
//!
//! 1. **曲バンク優先** — 本人の曲バンクの未収録単位を、最も少ない行数で被覆する順
//! 2. **被覆効率** — 方式全体の未収録単位を、最も少ない行数で被覆する順
//!
//! # リストそのものは書き換えない
//!
//! > どちらのモードも、録音リストの正準順（`TR-RCL-27`）とカバレッジ台帳の
//! > 行集合（`TR-RCL-18`）を書き換えない
//!
//! 変わるのは「次に何を録るか」の並びだけ。 生成したリストは同じプリセットから
//! 常にバイト単位で同一で、収録済みの行が提示から外れても台帳からは消えない。
//!
//! # 近い単位を離して録らない
//!
//! 被覆効率では、連続する行の収録単位が近くなるようにまとめる
//! （子音行が揃う、母音環境が続く）。**声質はセッション内でも移ろう**ので、
//! 「か」と「が」を1時間離して録ると、並べたときに違う声に聞こえる。

use std::collections::BTreeSet;

use crate::alias::Method;
use crate::reclist::{Row, row_aliases};

/// 提示順のモード（`TR-SYN-19`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// 曲バンクの未収録単位を先に埋める。
    SongBankFirst,
    /// 方式全体の未収録単位を、少ない行数で埋める。
    CoverageEfficiency,
}

impl Mode {
    /// 画面と IPC へ渡す識別子。
    ///
    /// `Debug` を wire 形式にしない。 variant を改名すると、
    /// TypeScript 側のリテラル union が黙って外れる。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SongBankFirst => "SongBankFirst",
            Self::CoverageEfficiency => "CoverageEfficiency",
        }
    }
}

/// 曲バンク優先から被覆効率へ自動で移ってよいか（`TR-SYN-19` (a)）。
///
/// > 曲バンクの全曲が「完全」になったとき。バンクが空なら初めから被覆効率で始まる
///
/// 本人の明示的な切り替え（(b)）はここが答えない。 いつでもできて、可逆。
#[must_use]
pub const fn auto_switches(song_bank_empty: bool, song_bank_complete: bool) -> bool {
    song_bank_empty || song_bank_complete
}

/// 提示順を作る（`TR-SYN-19`）。
///
/// 返すのは行 ID の並び。 収録済みの行は落とす——次に録るものの並びなので。
/// 台帳からは消えない。
///
/// `covered` は収録済みのエイリアス、`wanted` はそのモードが狙う未収録エイリアス。
#[must_use]
pub fn present(
    mode: Mode,
    method: Method,
    rows: &[Row],
    recorded_rows: &BTreeSet<String>,
    covered: &BTreeSet<String>,
    song_required: &BTreeSet<String>,
) -> Vec<String> {
    let remaining: Vec<&Row> = rows
        .iter()
        .filter(|r| !recorded_rows.contains(&r.id))
        .collect();

    let wanted: BTreeSet<String> = match mode {
        // 曲が要るもののうち、まだ無いもの。
        Mode::SongBankFirst => song_required.difference(covered).cloned().collect(),
        // 方式全体のうち、まだ無いもの。
        Mode::CoverageEfficiency => rows
            .iter()
            .flat_map(|r| row_aliases(method, &r.units))
            .filter(|a| !covered.contains(a))
            .collect(),
    };

    let mut left = wanted;
    let mut pool = remaining;
    let mut out = Vec::new();
    let mut last: Option<&Row> = None;

    while !pool.is_empty() {
        // いちばん多く埋まる行。同数のときの裁き方がモードで変わる。
        let pick = pool
            .iter()
            .enumerate()
            .max_by_key(|(i, r)| {
                let gain = row_aliases(method, &r.units)
                    .into_iter()
                    .filter(|a| left.contains(a))
                    .count();
                // 被覆効率では、直前の行と近いものを先に出す（`TR-SYN-19`）。
                let nearness = match mode {
                    Mode::CoverageEfficiency => last.map_or(0, |p| nearness(p, r)),
                    Mode::SongBankFirst => 0,
                };
                // 同点は正準順で裁く。 並びを決定的にする（`TR-RCL-27`）。
                (gain, nearness, std::cmp::Reverse(*i))
            })
            .map(|(i, _)| i);
        let Some(i) = pick else { break };
        let row = pool.remove(i);
        for a in row_aliases(method, &row.units) {
            left.remove(&a);
        }
        out.push(row.id.clone());
        last = Some(row);
    }
    out
}

/// 2つの行がどれだけ近いか（`TR-SYN-19`）。
///
/// 子音行が揃う、母音環境が続く。 大きいほど近い。
fn nearness(a: &Row, b: &Row) -> usize {
    let consonants: BTreeSet<&str> = a.units.iter().map(|u| u.consonant).collect();
    let vowels: BTreeSet<&str> = a.units.iter().map(|u| u.vowel).collect();
    let shared_c = b
        .units
        .iter()
        .filter(|u| consonants.contains(u.consonant))
        .count();
    let shared_v = b.units.iter().filter(|u| vowels.contains(u.vowel)).count();
    // 子音のほうを重く見る。 「か き く け こ」の並びが揃うことのほうが効く。
    shared_c * 2 + shared_v
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn empty() -> BTreeSet<String> {
        BTreeSet::new()
    }

    fn have(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 曲が要る単位を持つ行が先に来る（`TR-SYN-19` (1)）。
    #[test]
    fn 曲バンク優先は曲の行を先に出す() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let song = have(&["さ", "く", "ら"]);
        let order = present(
            Mode::SongBankFirst,
            Method::Single,
            &rows,
            &empty(),
            &empty(),
            &song,
        );
        // 先頭の行は曲の単位を持つ。
        let first = rows.iter().find(|r| r.id == order[0]).expect("ある");
        assert!(
            first.units.iter().any(|u| song.contains(u.kana)),
            "{}",
            first.text
        );
    }

    /// 収録済みの行は提示から外れる。台帳からは消えない（`TR-SYN-19`）。
    #[test]
    fn 収録済みの行は提示から外れる() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let done = have(&[rows[0].id.as_str()]);
        let order = present(
            Mode::CoverageEfficiency,
            Method::Single,
            &rows,
            &done,
            &empty(),
            &empty(),
        );
        assert!(!order.contains(&rows[0].id));
        assert_eq!(order.len(), rows.len() - 1);
        // リストそのものは変わらない。
        assert_eq!(rows, generate_single(UnitSet::Core, 5).expect("生成できる"));
    }

    /// 全部の行が並ぶ。落とさない。
    #[test]
    fn すべての行が並ぶ() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        for mode in [Mode::SongBankFirst, Mode::CoverageEfficiency] {
            let order = present(mode, Method::Single, &rows, &empty(), &empty(), &empty());
            assert_eq!(order.len(), rows.len(), "{mode:?}");
            let uniq: BTreeSet<&String> = order.iter().collect();
            assert_eq!(uniq.len(), rows.len(), "{mode:?} が同じ行を2度出す");
        }
    }

    /// 並びが決定的（`TR-RCL-27`）。
    #[test]
    fn 提示順は決定的() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let a = present(
            Mode::CoverageEfficiency,
            Method::Single,
            &rows,
            &empty(),
            &empty(),
            &empty(),
        );
        let b = present(
            Mode::CoverageEfficiency,
            Method::Single,
            &rows,
            &empty(),
            &empty(),
            &empty(),
        );
        assert_eq!(a, b);
    }

    /// 近い単位を離して録らない（`TR-SYN-19`）。
    ///
    /// 被覆効率では、連続する行の子音行が揃いやすい。
    #[test]
    fn 被覆効率は近い行を続けて出す() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let by_id = |id: &String| rows.iter().find(|r| &r.id == id).expect("ある");
        let order = present(
            Mode::CoverageEfficiency,
            Method::Single,
            &rows,
            &empty(),
            &empty(),
            &empty(),
        );
        let efficiency: usize = order
            .windows(2)
            .map(|w| nearness(by_id(&w[0]), by_id(&w[1])))
            .sum();
        let canonical: usize = rows.windows(2).map(|w| nearness(&w[0], &w[1])).sum();
        assert!(
            efficiency >= canonical,
            "正準順より近い: {efficiency} < {canonical}"
        );
    }

    /// 曲バンクが空なら初めから被覆効率（`TR-SYN-19` (a)）。
    #[test]
    fn 曲バンクが空なら被覆効率から始まる() {
        assert!(auto_switches(true, false));
        assert!(auto_switches(false, true), "全曲が完全になったら移る");
        assert!(!auto_switches(false, false), "まだ曲が残っていれば移らない");
    }

    /// wire 形式は `Debug` に頼らない。
    #[test]
    fn モードの識別子が固定されている() {
        assert_eq!(Mode::SongBankFirst.as_str(), "SongBankFirst");
        assert_eq!(Mode::CoverageEfficiency.as_str(), "CoverageEfficiency");
    }
}
