//! 「あと何行録れば歌えるか」（`TR-RCL-16`, `TR-RCL-17`, `TR-RCL-09`）。
//!
//! 曲先行のミニ音源は、フルリストの行の部分集合として選ぶ（`TR-RCL-16`）。
//! 詰め直し——必要単位専用の行を作り直すこと——は採らない。
//!
//! 単独音は1項目＝1モーラなので、部分集合で無条件に最小になる。
//! 連続音では詰め直し版に対して数分の増加を受け入れる。
//! その代わり、フル方式への継続性が定義上保証される。
//! 詰め直すと、「曲のために録った分」がフルリストのどこにも当たらなくなり、
//! 同じ声をもう一度録り直すことになる。

use std::collections::BTreeSet;

use crate::reclist::Row;

/// 1単位あたりの収録サイクル（秒、`TR-RCL-09`）。
///
/// [Unknown] 自前実測まで暫定値。 OREMO の録音周期に固有の値を一次基準に置いている。
/// KOERU の録音 UI では連続収録でオーバーヘッドが下がるので、実測で確定する。
pub const SECONDS_PER_UNIT: f64 = 8.3;

/// 行読み上げ方式の1行あたり（秒、`TR-RCL-09`）。6モーラ以下のとき。
pub const SECONDS_PER_ROW_BASE: f64 = 12.0;

/// 6モーラを超えた分の1モーラあたり（秒）。
pub const SECONDS_PER_EXTRA_MORA: f64 = 1.2;

/// 1行あたりの基準モーラ数。
const BASE_MORAS: usize = 6;

/// 追加で録る計画（`TR-RCL-17`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    /// 録る行（フルリストの部分集合、`TR-RCL-16`）。
    pub rows: Vec<Row>,
    /// これで埋まる単位の数。
    pub covers: usize,
    /// どの行にも無くて埋まらない単位。
    ///
    /// 空でないなら、その方式では歌えない。
    pub unreachable: BTreeSet<String>,
    /// 推定所要時間（秒、`TR-RCL-09`）。
    pub seconds: f64,
}

/// 未収録の単位を埋める行を、フルリストから選ぶ（`TR-RCL-16`, `TR-RCL-17`）。
///
/// 詰め直さない。 選ぶのはフルリストの行そのもの。
///
/// 貪欲に選ぶ——毎回、いちばん多く埋まる行を採る。
/// 完全最小ではないが、行の中身が固定されている以上、差は小さい。
/// それより「選んだ行がフルリストの行と同じであること」のほうが効く。
#[must_use]
pub fn rows_to_cover(missing: &BTreeSet<String>, full_list: &[Row]) -> Plan {
    let mut left: BTreeSet<String> = missing.clone();
    let mut chosen: Vec<Row> = Vec::new();

    // どの行にも無い単位を先に外す。選びようが無い。
    let all: BTreeSet<String> = full_list
        .iter()
        .flat_map(|r| r.units.iter().map(|u| u.kana.to_owned()))
        .collect();
    let unreachable: BTreeSet<String> = left.difference(&all).cloned().collect();
    for u in &unreachable {
        left.remove(u);
    }

    while !left.is_empty() {
        // いちばん多く埋まる行。同数なら、リストの並び順で先のもの（決定的にする）。
        let best = full_list
            .iter()
            .filter(|r| !chosen.iter().any(|c| c.id == r.id))
            .map(|r| {
                let gain = r.units.iter().filter(|u| left.contains(u.kana)).count();
                (gain, r)
            })
            .filter(|(gain, _)| *gain > 0)
            .max_by_key(|(gain, _)| *gain);

        let Some((_, row)) = best else { break };
        for u in &row.units {
            left.remove(u.kana);
        }
        chosen.push(row.clone());
    }

    let covers = missing.len() - left.len() - unreachable.len();
    let seconds = estimate_seconds(&chosen);
    Plan {
        rows: chosen,
        covers,
        unreachable,
        seconds,
    }
}

