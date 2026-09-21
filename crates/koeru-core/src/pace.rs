//! 収録ペースと所要時間（`TR-RCL-09`, `TR-RCL-11`）。
//!
//! 式は2本ある。 単独音は1項目＝1モーラなので「1単位あたりのサイクル × 単位数」、
//! 行読み上げ（連続音 / CVVC）は「1行 12.0 秒＋モーラ超過分」。周期が違う。
//!
//! # 実測でペースを推定しない
//!
//! 一度は実装した。 直近20行の中央値から推定して残り時間へ効かせていたが、
//! **落とした**（`DEC-RCL-013`）。選択画面が固定値・進行中が実測値という
//! 二重表示になり、席を立った・数行だけ録った・録り直しが続いたといった
//! 例外を数えはじめることになる。値の出どころが1本なら、その手当てが要らない。
//!
//! # 12.0 秒は実務値であって実測ではない
//!
//! `DEC-RCL-008` が「暫定値のまま確定する」と決めている。 置き換える条件は
//! その判断記録の `review_triggers` が持つ。
//!
//! # 時間だけで示さない
//!
//! 残りは時間と件数の両方で出す（`TR-RCL-09`）。 固定値の見積もりは桁しか
//! 合っていないので、時間だけ出すと精度を騙ることになる。
//! 件数は数え上げなので正確で、本人が自分のペースを当てはめられる。

use crate::alias::Method;
use crate::inventory::{Unit, UnitSet, units};
use crate::reclist::Row;

/// 1単位あたりの収録サイクル（秒、`TR-RCL-09`）。単独音の式で使う。
pub const SECONDS_PER_UNIT: f64 = 8.3;

/// 行読み上げの1行あたり（秒、`TR-RCL-09`）。6モーラ以下のとき。
pub const SECONDS_PER_ROW_BASE: f64 = 12.0;

/// 6モーラを超えた分の1モーラあたり（秒）。
pub const SECONDS_PER_EXTRA_MORA: f64 = 1.2;

/// 1行あたりの基準モーラ数。
const BASE_MORAS: usize = 6;

/// 録り直し率の既定（`TR-RCL-09`）。
///
/// [Unknown] 実測の裏付けが無い。 `TR-RCL-09` の notes がそう書いている。
pub const DEFAULT_RETAKE_RATE: f64 = 0.20;

/// 1回の録音セッションの想定長（秒）。`TR-RCL-11` (c) の回数を出すのに使う。
///
/// [Unknown] 実測も根拠も無い。 `TR-REC-30` はセッションを切る条件（30 分の無操作）を
/// 定めているが、1回の長さは定めていない。ここは表示のための刻みでしかなく、
/// 所要時間そのものには効かない。
pub const SESSION_SECONDS: f64 = 30.0 * 60.0;

/// 所要時間（秒、`TR-RCL-09`）。
///
/// 固定値で計算する。 方式選択画面も残り時間も、同じこの式から出す
/// （`DEC-RCL-013`）。`tones` は収録音高の本数——多音階は同じリストを
/// 音高の数だけ録る。
#[must_use]
pub fn fixed_seconds(rows: &[Row], tones: usize) -> f64 {
    let one_pass: f64 = rows
        .iter()
        .map(|r| {
            let moras = r.units.len();
            if moras <= 1 {
                // 単独音の1項目。行読み上げとは周期が違う。
                SECONDS_PER_UNIT
            } else if moras <= BASE_MORAS {
                SECONDS_PER_ROW_BASE
            } else {
                SECONDS_PER_ROW_BASE + (moras - BASE_MORAS) as f64 * SECONDS_PER_EXTRA_MORA
            }
        })
        .sum();
    one_pass * tones.max(1) as f64
}

/// 1行の読み間違いリスク（`TR-RCL-07`）。
///
/// 成分を持ったまま返す。 合成した1個の数だけを返すと、
/// 「何が難しいのか」を画面が言えない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowRisk {
    /// 難読音の数。拗音かつ外来音の拍（てゅ、ヴぃ など）。
    pub hard: usize,
    /// 拗音の数。
    pub palatalized: usize,
    /// モーラ数。
    pub moras: usize,
    /// 合成スコア。大きいほど読み間違えやすい。
    ///
    /// [Unknown] 重みに根拠が無い。 `TR-RCL-07` は「難読音数、拗音数、モーラ数から
    /// 算出する」としか定めていない。並べ替えの鍵として使い、絶対値を見せない。
    pub score: f64,
}

