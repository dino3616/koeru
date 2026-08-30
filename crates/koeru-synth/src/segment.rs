//! 単独音の境界検出（`TR-ALN-11` の専用経路）。
//!
//! **単独音では1ファイルに1モーラしか入っていない。** その前提を使い、
//! 汎用の連続音アライメント経路を流用しない（`TR-ALN-11`）。
//!
//! 工程は3つ。
//!
//! 1. **無音区間トリミングとオンセット検出**で発声開始を求める
//! 2. **子音から母音への境界**を求める（`[pau, C, V, pau]` の2〜3境界に限定）
//! 3. **母音の定常区間終端**を別途推定する
//!
//! ## 境界の求め方
//!
//! 短時間パワーとゼロ交差率を使う。**子音（とくに無声摩擦音・破裂音）は
//! パワーが低くゼロ交差率が高い。母音は逆。** この差が境界になる。
//!
//! **音響モデルを使わない。** `TR-ALN-02` は「アプリ本体と同じ言語での
//! ネイティブ実装を既定とする」と定めており、単独音では音素列が
//! 2〜3境界しかないので、統計的なアライメントを持ち出す必要がない。

/// 検出した境界（ミリ秒）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Boundaries {
    /// 発声開始。無音の終わり。
    pub voice_start_ms: f64,
    /// 子音から母音への境界。**母音始まりなら `voice_start_ms` と同じ。**
    pub vowel_start_ms: f64,
    /// 母音の定常区間終端。
    pub vowel_end_ms: f64,
}

/// 検出の設定。**定数を直に書かない**（`TR-ALN-23` の規約プリセット）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SegmentConfig {
    /// 分析窓の長さ（ミリ秒）。
    pub window_ms: f64,
    /// 窓を進める幅（ミリ秒）。
    pub hop_ms: f64,
    /// 無音とみなすパワーの閾値。**最大パワーに対する比。**
    pub silence_ratio: f64,
    /// 子音とみなすゼロ交差率の下限（1秒あたりの交差数）。
    pub consonant_zcr_per_sec: f64,
    /// 母音の定常区間が終わったとみなすパワーの比。
    pub vowel_end_ratio: f64,
}

impl Default for SegmentConfig {
    fn default() -> Self {
        Self {
            window_ms: 20.0,
            hop_ms: 5.0,
            // -40 dB 相当。**環境ノイズを拾わない程度に低く。**
            silence_ratio: 0.01,
            // 有声母音のゼロ交差率は概ね 1000〜2000/秒。無声子音はその数倍。
            consonant_zcr_per_sec: 4000.0,
            // 減衰して最大の 1/4 を割ったら定常区間の外とみなす。
            vowel_end_ratio: 0.25,
        }
    }
}

/// フレームごとの特徴量。
struct Frames {
    power: Vec<f64>,
    zcr: Vec<f64>,
    hop_ms: f64,
}

fn analyze(samples: &[f64], sample_rate_hz: u32, cfg: &SegmentConfig) -> Frames {
    let per_ms = f64::from(sample_rate_hz) / 1000.0;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let win = (cfg.window_ms * per_ms) as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let hop = (cfg.hop_ms * per_ms).max(1.0) as usize;

    let mut power = Vec::new();
    let mut zcr = Vec::new();
    let mut at = 0;
    while at + win <= samples.len() {
        let w = &samples[at..at + win];
        let p = w.iter().map(|v| v * v).sum::<f64>() / win as f64;
        let crossings = w
            .windows(2)
            .filter(|p| (p[0] < 0.0) != (p[1] < 0.0))
            .count();
        // 1秒あたりに直す
        let z = crossings as f64 * f64::from(sample_rate_hz) / win as f64;
        power.push(p);
        zcr.push(z);
        at += hop;
    }
    Frames {
        power,
        zcr,
        hop_ms: cfg.hop_ms,
    }
}

