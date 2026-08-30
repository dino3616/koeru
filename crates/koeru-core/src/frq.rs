//! 周波数表（`.frq`）の書き出し（`TR-PKG-05`）。
//!
//! **書き出しのために F0 を推定し直さない。** 即時試唱のために録音時点で
//! 走らせた解析の副産物として作る（`docs/product-vision.md`）。
//! この層が持つのは書式だけで、推定は `koeru-synth` が持つ。
//!
//! # 書式（FREQ0003）
//!
//! | 位置 | 内容 |
//! |---|---|
//! | 0 | ASCII 8バイト `FREQ0003` |
//! | 8 | int32 hopSize（256） |
//! | 12 | double 平均 F0 |
//! | 20 | 16バイトの予約領域（ゼロ） |
//! | 36 | int32 フレーム数 |
//! | 40 | double f0 と double amp の対 × フレーム数 |
//!
//! **36 バイト目の int32 はフレーム数（＝ f0/amp の対の数）であって、
//! WAV のサンプル数ではない。** ここにサンプル数を書くと、読み手は 256 倍の
//! 要素を確保しようとして EOF に当たる。`TR-PKG-05` の字面は「サンプル数」だが、
//! 直後に「フレーム数ぶんの f0/amp を用意」とあり、書式として成立するのは
//! フレーム数のほうだけ。

use std::io::Write as _;
use std::path::{Path, PathBuf};

/// `.frq` の hop（サンプル）。**UTAU 側の固定値**（`TR-PKG-05`）。
pub const HOP_SIZE: u32 = 256;

/// 書式の識別子。
const MAGIC: &[u8; 8] = b"FREQ0003";

/// 無声・無音を表す F0。**内部で補間している連続 F0 をここに書かない**（`TR-PKG-05`）。
pub const UNVOICED: f64 = 0.0;

/// 周波数表を作るときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum FrqError {
    #[error("入出力に失敗した")]
    Io(#[from] std::io::Error),

    /// f0 と amp の長さが違う。
    #[error("f0 と amp の長さが揃っていない")]
    LengthMismatch,

    /// WAV 名の拡張子が `.wav` でない。
    #[error("WAV 名から .frq 名を作れない")]
    NotAWavName,
}

impl FrqError {
    /// 送信してよい種別文字列。**`Display` は送らない。**
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "frq.io",
            Self::LengthMismatch => "frq.length_mismatch",
            Self::NotAWavName => "frq.not_a_wav_name",
        }
    }
}

type Result<T> = std::result::Result<T, FrqError>;

/// hop=256 の格子に載せた周波数表。
///
/// **これを録音停止時に作って DB へ入れる。** 書き出し時に WAV を再走査しない
/// （`TR-PKG-05`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Frq {
    /// フレームごとの F0（Hz）。無声・無音は [`UNVOICED`]。
    pub f0: Vec<f64>,
    /// フレームごとの振幅。
    pub amp: Vec<f64>,
}

impl Frq {
    /// 内部の F0 系列と波形から、hop=256 の格子へ載せ替える（`TR-PKG-05`）。
    ///
    /// `source_f0` は `source_period_s` 秒ごと、無声は [`UNVOICED`]。
    /// **フレーム数は WAV 全長を覆う数にする。** 端が欠けると、読み手が
    /// 最後のフレームの手前で止まる。
    ///
    /// 有声どうしの間だけ線形で埋め、片側が無声なら近いほうを採る。
    /// **有声と無声をまたいで補間すると、無声フレームに音高が生えてしまう。**
    #[must_use]
    pub fn from_analysis(
        samples: &[f32],
        rate_hz: u32,
        source_f0: &[f64],
        source_period_s: f64,
    ) -> Self {
        let hop = HOP_SIZE as usize;
        let frames = samples.len().div_ceil(hop).max(1);
        let mut f0 = Vec::with_capacity(frames);
        let mut amp = Vec::with_capacity(frames);

        for i in 0..frames {
            let t = (i * hop) as f64 / f64::from(rate_hz);
            f0.push(sample_f0(source_f0, source_period_s, t));

            // 振幅は窓の RMS。**WAV を書き出し時に読み直さないために、ここで確定させる。**
            let start = i * hop;
            let end = (start + hop).min(samples.len());
            let win = samples.get(start..end).unwrap_or(&[]);
            let rms = if win.is_empty() {
                0.0
            } else {
                let sum: f64 = win.iter().map(|s| f64::from(*s) * f64::from(*s)).sum();
                (sum / win.len() as f64).sqrt()
            };
            amp.push(rms);
        }

        Self { f0, amp }
    }

