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

use crate::alias::Method;
use crate::pace;
use crate::reclist::Row;

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
pub fn rows_to_cover(
    rules: &crate::presamp::Rules,
    method: Method,
    missing: &BTreeSet<String>,
    full_list: &[Row],
) -> Plan {
    let mut left: BTreeSet<String> = missing.clone();
    let mut chosen: Vec<Row> = Vec::new();

    // 行が生む綴りで突き合わせる（`TR-RCL-18`）。
    //
    // **仮名で突き合わせていた。** `missing` は方式ごとの綴りで来るので、
    // 連続音や CVVC では1つも一致せず、あと何行かが常に 0 行になっていた。
    let aliases_of = |r: &Row| crate::reclist::row_aliases(rules, method, &r.units);

    // どの行にも無い綴りを先に外す。選びようが無い。
    let all: BTreeSet<String> = full_list.iter().flat_map(&aliases_of).collect();
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
                let gain = aliases_of(r).iter().filter(|a| left.contains(*a)).count();
                (gain, r)
            })
            .filter(|(gain, _)| *gain > 0)
            .max_by_key(|(gain, _)| *gain);

        let Some((_, row)) = best else { break };
        // 選んだ行が埋めた綴りを外す。 **仮名を外していた**——点数は綴りで
        // 数えるよう直したのに、ここだけ仮名のままだった。連続音や CVVC では
        // 何も外れないので、`left` に触れる行を全部選び、行数も時間も膨らんだ。
        for a in aliases_of(row) {
            left.remove(&a);
        }
        chosen.push(row.clone());
    }

    let covers = missing.len() - left.len() - unreachable.len();
    let seconds = estimate_seconds(method, &chosen);
    Plan {
        rows: chosen,
        covers,
        unreachable,
        seconds,
    }
}

/// 行を録るのに掛かる時間（秒、`TR-RCL-09`）。
///
/// 式は [`crate::pace`] が持つ。 ここは単音階の場合の入口。
#[must_use]
pub fn estimate_seconds(method: Method, rows: &[Row]) -> f64 {
    pace::fixed_seconds(method, rows, 1)
}

