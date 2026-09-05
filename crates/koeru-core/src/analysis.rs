//! 録音停止時に算出して DB へ入れる値（`TR-PKG-05`, `TR-PKG-42`）。
//!
//! 書き出しと再開で WAV を再走査しないのが目的。
//! 3時間ぶんの WAV を起動のたびに読み直すと、再開が即座でなくなる。
//!
//! ここは純粋な計算だけを持つ。保存は [`crate::db`]。

use crate::frq::{Frq, HOP_SIZE};

/// 波形サムネイルのバケット数。
///
/// 表示幅に依らない固定値にする。 ウィンドウ幅ごとに作り直すと、
/// 結局 WAV を読み直すことになる。描画側はここから間引くか補間する。
pub const THUMBNAIL_BUCKETS: usize = 512;

/// クリップとみなす下限。
///
/// 1.0 ではなく 0.999 にしてある。16bit で受けた値を f32 へ正規化すると
/// 天井は 32767/32768 = 0.99997 で、負側にしか 1.0 は現れない。
/// 1.0 で判定すると、実際に張り付いた録音が「割れていない」と出る。
///
/// 画面側にも同じ値がある（`ui/src/lib/levels.ts`）。ずれは
/// `koeru-app` の `画面のクリップ閾値がrustと一致する` が落とす。
pub const CLIP_THRESHOLD: f32 = 0.999;

/// 録音停止時に確定させる値。
#[derive(Debug, Clone, PartialEq)]
pub struct TakeAnalysis {
    /// 絶対値の最大。[`CLIP_THRESHOLD`] 以上ならクリップ。
    pub peak: f32,
    /// 周波数表（`.frq`）。書き出し時に推定し直さない。
    pub frq: Frq,
    /// 波形サムネイル。バケットごとのピークを 0〜255 で持つ。
    pub thumbnail: Vec<u8>,
}

impl TakeAnalysis {
    /// 波形と、既に走っている解析の F0 系列から作る。
    ///
    /// `source_f0` は `source_period_s` 秒ごと、無声は 0
    /// （`koeru-synth` の推定結果をそのまま渡す）。
    #[must_use]
    pub fn compute(samples: &[f32], rate_hz: u32, source_f0: &[f64], source_period_s: f64) -> Self {
        Self {
            peak: peak(samples),
            frq: Frq::from_analysis(samples, rate_hz, source_f0, source_period_s),
            thumbnail: thumbnail(samples, THUMBNAIL_BUCKETS),
        }
    }

    /// `.frq` の hop。
    #[must_use]
    pub const fn hop_size(&self) -> u32 {
        HOP_SIZE
    }
}

/// 絶対値の最大。
#[must_use]
pub fn peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0_f32, |m, s| m.max(s.abs()))
}

/// バケットごとのピークを 0〜255 で持つ包絡線。
///
/// 平均ではなくピークを採る。 平均にすると、短いクリックや破裂音が
/// 見えなくなり、波形を見て切り直す判断ができない。
#[must_use]
pub fn thumbnail(samples: &[f32], buckets: usize) -> Vec<u8> {
    if buckets == 0 {
        return Vec::new();
    }
    if samples.is_empty() {
        return vec![0; buckets];
    }
    let mut out = Vec::with_capacity(buckets);
    for b in 0..buckets {
        let start = b * samples.len() / buckets;
        let end = ((b + 1) * samples.len() / buckets)
            .max(start + 1)
            .min(samples.len());
        let p = peak(samples.get(start..end).unwrap_or(&[]));
        // 1.0 を超える入力（クリップ）でも 255 に収める。
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamp で 0.0..=255.0 に収めてから丸める"
        )]
        out.push((p.clamp(0.0, 1.0) * 255.0).round() as u8);
    }
    out
}

/// テイクごとに測る値（`TR-REC-16`）。
///
/// 測るだけで、判定も指摘もしない。 「小さすぎます」「歪んでいます」を出さない。
/// 自動で無効化もしない（自動無効化は `TR-REC-07` の取りこぼしと
/// `TR-REC-04` のデバイス消失の2つだけ）。
///
/// フルスケール到達だけは、書き出しの直前に一度だけ集計して提示する（`TR-REC-16`）。
/// 収録中の判定ではないのでスコープを侵さず、壊れた成果物が完成に到達する経路を塞げる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TakeMetrics {
    /// サンプルピーク（dBFS）。無音なら [`f64::NEG_INFINITY`]。
    pub peak_dbfs: f64,
    pub rms: f64,
    /// フルスケールに達したサンプルが3つ以上続いた回数。
    pub full_scale_runs: u32,
    /// DC オフセットの平均。
    pub dc_offset: f64,
    /// 推定ノイズフロア（先頭マージン区間の RMS）。
    pub noise_floor_rms: f64,
    /// 発声の前に確保できた無音（ミリ秒）。
    pub leading_margin_ms: f64,
    /// 発声の後に確保できた無音（ミリ秒）。
    pub trailing_margin_ms: f64,
}

/// 無音マージンの下限（ミリ秒、`TR-REC-38`）。
///
/// 足りなくてもトリミングしない。 足りなかったという事実を記録するだけ。
pub const REQUIRED_MARGIN_MS: f64 = 300.0;