    /// 有声フレームだけの平均 F0。
    ///
    /// **無声の 0 を混ぜて平均すると、値が音楽的な意味を失う。**
    /// 有声が1つも無ければ 0 を返す。
    #[must_use]
    pub fn average_f0(&self) -> f64 {
        let voiced: Vec<f64> = self.f0.iter().copied().filter(|v| *v > 0.0).collect();
        if voiced.is_empty() {
            return 0.0;
        }
        voiced.iter().sum::<f64>() / voiced.len() as f64
    }

    /// FREQ0003 のバイト列にする。
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        if self.f0.len() != self.amp.len() {
            return Err(FrqError::LengthMismatch);
        }
        let frames = i32::try_from(self.f0.len()).unwrap_or(i32::MAX);

        let mut out = Vec::with_capacity(40 + self.f0.len() * 16);
        out.extend_from_slice(MAGIC);
        #[allow(clippy::cast_possible_wrap, reason = "HOP_SIZE は 256 の定数")]
        out.extend_from_slice(&(HOP_SIZE as i32).to_le_bytes());
        out.extend_from_slice(&self.average_f0().to_le_bytes());
        out.extend_from_slice(&[0_u8; 16]);
        out.extend_from_slice(&frames.to_le_bytes());
        for (v, a) in self.f0.iter().zip(&self.amp) {
            out.extend_from_slice(&v.to_le_bytes());
            out.extend_from_slice(&a.to_le_bytes());
        }
        Ok(out)
    }

    /// ファイルへ書く。**fsync してから rename**（途中で落ちても半端な表を残さない）。
    #[tracing::instrument(skip(self), err)]
    pub fn write(&self, path: &Path) -> Result<()> {
        let bytes = self.to_bytes()?;
        let tmp = path.with_extension("frq.part");
        {
            let mut f = std::fs::File::create(&tmp)?;
            f.write_all(&bytes)?;
            f.sync_all()?;
        }
        std::fs::rename(&tmp, path)?;
        Ok(())
    }
}

/// WAV 名から `.frq` 名を作る（`TR-PKG-05`）。
///
/// **拡張子のドットをアンダースコアに置き換えて `.frq` を付ける。**
/// `あ.wav` → `あ_wav.frq`。この規則を外すと UTAU 側が表を見つけられない。
pub fn frq_path(wav: &Path) -> Result<PathBuf> {
    let name = wav
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or(FrqError::NotAWavName)?;
    let stem = name.strip_suffix(".wav").ok_or(FrqError::NotAWavName)?;
    Ok(wav.with_file_name(format!("{stem}_wav.frq")))
}