#[cfg(test)]
mod tests {

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }
    use super::*;
    use crate::inventory::UnitSet;
    use crate::pace::{SECONDS_PER_EXTRA_MORA, SECONDS_PER_ROW_BASE, SECONDS_PER_UNIT};
    use crate::reclist::generate_single;

    fn set(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 連続音でも、埋めた綴りを外してから次を選ぶ（`TR-RCL-16`）。
    ///
    /// **仮名を外していた。** 連続音の綴り（`- か`、`a か`）は仮名と一致しないので
    /// 何も外れず、`left` に触れる行を全部選んでいた。単独音は綴りが仮名
    /// そのものなので、単独音の試験では見えなかった。
    #[test]
    fn 連続音でも一行で足りるなら一行だけ選ぶ() {
        let rules = builtin_rules();
        let list = crate::reclist::generate_sequential(UnitSet::Core, 8).expect("生成できる");
        let row = &list[0];
        let missing: BTreeSet<String> =
            crate::reclist::row_aliases(&rules, Method::Sequential, &row.units)
                .into_iter()
                .collect();
        let plan = rows_to_cover(&rules, Method::Sequential, &missing, &list);
        assert_eq!(plan.rows.len(), 1, "1行で足りる: {:?}", plan.rows.len());
        assert_eq!(plan.covers, missing.len(), "全部埋まる");
    }

    /// 選ぶのはフルリストの行そのもの（`TR-RCL-16`）。詰め直さない。
    #[test]
    fn 選ぶ行はフルリストの行と同じ() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let plan = rows_to_cover(
            &builtin_rules(),
            Method::Single,
            &set(&["さ", "く", "ら"]),
            &list,
        );

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
        let plan = rows_to_cover(&builtin_rules(), Method::Single, &missing, &list);

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
        let plan = rows_to_cover(&builtin_rules(), Method::Single, &set(&["さ"]), &list);
        assert_eq!(plan.rows.len(), 1, "1行で足りること");
    }

    #[test]
    fn 何も足りていなければ何も選ばない() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let plan = rows_to_cover(&builtin_rules(), Method::Single, &BTreeSet::new(), &list);
        assert!(plan.rows.is_empty());
        assert!((plan.seconds - 0.0).abs() < f64::EPSILON);
    }

    /// どの行にも無い単位は「届かない」として分ける。
    #[test]
    fn 届かない単位を分けて返す() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        // 「ヴぁ」は拡張セットにしか無い。
        let plan = rows_to_cover(
            &builtin_rules(),
            Method::Single,
            &set(&["さ", "ヴぁ"]),
            &list,
        );
        assert_eq!(plan.unreachable, set(&["ヴぁ"]));
        assert_eq!(plan.covers, 1, "届く分だけ数える");
    }

    /// 同じ入力からは同じ計画が出る（`TR-RCL-27` の決定性）。
    #[test]
    fn 決定的に選ぶ() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let missing = set(&["さ", "く", "ら", "な", "に", "ぬ"]);
        let a = rows_to_cover(&builtin_rules(), Method::Single, &missing, &list);
        let b = rows_to_cover(&builtin_rules(), Method::Single, &missing, &list);
        assert_eq!(a, b);
    }

    /// 式は方式で選ぶ（`TR-RCL-09`）。
    ///
    /// **行の長さで選んでいた。** 単独音の生成器は1行に5単位まで詰めるので、
    /// ほとんどの行が「行読み上げ」の枝へ落ち、1行 12 秒で数えられていた
    /// ——102 単位の音源が実際の 1/3 に見える。
    #[test]
    fn 単独音は単位ごとに数える() {
        let list = generate_single(UnitSet::Core, 5).expect("生成できる");
        let five = list
            .iter()
            .find(|r| r.units.len() == 5)
            .cloned()
            .expect("5単位の行があること");
        let want = 5.0 * SECONDS_PER_UNIT;
        assert!(
            (estimate_seconds(Method::Single, std::slice::from_ref(&five)) - want).abs() < 1e-9,
            "単独音は「1単位あたり × 単位数」"
        );

        // リスト全体でも、単位数に比例する。
        let units: usize = list.iter().map(|r| r.units.len()).sum();
        let all = estimate_seconds(Method::Single, &list);
        assert!((all - units as f64 * SECONDS_PER_UNIT).abs() < 1e-6);
    }

    /// 行読み上げは1行あたりで数え、6モーラを超えたら超過分を足す（`TR-RCL-09`）。
    #[test]
    fn 行読み上げは行ごとに数える() {
        let all = crate::inventory::units(UnitSet::Core);
        let row = |n: usize| Row {
            id: "x".to_owned(),
            text: "行".to_owned(),
            units: all.iter().take(n).cloned().collect(),
            file_stem: "x".to_owned(),
        };
        for m in [Method::Sequential, Method::Cvvc] {
            assert!(
                (estimate_seconds(m, &[row(5)]) - SECONDS_PER_ROW_BASE).abs() < 1e-9,
                "6モーラ以下は基準どおり"
            );
            let want = SECONDS_PER_ROW_BASE + 2.0 * SECONDS_PER_EXTRA_MORA;
            assert!(
                (estimate_seconds(m, &[row(8)]) - want).abs() < 1e-9,
                "超過分を足す"
            );
        }
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
        let plan = rows_to_cover(&builtin_rules(), Method::Single, &missing, &list);
        let elapsed = t.elapsed();

        assert_eq!(plan.covers, missing.len());
        assert!(
            elapsed < std::time::Duration::from_millis(50),
            "TGT-RCL-004 の 50ms 以内: {elapsed:?}"
        );
    }
}