/// 16 bit の 1LSB。これ以上をフルスケール到達とみなす（`TR-REC-16`）。
const FULL_SCALE: f32 = 1.0 - 1.0 / 32_768.0;

/// フルスケール到達とみなす連続長。
const FULL_SCALE_RUN: usize = 3;

impl TakeMetrics {
    /// 波形と、検出した発声区間から測る。
    ///
    /// `voice_start_ms` / `voice_end_ms` は検出できなければ `None`。
    /// 検出できなくても測れるものは測る。
    #[must_use]
    pub fn measure(
        samples: &[f32],
        rate_hz: u32,
        voice_start_ms: Option<f64>,
        voice_end_ms: Option<f64>,
    ) -> Self {
        let len_ms = samples.len() as f64 * 1000.0 / f64::from(rate_hz);
        let peak = peak(samples);
        let peak_dbfs = if peak > 0.0 {
            20.0 * f64::from(peak).log10()
        } else {
            f64::NEG_INFINITY
        };

        let sum_sq: f64 = samples.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
        let rms = if samples.is_empty() {
            0.0
        } else {
            (sum_sq / samples.len() as f64).sqrt()
        };

        let dc_offset = if samples.is_empty() {
            0.0
        } else {
            samples.iter().map(|s| f64::from(*s)).sum::<f64>() / samples.len() as f64
        };

        // 3サンプル以上続いたときだけ数える（`TR-REC-16`）。
        // 単発のフルスケールは歪みの証拠にならない。
        let mut full_scale_runs = 0_u32;
        let mut run = 0_usize;
        for s in samples {
            if s.abs() >= FULL_SCALE {
                run += 1;
                if run == FULL_SCALE_RUN {
                    full_scale_runs += 1;
                }
            } else {
                run = 0;
            }
        }

        let leading_margin_ms = voice_start_ms.unwrap_or(0.0).max(0.0);
        let trailing_margin_ms = voice_end_ms.map_or(0.0, |e| (len_ms - e).max(0.0));

        // ノイズフロアは先頭マージンの RMS。マージンが無ければ測らない。
        let head = ((leading_margin_ms / 1000.0) * f64::from(rate_hz)) as usize;
        let head = head.min(samples.len());
        let noise_floor_rms = if head == 0 {
            0.0
        } else {
            let s: f64 = samples[..head]
                .iter()
                .map(|v| f64::from(*v) * f64::from(*v))
                .sum();
            (s / head as f64).sqrt()
        };

        Self {
            peak_dbfs,
            rms,
            full_scale_runs,
            dc_offset,
            noise_floor_rms,
            leading_margin_ms,
            trailing_margin_ms,
        }
    }

    /// `TR-REC-38` の無音マージンを満たしているか。
    ///
    /// 満たしていなくてもテイクは有効。 事実を記録するだけで、
    /// トリミングも無効化もしない。
    #[must_use]
    pub fn has_required_margins(&self) -> bool {
        self.leading_margin_ms >= REQUIRED_MARGIN_MS
            && self.trailing_margin_ms >= REQUIRED_MARGIN_MS
    }
}

/// `f64` の並びを little-endian のバイト列にする。
#[must_use]
pub fn f64s_to_bytes(xs: &[f64]) -> Vec<u8> {
    let mut out = Vec::with_capacity(xs.len() * 8);
    for x in xs {
        out.extend_from_slice(&x.to_le_bytes());
    }
    out
}