/// 小書き仮名。拗音と外来音の拍を見分ける軸。
const SMALL_KANA: [char; 8] = ['ゃ', 'ゅ', 'ょ', 'ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ'];

/// 拗音か。小書き仮名を含む拍。
#[must_use]
pub fn is_palatalized(u: &Unit) -> bool {
    u.kana.chars().any(|c| SMALL_KANA.contains(&c))
}

/// 行の読み間違いリスク（`TR-RCL-07`）。
///
/// 難読音は「拗音かつ外来音」。 中核セットに無い拗音の拍がそれに当たる
/// （てゃ、でゅ、ヴぃ など）。拗音そのものは日本語の常用拍なので、
/// 同じ重みでは数えない。
#[must_use]
pub fn row_risk(row: &Row) -> RowRisk {
    let core = units(UnitSet::Core);
    let loanword = |u: &Unit| !core.iter().any(|c| c.kana == u.kana);

    let palatalized = row.units.iter().filter(|u| is_palatalized(u)).count();
    let hard = row
        .units
        .iter()
        .filter(|u| is_palatalized(u) && loanword(u))
        .count();
    let moras = row.units.len();
    let score = hard as f64 * 3.0 + palatalized as f64 + moras.saturating_sub(BASE_MORAS) as f64;
    RowRisk {
        hard,
        palatalized,
        moras,
        score,
    }
}

/// 方式選択画面に出す1件（`TR-RCL-11`）。
///
/// 到達点の文言はここが持たない。 「歌える曲の範囲を具体語で1行」は画面の言葉で、
/// 計算できるのは元になる数まで。
#[derive(Debug, Clone, PartialEq)]
pub struct MethodOffer {
    pub method: Method,
    /// 所要時間1個。レンジではなく代表値（`TR-RCL-11` (a)）。
    pub seconds: f64,
    /// 1回のセッションで区切った場合の想定回数（`TR-RCL-11` (c)）。
    pub sessions: usize,
    /// 1行あたりの平均モーラ数（`TR-RCL-11` (d)）。
    pub moras_per_row: f64,
    /// 難読音の含有率（`TR-RCL-11` (d)）。全モーラに対する割合。
    pub hard_ratio: f64,
    /// 行数。到達点の説明を画面が組み立てるのに要る。
    pub rows: usize,
}

/// そのリストが生む収録単位の数（カバレッジの分母）。
#[must_use]
pub fn offer_of(rows: &[Row]) -> usize {
    rows.iter().map(|r| r.units.len()).sum()
}

/// 方式選択画面の1件を作る（`TR-RCL-11`）。
#[must_use]
pub fn offer(method: Method, rows: &[Row], tones: usize) -> MethodOffer {
    let seconds = fixed_seconds(rows, tones);
    let moras: usize = rows.iter().map(|r| r.units.len()).sum();
    let hard: usize = rows.iter().map(|r| row_risk(r).hard).sum();
    MethodOffer {
        method,
        seconds,
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "所要時間は正の有限値で、セッション数は表示用の刻み"
        )]
        sessions: (seconds / SESSION_SECONDS).ceil() as usize,
        moras_per_row: if rows.is_empty() {
            0.0
        } else {
            moras as f64 / rows.len() as f64
        },
        hard_ratio: if moras == 0 {
            0.0
        } else {
            hard as f64 / moras as f64
        },
        rows: rows.len(),
    }
}

/// 同じ画面に並べてよい組み合わせに絞る（`TR-RCL-11`）。
///
/// > 所要時間の差が5分未満のプリセットを同一画面に2つ以上並べない
///
/// 並べると、選ぶ側は「どちらでもいい」と読む。 差が出ないなら1つでよい。
/// 残すのは所要時間が長いほう——到達点が広い。
#[must_use]
pub fn distinct_offers(mut offers: Vec<MethodOffer>) -> Vec<MethodOffer> {
    offers.sort_by(|a, b| a.seconds.total_cmp(&b.seconds));
    let mut out: Vec<MethodOffer> = Vec::new();
    for o in offers {
        match out.last_mut() {
            Some(prev) if (o.seconds - prev.seconds).abs() < MIN_OFFER_GAP_SECONDS => {
                *prev = o;
            }
            _ => out.push(o),
        }
    }
    out
}

