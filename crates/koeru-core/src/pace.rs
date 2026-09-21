//! 収録ペースと所要時間（`TR-RCL-09`, `TR-RCL-10`, `TR-RCL-11`）。
//!
//! 式は2本ある。 単独音は1項目＝1モーラなので「1単位あたりのサイクル × 単位数」、
//! 行読み上げ（連続音 / CVVC）は「1行 12.0 秒＋モーラ超過分」。周期が違う。
//!
//! # 固定値と実測を混ぜない
//!
//! 方式選択画面に出す値は固定値のまま（`TR-RCL-10`）。 未着手のユーザーには実測が無く、
//! 片方だけ実測で書き換えると方式間の比較にならない。実測が効くのは、
//! そのプロジェクトの「残り所要時間」だけ。
//!
//! # 12.0 秒は実務値であって実測ではない
//!
//! `DEC-RCL-008` が「暫定値のまま確定する」と決めている。 置き換える条件は
//! その判断記録の `review_triggers` が持つ。

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

/// 実測が効き始めるまでの行数（`TR-RCL-10` の「実測が 10 行に達するまでは固定値を使う」）。
pub const MEASUREMENT_WARMUP_ROWS: usize = 10;

/// 実測を見る窓（`TR-RCL-10` の「直近 20 行の中央値」）。
pub const MEASUREMENT_WINDOW_ROWS: usize = 20;

/// 1回の録音セッションの想定長（秒）。`TR-RCL-11` (c) の回数を出すのに使う。
///
/// [Unknown] 実測も根拠も無い。 `TR-REC-30` はセッションを切る条件（30 分の無操作）を
/// 定めているが、1回の長さは定めていない。ここは表示のための刻みでしかなく、
/// 所要時間そのものには効かない。
pub const SESSION_SECONDS: f64 = 30.0 * 60.0;

/// 収録ペース（`TR-RCL-10`）。
///
/// 固定値から始まり、実測が溜まると置き換わる。 置き換わるのは残り時間の表示だけで、
/// 方式選択画面の値は固定値のまま。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pace {
    /// 1行あたりの固定オーバーヘッド（秒）。保存から次の発声開始まで。
    pub overhead_s: f64,
    /// 1モーラあたりの発声時間（秒）。
    pub per_mora_s: f64,
    /// 録り直し率。0.2 なら 1.2 倍かかる。
    pub retake_rate: f64,
}

impl Pace {
    /// 実測が無いときの値（`TR-RCL-09`, `DEC-RCL-008`）。
    ///
    /// 1行 12.0 秒を「オーバーヘッド＋6モーラ分の発声」として割る。
    /// 超過分の 1.2 秒/モーラがそのまま発声時間になる。
    #[must_use]
    pub const fn fixed() -> Self {
        Self {
            overhead_s: SECONDS_PER_ROW_BASE - BASE_MORAS as f64 * SECONDS_PER_EXTRA_MORA,
            per_mora_s: SECONDS_PER_EXTRA_MORA,
            retake_rate: DEFAULT_RETAKE_RATE,
        }
    }
}

/// 1行の実測（`TR-RCL-10`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RowMeasurement {
    /// 発声開始から終了まで（秒）。
    pub utterance_s: f64,
    /// 保存から次の行の発声開始まで（秒）。
    pub gap_s: f64,
    /// 同一行のテイク数。1 なら録り直していない。
    pub takes: u32,
    /// その行のモーラ数。
    pub moras: usize,
}

/// 直近の実測からペースを推定する（`TR-RCL-10`）。
///
/// 中央値を採る。 平均だと、1行の長い中断（席を立つ）が全体を引っ張る。
///
/// 実測が [`MEASUREMENT_WARMUP_ROWS`] に満たなければ `None`。
/// そのときは固定値を使う——数行の実測で全体を見積もると、最初の慣れない数行が
/// そのまま「残り3時間」として出る。
#[must_use]
pub fn estimate_pace(history: &[RowMeasurement]) -> Option<Pace> {
    if history.len() < MEASUREMENT_WARMUP_ROWS {
        return None;
    }
    let window = &history[history.len().saturating_sub(MEASUREMENT_WINDOW_ROWS)..];
    let per_mora: Vec<f64> = window
        .iter()
        .filter(|m| m.moras > 0)
        .map(|m| m.utterance_s / m.moras as f64)
        .collect();
    Some(Pace {
        overhead_s: median(&window.iter().map(|m| m.gap_s).collect::<Vec<_>>()),
        per_mora_s: median(&per_mora),
        retake_rate: (median(
            &window
                .iter()
                .map(|m| f64::from(m.takes))
                .collect::<Vec<_>>(),
        ) - 1.0)
            .max(0.0),
    })
}

