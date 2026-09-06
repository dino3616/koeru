//! キャプチャからマスターまでの、ただ1回の変換（`TR-REC-02`）。
//!
//! > キャプチャは 32bit float・**デバイスのネイティブレートで受ける。44100 Hz で
//! > ない場合はアプリ内の固定リサンプラで1回だけ変換し、44100 Hz / 32bit float の
//! > WAV（マスター）として保存する**
//!
//! ネイティブレートで受けるのは要件。 開くレートをこちらから指定すると、
//! ドライバや APO が黙って変換する経路に落ちうる（`TR-REC-05`）。
//! 変換はアプリの中で、ここ1箇所だけで行う。
//!
//! # 流し込み型である理由
//!
//! 排出は塊で来る。塊ごとに独立して変換すると、継ぎ目に段差が出る。
//! [`Resampler`] は窓に要る手前の入力を持ち越し、出力の位相も持ち回すので、
//! どこで塊が切れても、通しで変換したのと同じ列が出る。
//!
//! 書きかけの `.wav.part` は確定済みのマスターと同じ形式でなければならない
//! （`DEC-REC-004` の fsync → rename）。あとから作り直す形は採れない。
//!
//! # `koeru-align` のリサンプラとは別物
//!
//! あちらはマスターを読む側の 44100 → 16000 で、マスターには触らない。
//! `TR-ALN-29` がビット単位の同一を要求しているので、あちらは動かさない。
//! 核（窓関数付き sinc）は同じ形だが、要求が違うので別に持つ。

use crate::wav::MASTER_RATE_HZ;

/// 窓関数付き sinc の片側の幅（タップ数の半分）。
const HALF_WIDTH: isize = 16;

/// このリサンプラの識別子（`TR-REC-02` が記録を要求している）。
///
/// 核か窓か幅を変えたら、この版を上げる。 メタデータに残るので、
/// あとから「どの変換で作られた素材か」を辿れる。
pub const IDENTIFIER: &str = "koeru-sinc-blackman-16/1";

/// リサンプルの失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResampleError {
    /// 入力または出力のサンプリング周波数が 0。
    #[error("サンプリング周波数が 0")]
    ZeroRate,
}

impl ResampleError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ZeroRate => "resample.zero_rate",
        }
    }
}

/// 正規化した sinc。`sinc(0) = 1`。
fn sinc(x: f64) -> f64 {
    if x.abs() < 1e-12 {
        return 1.0;
    }
    let px = std::f64::consts::PI * x;
    px.sin() / px
}

/// Blackman 窓。両端で 0 になるので、打ち切りの段差が出ない。
fn blackman(n: f64, width: f64) -> f64 {
    let t = (n / width).mul_add(0.5, 0.5).clamp(0.0, 1.0);
    let tau = std::f64::consts::TAU;
    0.42 - 0.5 * (tau * t).cos() + 0.08 * (2.0 * tau * t).cos()
}

/// 流し込みでサンプリング周波数を変える（`TR-REC-02`）。
///
/// 塊の切れ目に依存しない。 手前の入力と出力の位相を持ち回す。
#[derive(Debug)]
pub struct Resampler {
    /// 出力1つあたりの入力サンプル数。
    ratio: f64,
    /// 遮断周波数（入力側ナイキストに対する比）。
    cutoff: f64,
    /// 窓の片側の幅（入力サンプル）。
    half: f64,
    /// 窓の片側のタップ数。
    taps: i64,
    /// レートが同じなら素通しする。
    passthrough: bool,
    /// まだ使う入力。`buf[0]` の絶対添字が [`Self::origin`]。
    buf: Vec<f32>,
    /// `buf[0]` の絶対入力添字。
    origin: i64,
    /// これまでに出した出力の数。
    produced: u64,
}

impl Resampler {
    /// 変換器を作る。
    ///
    /// 入力より出力が低いときは、遮断周波数を出力側のナイキストに合わせる
    /// （折り返しを防ぐ）。上げるときは入力側のナイキストのまま。
    ///
    /// # Errors
    ///
    /// サンプリング周波数が 0。
    pub fn new(from_hz: u32, to_hz: u32) -> Result<Self, ResampleError> {
        if from_hz == 0 || to_hz == 0 {
            return Err(ResampleError::ZeroRate);
        }
        let ratio = f64::from(from_hz) / f64::from(to_hz);
        // 間引くときだけ帯域を絞る。 補間のときに絞ると、要らない鈍りが入る。
        let cutoff = if ratio > 1.0 { 1.0 / ratio } else { 1.0 };
        // 窓の幅を sinc の伸びに合わせる。 間引くとき sinc は `1/cutoff` 倍に伸びる。
        // 幅を固定したまま遮断だけ下げると、ローブが数本しか入らず阻止域が緩む。
        #[allow(clippy::cast_precision_loss)]
        let half = HALF_WIDTH as f64 / cutoff;
        #[allow(clippy::cast_possible_truncation)]
        let taps = half.ceil() as i64;
        Ok(Self {
            ratio,
            cutoff,
            half,
            taps,
            passthrough: from_hz == to_hz,
            buf: Vec::new(),
            origin: 0,
            produced: 0,
        })
    }

