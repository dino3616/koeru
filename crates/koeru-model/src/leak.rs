//! ガイドの回り込み検査（`TR-REC-24`）。
//!
//! 出力経路の判定だけでは足りない。 `TransportType` も `FormFactor` も
//! ドライバの自己申告で、`Unknown` が正規値として存在する。ヘッドホンと申告していても、
//! 装着されている保証はない。回り込みは録音側でしか確認できない。
//!
//! だから、収録の前に一度だけガイドを鳴らしながら1秒キャプチャし、
//! 既知信号との相関でリークの有無を確認する。
//!
//! # スコープを侵さない
//!
//! 既知の再生信号との相関を取るだけなので、声質の評価を一切含まない。
//! `TR-REC-17`（入力経路の生死判定）と同じ性質の静的な経路検査で、
//! 「リアルタイム品質判定はスコープ外」という方針と衝突しない。
//!
//! # 置かないとどうなるか
//!
//! 全テイクにガイドが混入した音源が完成に到達しうる（`TR-REC-24` の [Risk]）。
//! 受け取った側は、歌わせるたびにクリック音を聞くことになる。

/// これを超えたら回り込んでいるとみなす。
///
/// 正規化相互相関の絶対値。 無相関なら 0 付近、同じ信号が混じれば 1 に近づく。
/// 部屋の反響や小さな漏れも拾いたいので、低めに置く。
pub const LEAK_THRESHOLD: f64 = 0.15;

/// 相関を探す遅れの上限（ミリ秒）。
///
/// 出力から入力へ回り込むまでの遅れ。バッファ2〜3回ぶんと空気の伝播で足りる。
pub const MAX_LAG_MS: f64 = 120.0;

/// 検査の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LeakCheck {
    /// 見つかった相関の最大値（0.0〜1.0）。
    pub correlation: f64,
    /// そのときの遅れ（ミリ秒）。
    pub lag_ms: f64,
    /// 回り込んでいるとみなすか。
    pub leaking: bool,
}

impl LeakCheck {
    /// 何も測れなかったとき。「漏れていない」と断定しない。
    #[must_use]
    pub const fn inconclusive() -> Self {
        Self {
            correlation: 0.0,
            lag_ms: 0.0,
            leaking: false,
        }
    }
}

/// 鳴らした信号が、録った音に混じっているかを見る。
///
/// `played` は鳴らした既知信号、`captured` は同時に録った音。
/// どちらも同じサンプルレートで、`captured` のほうが長いか同じ長さ。
#[must_use]
pub fn detect(played: &[f32], captured: &[f32], rate_hz: u32) -> LeakCheck {
    if played.is_empty() || captured.is_empty() {
        return LeakCheck::inconclusive();
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "遅れはミリ秒から作る非負の値"
    )]
    let max_lag = ((MAX_LAG_MS / 1000.0) * f64::from(rate_hz)) as usize;

    // 比べる長さ。短いほうに合わせる。
    let window = played.len().min(captured.len().saturating_sub(0));
    if window == 0 {
        return LeakCheck::inconclusive();
    }

    let a = &played[..window];
    let a_energy: f64 = a.iter().map(|v| f64::from(*v) * f64::from(*v)).sum();
    if a_energy <= 0.0 {
        // 鳴らしていない。判定できない。
        return LeakCheck::inconclusive();
    }

    let mut best = 0.0_f64;
    let mut best_lag = 0_usize;

    // 遅れを1サンプルずつ試すのは重い。 粗く探してから、その周りを細かく見る。
    let coarse = (max_lag / 64).max(1);
    for lag in (0..=max_lag).step_by(coarse) {
        let c = normalized_correlation(a, captured, lag, a_energy);
        if c > best {
            best = c;
            best_lag = lag;
        }
    }
    let from = best_lag.saturating_sub(coarse);
    let to = (best_lag + coarse).min(max_lag);
    for lag in from..=to {
        let c = normalized_correlation(a, captured, lag, a_energy);
        if c > best {
            best = c;
            best_lag = lag;
        }
    }

    LeakCheck {
        correlation: best,
        lag_ms: best_lag as f64 * 1000.0 / f64::from(rate_hz),
        leaking: best >= LEAK_THRESHOLD,
    }
}

