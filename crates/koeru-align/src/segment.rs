//! 単独音の境界検出（`TR-ALN-11` の専用経路）。**退避経路**（`DEC-ALN-006`）。
//!
//! **一次経路は MFA の音響モデル**（`DEC-ALN-008`）。ここは MFA が使えないときと、
//! MFA の統合が終わるまでの試唱に使う。
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

use crate::confidence::{Confidence, acoustic_score};

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

impl Boundaries {
    /// アライメントの結果から、単独音の3境界を取り出す（`TR-ALN-11`）。
    ///
    /// **どのアライナが出したものでも同じ形で受ける**（`TR-ALN-03` の trait 経由）。
    /// 並びは `[sil, C, V, sil]` か `[sil, V, sil]`。
    ///
    /// それ以外の並び（連続音・CVVC）は `None`。**単独音の専用経路なので、
    /// 黙って先頭2つを使ったりしない。**
    #[must_use]
    pub fn from_alignment(a: &crate::aligner::Alignment) -> Option<Self> {
        match a.segments.len() {
            // [sil, C, V, sil]
            4 => Some(Self {
                voice_start_ms: a.segments[1].start_ms,
                vowel_start_ms: a.segments[2].start_ms,
                vowel_end_ms: a.segments[2].end_ms,
            }),
            // [sil, V, sil]。**母音始まりは発声開始と母音開始が同じ。**
            3 => Some(Self {
                voice_start_ms: a.segments[1].start_ms,
                vowel_start_ms: a.segments[1].start_ms,
                vowel_end_ms: a.segments[1].end_ms,
            }),
            _ => None,
        }
    }
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
            path: None,
            sharpness: 0.0,
            prior: 1.0,
            acoustic: acoustic_score(samples),
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

    // 音響異常度は一次経路と共通（`TR-ALN-24` の成分 (4)）。
    let acoustic = crate::confidence::acoustic_score(samples);

    Confidence {
        // **退避経路は経路確信度を出せない**（TR-ALN-24 の成分 (1)）。
        // 音響モデルを通していないので、経路という概念がそもそも無い。
        path: None,
        sharpness,
        // **1テイクだけでは集団中央値からの逸脱を測れない**（TR-ALN-12）。
        // 呼び出し側が集団統計を持ったときに差し替える。
        prior: 1.0,
        acoustic,
    }
}

/// 退避経路のアライナ（`DEC-ALN-006`, `TR-ALN-11`）。
///
/// **音響モデルを使わない。** 短時間パワーとゼロ交差率で境界を出す。
/// MFA が使えないときと、MFA の統合が終わるまでの試唱に使う。
///
/// # 出せないものがある
///
/// `TR-ALN-03` は「いずれの実装も emission 行列を返す」と求めているが、
/// **ここには経路という概念が無い**ので [`crate::aligner::Alignment::posteriors`] は
/// `None`。**0 を入れない**——0 は「確信が無い」であって「測れない」ではない。
///
/// その結果、確信度の成分 (1) 経路確信度（`TR-ALN-24`）と、
/// 次善候補（`TR-ALN-26` (4)）が出せない。**欠けた状態として扱う。**
#[derive(Debug, Clone)]
pub struct HeuristicAligner {
    config: SegmentConfig,
    identity: String,
}

impl HeuristicAligner {
    /// 既定の設定で作る。
    ///
    /// `identity` は決定性の鍵に混ぜる文字列（`TR-ALN-29`）。
    #[must_use]
    pub fn new(identity: impl Into<String>) -> Self {
        Self {
            config: SegmentConfig::default(),
            identity: identity.into(),
        }
    }

    /// 設定を差し替える。
    #[must_use]
    pub const fn with_config(mut self, config: SegmentConfig) -> Self {
        self.config = config;
        self
    }
}