    /// マスター（44100 Hz）へ落とす変換器。
    ///
    /// # Errors
    ///
    /// サンプリング周波数が 0。
    pub fn to_master(from_hz: u32) -> Result<Self, ResampleError> {
        Self::new(from_hz, MASTER_RATE_HZ)
    }

    /// 変換が要るか。要らないなら素通しする。
    #[must_use]
    pub const fn is_passthrough(&self) -> bool {
        self.passthrough
    }

    /// 入力を流し込み、出せるぶんだけ出す。
    ///
    /// 足りないぶんは持ち越す。 次の [`Self::push`] か [`Self::flush`] で出る。
    pub fn push(&mut self, input: &[f32], out: &mut Vec<f32>) {
        if self.passthrough {
            out.extend_from_slice(input);
            return;
        }
        self.buf.extend_from_slice(input);
        self.emit(out, false);
        self.trim();
    }

    /// 残りを出し切る。窓の外は 0 として扱う。
    ///
    /// 欠けた窓の重みで割り戻さない——割り戻すと、信号が切れる直前に無い音を作る。
    pub fn flush(&mut self, out: &mut Vec<f32>) {
        if self.passthrough {
            return;
        }
        self.emit(out, true);
        self.buf.clear();
        self.origin = 0;
        self.produced = 0;
    }

    /// 出せるものを出す。`final_pass` なら、窓の外を 0 と見なして最後まで出す。
    fn emit(&mut self, out: &mut Vec<f32>, final_pass: bool) {
        #[allow(clippy::cast_possible_wrap)]
        let available = self.origin + self.buf.len() as i64;
        loop {
            #[allow(clippy::cast_precision_loss)]
            let center = self.produced as f64 * self.ratio;
            #[allow(clippy::cast_possible_truncation)]
            let base = center.floor() as i64;
            let last_needed = base + self.taps;
            if final_pass {
                // 入力の長さを超えたら終わり（通しで変換したときの出力数と揃える）。
                if base >= available {
                    break;
                }
            } else if last_needed >= available {
                // 窓の右端がまだ届いていない。次の塊を待つ。
                break;
            }
            out.push(self.sample_at(center, base));
            self.produced += 1;
        }
    }

    /// 1つの出力点を、窓関数付き sinc で作る。
    fn sample_at(&self, center: f64, base: i64) -> f32 {
        // 和の順序を固定する。 低い添字から順に足す。
        let mut acc = 0.0_f64;
        for k in (base - self.taps + 1)..=(base + self.taps) {
            let Ok(idx) = usize::try_from(k - self.origin) else {
                continue;
            };
            // 範囲外は 0。 端は素直に立ち上がるだけで、作り物が出ない。
            let Some(x) = self.buf.get(idx) else { continue };
            if k < 0 {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let d = k as f64 - center;
            let w = sinc(d * self.cutoff) * blackman(d, self.half) * self.cutoff;
            acc = w.mul_add(f64::from(*x), acc);
        }
        #[allow(clippy::cast_possible_truncation)]
        let v = acc as f32;
        v
    }

    /// 次の出力に要らなくなった手前の入力を捨てる。際限なく溜めない。
    fn trim(&mut self) {
        #[allow(clippy::cast_precision_loss)]
        let center = self.produced as f64 * self.ratio;
        #[allow(clippy::cast_possible_truncation)]
        let base = center.floor() as i64;
        let first_needed = base - self.taps + 1;
        let Ok(drop_n) = usize::try_from(first_needed - self.origin) else {
            return;
        };
        let drop_n = drop_n.min(self.buf.len());
        if drop_n == 0 {
            return;
        }
        self.buf.drain(..drop_n);
        #[allow(clippy::cast_possible_wrap)]
        {
            self.origin += drop_n as i64;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f64, rate: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 / f64::from(rate);
                #[allow(clippy::cast_possible_truncation)]
                let v = (std::f64::consts::TAU * hz * t).sin() as f32;
                v
            })
            .collect()
    }