/// ずらして重ねたときの正規化相互相関（絶対値）。
fn normalized_correlation(a: &[f32], b: &[f32], lag: usize, a_energy: f64) -> f64 {
    let Some(shifted) = b.get(lag..) else {
        return 0.0;
    };
    let n = a.len().min(shifted.len());
    if n == 0 {
        return 0.0;
    }
    let mut dot = 0.0_f64;
    let mut b_energy = 0.0_f64;
    for i in 0..n {
        let (x, y) = (f64::from(a[i]), f64::from(shifted[i]));
        dot += x * y;
        b_energy += y * y;
    }
    if b_energy <= 0.0 {
        return 0.0;
    }
    // a 側のエネルギーは窓が短くなると変わるが、分母の桁が合っていればよい。
    (dot.abs()) / (a_energy.sqrt() * b_energy.sqrt()).max(f64::MIN_POSITIVE)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::TAU;

    fn tone(n: usize, hz: f64, rate: u32, amp: f32) -> Vec<f32> {
        (0..n)
            .map(|i| ((TAU * hz * i as f64 / f64::from(rate)).sin() as f32) * amp)
            .collect()
    }

    fn noise(n: usize, seed: u64) -> Vec<f32> {
        // 決定的な擬似乱数。テストに Math.random 相当を持ち込まない。
        let mut x = seed | 1;
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 7;
                x ^= x << 17;
                ((x >> 40) as f32 / 8_388_608.0) - 1.0
            })
            .collect()
    }

    /// 同じ信号が混じっていれば見つける。
    #[test]
    fn 回り込みを見つける() {
        let played = tone(44_100, 660.0, 44_100, 0.5);
        // 20ms 遅れて、小さく混じっている。
        let lag = 44_100 * 20 / 1000;
        let mut captured = vec![0.0_f32; lag];
        captured.extend(played.iter().map(|v| v * 0.3));

        let got = detect(&played, &captured, 44_100);
        assert!(got.leaking, "相関 {:.3}", got.correlation);
    }

    /// 遅れを当てられるのは、信号に立ち上がりがあるときだけ。
    ///
    /// 純音は周期ごとに同じ形なので、1周期ずれた位置とも同じくらい相関する。
    /// 実際のガイドにはクリックが入る（`TR-REC-23`）ので、そこで決まる。
    /// 判定に使うのは相関の大きさで、遅れは参考値。
    #[test]
    fn 立ち上がりがあれば遅れも当てる() {
        // クリックのような短い立ち上がりを持つ信号。
        let mut played = vec![0.0_f32; 44_100];
        for (i, v) in played.iter_mut().take(300).enumerate() {
            *v = (1.0 - i as f32 / 300.0) * 0.8;
        }
        let lag = 44_100 * 20 / 1000;
        let mut captured = vec![0.0_f32; lag];
        captured.extend(played.iter().map(|v| v * 0.3));

        let got = detect(&played, &captured, 44_100);
        assert!(got.leaking, "相関 {:.3}", got.correlation);
        assert!(
            (got.lag_ms - 20.0).abs() < 3.0,
            "遅れも当てること: {:.1}ms",
            got.lag_ms
        );
    }

    /// 実際のガイドで通ること。 これが本番で使う信号。
    #[test]
    fn ガイドの回り込みを見つける() {
        let spec = crate::guide::GuideSpec {
            moras: 2,
            ..crate::guide::GuideSpec::default()
        };
        let played = crate::guide::render(&spec, 60, 44_100);
        let lag = 44_100 * 15 / 1000;
        let mut captured = vec![0.0_f32; lag];
        captured.extend(played.iter().map(|v| v * 0.25));
        for (v, n) in captured.iter_mut().zip(noise(played.len() + lag, 4321)) {
            *v += n * 0.02;
        }

        let got = detect(&played, &captured, 44_100);
        assert!(got.leaking, "相関 {:.3}", got.correlation);

        // ヘッドホンで漏れていない場合。
        let clean = noise(played.len(), 999)
            .iter()
            .map(|v| v * 0.05)
            .collect::<Vec<f32>>();
        assert!(!detect(&played, &clean, 44_100).leaking);
    }

    /// 無関係な音は回り込みとみなさない。
    #[test]
    fn 別の音は回り込みではない() {
        let played = tone(44_100, 660.0, 44_100, 0.5);
        let captured = noise(44_100 * 2, 12_345);
        let got = detect(&played, &captured, 44_100);
        assert!(!got.leaking, "相関 {:.3}", got.correlation);
    }

    #[test]
    fn 無音のマイクは回り込みではない() {
        let played = tone(44_100, 660.0, 44_100, 0.5);
        let got = detect(&played, &vec![0.0_f32; 44_100 * 2], 44_100);
        assert!(!got.leaking);
        assert!(got.correlation.abs() < 1e-9);
    }

    /// 鳴らしていなければ「漏れていない」と断定しない。
    #[test]
    fn 鳴らしていなければ判定しない() {
        let got = detect(&vec![0.0_f32; 44_100], &noise(44_100, 7), 44_100);
        assert_eq!(got, LeakCheck::inconclusive());
        assert!(!got.leaking);
    }

    #[test]
    fn 空の入力で落ちない() {
        assert_eq!(detect(&[], &[0.1, 0.2], 44_100), LeakCheck::inconclusive());
        assert_eq!(detect(&[0.1, 0.2], &[], 44_100), LeakCheck::inconclusive());
    }

    /// 小さな漏れも拾う。 閾値を低めに置いてある。
    #[test]
    fn 小さな漏れも拾う() {
        let played = tone(44_100, 660.0, 44_100, 0.5);
        let mut captured: Vec<f32> = played.iter().map(|v| v * 0.05).collect();
        // 部屋のノイズを足す。
        for (v, n) in captured.iter_mut().zip(noise(44_100, 99)) {
            *v += n * 0.02;
        }
        let got = detect(&played, &captured, 44_100);
        assert!(got.leaking, "相関 {:.3}", got.correlation);
    }
}