/// 単独音1ファイルの境界を求める（`TR-ALN-11`）。
///
/// 戻り値が `None` なのは、**発声が見つからなかったとき**。
/// 無音のファイル、または閾値を超えるパワーが無いとき。
#[tracing::instrument(skip(samples), fields(len = samples.len()))]
#[must_use]
pub fn detect_single(
    samples: &[f64],
    sample_rate_hz: u32,
    cfg: &SegmentConfig,
) -> Option<Boundaries> {
    let f = analyze(samples, sample_rate_hz, cfg);
    if f.power.is_empty() {
        return None;
    }
    let peak = f.power.iter().copied().fold(0.0_f64, f64::max);
    if peak <= 0.0 {
        return None;
    }
    let floor = peak * cfg.silence_ratio;

    // ── 1. 無音を越えて発声が始まる位置 ──────────────────
    let start_idx = f.power.iter().position(|p| *p > floor)?;
    let voice_start_ms = start_idx as f64 * f.hop_ms;

    // ── 2. 子音から母音への境界 ──────────────────────────
    // **発声開始からゼロ交差率が下がるまでが子音。** 高いままなら母音始まりとみなす。
    let mut vowel_idx = start_idx;
    for i in start_idx..f.zcr.len() {
        if f.power[i] <= floor {
            break; // 発声が終わった
        }
        if f.zcr[i] < cfg.consonant_zcr_per_sec {
            vowel_idx = i;
            break;
        }
        vowel_idx = i;
    }
    let vowel_start_ms = vowel_idx as f64 * f.hop_ms;

    // ── 3. 母音の定常区間終端 ────────────────────────────
    // **母音のピークから減衰して閾値を割るところ。**
    let vowel_peak = f.power[vowel_idx..]
        .iter()
        .copied()
        .fold(0.0_f64, f64::max)
        .max(f64::MIN_POSITIVE);
    let end_floor = vowel_peak * cfg.vowel_end_ratio;
    let mut end_idx = f.power.len() - 1;
    // ピークを過ぎてから探す
    let peak_idx = f.power[vowel_idx..]
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map_or(vowel_idx, |(i, _)| vowel_idx + i);
    for i in peak_idx..f.power.len() {
        if f.power[i] < end_floor {
            end_idx = i;
            break;
        }
    }
    let vowel_end_ms = end_idx as f64 * f.hop_ms;

    Some(Boundaries {
        voice_start_ms,
        vowel_start_ms: vowel_start_ms.max(voice_start_ms),
        vowel_end_ms: vowel_end_ms.max(vowel_start_ms),
    })
}

/// 確信度の成分（`TR-ALN-24`）。
///
/// **単独音では収録グリッド由来の項が存在せず、実質3成分**（`TR-ALN-24` の [Fact]）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confidence {
    /// 境界の鋭さ。**境界前後のパワー比が大きいほど高い。**
    pub sharpness: f64,
    /// 音響異常度の裏返し。クリッピングやレベル不足が無いほど高い。
    pub acoustic: f64,
    /// 集団中央値からの逸脱の裏返し。**外れ値でないほど高い**（`TR-ALN-12`）。
    /// 1テイクだけでは判定できないので、既定は 1.0。
    pub consistency: f64,
}

impl Confidence {
    /// 合成スコア。**機械導出群にのみ付与する**（`TR-ALN-24`）。
    #[must_use]
    pub fn score(&self) -> f64 {
        (self.sharpness * self.acoustic * self.consistency).clamp(0.0, 1.0)
    }
}