/// little-endian のバイト列を `f64` の並びに戻す。
///
/// 8で割り切れない端は捨てる。 途中で切れた BLOB を読んだときに
/// でたらめな値を作らない。
#[must_use]
pub fn bytes_to_f64s(b: &[u8]) -> Vec<f64> {
    b.chunks_exact(8)
        .map(|c| f64::from_le_bytes(c.try_into().unwrap_or([0; 8])))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn peak_is_the_absolute_maximum() {
        assert!((peak(&[0.1, -0.9, 0.3]) - 0.9).abs() < 1e-6);
        assert!(peak(&[]).abs() < 1e-6);
    }

    #[test]
    fn thumbnail_has_the_requested_length() {
        assert_eq!(thumbnail(&vec![0.0; 44100], 512).len(), 512);
        // サンプルよりバケットが多くても長さは守る。
        assert_eq!(thumbnail(&[0.5, -0.5], 512).len(), 512);
        assert_eq!(thumbnail(&[], 512), vec![0_u8; 512]);
    }

    /// 平均ではなくピークを採る（短い破裂音を消さない）。
    #[test]
    fn thumbnail_keeps_short_transients() {
        let mut s = vec![0.0_f32; 4096];
        s[10] = 1.0; // 1サンプルだけの立ち上がり。
        let t = thumbnail(&s, 8);
        assert_eq!(t[0], 255, "最初のバケットにピークが残ること");
        assert!(t[1..].iter().all(|v| *v == 0));
    }

    #[test]
    fn thumbnail_clamps_clipped_input() {
        assert_eq!(thumbnail(&[3.0, -3.0], 1), vec![255]);
    }

    #[test]
    fn f64_blobs_round_trip() {
        let xs = [0.0, 440.0, -1.5, f64::MAX];
        assert_eq!(bytes_to_f64s(&f64s_to_bytes(&xs)), xs);
    }

    /// 途中で切れた BLOB からでたらめな値を作らない。
    #[test]
    fn truncated_blobs_drop_the_tail() {
        let mut b = f64s_to_bytes(&[1.0, 2.0]);
        b.truncate(12);
        assert_eq!(bytes_to_f64s(&b), [1.0]);
    }

    #[test]
    fn compute_fills_every_field() {
        let s: Vec<f32> = (0..44100)
            .map(|i| ((i as f32) / 100.0).sin() * 0.5)
            .collect();
        let a = TakeAnalysis::compute(&s, 44100, &[220.0; 200], 0.005);

        assert!(a.peak > 0.4 && a.peak <= 0.5);
        assert_eq!(a.thumbnail.len(), THUMBNAIL_BUCKETS);
        assert_eq!(a.frq.f0.len(), 44100_usize.div_ceil(256));
        assert_eq!(a.frq.amp.len(), a.frq.f0.len());
        assert_eq!(a.hop_size(), 256);
    }
    fn tone(n: usize, amp: f32) -> Vec<f32> {
        (0..n).map(|i| (i as f32 * 0.05).sin() * amp).collect()
    }

    #[test]
    fn ピークをdbfsで返す() {
        let m = TakeMetrics::measure(&[0.5, -0.5, 0.1], 44_100, None, None);
        // 0.5 → -6.02 dBFS
        assert!((m.peak_dbfs + 6.02).abs() < 0.05, "{}", m.peak_dbfs);
    }

    #[test]
    fn 無音のピークは負の無限大() {
        let m = TakeMetrics::measure(&[0.0; 100], 44_100, None, None);
        assert!(m.peak_dbfs.is_infinite() && m.peak_dbfs < 0.0);
        assert!(m.rms.abs() < 1e-12);
    }

    /// 単発のフルスケールは数えない。 3サンプル以上続いたときだけ（`TR-REC-16`）。
    #[test]
    fn フルスケールは連続したときだけ数える() {
        let one = TakeMetrics::measure(&[0.0, 1.0, 0.0, -1.0, 0.0], 44_100, None, None);
        assert_eq!(one.full_scale_runs, 0, "単発は数えない");

        let run = TakeMetrics::measure(&[0.0, 1.0, 1.0, 1.0, 0.0], 44_100, None, None);
        assert_eq!(run.full_scale_runs, 1);

        let two = TakeMetrics::measure(&[1.0, 1.0, 1.0, 0.0, -1.0, -1.0, -1.0], 44_100, None, None);
        assert_eq!(two.full_scale_runs, 2, "符号は問わない");
    }

    #[test]
    fn dcオフセットを測る() {
        let m = TakeMetrics::measure(&[0.2; 1000], 44_100, None, None);
        assert!((m.dc_offset - 0.2).abs() < 1e-6);

        let centred = TakeMetrics::measure(&tone(4000, 0.5), 44_100, None, None);
        assert!(centred.dc_offset.abs() < 0.05, "中心にある波は 0 付近");
    }

    /// 無音マージンは測るだけで、削らない（`TR-REC-38`）。
    #[test]
    fn 無音マージンを前後で測る() {
        // 1秒。発声が 0.4s〜0.6s。
        let m = TakeMetrics::measure(&vec![0.1_f32; 44_100], 44_100, Some(400.0), Some(600.0));
        assert!((m.leading_margin_ms - 400.0).abs() < 1.0);
        assert!((m.trailing_margin_ms - 400.0).abs() < 1.0);
        assert!(m.has_required_margins(), "300ms を超えている");
    }

    #[test]
    fn マージンが足りないことを記録する() {
        let m = TakeMetrics::measure(&vec![0.1_f32; 44_100], 44_100, Some(100.0), Some(900.0));
        assert!(!m.has_required_margins());
        assert!((m.leading_margin_ms - 100.0).abs() < 1.0);
    }

    /// ノイズフロアは先頭マージン区間の RMS。
    #[test]
    fn ノイズフロアは先頭マージンから測る() {
        // 先頭 0.5s が静か、その後が大きい。
        let mut x = vec![0.001_f32; 22_050];
        x.extend(std::iter::repeat_n(0.5_f32, 22_050));
        let m = TakeMetrics::measure(&x, 44_100, Some(500.0), Some(1000.0));
        assert!(
            m.noise_floor_rms < 0.01,
            "静かな側だけを見る: {}",
            m.noise_floor_rms
        );
        assert!(m.rms > 0.1, "全体の RMS は大きい");
    }

    #[test]
    fn 発声を検出できなくても測れるものは測る() {
        let m = TakeMetrics::measure(&tone(4000, 0.5), 44_100, None, None);
        assert!(m.peak_dbfs.is_finite());
        assert!((m.leading_margin_ms - 0.0).abs() < 1e-9);
        assert!(!m.has_required_margins());
    }
}