/// 時刻 `t` 秒における F0 を、元の格子から取る。
fn sample_f0(source: &[f64], period_s: f64, t: f64) -> f64 {
    if source.is_empty() || period_s <= 0.0 {
        return UNVOICED;
    }
    let x = t / period_s;
    if x <= 0.0 {
        return source[0];
    }
    let last = source.len() - 1;
    #[allow(clippy::cast_possible_truncation, reason = "x >= 0 を上で確かめている")]
    let lo = (x.floor() as usize).min(last);
    if lo == last {
        return source[last];
    }
    let hi = lo + 1;
    let (a, b) = (source[lo], source[hi]);
    if a > 0.0 && b > 0.0 {
        // 有声どうし。線形で埋める。
        let w = x - x.floor();
        a + (b - a) * w
    } else if x - x.floor() < 0.5 {
        a
    } else {
        b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_replaces_the_extension_dot_with_an_underscore() {
        let p = frq_path(Path::new("/x/あ.wav")).expect("作れること");
        assert_eq!(p, Path::new("/x/あ_wav.frq"));
    }

    #[test]
    fn non_wav_names_are_refused() {
        assert_eq!(
            frq_path(Path::new("/x/a.aiff"))
                .expect_err("拒むこと")
                .kind(),
            "frq.not_a_wav_name"
        );
    }

    #[test]
    fn header_matches_the_format() {
        let f = Frq {
            f0: vec![440.0, 0.0, 442.0],
            amp: vec![0.5, 0.0, 0.5],
        };
        let b = f.to_bytes().expect("書けること");

        assert_eq!(&b[0..8], MAGIC);
        assert_eq!(i32::from_le_bytes([b[8], b[9], b[10], b[11]]), 256);
        let avg = f64::from_le_bytes(b[12..20].try_into().expect("8バイト"));
        assert!((avg - 441.0).abs() < 1e-9, "有声だけの平均であること");
        assert_eq!(&b[20..36], &[0_u8; 16], "予約領域はゼロ");

        // **ここはフレーム数。サンプル数ではない。**
        assert_eq!(i32::from_le_bytes([b[36], b[37], b[38], b[39]]), 3);
        assert_eq!(b.len(), 40 + 3 * 16);
    }

    #[test]
    fn pairs_are_written_in_order() {
        let f = Frq {
            f0: vec![100.0, 200.0],
            amp: vec![0.25, 0.75],
        };
        let b = f.to_bytes().expect("書けること");
        let at = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().expect("8バイト"));
        assert!((at(40) - 100.0).abs() < 1e-12);
        assert!((at(48) - 0.25).abs() < 1e-12);
        assert!((at(56) - 200.0).abs() < 1e-12);
        assert!((at(64) - 0.75).abs() < 1e-12);
    }

    #[test]
    fn average_ignores_unvoiced_frames() {
        let f = Frq {
            f0: vec![0.0, 0.0, 300.0],
            amp: vec![0.0, 0.0, 1.0],
        };
        assert!((f.average_f0() - 300.0).abs() < 1e-12);

        let silent = Frq {
            f0: vec![0.0, 0.0],
            amp: vec![0.0, 0.0],
        };
        assert!((silent.average_f0() - 0.0).abs() < 1e-12);
    }

    #[test]
    fn frames_cover_the_whole_wav() {
        // 1000 サンプル → ceil(1000/256) = 4 フレーム。
        let f = Frq::from_analysis(&vec![0.0_f32; 1000], 44100, &[440.0; 10], 0.005);
        assert_eq!(f.f0.len(), 4);
        assert_eq!(f.amp.len(), 4);
    }

    /// **有声と無声をまたいで補間しない**（`TR-PKG-05`）。
    /// 内部で連続にしている F0 をそのまま書くと、無声フレームに音高が生える。
    #[test]
    fn unvoiced_frames_stay_at_zero() {
        // 前半有声・後半無声の素材。
        let src: Vec<f64> = (0..40)
            .map(|i| if i < 20 { 440.0 } else { UNVOICED })
            .collect();
        let f = Frq::from_analysis(&vec![0.1_f32; 44100 / 5], 44100, &src, 0.005);

        assert!(f.f0.iter().any(|v| *v > 0.0), "有声側が残ること");
        assert!(f.f0.contains(&UNVOICED), "無声側が 0 のままであること");
        // 440 と 0 の間の値（＝またいで補間した跡）が無いこと。
        assert!(
            !f.f0.iter().any(|v| *v > 1.0 && *v < 400.0),
            "境界に中間の音高が生えないこと"
        );
    }

    #[test]
    fn amp_tracks_the_waveform() {
        let mut s = vec![0.0_f32; 512];
        for x in s.iter_mut().take(256) {
            *x = 1.0;
        }
        let f = Frq::from_analysis(&s, 44100, &[440.0; 10], 0.005);
        assert!((f.amp[0] - 1.0).abs() < 1e-6, "鳴っている窓は RMS 1.0");
        assert!(f.amp[1].abs() < 1e-6, "無音の窓は 0");
    }

    #[test]
    fn mismatched_lengths_are_refused() {
        let f = Frq {
            f0: vec![1.0, 2.0],
            amp: vec![1.0],
        };
        assert_eq!(
            f.to_bytes().expect_err("拒むこと").kind(),
            "frq.length_mismatch"
        );
    }
}
