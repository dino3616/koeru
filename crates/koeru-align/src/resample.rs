//! アライメントの入口で 44100 Hz を 16000 Hz へ落とす。
//!
//! **モデルが 16kHz を前提にしている**（`EVID-ALN-001`。`meta.json` の
//! `sample_frequency: 16000`）。KOERU のマスターは 44100 Hz（`TR-REC-02`）なので、
//! ここで1回だけ変換する。
//!
//! # 録音側のリサンプラとは別物
//!
//! `TR-REC-02` の「リサンプルは1回だけ」は**キャプチャからマスターまで**の話。
//! ここはマスターを読む側の変換で、**マスターには触らない**（`TR-PKG-39` の不変性）。
//! 配布用の 16bit も、試唱も、この変換を通らない。**アライメントだけが通る。**
//!
//! # 決定性を実装で固定する
//!
//! `TR-ALN-29` が「同一の WAV から出力がビット単位で同一」を要求している。
//! **窓関数付き sinc の係数を固定し、足し合わせる順序も固定する。**
//! 外部のリサンプラを呼ばないのは、版が変わると係数が変わるため。
//!
//! **[Risk] 浮動小数点の決定性はプラットフォームを跨ぐと崩れうる**
//! （`TR-ALN-29` notes）。ここが保証するのは同一環境での再現性まで。
//!
//! # なぜ手前で帯域を落とすのか
//!
//! 44100 → 16000 は間引きなので、**8kHz より上を落とさないと折り返す。**
//! sinc の遮断周波数を出力側のナイキストに合わせてあるので、
//! 補間と帯域制限が同じ畳み込みで済む。

/// 窓関数付き sinc の片側の幅（タップ数の半分）。
///
/// **16 は「アライメントに要る精度」と「1テイクの変換にかかる時間」の折り合い。**
/// 40ms・16kHz のフレームに対して十分に平坦な通過域が出る。
const HALF_WIDTH: isize = 16;

/// リサンプルの失敗。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ResampleError {
    /// 入力または出力のサンプリング周波数が 0。
    #[error("サンプリング周波数が 0")]
    ZeroRate,

    /// 入力が空。
    #[error("入力が空")]
    Empty,
}

impl ResampleError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::ZeroRate => "resample.zero_rate",
            Self::Empty => "resample.empty",
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

/// Blackman 窓。**両端で 0 になるので、打ち切りの段差が出ない。**
fn blackman(n: f64, width: f64) -> f64 {
    let t = (n / width).mul_add(0.5, 0.5).clamp(0.0, 1.0);
    let tau = std::f64::consts::TAU;
    0.42 - 0.5 * (tau * t).cos() + 0.08 * (2.0 * tau * t).cos()
}

