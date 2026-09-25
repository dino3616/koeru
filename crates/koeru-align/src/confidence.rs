//! 確信度の成分を測る（`TR-ALN-24`）。
//!
//! 値の型と、値から読む規則（合成・主因）は `koeru_model::confidence` が持つ
//! （`DEC-PLT-034`）。 ここはアライメントの事後確率とサンプルから成分を組み立てる計算だけ。
//! 成分の意味と、合成の重みに根拠が無いことは向こうの文書にある。

pub use koeru_model::confidence::{Cause, Confidence};

/// 音響異常度の裏返し（`TR-ALN-24` の成分 (4)）。
///
/// 一次経路と退避経路で同じものを使う。 クリッピングもレベル不足も、
/// どちらのアライナを通ったかとは関係がない。
#[must_use]
pub fn acoustic_score(samples: &[f64]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let ceiling = f64::from(koeru_core::analysis::CLIP_THRESHOLD);
    let clipped = samples.iter().filter(|v| v.abs() >= ceiling).count();
    let peak = samples.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    // 掛けて比べる。 `clipped > len / 1000` だと整数の切り捨てで境界が抜ける
    // ——ちょうど 1000 サンプルに1つのとき `1 > 1` が偽になり、
    // **「1つ以上で疑う」と書いてあるのに素通りしていた。**
    if clipped * 1000 >= samples.len() {
        0.2 // **1000サンプルに1つ以上張り付いていたら疑う。**
    } else if peak < 0.01 {
        0.3 // レベル不足
    } else {
        1.0
    }
}

/// アライメントの結果から確信度を組み立てる（`TR-ALN-24`）。
///
/// 事後確率を持つアライナ専用。 持たない退避経路は `None` を返すので、
/// 呼び出し側は [`crate::segment::confidence`] へ落ちる。
///
/// # 成分の作り方
///
/// - (1) 経路確信度 — フレームごとに最大の事後確率を取り、その平均。
///   解が1つに定まっているほど高い。 競っていれば下がる
/// - (2) 境界鋭さ — 各境界の前後で「最大と次善の差」を測り、いちばん弱い境界を採る。
///   `TR-ALN-24` の「境界周辺の事後確率の集中度と次善解とのスコア差」がこれ
/// - (3) 事前分布逸脱 — 1テイクでは測れない（`TR-ALN-12` の集団統計が要る）。
///   `1.0` を入れ、呼び出し側が集団を持ったときに [`crate::consistency`] で差し替える
/// - (4) 音響異常度 — [`acoustic_score`]
///
/// 式に根拠はない（`TR-ALN-24` notes、`DEC-ALN-007`）。測れるようになったら差し替える。
#[must_use]
pub fn from_alignment(a: &crate::aligner::Alignment, samples: &[f64]) -> Option<Confidence> {
    from_alignment_span(a, samples, f64::NEG_INFINITY, f64::INFINITY)
}

/// 時間の範囲を区切って組み立てる（`TR-ALN-24`, `TR-ALN-26`）。
///
/// **1ファイルに複数モーラが入る**（`DEC-ALN-013`）。ファイル全体で1つの
/// 確信度を作ってエントリ全部へ写すと、**1モーラが曖昧なだけで全部の
/// 確信度と主因が同じになり、どのエントリを見ればよいかが消える。**
/// `TR-ALN-26` が要求しているのは「その項目の」内訳。
///
/// `samples` はその範囲だけを切ったもの。 音響異常度はサンプルから測るので、
/// ファイル全体を渡すと範囲外の割れが混ざる。
///
/// 範囲に境界が1つも無ければ、境界鋭さは 1.0 のまま。 境界が無いのは
/// 「曖昧でない」ではなく「測る相手が無い」だが、低く倒すと
/// 1音素のモーラが常に確認へ回る。
#[must_use]
pub fn from_alignment_span(
    a: &crate::aligner::Alignment,
    samples: &[f64],
    from_ms: f64,
    to_ms: f64,
) -> Option<Confidence> {
    let p = a.posteriors.as_ref()?;
    if p.frames == 0 || p.phonemes == 0 {
        return None;
    }

    // 範囲をフレームへ写す。
    let frame_of = |ms: f64| {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let f = (ms / p.hop_ms).round().max(0.0) as usize;
        f.min(p.frames - 1)
    };
    let lo = if from_ms.is_finite() {
        frame_of(from_ms)
    } else {
        0
    };
    let hi = if to_ms.is_finite() {
        frame_of(to_ms)
    } else {
        p.frames - 1
    };
    let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };

    // (1) 経路確信度。フレームごとの最大の平均。
    let mut sum = 0.0_f64;
    for t in lo..=hi {
        let mut best = 0.0_f32;
        for i in 0..p.phonemes {
            best = best.max(p.get(t, i));
        }
        sum += f64::from(best);
    }
    #[allow(clippy::cast_precision_loss)]
    let path = (sum / (hi - lo + 1) as f64).clamp(0.0, 1.0);

    // (2) 境界鋭さ。範囲の中でいちばん弱い境界が決める。
    // 1箇所でも曖昧なら、そのエントリは確認へ回したい。
    let mut sharpness = 1.0_f64;
    for w in a.segments.windows(2) {
        let f = frame_of(w[0].end_ms);
        if f < lo || f > hi {
            continue;
        }
        // 境界をまたぐ2フレームを見る。
        let before = f.saturating_sub(1);
        let m = margin(p, before).min(margin(p, f));
        sharpness = sharpness.min(m);
    }

    Some(Confidence {
        path: Some(path),
        sharpness,
        // 1テイクでは測れない（`TR-ALN-12`）。
        prior: 1.0,
        acoustic: acoustic_score(samples),
    })
}