/// 境界の確信度を求める。
#[must_use]
pub fn confidence(
    samples: &[f64],
    sample_rate_hz: u32,
    b: &Boundaries,
    cfg: &SegmentConfig,
) -> Confidence {
    let f = analyze(samples, sample_rate_hz, cfg);
    if f.power.is_empty() {
        return Confidence {
            sharpness: 0.0,
            acoustic: 0.0,
            consistency: 1.0,
        };
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let at = ((b.vowel_start_ms / f.hop_ms) as usize).min(f.power.len() - 1);

    // **境界「周辺」を見る**（TR-ALN-24 の境界鋭さ）。ファイル全体の最大値を比べると、
    // 離れた位置の山に引きずられて、境界そのものの立ち方が映らない。
    // 窓が境界をまたぐぶん（window/hop フレーム）は避けて、その外側を取る。
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let skip = (cfg.window_ms / cfg.hop_ms).ceil() as usize;
    let span = skip.max(1) * 3;
    let lo_end = at.saturating_sub(skip);
    let lo_start = lo_end.saturating_sub(span);
    let hi_start = (at + skip).min(f.power.len());
    let hi_end = (hi_start + span).min(f.power.len());

    let before = f.power[lo_start..lo_end]
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);
    let after = f.power[hi_start..hi_end]
        .iter()
        .copied()
        .fold(0.0_f64, f64::max);
    // **境界の前後でパワーが変わるほど、境界がはっきりしている。**
    let sharpness = if after > 0.0 {
        (1.0 - (before / after).min(1.0)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    // クリッピング（±1.0 に張り付く）とレベル不足を見る。
    let clipped = samples.iter().filter(|v| v.abs() >= 0.999).count();
    let peak = samples.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    let acoustic = if clipped > samples.len() / 1000 {
        0.2 // **1000サンプルに1つ以上張り付いていたら疑う。**
    } else if peak < 0.01 {
        0.3 // レベル不足
    } else {
        1.0
    };

    Confidence {
        sharpness,
        acoustic,
        consistency: 1.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FS: u32 = 44_100;

    /// 無音 + 無声子音（雑音）+ 母音（倍音）+ 減衰 という形を作る。
    fn syllable(silence_ms: f64, consonant_ms: f64, vowel_ms: f64, tail_ms: f64) -> Vec<f64> {
        let per_ms = f64::from(FS) / 1000.0;
        let n = |ms: f64| (ms * per_ms) as usize;
        let mut out = vec![0.0; n(silence_ms)];

        // 無声子音: 高いゼロ交差率の雑音。**振幅は母音より小さい。**
        let mut state = 0x1234_5678_9abc_def0_u64;
        for _ in 0..n(consonant_ms) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let r = (state >> 11) as f64 / (1_u64 << 53) as f64;
            out.push((r - 0.5) * 0.1);
        }

        // 母音: 倍音を持つ 220 Hz。
        let start = out.len();
        for i in 0..n(vowel_ms) {
            let t = (start + i) as f64 / f64::from(FS);
            let mut v = 0.0;
            for k in 1..=8 {
                v += (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin() / f64::from(k);
            }
            out.push(v * 0.3);
        }

        // 減衰
        let tail = n(tail_ms);
        for i in 0..tail {
            let t = (start + n(vowel_ms) + i) as f64 / f64::from(FS);
            let decay = 1.0 - (i as f64 / tail as f64);
            let mut v = 0.0;
            for k in 1..=8 {
                v += (std::f64::consts::TAU * 220.0 * f64::from(k) * t).sin() / f64::from(k);
            }
            out.push(v * 0.3 * decay);
        }
        out
    }

    #[test]
    fn 発声開始を見つけられる() {
        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出できる");
        assert!(
            (b.voice_start_ms - 100.0).abs() < 25.0,
            "無音 100ms の直後: {:.1}ms",
            b.voice_start_ms
        );
    }

    /// **子音から母音への境界を見つけられる**（TR-ALN-11 の (2)）。
    #[test]
    fn 子音から母音への境界を見つけられる() {
        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出できる");
        assert!(
            (b.vowel_start_ms - 150.0).abs() < 30.0,
            "無音 100 + 子音 50 の位置: {:.1}ms",
            b.vowel_start_ms
        );
        assert!(b.vowel_start_ms > b.voice_start_ms, "子音ぶん右にある");
    }

    /// **母音の定常区間終端を見つけられる**（TR-ALN-11 の (3)）。
    #[test]
    fn 母音の定常区間終端を見つけられる() {
        let x = syllable(100.0, 50.0, 300.0, 200.0);
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出できる");
        assert!(
            b.vowel_end_ms > b.vowel_start_ms + 200.0,
            "母音のあいだは続く: {:.1}ms",
            b.vowel_end_ms
        );
        assert!(
            b.vowel_end_ms < 700.0,
            "減衰しきる前に終わる: {:.1}ms",
            b.vowel_end_ms
        );
    }

    /// **母音始まりでは子音の境界が発声開始と一致する。**
    #[test]
    fn 母音始まりでも検出できる() {
        let x = syllable(100.0, 0.0, 300.0, 100.0);
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出できる");
        assert!(
            (b.vowel_start_ms - b.voice_start_ms).abs() < 15.0,
            "子音が無いので一致する: {:.1} / {:.1}",
            b.voice_start_ms,
            b.vowel_start_ms
        );
    }

    /// **無音のファイルからは何も返さない。**
    #[test]
    fn 無音からは境界を返さない() {
        let x = vec![0.0_f64; 44_100];
        assert!(detect_single(&x, FS, &SegmentConfig::default()).is_none());
    }

    #[test]
    fn 短すぎる入力で落ちない() {
        let x = vec![0.1_f64; 10];
        let _ = detect_single(&x, FS, &SegmentConfig::default());
    }

    /// **境界がはっきりしているほど確信度が高い。**
    #[test]
    fn 確信度は境界の鋭さを映す() {
        let clear = syllable(100.0, 50.0, 300.0, 100.0);
        let b = detect_single(&clear, FS, &SegmentConfig::default()).expect("検出");
        let c = confidence(&clear, FS, &b, &SegmentConfig::default());
        assert!(c.sharpness > 0.5, "はっきりした境界: {:.3}", c.sharpness);
        assert!((c.acoustic - 1.0).abs() < 1e-9, "音響の異常なし");
        assert!(c.score() > 0.5);
    }

    /// **クリッピングがあると確信度を下げる**（TR-ALN-24 の音響異常度）。
    #[test]
    fn クリッピングは確信度を下げる() {
        let mut x = syllable(100.0, 50.0, 300.0, 100.0);
        for v in x.iter_mut().skip(10_000).take(5_000) {
            *v = 1.0;
        }
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出");
        let c = confidence(&x, FS, &b, &SegmentConfig::default());
        assert!(c.acoustic < 0.5, "疑う: {:.3}", c.acoustic);
    }

    /// **レベル不足も確信度を下げる。**
    #[test]
    fn レベル不足は確信度を下げる() {
        let x: Vec<f64> = syllable(100.0, 50.0, 300.0, 100.0)
            .iter()
            .map(|v| v * 0.001)
            .collect();
        let b = detect_single(&x, FS, &SegmentConfig::default()).expect("検出");
        let c = confidence(&x, FS, &b, &SegmentConfig::default());
        assert!(c.acoustic < 0.5, "疑う: {:.3}", c.acoustic);
    }
}