/// サンプリング周波数を変える。
///
/// **入力より出力が低いときは、遮断周波数を出力側のナイキストに合わせる**
/// （折り返しを防ぐ）。上げるときは入力側のナイキストのまま。
///
/// # Errors
///
/// サンプリング周波数が 0、入力が空。
pub fn resample(input: &[f32], from_hz: u32, to_hz: u32) -> Result<Vec<f32>, ResampleError> {
    if from_hz == 0 || to_hz == 0 {
        return Err(ResampleError::ZeroRate);
    }
    if input.is_empty() {
        return Err(ResampleError::Empty);
    }
    if from_hz == to_hz {
        return Ok(input.to_vec());
    }

    let ratio = f64::from(from_hz) / f64::from(to_hz); // 出力1つあたりの入力サンプル数
    // **間引くときだけ帯域を絞る。** 補間のときに絞ると、要らない鈍りが入る。
    let cutoff = if ratio > 1.0 { 1.0 / ratio } else { 1.0 };

    #[allow(clippy::cast_precision_loss)]
    let n_in = input.len() as f64;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n_out = (n_in / ratio).floor() as usize;
    let mut out = Vec::with_capacity(n_out);

    // **窓の幅を sinc の伸びに合わせる。** 間引くとき sinc は `1/cutoff` 倍に伸びるので、
    // 入力サンプル数で数えた幅も同じだけ広げないと、**ローブが数本しか入らず阻止域が緩む。**
    // 幅を固定したまま遮断だけ下げて、12kHz が -10dB しか落ちなかった（**踏んだ**）。
    #[allow(clippy::cast_precision_loss)]
    let half = HALF_WIDTH as f64 / cutoff;
    #[allow(clippy::cast_possible_truncation)]
    let taps = half.ceil() as isize;
    for n in 0..n_out {
        #[allow(clippy::cast_precision_loss)]
        let center = n as f64 * ratio;
        #[allow(clippy::cast_possible_truncation)]
        let base = center.floor() as isize;

        // **和の順序を固定する。** 低い添字から順に足す。
        //
        // **範囲外は 0 として扱う。** 欠けた窓の重みで割り戻すと、
        // 信号が急に始まるとき**先頭に無い音を作る**——12kHz の正弦波で
        // 定常部が -88dB まで落ちているのに、先頭だけ -10dB しか落ちなかった（**踏んだ**）。
        // ゼロ詰めなら端は素直に立ち上がるだけで、作り物が出ない。
        let mut acc = 0.0_f64;
        for k in (base - taps + 1)..=(base + taps) {
            if k < 0 || k as usize >= input.len() {
                continue;
            }
            #[allow(clippy::cast_precision_loss)]
            let d = k as f64 - center;
            let w = sinc(d * cutoff) * blackman(d, half) * cutoff;
            #[allow(clippy::cast_sign_loss)]
            let x = f64::from(input[k as usize]);
            acc = w.mul_add(x, acc);
        }
        #[allow(clippy::cast_possible_truncation)]
        out.push(acc as f32);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 正弦波を作る。
    fn sine(hz: f64, rate: u32, samples: usize) -> Vec<f32> {
        (0..samples)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f64 / f64::from(rate);
                #[allow(clippy::cast_possible_truncation)]
                let v = (std::f64::consts::TAU * hz * t).sin() as f32;
                v * 0.5
            })
            .collect()
    }

    /// 帯域の中心あたりの周波数を測る（ゼロ交差の数から）。
    fn measure_hz(x: &[f32], rate: u32) -> f64 {
        let crossings = x
            .windows(2)
            .filter(|w| (w[0] < 0.0) != (w[1] < 0.0))
            .count();
        #[allow(clippy::cast_precision_loss)]
        let secs = x.len() as f64 / f64::from(rate);
        #[allow(clippy::cast_precision_loss)]
        let c = crossings as f64;
        c / 2.0 / secs
    }

    #[test]
    fn 同じ周波数なら素通り() {
        let x = sine(440.0, 44_100, 1000);
        assert_eq!(resample(&x, 44_100, 44_100).expect("通る"), x);
    }

    #[test]
    fn 長さが比率どおりになる() {
        let x = sine(440.0, 44_100, 44_100);
        let y = resample(&x, 44_100, 16_000).expect("通る");
        // 44100 サンプル ÷ (44100/16000) = 16000
        assert!((15_990..=16_010).contains(&y.len()), "{}", y.len());
    }

    /// **音の高さが変わらない。** ここが崩れると、モデルが別の声を見る。
    #[test]
    fn 音の高さが保たれる() {
        let x = sine(440.0, 44_100, 44_100);
        let y = resample(&x, 44_100, 16_000).expect("通る");
        let hz = measure_hz(&y, 16_000);
        assert!((430.0..=450.0).contains(&hz), "{hz} Hz");
    }

    /// **定常部で振幅が保たれる。**
    ///
    /// 端は窓が欠けるぶん立ち上がりになる。**そこを割り戻して埋めない**——
    /// 埋めると無い音を作る（`resample` の説明を参照）。
    /// 実際の録音は前後に 300ms 以上の無音余白を持つ（`TR-REC-38`）ので、
    /// 端の立ち上がりが声に掛かることはない。
    #[test]
    fn 定常部で振幅が保たれる() {
        let x = sine(440.0, 44_100, 44_100);
        let y = resample(&x, 44_100, 16_000).expect("通る");
        let body = &y[200..y.len() - 200];
        let peak = body.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!((0.45..=0.55).contains(&peak), "peak {peak}");
    }

    /// **8kHz より上は落ちる。** 落とさないと折り返して、
    /// 存在しない低い音として現れる。
    #[test]
    fn ナイキストより上は落ちる() {
        // 12kHz は 16kHz のナイキスト（8kHz）より上。**折り返せば 4kHz に出る。**
        let x = sine(12_000.0, 44_100, 44_100);
        let y = resample(&x, 44_100, 16_000).expect("通る");

        // **定常部で測る。** 試験信号が t=0 で急に始まるので、
        // 端には信号自身の過渡（広帯域）が乗る。**それは折り返しではなく、
        // 正当に通過した低域成分。** ここで見たいのは定常状態の阻止量。
        let body = &y[200..y.len() - 200];
        let peak = body.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
        assert!(peak < 0.001, "折り返している: peak {peak}");
    }

    /// **同じ入力からは同じ出力**（`TR-ALN-29`）。
    #[test]
    fn 決定的に同じ結果が出る() {
        let x = sine(440.0, 44_100, 10_000);
        assert_eq!(
            resample(&x, 44_100, 16_000).expect("通る"),
            resample(&x, 44_100, 16_000).expect("通る")
        );
    }

    #[test]
    fn 空や周波数ゼロは拒む() {
        assert_eq!(
            resample(&[], 44_100, 16_000).unwrap_err(),
            ResampleError::Empty
        );
        assert_eq!(
            resample(&[0.0], 0, 16_000).unwrap_err(),
            ResampleError::ZeroRate
        );
        assert_eq!(
            resample(&[0.0], 44_100, 0).unwrap_err(),
            ResampleError::ZeroRate
        );
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [ResampleError::ZeroRate, ResampleError::Empty] {
            assert!(e.kind().starts_with("resample."));
        }
    }
}
