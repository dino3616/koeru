//! 波形とスペクトログラムの描画データ（`TR-PLT-04`）。
//!
//! **可視域のみ計算し、可視域のみ描く。**
//! ズーム倍率が変わっても、**表示中の画素数に比例した計算量に収める。**
//!
//! # なぜミップマップなのか
//!
//! 3時間の収録では、1テイクでも数十万サンプルある。引いて全体を見るとき、
//! 画素は千個しか無いのに数十万サンプルを毎回走査すると、
//! **つまみを動かすたびに固まる。**
//!
//! あらかじめ段を作っておけば、どの倍率でも読むのは画素数ぶんで済む。
//!
//! **素材ファイル全体の STFT を一括で先行計算しない**（`TR-PLT-04`）。
//! スペクトログラムは見えている範囲だけを計算する。

/// 段を作るときの間引き率。**1段ごとに半分。**
const DECIMATION: usize = 2;

/// いちばん粗い段の下限。**これ以下にしても得が無い。**
const MIN_LEVEL_LEN: usize = 64;

/// 1画素ぶんの上下（`TR-PLT-04`）。
///
/// **平均ではなく min/max。** 平均にすると、短いクリックや破裂音が見えなくなり、
/// 波形を見て切り直す判断ができない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinMax {
    /// 下端。
    pub min: f32,
    /// 上端。
    pub max: f32,
}

/// 段を積んだ波形（`TR-PLT-04`）。
///
/// **段 0 が元のサンプル数、以降は半分ずつ。**
#[derive(Debug, Clone, PartialEq)]
pub struct Mipmap {
    levels: Vec<Vec<MinMax>>,
    /// 元のサンプル数。
    total: usize,
}

impl Mipmap {
    /// 波形から段を積む。
    #[must_use]
    pub fn build(samples: &[f32]) -> Self {
        let total = samples.len();
        if total == 0 {
            return Self {
                levels: Vec::new(),
                total,
            };
        }

        // 段 0 は元のサンプルそのもの（上下が同じ）。
        let mut levels: Vec<Vec<MinMax>> = vec![
            samples
                .iter()
                .map(|v| MinMax { min: *v, max: *v })
                .collect(),
        ];

        while levels
            .last()
            .is_some_and(|l| l.len() > MIN_LEVEL_LEN * DECIMATION)
        {
            let prev = levels.last().unwrap_or(&Vec::new()).clone();
            let next: Vec<MinMax> = prev
                .chunks(DECIMATION)
                .map(|c| MinMax {
                    min: c.iter().fold(f32::INFINITY, |m, v| m.min(v.min)),
                    max: c.iter().fold(f32::NEG_INFINITY, |m, v| m.max(v.max)),
                })
                .collect();
            levels.push(next);
        }

        Self { levels, total }
    }

    /// 元のサンプル数。
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// 段の数。
    #[must_use]
    pub fn level_count(&self) -> usize {
        self.levels.len()
    }

    /// 見えている範囲を、指定した画素数で読む（`TR-PLT-04`）。
    ///
    /// **読む量は画素数に比例する。** 範囲の広さには比例しない。
    /// `from` / `to` は元のサンプル位置。
    #[must_use]
    pub fn window(&self, from: usize, to: usize, pixels: usize) -> Vec<MinMax> {
        if pixels == 0 || self.levels.is_empty() || from >= to {
            return Vec::new();
        }
        let to = to.min(self.total);
        let span = to.saturating_sub(from);
        if span == 0 {
            return Vec::new();
        }

        // **1画素ぶんが1〜2要素になる段を選ぶ。** これで読む量が画素数に比例する。
        let want_per_pixel = span / pixels.max(1);
        let level = self.levels.iter().position(|_| false).unwrap_or_else(|| {
            let mut l = 0;
            let mut step = 1;
            while l + 1 < self.levels.len() && step * DECIMATION <= want_per_pixel.max(1) {
                step *= DECIMATION;
                l += 1;
            }
            l
        });
        let scale = DECIMATION.pow(u32::try_from(level).unwrap_or(0));
        let lv = self.levels.get(level).map_or(&[][..], Vec::as_slice);

        (0..pixels)
            .map(|i| {
                let a = (from + i * span / pixels) / scale;
                let b = ((from + (i + 1) * span / pixels) / scale).max(a + 1);
                let slice = lv.get(a..b.min(lv.len())).unwrap_or(&[]);
                if slice.is_empty() {
                    return MinMax { min: 0.0, max: 0.0 };
                }
                MinMax {
                    min: slice.iter().fold(f32::INFINITY, |m, v| m.min(v.min)),
                    max: slice.iter().fold(f32::NEG_INFINITY, |m, v| m.max(v.max)),
                }
            })
            .collect()
    }
}