    /// 自己相関で基本周波数を測る。
    fn measure_hz(y: &[f32], rate: u32) -> f64 {
        let a = y.len() / 4;
        let x = &y[a..y.len() - a];
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let (lo, hi) = (
            (f64::from(rate) / 2000.0) as usize,
            (f64::from(rate) / 100.0) as usize,
        );
        let mut best = (0.0_f64, lo);
        for lag in lo..hi.min(x.len() / 2) {
            let c: f64 = x[..x.len() - lag]
                .iter()
                .zip(&x[lag..])
                .map(|(p, q)| f64::from(*p) * f64::from(*q))
                .sum();
            if c > best.0 {
                best = (c, lag);
            }
        }
        #[allow(clippy::cast_precision_loss)]
        let lag = best.1 as f64;
        f64::from(rate) / lag
    }

    fn all_at_once(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        let mut r = Resampler::new(from, to).expect("作れる");
        let mut out = Vec::new();
        r.push(input, &mut out);
        r.flush(&mut out);
        out
    }

    /// 塊の切れ目に依存しない。 ここが崩れると継ぎ目に段差が出る。
    #[test]
    fn 塊に切っても通しでも同じ列が出る() {
        let x = sine(440.0, 48_000, 48_000);
        let whole = all_at_once(&x, 48_000, 44_100);

        for chunk in [1_usize, 7, 512, 8192, 33_333] {
            let mut r = Resampler::new(48_000, 44_100).expect("作れる");
            let mut out = Vec::new();
            for c in x.chunks(chunk) {
                r.push(c, &mut out);
            }
            r.flush(&mut out);
            assert_eq!(out.len(), whole.len(), "塊 {chunk} で長さが違う");
            for (i, (a, b)) in out.iter().zip(&whole).enumerate() {
                assert!(
                    (a - b).abs() < 1e-6,
                    "塊 {chunk} の {i} 番目が違う: {a} と {b}"
                );
            }
        }
    }

    /// 48000 → 44100 で音の高さが変わらない（`TR-REC-02`）。
    #[test]
    fn 変換しても音高が変わらない() {
        let y = all_at_once(&sine(440.0, 48_000, 48_000), 48_000, 44_100);
        let hz = measure_hz(&y, 44_100);
        assert!((hz - 440.0).abs() < 5.0, "{hz:.1} Hz");
    }

    /// 長さが比で決まる。 8.8% 短くなる。
    #[test]
    fn 長さが比で決まる() {
        // 1秒ぶん入れたら1秒ぶん出る。48000 → 44100 は 8.8% 短くなる。
        assert_eq!(
            all_at_once(&sine(440.0, 48_000, 48_000), 48_000, 44_100).len(),
            44_100
        );
        assert_eq!(
            all_at_once(&sine(440.0, 48_000, 24_000), 48_000, 44_100).len(),
            22_050
        );
    }

    /// 折り返さない。 出力側のナイキストより上は落ちる。
    ///
    /// 48000 → 44100 では遷移帯を測れない。 遮断（22.05kHz）と
    /// 入力のナイキスト（24kHz）の間が 2kHz しかなく、窓の遷移幅より狭い。
    /// 比の大きい変換で阻止域を見る。
    #[test]
    fn 折り返しを防ぐ() {
        let level = |hz: f64, from: u32| {
            let y = all_at_once(&sine(hz, from, from as usize), from, 44_100);
            let mid = &y[y.len() / 4..y.len() * 3 / 4];
            f64::from(mid.iter().fold(0.0_f32, |m, v| m.max(v.abs())))
        };
        // 96000 入力なら 30kHz は阻止域の奥。折り返れば 14.1kHz で聞こえる。
        let stop = level(30_000.0, 96_000);
        let pass = level(1_000.0, 96_000);
        let db = 20.0 * (stop / pass).log10();
        assert!(db < -60.0, "阻止域が緩い: {db:.1} dB");

        // 48000 → 44100 でも、遮断のすぐ上は減る（遷移帯なので緩い）。
        let db48 = 20.0 * (level(23_000.0, 48_000) / level(1_000.0, 48_000)).log10();
        assert!(db48 < -6.0, "遷移帯で落ちていない: {db48:.1} dB");
    }

    /// 同じレートなら素通し。 変換で鈍らせない（`TR-REC-02` の「1回だけ」）。
    #[test]
    fn 同じレートなら素通し() {
        let x = sine(440.0, 44_100, 4410);
        let r = Resampler::to_master(44_100).expect("作れる");
        assert!(r.is_passthrough());
        assert_eq!(all_at_once(&x, 44_100, 44_100), x);
    }

    #[test]
    fn レートが0なら弾く() {
        assert_eq!(
            Resampler::new(0, 44_100).err(),
            Some(ResampleError::ZeroRate)
        );
        assert_eq!(
            Resampler::new(48_000, 0).err(),
            Some(ResampleError::ZeroRate)
        );
        assert!(ResampleError::ZeroRate.kind().starts_with("resample."));
    }
}