/// 行を録るのに掛かる時間（秒、`TR-RCL-09`）。
///
/// 単独音は「1単位あたり × 単位数」、行読み上げは「1行 12 秒 ＋ 超過分」。
/// 式を2本に分けるのは、単独音が1項目＝1モーラで、行読み上げとは周期が違うから。
#[must_use]
pub fn estimate_seconds(rows: &[Row]) -> f64 {
    rows.iter()
        .map(|r| {
            let moras = r.units.len();
            if moras <= 1 {
                // 単独音の1項目。
                SECONDS_PER_UNIT
            } else if moras <= BASE_MORAS {
                SECONDS_PER_ROW_BASE
            } else {
                SECONDS_PER_ROW_BASE + (moras - BASE_MORAS) as f64 * SECONDS_PER_EXTRA_MORA
            }
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::UnitSet;
    use crate::reclist::generate_single;

    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 選ぶのはフルリストの行そのもの（`TR-RCL-16`）。詰め直さない。
    #[test]
    fn 選ぶ行はフルリストの行と同じ() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let plan = rows_to_cover(&set(&["さ", "く", "ら"]), &list);

        for row in &plan.rows {
            assert!(
                list.iter().any(|r| r.id == row.id && r.text == row.text),
                "フルリストに同じ行があること: {}",
                row.id
            );
        }
    }

    #[test]
    fn 必要な単位が全部埋まる() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let missing = set(&["さ", "く", "ら", "や", "よ", "い", "の", "そ", "は"]);
        let plan = rows_to_cover(&missing, &list);

        let covered: BTreeSet<String> = plan
            .rows
            .iter()
            .flat_map(|r| r.units.iter().map(|u| u.kana.to_owned()))
            .collect();
        assert!(missing.is_subset(&covered), "全部埋まること");
        assert_eq!(plan.covers, missing.len());
        assert!(plan.unreachable.is_empty());
    }

    /// 貪欲に選ぶので、無駄な行を採らない。
    #[test]
    fn 要らない行を採らない() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let plan = rows_to_cover(&set(&["さ"]), &list);
        assert_eq!(plan.rows.len(), 1, "1行で足りること");
    }

    #[test]
    fn 何も足りていなければ何も選ばない() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let plan = rows_to_cover(&BTreeSet::new(), &list);
        assert!(plan.rows.is_empty());
        assert!((plan.seconds - 0.0).abs() < f64::EPSILON);
    }

    /// どの行にも無い単位は「届かない」として分ける。
    #[test]
    fn 届かない単位を分けて返す() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        // 「ヴぁ」は拡張セットにしか無い。
        let plan = rows_to_cover(&set(&["さ", "ヴぁ"]), &list);
        assert_eq!(plan.unreachable, set(&["ヴぁ"]));
        assert_eq!(plan.covers, 1, "届く分だけ数える");
    }

    /// 同じ入力からは同じ計画が出る（`TR-RCL-27` の決定性）。
    #[test]
    fn 決定的に選ぶ() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let missing = set(&["さ", "く", "ら", "な", "に", "ぬ"]);
        let a = rows_to_cover(&missing, &list);
        let b = rows_to_cover(&missing, &list);
        assert_eq!(a, b);
    }

    /// 式を2本に分ける（`TR-RCL-09`）。
    #[test]
    fn 所要時間の式が要件どおり() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        // 1行5単位 → 6モーラ以下なので基準どおり。
        let five = list
            .iter()
            .find(|r| r.units.len() == 5)
            .cloned()
            .expect("5単位の行があること");
        assert!(
            (estimate_seconds(std::slice::from_ref(&five)) - SECONDS_PER_ROW_BASE).abs() < 1e-9
        );

        // 1単位の行は単独音の周期。
        if let Some(one) = list.iter().find(|r| r.units.len() == 1) {
            assert!((estimate_seconds(std::slice::from_ref(one)) - SECONDS_PER_UNIT).abs() < 1e-9);
        }
    }

    /// 6モーラを超えたら超過分を足す（`TR-RCL-09`）。
    ///
    /// 単独音の生成器は子音行で割るので8単位の行を作らない。
    /// 式そのものを確かめたいので、行を組み立てて渡す。
    #[test]
    fn 長い行は超過分を足す() {
        let all = crate::inventory::units(UnitSet::Core);
        let long = Row {
            id: "x".to_owned(),
            text: "長い行".to_owned(),
            units: all.iter().take(8).cloned().collect(),
            file_stem: "x".to_owned(),
        };
        let want = SECONDS_PER_ROW_BASE + 2.0 * SECONDS_PER_EXTRA_MORA;
        assert!((estimate_seconds(&[long]) - want).abs() < 1e-9);
    }

    /// 被覆の計算が目標の中に収まる（TGT-RCL-004: 50ms 以内）。
    #[test]
    fn 被覆の計算が速い() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let missing: BTreeSet<String> = list
            .iter()
            .flat_map(|r| r.units.iter().map(|u| u.kana.to_owned()))
            .collect();

        let t = std::time::Instant::now();
        let plan = rows_to_cover(&missing, &list);
        let elapsed = t.elapsed();

        assert_eq!(plan.covers, missing.len());
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "TGT-RCL-004 の 50ms 以内: {elapsed:?}"
        );
    }
}