/// 中央値。空なら 0。
fn median(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    let mut v = xs.to_vec();
    v.sort_by(f64::total_cmp);
    let mid = v.len() / 2;
    if v.len().is_multiple_of(2) {
        (v[mid - 1] + v[mid]) / 2.0
    } else {
        v[mid]
    }
}

/// 方式選択画面に出す所要時間（秒、`TR-RCL-09`）。
///
/// 固定値で計算する。 実測では書き換えない（`TR-RCL-10`）。
/// `tones` は収録音高の本数——多音階は同じリストを音高の数だけ録る。
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

/// 残り所要時間（秒、`TR-RCL-10`）。
///
/// 実測があればそれを使い、無ければ固定値へ落ちる。
/// 式は `行数 × オーバーヘッド + 総モーラ数 × 発声時間 × (1 + 録り直し率) × 音高数`。
#[must_use]
pub fn remaining_seconds(rows: &[Row], tones: usize, history: &[RowMeasurement]) -> f64 {
    let Some(pace) = estimate_pace(history) else {
        return fixed_seconds(rows, tones);
    };
    let moras: usize = rows.iter().map(|r| r.units.len()).sum();
    let one_pass = rows.len() as f64 * pace.overhead_s
        + moras as f64 * pace.per_mora_s * (1.0 + pace.retake_rate);
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
    use super::*;
    use crate::reclist::{generate_cvvc, generate_sequential, generate_single};

    fn m(utterance_s: f64, gap_s: f64, takes: u32, moras: usize) -> RowMeasurement {
        RowMeasurement {
            utterance_s,
            gap_s,
            takes,
            moras,
        }
    }

    /// 固定値の分解が 1行 12.0 秒に戻る（`TR-RCL-09`）。
    #[test]
    fn 固定ペースは一行十二秒に一致する() {
        let p = Pace::fixed();
        let six = p.overhead_s + 6.0 * p.per_mora_s;
        assert!((six - SECONDS_PER_ROW_BASE).abs() < 1e-9, "{six}");
    }

    /// 音高の本数だけ掛かる（`TR-RCL-09`）。
    #[test]
    fn 多音階は音高の本数だけ掛かる() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let one = fixed_seconds(&rows, 1);
        assert!((fixed_seconds(&rows, 3) - one * 3.0).abs() < 1e-9);
        // 0 本は 1 本として扱う。掛け算が 0 になると「一瞬で終わる」と出る。
        assert!((fixed_seconds(&rows, 0) - one).abs() < 1e-9);
    }

    /// 実測が 10 行に達するまでは固定値（`TR-RCL-10`）。
    #[test]
    fn 実測が足りなければ固定値のまま() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let few: Vec<RowMeasurement> = (0..9).map(|_| m(4.0, 2.0, 1, 8)).collect();
        assert_eq!(estimate_pace(&few), None);
        assert!((remaining_seconds(&rows, 1, &few) - fixed_seconds(&rows, 1)).abs() < 1e-9);
    }

    /// 中央値を採る。 1行の長い中断が全体を引っ張らない（`TR-RCL-10`）。
    #[test]
    fn 外れ値が推定を引っ張らない() {
        let mut h: Vec<RowMeasurement> = (0..19).map(|_| m(4.0, 2.0, 1, 8)).collect();
        let steady = estimate_pace(&h).expect("10 行を超えている");
        h.push(m(4.0, 600.0, 1, 8));
        let with_break = estimate_pace(&h).expect("推定できる");
        assert!(
            (with_break.overhead_s - steady.overhead_s).abs() < 0.5,
            "席を立った1行で {} から {} へ動いた",
            steady.overhead_s,
            with_break.overhead_s
        );
    }

    /// 直近 20 行だけを見る（`TR-RCL-10`）。
    #[test]
    fn 窓の外は見ない() {
        let mut h: Vec<RowMeasurement> = (0..20).map(|_| m(40.0, 20.0, 3, 8)).collect();
        h.extend((0..20).map(|_| m(4.0, 2.0, 1, 8)));
        let p = estimate_pace(&h).expect("推定できる");
        assert!((p.overhead_s - 2.0).abs() < 1e-9, "{}", p.overhead_s);
        assert!((p.retake_rate - 0.0).abs() < 1e-9);
    }

    /// 録り直しは時間を増やす。
    #[test]
    fn 録り直しが多いと残り時間が伸びる() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let clean: Vec<RowMeasurement> = (0..20).map(|_| m(4.0, 2.0, 1, 8)).collect();
        let retaken: Vec<RowMeasurement> = (0..20).map(|_| m(4.0, 2.0, 3, 8)).collect();
        assert!(remaining_seconds(&rows, 1, &retaken) > remaining_seconds(&rows, 1, &clean));
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
        let rows = generate_cvvc(UnitSet::Extended, 8).expect("生成できる");
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