/// 同一画面に並べてよい所要時間の差（秒、`TR-RCL-11`）。
pub const MIN_OFFER_GAP_SECONDS: f64 = 5.0 * 60.0;

#[cfg(test)]
mod tests {

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }
    use super::*;
    use crate::reclist::{generate_cvvc, generate_sequential, generate_single};

    /// 音高の本数だけ掛かる（`TR-RCL-09`）。
    #[test]
    fn 多音階は音高の本数だけ掛かる() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let one = fixed_seconds(&rows, 1);
        assert!((fixed_seconds(&rows, 3) - one * 3.0).abs() < 1e-9);
        // 0 本は 1 本として扱う。掛け算が 0 になると「一瞬で終わる」と出る。
        assert!((fixed_seconds(&rows, 0) - one).abs() < 1e-9);
    }

    /// 難読音は「拗音かつ外来音」（`TR-RCL-07`）。拗音そのものは常用拍。
    #[test]
    fn 難読音は拗音かつ外来音だけ() {
        let all = units(UnitSet::Extended);
        let pick = |kana: &str| {
            all.iter()
                .find(|u| u.kana == kana)
                .expect("インベントリにある")
                .clone()
        };
        let row = |units: Vec<Unit>| Row {
            id: "t".into(),
            text: String::new(),
            file_stem: "t".into(),
            units,
        };
        // きゃ は拗音だが中核セットにある。てゅ は拡張セットだけ。
        let r = row(vec![pick("きゃ"), pick("てゅ"), pick("か")]);
        let risk = row_risk(&r);
        assert_eq!((risk.palatalized, risk.hard, risk.moras), (2, 1, 3));
    }

    /// 長い行はそれだけで読み間違えやすい（`TR-RCL-07`）。
    #[test]
    fn モーラ数がスコアに効く() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let long = rows
            .iter()
            .find(|r| r.units.len() == 8)
            .expect("8モーラの行がある");
        let short = rows
            .iter()
            .find(|r| r.units.len() == 2)
            .expect("2モーラの行がある");
        assert!(row_risk(long).score > row_risk(short).score);
    }

    /// 方式選択の1件が要件どおりの成分を持つ（`TR-RCL-11`）。
    #[test]
    fn 方式選択の一件は代表値と到達点の材料を持つ() {
        let rows = generate_cvvc(&builtin_rules(), UnitSet::Extended, 8).expect("生成できる");
        let o = offer(Method::Cvvc, &rows, 1);
        assert_eq!(o.rows, rows.len());
        assert!(o.seconds > 0.0);
        assert!(o.sessions >= 1);
        assert!(o.moras_per_row > 1.0, "{}", o.moras_per_row);
        assert!((0.0..=1.0).contains(&o.hard_ratio));
    }

    /// 差が5分未満のものを2つ並べない（`TR-RCL-11`）。残すのは長いほう。
    #[test]
    fn 近すぎる所要時間は並べない() {
        let mk = |method, seconds| MethodOffer {
            method,
            seconds,
            sessions: 1,
            moras_per_row: 1.0,
            hard_ratio: 0.0,
            rows: 1,
        };
        let out = distinct_offers(vec![
            mk(Method::Single, 600.0),
            mk(Method::Sequential, 700.0),
            mk(Method::Cvvc, 3000.0),
        ]);
        assert_eq!(out.len(), 2);
        assert!((out[0].seconds - 700.0).abs() < 1e-9, "長いほうを残す");
        assert!((out[1].seconds - 3000.0).abs() < 1e-9);
    }

    /// 5分以上離れていれば両方出す。
    #[test]
    fn 離れていれば両方並べる() {
        let mk = |method, seconds| MethodOffer {
            method,
            seconds,
            sessions: 1,
            moras_per_row: 1.0,
            hard_ratio: 0.0,
            rows: 1,
        };
        let out = distinct_offers(vec![mk(Method::Single, 600.0), mk(Method::Cvvc, 1200.0)]);
        assert_eq!(out.len(), 2);
    }
}