/// そのフレームでの「最大と次善の差」（`TR-ALN-24` の「次善解とのスコア差」）。
fn margin(p: &crate::aligner::Posteriors, t: usize) -> f64 {
    let (mut top, mut second) = (0.0_f32, 0.0_f32);
    for i in 0..p.phonemes {
        let v = p.get(t, i);
        if v > top {
            second = top;
            top = v;
        } else if v > second {
            second = v;
        }
    }
    f64::from(top - second).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn posteriors(rows: &[[f32; 3]]) -> crate::aligner::Posteriors {
        crate::aligner::Posteriors {
            frames: rows.len(),
            phonemes: 3,
            hop_ms: 10.0,
            values: rows.iter().flatten().copied().collect(),
        }
    }

    fn alignment(rows: &[[f32; 3]], edges: [f64; 4]) -> crate::aligner::Alignment {
        let sil = crate::phoneme::Phoneme::new(crate::phoneme::SILENCE).expect("ある");
        crate::aligner::Alignment {
            segments: (0..3)
                .map(|i| crate::aligner::Segment {
                    phoneme: sil,
                    start_ms: edges[i],
                    end_ms: edges[i + 1],
                })
                .collect(),
            posteriors: Some(posteriors(rows)),
            log_likelihood: Some(-1.0),
            grid_divergence: None,
        }
    }

    /// MFA の結果からは経路確信度が出る（`TR-ALN-24` の成分 (1)）。
    /// ここが `None` のままだと、確認キューの並びが曖昧さを区別できない。
    #[test]
    fn アライメントから経路確信度が出る() {
        let rows = [
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let a = alignment(&rows, [0.0, 20.0, 40.0, 60.0]);
        let c = from_alignment(&a, &[0.5; 1000]).expect("出る");
        assert!(c.is_complete(), "経路確信度が入っている");
        assert!((c.path.unwrap() - 1.0).abs() < 1e-6);
        assert!((c.sharpness - 1.0).abs() < 1e-6, "はっきり切り替わっている");
    }

    /// 解が競っていれば経路確信度が落ちる。
    #[test]
    fn 競っている解では経路確信度が落ちる() {
        let rows = [[0.4, 0.35, 0.25]; 6];
        let a = alignment(&rows, [0.0, 20.0, 40.0, 60.0]);
        let c = from_alignment(&a, &[0.5; 1000]).expect("出る");
        assert!(c.path.unwrap() < 0.5, "path {:?}", c.path);
        assert!(c.sharpness < 0.2, "次善との差が小さい: {}", c.sharpness);
        assert!(c.score() < 0.2);
    }

    /// いちばん弱い境界が全体を決める（1箇所でも曖昧なら確認へ回したい）。
    #[test]
    fn 弱い境界が全体を決める() {
        let rows = [
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            // ここの切り替わりが曖昧
            [0.5, 0.5, 0.0],
            [0.4, 0.6, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ];
        let a = alignment(&rows, [0.0, 20.0, 40.0, 60.0]);
        let c = from_alignment(&a, &[0.5; 1000]).expect("出る");
        assert!(c.sharpness < 0.3, "sharpness {}", c.sharpness);
    }

    /// 事後確率を持たない退避経路には使えない。
    #[test]
    fn 事後確率が無ければ組み立てない() {
        let mut a = alignment(&[[1.0, 0.0, 0.0]; 3], [0.0, 10.0, 20.0, 30.0]);
        a.posteriors = None;
        assert!(from_alignment(&a, &[0.5; 100]).is_none());
    }

    /// 音響異常度は経路によらず同じ（`TR-ALN-24` の成分 (4)）。
    #[test]
    fn 音響異常度は共通() {
        assert!((acoustic_score(&[0.5; 1000]) - 1.0).abs() < 1e-9);
        assert!(acoustic_score(&[0.0001; 1000]) < 0.5, "レベル不足");
        let mut clipped = vec![0.5; 1000];
        for v in clipped.iter_mut().take(5) {
            *v = 1.0;
        }
        assert!(acoustic_score(&clipped) < 0.5, "クリッピング");
        assert_eq!(acoustic_score(&[]), 0.0);
    }
}