impl crate::aligner::Aligner for HeuristicAligner {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn align(
        &self,
        req: &crate::aligner::AlignRequest<'_>,
    ) -> Result<crate::aligner::Alignment, crate::aligner::AlignError> {
        use crate::aligner::{AlignError, Alignment, Segment};

        // **単独音の専用経路**（`TR-ALN-11`）。子音＋母音か、母音だけ。
        if req.phonemes.is_empty() || req.phonemes.len() > 2 {
            return Err(AlignError::EmptyPhonemes);
        }
        if req.sample_rate_hz == 0 {
            return Err(AlignError::RateMismatch);
        }

        let b = detect_single(req.samples, req.sample_rate_hz, &self.config)
            .ok_or(AlignError::TooShort)?;

        #[allow(clippy::cast_precision_loss)]
        let total_ms = req.samples.len() as f64 / f64::from(req.sample_rate_hz) * 1000.0;
        let sil = crate::phoneme::Phoneme::new(crate::phoneme::SILENCE)
            .ok_or(AlignError::ModelUnavailable)?;

        // 前後の `sil` を足した並び（`TR-ALN-09` の (a)(b) と同じ形にする）。
        let mut segments = vec![Segment {
            phoneme: sil,
            start_ms: 0.0,
            end_ms: b.voice_start_ms,
        }];
        if req.phonemes.len() == 2 {
            segments.push(Segment {
                phoneme: req.phonemes[0],
                start_ms: b.voice_start_ms,
                end_ms: b.vowel_start_ms,
            });
            segments.push(Segment {
                phoneme: req.phonemes[1],
                start_ms: b.vowel_start_ms,
                end_ms: b.vowel_end_ms,
            });
        } else {
            // 母音始まり。**`voice_start` と `vowel_start` は同じ位置。**
            segments.push(Segment {
                phoneme: req.phonemes[0],
                start_ms: b.voice_start_ms,
                end_ms: b.vowel_end_ms,
            });
        }
        segments.push(Segment {
            phoneme: sil,
            start_ms: b.vowel_end_ms,
            end_ms: total_ms.max(b.vowel_end_ms),
        });

        Ok(Alignment {
            segments,
            // **経路という概念が無い。** 0 を入れずに欠けたままにする。
            posteriors: None,
            log_likelihood: None,
            grid_divergence: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// **アライメントの結果から単独音の境界を取り出せる**（`TR-ALN-03` 経由）。
    #[test]
    fn アライメントから境界を取り出せる() {
        use crate::aligner::{AlignRequest, Aligner as _};

        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let a = HeuristicAligner::new("h@1");
        let k = crate::phoneme::Phoneme::new("k").expect("ある");
        let v = crate::phoneme::Phoneme::new("a").expect("ある");
        let r = a
            .align(&AlignRequest {
                samples: &x,
                sample_rate_hz: FS,
                phonemes: &[k, v],
                grid: None,
            })
            .expect("できる");

        let b = Boundaries::from_alignment(&r).expect("取り出せる");
        let direct = detect_single(&x, FS, &SegmentConfig::default()).expect("検出");
        assert!((b.voice_start_ms - direct.voice_start_ms).abs() < 1e-9);
        assert!((b.vowel_start_ms - direct.vowel_start_ms).abs() < 1e-9);
        assert!((b.vowel_end_ms - direct.vowel_end_ms).abs() < 1e-9);
    }

    /// **母音始まりでは発声開始と母音開始が同じ。**
    #[test]
    fn 母音始まりの境界も取り出せる() {
        use crate::aligner::{AlignRequest, Aligner as _};

        let x = syllable(100.0, 0.0, 300.0, 100.0);
        let a = HeuristicAligner::new("h@1");
        let v = crate::phoneme::Phoneme::new("a").expect("ある");
        let r = a
            .align(&AlignRequest {
                samples: &x,
                sample_rate_hz: FS,
                phonemes: &[v],
                grid: None,
            })
            .expect("できる");
        let b = Boundaries::from_alignment(&r).expect("取り出せる");
        assert!((b.voice_start_ms - b.vowel_start_ms).abs() < 1e-9);
    }

    /// **連続音の並びは受けない。** 黙って先頭2つを使わない。
    #[test]
    fn 単独音でない並びは受けない() {
        use crate::aligner::{Alignment, Segment};
        let v = crate::phoneme::Phoneme::new("a").expect("ある");
        let a = Alignment {
            segments: (0..6)
                .map(|i| Segment {
                    phoneme: v,
                    start_ms: f64::from(i) * 10.0,
                    end_ms: f64::from(i + 1) * 10.0,
                })
                .collect(),
            posteriors: None,
            log_likelihood: None,
            grid_divergence: None,
        };
        assert!(Boundaries::from_alignment(&a).is_none());
    }

    /// **退避経路も `Aligner` を実装する**（`TR-ALN-03` の「いずれの実装も」）。
    #[test]
    fn 退避経路も同じ口で呼べる() {
        use crate::aligner::{AlignRequest, Aligner as _};

        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let a = HeuristicAligner::new("heuristic@1");
        let k = crate::phoneme::Phoneme::new("k").expect("ある");
        let v = crate::phoneme::Phoneme::new("a").expect("ある");

        let r = a
            .align(&AlignRequest {
                samples: &x,
                sample_rate_hz: FS,
                phonemes: &[k, v],
                grid: None,
            })
            .expect("できる");

        assert_eq!(r.segments.len(), 4);
        assert_eq!(r.segments[1].phoneme, k);
        assert_eq!(r.segments[2].phoneme, v);
        // 区間が繋がっていて単調。
        for w in r.segments.windows(2) {
            assert!((w[0].end_ms - w[1].start_ms).abs() < 1e-9);
            assert!(w[1].end_ms >= w[1].start_ms);
        }
    }

    /// **経路確信度を出せないことを、`None` で言う**（`TR-ALN-24` の成分 (1)）。
    /// 0 を入れると「確信が無い」と読まれる。
    #[test]
    fn 退避経路は事後確率を持たない() {
        use crate::aligner::{AlignRequest, Aligner as _};

        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let a = HeuristicAligner::new("heuristic@1");
        let v = crate::phoneme::Phoneme::new("a").expect("ある");
        let r = a
            .align(&AlignRequest {
                samples: &x,
                sample_rate_hz: FS,
                phonemes: &[v],
                grid: None,
            })
            .expect("できる");
        assert!(r.posteriors.is_none());
        assert!(r.log_likelihood.is_none());
        // 母音だけなら3区間。
        assert_eq!(r.segments.len(), 3);
    }

    /// **単独音の専用経路なので、3音素以上は受けない**（`TR-ALN-11`）。
    #[test]
    fn 三音素以上は受けない() {
        use crate::aligner::{AlignRequest, Aligner as _};

        let x = syllable(100.0, 50.0, 300.0, 100.0);
        let a = HeuristicAligner::new("heuristic@1");
        let v = crate::phoneme::Phoneme::new("a").expect("ある");
        let e = a
            .align(&AlignRequest {
                samples: &x,
                sample_rate_hz: FS,
                phonemes: &[v, v, v],
                grid: None,
            })
            .unwrap_err();
        assert_eq!(e.kind(), "align.empty_phonemes");
    }

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