/// スペクトログラムの1列（`TR-PLT-04`）。
///
/// **見えている範囲だけを計算する。** 素材ファイル全体の STFT を先に作らない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spectrogram {
    /// 列ごとの強度（0〜255）。**列を並べたもの。**
    pub bins: Vec<u8>,
    /// 列の数（時間方向）。
    pub columns: usize,
    /// 1列あたりの高さ（周波数方向）。
    pub rows: usize,
}

/// FFT の窓の大きさ。**試唱は速度優先で 2048 固定**（`TR-SYN-22`）。
pub const FFT_SIZE: usize = 2048;

/// 見えている範囲のスペクトログラムを作る（`TR-PLT-04`）。
///
/// **列数は画素数、行数は指定した高さ。** 範囲が広くても計算量は変わらない。
#[must_use]
pub fn spectrogram(
    samples: &[f32],
    from: usize,
    to: usize,
    columns: usize,
    rows: usize,
) -> Spectrogram {
    if columns == 0 || rows == 0 || from >= to || samples.is_empty() {
        return Spectrogram {
            bins: Vec::new(),
            columns: 0,
            rows: 0,
        };
    }
    let to = to.min(samples.len());
    let span = to.saturating_sub(from);
    let mut bins = Vec::with_capacity(columns * rows);

    for c in 0..columns {
        let centre = from + c * span / columns;
        let start = centre.saturating_sub(FFT_SIZE / 2);
        let window: Vec<f32> = (0..FFT_SIZE)
            .map(|i| {
                let v = samples.get(start + i).copied().unwrap_or(0.0);
                // ハン窓。**矩形だと側帯が出て、縦縞に見える。**
                #[allow(clippy::cast_possible_truncation, reason = "窓関数は 0.0..=1.0")]
                let w = (0.5
                    - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / FFT_SIZE as f64).cos())
                    as f32;
                v * w
            })
            .collect();

        let power = magnitudes(&window, rows);
        bins.extend(power);
    }

    Spectrogram {
        bins,
        columns,
        rows,
    }
}

