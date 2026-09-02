//! 話者内一貫性による外れ値補正（`TR-ALN-12`）。
//!
//! > 同一プロジェクト内で、(a) 同一エイリアス、(b) 同一音素、(c) 同一音階、の3つの集団ごとに、
//! > 先行発声位置・子音長・母音長の中央値と四分位範囲を算出し、
//! > 各テイクの推定値の集団からの逸脱を検出して補正候補を出す。
//!
//! # 中央値と四分位範囲で見る
//!
//! **平均と標準偏差を使わない。** 外れ値を探すのに、外れ値に引きずられる統計量は使えない。
//! 逸脱の大きさは「中央値から何 IQR 離れているか」で測る。
//!
//! # 補正候補であって、補正ではない
//!
//! `TR-ALN-12` は「補正候補を出す」と定めている。**勝手に値を書き換えない。**
//! ここが返すのは [`Deviation`] で、それを確信度の成分 (3) に落とすか
//! （`TR-ALN-24`）、確認キューの次善候補に出すか（`TR-ALN-26` (4)）は呼び出し側が決める。
//!
//! # 序盤は効かない
//!
//! `TR-ALN-10` notes:
//!
//! > 1テイクずつでは、`TR-ALN-12` の話者内一貫性補正に必要な集団統計が序盤に揃わない
//!
//! **[`MIN_SAMPLES`] に満たない集団では逸脱を出さない。** 2〜3件の中央値から
//! 外れ値を判定すると、最初に録ったテイクが常に「外れている」ことになる。
//!
//! # 集団は音階の中に閉じる
//!
//! `TR-ALN-22` が「話者内一貫性補正の集団統計は音階内に閉じる」と定めている。
//! [`Population`] が音階を鍵に含めているのはそのため。

/// 集団統計を作るのに要る最小の件数（`TR-ALN-10` notes）。
pub const MIN_SAMPLES: usize = 5;

/// 逸脱とみなす、中央値からの距離（IQR の倍数）。
///
/// **1.5 IQR は箱ひげ図の外れ値の定義そのもの。** 根拠のある値ではなく、
/// 慣習として広く使われている線を借りている（`TR-ALN-24` notes の
/// 「合成重みを決める根拠がない」と同じ性質の値）。
pub const OUTLIER_IQR: f64 = 1.5;

/// 集団の分け方（`TR-ALN-12`）。
///
/// **音階を必ず含む**（`TR-ALN-22`）。多音階では高音階と低音階で発声が変わるので、
/// 混ぜると全部が外れ値になる。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Population {
    /// (a) 同一エイリアス。
    Alias {
        /// エイリアス名。
        alias: String,
        /// 音階（`TR-ALN-22`）。単一音階なら空。
        pitch: String,
    },
    /// (b) 同一音素。
    Phoneme {
        /// 音素の記号。
        phoneme: String,
        /// 音階。
        pitch: String,
    },
}

/// 測る量（`TR-ALN-12`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measure {
    /// 先行発声位置。
    Preutterance,
    /// 子音長。
    ConsonantLength,
    /// 母音長。
    VowelLength,
}

impl Measure {
    /// 送信してよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::Preutterance => "measure.preutterance",
            Self::ConsonantLength => "measure.consonant_length",
            Self::VowelLength => "measure.vowel_length",
        }
    }
}

/// 集団の要約。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Summary {
    /// 件数。
    pub count: usize,
    /// 中央値。
    pub median: f64,
    /// 四分位範囲。
    pub iqr: f64,
}

/// 集団からの逸脱（`TR-ALN-12`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Deviation {
    /// 測った量。
    pub measure: Measure,
    /// そのテイクの値。
    pub value: f64,
    /// 集団の中央値。**補正候補はこの値。**
    pub median: f64,
    /// 中央値から何 IQR 離れているか。
    pub iqr_distance: f64,
}

impl Deviation {
    /// 外れ値か。
    #[must_use]
    pub fn is_outlier(&self) -> bool {
        self.iqr_distance > OUTLIER_IQR
    }

    /// 確信度の成分 (3) 事前分布逸脱の裏返し（`TR-ALN-24`）。
    ///
    /// **1.0 が「集団の真ん中」、0.0 が「大きく外れている」。**
    /// `OUTLIER_IQR` の2倍で 0 に届く形にしてある。
    #[must_use]
    pub fn prior_score(&self) -> f64 {
        (1.0 - self.iqr_distance / (OUTLIER_IQR * 2.0)).clamp(0.0, 1.0)
    }
}

