//! 録音停止時に算出して DB へ入れる値（`TR-PKG-05`, `TR-PKG-42`）。
//!
//! **書き出しと再開で WAV を再走査しない**のが目的。
//! 3時間ぶんの WAV を起動のたびに読み直すと、再開が即座でなくなる。
//!
//! ここは純粋な計算だけを持つ。保存は [`crate::db`]。

use crate::frq::{Frq, HOP_SIZE};

/// 波形サムネイルのバケット数。
///
/// **表示幅に依らない固定値にする。** ウィンドウ幅ごとに作り直すと、
/// 結局 WAV を読み直すことになる。描画側はここから間引くか補間する。
pub const THUMBNAIL_BUCKETS: usize = 512;

/// 録音停止時に確定させる値。
#[derive(Debug, Clone, PartialEq)]
pub struct TakeAnalysis {
    /// 絶対値の最大。クリップ判定と表示に使う。
    pub peak: f32,
    /// 周波数表（`.frq`）。**書き出し時に推定し直さない。**
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
/// **平均ではなくピークを採る。** 平均にすると、短いクリックや破裂音が
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
/// **8で割り切れない端は捨てる。** 途中で切れた BLOB を読んだときに
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

    /// **平均ではなくピークを採る**（短い破裂音を消さない）。
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

    /// **途中で切れた BLOB からでたらめな値を作らない。**
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
}