/// 窓のスペクトルを `rows` 段へ畳む。
///
/// **周波数は対数で割る。** 線形だと、声の帯域が下の数%に潰れる。
fn magnitudes(window: &[f32], rows: usize) -> Vec<u8> {
    // 素朴な DFT。**行数ぶんしか要らない**ので、必要な周波数だけを直に求める。
    // FFT を回して捨てるより、こちらのほうが速い（rows は 128 程度）。
    let n = window.len();
    let mut out = Vec::with_capacity(rows);
    for r in 0..rows {
        // 対数でビンを割る。20Hz 相当から Nyquist まで。
        let t = (r as f64 + 0.5) / rows as f64;
        let k = (2.0_f64.powf(t * 10.0) - 1.0) / 1023.0 * (n as f64 / 2.0);
        let (mut re, mut im) = (0.0_f64, 0.0_f64);
        let w = 2.0 * std::f64::consts::PI * k / n as f64;
        for (i, v) in window.iter().enumerate() {
            let a = w * i as f64;
            re += f64::from(*v) * a.cos();
            im -= f64::from(*v) * a.sin();
        }
        let mag = (re * re + im * im).sqrt() / (n as f64 / 2.0);
        // dB へ直して 0〜255 に収める。**-80dB を下端にする。**
        let db = 20.0 * mag.max(1e-9).log10();
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "clamp で 0.0..=255.0 に収めてから丸める"
        )]
        let v = (((db + 80.0) / 80.0).clamp(0.0, 1.0) * 255.0) as u8;
        out.push(v);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp(n: usize) -> Vec<f32> {
        (0..n).map(|i| (i as f32 / n as f32) * 2.0 - 1.0).collect()
    }

    #[test]
    fn 段を積む() {
        let m = Mipmap::build(&ramp(10_000));
        assert_eq!(m.total(), 10_000);
        assert!(m.level_count() > 1, "段が積まれること");
    }

    #[test]
    fn 空でも落ちない() {
        let m = Mipmap::build(&[]);
        assert_eq!(m.total(), 0);
        assert!(m.window(0, 100, 10).is_empty());
    }

    /// **求めた画素数ぶん返る。**
    #[test]
    fn 画素数ぶん返る() {
        let m = Mipmap::build(&ramp(100_000));
        for pixels in [1, 10, 800, 4000] {
            assert_eq!(m.window(0, 100_000, pixels).len(), pixels, "{pixels}");
        }
    }

    /// **平均ではなく min/max。** 短い立ち上がりを消さない。
    #[test]
    fn 短い立ち上がりが残る() {
        let mut x = vec![0.0_f32; 100_000];
        x[50_000] = 1.0;
        let m = Mipmap::build(&x);

        let w = m.window(0, 100_000, 100);
        let peak = w.iter().fold(0.0_f32, |a, v| a.max(v.max));
        assert!((peak - 1.0).abs() < 1e-6, "全体を見ても残ること: {peak}");
    }

    /// **拡大すると細かくなる。**
    #[test]
    fn 拡大すると細かく見える() {
        let x: Vec<f32> = (0..100_000)
            .map(|i| (i as f32 * 0.01).sin() * 0.5)
            .collect();
        let m = Mipmap::build(&x);

        let wide = m.window(0, 100_000, 200);
        let zoom = m.window(0, 2000, 200);

        // 引いた図は上下いっぱいに振れ、寄せた図は波の一部だけになる。
        let wide_span = wide.iter().map(|v| v.max - v.min).fold(0.0_f32, f32::max);
        let zoom_span = zoom.iter().map(|v| v.max - v.min).fold(0.0_f32, f32::max);
        assert!(wide_span > zoom_span, "{wide_span} > {zoom_span}");
    }

    /// **読む量が画素数に比例する**（TR-PLT-04）。
    ///
    /// 範囲を100倍にしても、掛かる時間が100倍にならないこと。
    #[test]
    fn 範囲を広げても計算量が増えない() {
        let m = Mipmap::build(&ramp(1_000_000));

        let t0 = std::time::Instant::now();
        let _ = m.window(0, 10_000, 800);
        let narrow = t0.elapsed();

        let t1 = std::time::Instant::now();
        let _ = m.window(0, 1_000_000, 800);
        let wide = t1.elapsed();

        // **段を選んでいるので、広げても十数倍には増えない。**
        // 時間は環境で揺れるので、桁で見る。
        assert!(
            wide < narrow * 20 + std::time::Duration::from_millis(5),
            "狭い {narrow:?} / 広い {wide:?}"
        );
    }

    #[test]
    fn 範囲の端で落ちない() {
        let m = Mipmap::build(&ramp(1000));
        assert!(m.window(999, 1000, 10).len() == 10);
        assert!(m.window(0, 5000, 10).len() == 10, "範囲を超えても収める");
        assert!(m.window(500, 500, 10).is_empty(), "幅が無ければ空");
    }

    /// **列数と行数のとおりに返る**（TR-PLT-04）。
    #[test]
    fn スペクトログラムの大きさが指定どおり() {
        let x: Vec<f32> = (0..44_100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5)
            .collect();
        let s = spectrogram(&x, 0, 44_100, 64, 128);
        assert_eq!(s.columns, 64);
        assert_eq!(s.rows, 128);
        assert_eq!(s.bins.len(), 64 * 128);
    }

    /// **音がある帯域が明るくなる。**
    #[test]
    fn 鳴っている帯域が明るい() {
        let x: Vec<f32> = (0..44_100)
            .map(|i| (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 44_100.0).sin() * 0.5)
            .collect();
        let tone = spectrogram(&x, 0, 44_100, 8, 64);
        let silence = spectrogram(&vec![0.0_f32; 44_100], 0, 44_100, 8, 64);

        let tone_max = tone.bins.iter().copied().max().unwrap_or(0);
        let silence_max = silence.bins.iter().copied().max().unwrap_or(0);
        assert!(
            tone_max > silence_max + 50,
            "鳴っているほうが明るいこと: {tone_max} / {silence_max}"
        );
    }

    #[test]
    fn スペクトログラムも空で落ちない() {
        let s = spectrogram(&[], 0, 100, 8, 8);
        assert_eq!(s.columns, 0);
        assert!(s.bins.is_empty());
    }
}