/// 集団の要約を作る。**件数が足りなければ `None`**（`TR-ALN-10` notes）。
#[must_use]
pub fn summarize(values: &[f64]) -> Option<Summary> {
    if values.len() < MIN_SAMPLES {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    Some(Summary {
        count: v.len(),
        median: quantile(&v, 0.5),
        iqr: quantile(&v, 0.75) - quantile(&v, 0.25),
    })
}

/// そのテイクの値が集団からどれだけ離れているか（`TR-ALN-12`）。
///
/// **集団の件数が足りなければ `None`。** 序盤のテイクを外れ値にしない。
#[must_use]
pub fn deviation(measure: Measure, value: f64, population: &[f64]) -> Option<Deviation> {
    let s = summarize(population)?;
    // **IQR が 0 の集団では距離を測れない。** 全部同じ値なら、
    // 違う値は「無限に離れている」ことになってしまう。中央値との一致だけを見る。
    let iqr_distance = if s.iqr <= f64::EPSILON {
        if (value - s.median).abs() <= f64::EPSILON {
            0.0
        } else {
            OUTLIER_IQR * 2.0
        }
    } else {
        (value - s.median).abs() / s.iqr
    };
    Some(Deviation {
        measure,
        value,
        median: s.median,
        iqr_distance,
    })
}

/// 線形補間の分位数。**並びは昇順であること。**
fn quantile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    if sorted.len() == 1 {
        return sorted[0];
    }
    #[allow(clippy::cast_precision_loss)]
    let pos = q * (sorted.len() - 1) as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(sorted.len() - 1);
    #[allow(clippy::cast_precision_loss)]
    let frac = pos - lo as f64;
    sorted[lo] + (sorted[hi] - sorted[lo]) * frac
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **序盤は集団統計が立たない**（`TR-ALN-10` notes）。
    /// ここが破れると、最初に録ったテイクが常に外れ値になる。
    #[test]
    fn 件数が足りなければ判定しない() {
        assert!(summarize(&[1.0, 2.0, 3.0, 4.0]).is_none());
        assert!(deviation(Measure::Preutterance, 99.0, &[1.0, 2.0]).is_none());
        assert!(summarize(&[1.0, 2.0, 3.0, 4.0, 5.0]).is_some());
    }

    #[test]
    fn 中央値と四分位範囲を出せる() {
        let s = summarize(&[1.0, 2.0, 3.0, 4.0, 5.0]).expect("足りる");
        assert_eq!(s.count, 5);
        assert!((s.median - 3.0).abs() < 1e-9);
        assert!((s.iqr - 2.0).abs() < 1e-9, "{}", s.iqr);
    }

    /// **外れ値に引きずられない。** 平均と標準偏差ならここで壊れる。
    #[test]
    fn 外れ値が中央値を動かさない() {
        let normal = summarize(&[10.0, 11.0, 12.0, 13.0, 14.0]).expect("足りる");
        let with_outlier = summarize(&[10.0, 11.0, 12.0, 13.0, 9999.0]).expect("足りる");
        assert!((normal.median - with_outlier.median).abs() < 1e-9);
    }

    #[test]
    fn 集団の真ん中は逸脱していない() {
        let d = deviation(Measure::Preutterance, 30.0, &[28.0, 29.0, 30.0, 31.0, 32.0])
            .expect("足りる");
        assert!(!d.is_outlier());
        assert!((d.prior_score() - 1.0).abs() < 1e-9);
        assert!((d.median - 30.0).abs() < 1e-9);
    }

    /// **補正候補は中央値。** 値を書き換えるのではなく、候補として返す。
    #[test]
    fn 外れているテイクを見つけて補正候補を出す() {
        let pop = [28.0, 29.0, 30.0, 31.0, 32.0];
        let d = deviation(Measure::Preutterance, 90.0, &pop).expect("足りる");
        assert!(d.is_outlier());
        assert!((d.median - 30.0).abs() < 1e-9, "補正候補は中央値");
        assert!(d.prior_score() < 0.5);
    }

    /// **全部同じ値の集団でも壊れない。** IQR が 0 のとき、
    /// 素直に割ると無限大になる。
    #[test]
    fn 散らばりの無い集団でも壊れない() {
        let pop = [5.0; 6];
        let same = deviation(Measure::VowelLength, 5.0, &pop).expect("足りる");
        assert!(!same.is_outlier());
        assert!(same.iqr_distance.is_finite());

        let other = deviation(Measure::VowelLength, 7.0, &pop).expect("足りる");
        assert!(other.is_outlier());
        assert!(other.iqr_distance.is_finite());
        assert_eq!(other.prior_score(), 0.0);
    }

    /// **確信度の成分は 0.0〜1.0 に収まる**（`TR-ALN-24`）。
    #[test]
    fn 確信度の成分は範囲に収まる() {
        let pop = [10.0, 11.0, 12.0, 13.0, 14.0];
        for v in [-1e6, 0.0, 12.0, 1e6] {
            let s = deviation(Measure::ConsonantLength, v, &pop)
                .expect("足りる")
                .prior_score();
            assert!((0.0..=1.0).contains(&s), "{v} で {s}");
        }
    }

    /// **集団は音階の中に閉じる**（`TR-ALN-22`）。
    #[test]
    fn 集団の鍵に音階が入る() {
        let a = Population::Alias {
            alias: "か".to_owned(),
            pitch: "C4".to_owned(),
        };
        let b = Population::Alias {
            alias: "か".to_owned(),
            pitch: "G4".to_owned(),
        };
        assert_ne!(a, b, "同じエイリアスでも音階が違えば別の集団");
    }

    #[test]
    fn 測る量の種別は固定文字列() {
        for m in [
            Measure::Preutterance,
            Measure::ConsonantLength,
            Measure::VowelLength,
        ] {
            assert!(m.kind().starts_with("measure."));
        }
    }
}
