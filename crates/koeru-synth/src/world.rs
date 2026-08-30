//! WORLD の束縛。
//!
//! **同梱した C++（`vendor/world`）を `extern "C"` で直接叩く**（`DEC-SYN-001`）。
//! `Rust-WORLD` のような既存の束縛を採らないのは、必要な関数が5つしかなく、
//! **束ねる相手を組織メンテのものに限るという方針**（`DEC-REC-001`）に対して、
//! この規模のものに例外を作らないため。
//!
//! ## 使う工程
//!
//! | 工程 | 関数 | 役割 |
//! |---|---|---|
//! | F0 推定 | `Dio` + `StoneMask` | 退避経路。既定は SwiftF0（`DEC-SYN-004`） |
//! | F0 推定 | `Harvest` | 同上。話者音域が判明したあとに引き直す（TR-SYN-22） |
//! | スペクトル包絡 | `CheapTrick` | |
//! | 非周期性指標 | `D4C` | |
//! | 合成 | `Synthesis` | |
//!
//! **CheapTrick・D4C・合成は WORLD のまま**（`DEC-SYN-001`）。

use std::os::raw::{c_double, c_int};

#[allow(non_snake_case)]
mod sys {
    use std::os::raw::{c_double, c_int};

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub(super) struct DioOption {
        pub(super) f0_floor: c_double,
        pub(super) f0_ceil: c_double,
        pub(super) channels_in_octave: c_double,
        pub(super) frame_period: c_double,
        pub(super) speed: c_int,
        pub(super) allowed_range: c_double,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub(super) struct HarvestOption {
        pub(super) f0_floor: c_double,
        pub(super) f0_ceil: c_double,
        pub(super) frame_period: c_double,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub(super) struct CheapTrickOption {
        pub(super) q1: c_double,
        pub(super) f0_floor: c_double,
        pub(super) fft_size: c_int,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    pub(super) struct D4COption {
        pub(super) threshold: c_double,
    }

    unsafe extern "C" {
        pub(super) fn InitializeDioOption(option: *mut DioOption);
        pub(super) fn GetSamplesForDIO(fs: c_int, x_length: c_int, frame_period: c_double)
        -> c_int;
        pub(super) fn Dio(
            x: *const c_double,
            x_length: c_int,
            fs: c_int,
            option: *const DioOption,
            time_axis: *mut c_double,
            f0: *mut c_double,
        );
        pub(super) fn StoneMask(
            x: *const c_double,
            x_length: c_int,
            fs: c_int,
            time_axis: *const c_double,
            f0: *const c_double,
            f0_length: c_int,
            refined_f0: *mut c_double,
        );

        pub(super) fn InitializeHarvestOption(option: *mut HarvestOption);
        pub(super) fn GetSamplesForHarvest(
            fs: c_int,
            x_length: c_int,
            frame_period: c_double,
        ) -> c_int;
        pub(super) fn Harvest(
            x: *const c_double,
            x_length: c_int,
            fs: c_int,
            option: *const HarvestOption,
            time_axis: *mut c_double,
            f0: *mut c_double,
        );

        pub(super) fn InitializeCheapTrickOption(fs: c_int, option: *mut CheapTrickOption);
        pub(super) fn GetFFTSizeForCheapTrick(fs: c_int, option: *const CheapTrickOption) -> c_int;
        pub(super) fn CheapTrick(
            x: *const c_double,
            x_length: c_int,
            fs: c_int,
            time_axis: *const c_double,
            f0: *const c_double,
            f0_length: c_int,
            option: *const CheapTrickOption,
            spectrogram: *mut *mut c_double,
        );

        pub(super) fn InitializeD4COption(option: *mut D4COption);
        pub(super) fn D4C(
            x: *const c_double,
            x_length: c_int,
            fs: c_int,
            time_axis: *const c_double,
            f0: *const c_double,
            f0_length: c_int,
            fft_size: c_int,
            option: *const D4COption,
            aperiodicity: *mut *mut c_double,
        );

        pub(super) fn Synthesis(
            f0: *const c_double,
            f0_length: c_int,
            spectrogram: *const *const c_double,
            aperiodicity: *const *const c_double,
            fft_size: c_int,
            frame_period: c_double,
            fs: c_int,
            y_length: c_int,
            y: *mut c_double,
        );
    }
}

/// WORLD のフレーム周期の既定（ミリ秒）。
pub const DEFAULT_FRAME_PERIOD_MS: f64 = 5.0;

/// F0 推定の手法（`TR-SYN-22`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum F0Method {
    /// 速い。**最初の数テイクはこれで即座に `.frq` を確定させる。**
    DioStoneMask,
    /// 精度が高い。**話者音域が判明したあとに静かに引き直す。**
    Harvest,
}

/// 分析の結果。合成へそのまま渡せる。
#[derive(Debug, Clone)]
pub struct Analysis {
    /// フレームごとの基本周波数（Hz）。無声は 0。
    pub f0: Vec<f64>,
    /// フレームごとの時刻（秒）。
    pub time_axis: Vec<f64>,
    /// スペクトル包絡。`f0.len()` × `fft_size / 2 + 1`。
    pub spectrogram: Vec<Vec<f64>>,
    /// 非周期性指標。形は `spectrogram` と同じ。
    pub aperiodicity: Vec<Vec<f64>>,
    pub fft_size: usize,
    pub frame_period_ms: f64,
    pub sample_rate_hz: u32,
}

/// F0 だけを推定する（`.frq` の生成に使う。TR-SYN-21）。
///
/// `floor_hz` / `ceil_hz` は探索範囲。**話者音域が判明したら下限を引き上げる**（TR-SYN-22）。
#[tracing::instrument(skip(samples), fields(len = samples.len()))]
#[must_use]
pub fn estimate_f0(
    samples: &[f64],
    sample_rate_hz: u32,
    method: F0Method,
    floor_hz: f64,
    ceil_hz: f64,
    frame_period_ms: f64,
) -> (Vec<f64>, Vec<f64>) {
    let fs = sample_rate_hz as c_int;
    let len = samples.len() as c_int;
    if len == 0 {
        return (Vec::new(), Vec::new());
    }

    match method {
        F0Method::DioStoneMask => {
            let mut opt = sys::DioOption {
                f0_floor: 0.0,
                f0_ceil: 0.0,
                channels_in_octave: 0.0,
                frame_period: 0.0,
                speed: 0,
                allowed_range: 0.0,
            };
            // SAFETY: opt は有効な領域。WORLD が既定値で埋める。
            unsafe { sys::InitializeDioOption(&raw mut opt) };
            opt.f0_floor = floor_hz;
            opt.f0_ceil = ceil_hz;
            opt.frame_period = frame_period_ms;

            // SAFETY: 引数はすべて有効。
            let n = unsafe { sys::GetSamplesForDIO(fs, len, frame_period_ms) } as usize;
            let mut time_axis = vec![0.0_f64; n];
            let mut f0 = vec![0.0_f64; n];
            // SAFETY: 出力は n 要素ぶん確保済み。GetSamplesForDIO が返した長さ。
            unsafe {
                sys::Dio(
                    samples.as_ptr(),
                    len,
                    fs,
                    &raw const opt,
                    time_axis.as_mut_ptr(),
                    f0.as_mut_ptr(),
                );
            }
            let mut refined = vec![0.0_f64; n];
            // SAFETY: 同上。refined も n 要素。
            unsafe {
                sys::StoneMask(
                    samples.as_ptr(),
                    len,
                    fs,
                    time_axis.as_ptr(),
                    f0.as_ptr(),
                    n as c_int,
                    refined.as_mut_ptr(),
                );
            }
            (refined, time_axis)
        }
        F0Method::Harvest => {
            let mut opt = sys::HarvestOption {
                f0_floor: 0.0,
                f0_ceil: 0.0,
                frame_period: 0.0,
            };
            // SAFETY: opt は有効な領域。
            unsafe { sys::InitializeHarvestOption(&raw mut opt) };
            opt.f0_floor = floor_hz;
            opt.f0_ceil = ceil_hz;
            opt.frame_period = frame_period_ms;

            // SAFETY: 引数はすべて有効。
            let n = unsafe { sys::GetSamplesForHarvest(fs, len, frame_period_ms) } as usize;
            let mut time_axis = vec![0.0_f64; n];
            let mut f0 = vec![0.0_f64; n];
            // SAFETY: 出力は n 要素ぶん確保済み。
            unsafe {
                sys::Harvest(
                    samples.as_ptr(),
                    len,
                    fs,
                    &raw const opt,
                    time_axis.as_mut_ptr(),
                    f0.as_mut_ptr(),
                );
            }
            (f0, time_axis)
        }
    }
}

/// 与えた F0 でスペクトル包絡と非周期性指標を求める。
///
/// **F0 は外から渡す。** 既定は SwiftF0（`DEC-SYN-004`）で、WORLD の推定は退避経路。
#[tracing::instrument(skip(samples, f0, time_axis), fields(frames = f0.len()))]
#[must_use]
pub fn analyze_with_f0(
    samples: &[f64],
    sample_rate_hz: u32,
    f0: &[f64],
    time_axis: &[f64],
    frame_period_ms: f64,
) -> Analysis {
    let fs = sample_rate_hz as c_int;
    let len = samples.len() as c_int;
    let n = f0.len();

    let mut ct = sys::CheapTrickOption {
        q1: 0.0,
        f0_floor: 0.0,
        fft_size: 0,
    };
    // SAFETY: ct は有効な領域。WORLD が既定値で埋める。
    unsafe { sys::InitializeCheapTrickOption(fs, &raw mut ct) };
    // SAFETY: ct は初期化済み。
    let fft_size = unsafe { sys::GetFFTSizeForCheapTrick(fs, &raw const ct) };
    ct.fft_size = fft_size;
    let width = (fft_size / 2 + 1) as usize;

    // **C 側は `double**` を要求する。** 行ごとの Vec を作り、その先頭ポインタの配列を渡す。
    let mut spec: Vec<Vec<f64>> = vec![vec![0.0; width]; n];
    let mut spec_ptrs: Vec<*mut c_double> = spec.iter_mut().map(|r| r.as_mut_ptr()).collect();
    // SAFETY: spec_ptrs は n 本のポインタで、それぞれ width 要素を指す。
    unsafe {
        sys::CheapTrick(
            samples.as_ptr(),
            len,
            fs,
            time_axis.as_ptr(),
            f0.as_ptr(),
            n as c_int,
            &raw const ct,
            spec_ptrs.as_mut_ptr(),
        );
    }

    let mut d4c = sys::D4COption { threshold: 0.0 };
    // SAFETY: d4c は有効な領域。
    unsafe { sys::InitializeD4COption(&raw mut d4c) };

    let mut ap: Vec<Vec<f64>> = vec![vec![0.0; width]; n];
    let mut ap_ptrs: Vec<*mut c_double> = ap.iter_mut().map(|r| r.as_mut_ptr()).collect();
    // SAFETY: 同上。
    unsafe {
        sys::D4C(
            samples.as_ptr(),
            len,
            fs,
            time_axis.as_ptr(),
            f0.as_ptr(),
            n as c_int,
            fft_size,
            &raw const d4c,
            ap_ptrs.as_mut_ptr(),
        );
    }

    Analysis {
        f0: f0.to_vec(),
        time_axis: time_axis.to_vec(),
        spectrogram: spec,
        aperiodicity: ap,
        fft_size: fft_size as usize,
        frame_period_ms,
        sample_rate_hz,
    }
}

/// 分析結果から波形を合成する。
///
/// `f0` を差し替えるとピッチが変わる。**resampler はここを使う**（`DEC-SYN-005`）。
#[tracing::instrument(skip(analysis, f0), fields(frames = f0.len()))]
#[must_use]
pub fn synthesize(analysis: &Analysis, f0: &[f64], out_len: usize) -> Vec<f64> {
    let n = f0.len().min(analysis.spectrogram.len());
    if n == 0 || out_len == 0 {
        return Vec::new();
    }
    let spec_ptrs: Vec<*const c_double> = analysis.spectrogram[..n]
        .iter()
        .map(|r| r.as_ptr())
        .collect();
    let ap_ptrs: Vec<*const c_double> = analysis.aperiodicity[..n]
        .iter()
        .map(|r| r.as_ptr())
        .collect();
    let mut y = vec![0.0_f64; out_len];
    // SAFETY: すべてのポインタは上で作った有効な領域を指す。
    // y は out_len 要素ぶん確保済み。
    unsafe {
        sys::Synthesis(
            f0.as_ptr(),
            n as c_int,
            spec_ptrs.as_ptr(),
            ap_ptrs.as_ptr(),
            analysis.fft_size as c_int,
            analysis.frame_period_ms,
            analysis.sample_rate_hz as c_int,
            out_len as c_int,
            y.as_mut_ptr(),
        );
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 倍音を持つ信号を作る。
    ///
    /// **純粋な正弦波はボコーダの入力として非現実的。** 倍音が1本しか無いと、
    /// スペクトル包絡がほぼ点になり、ピッチを変えて合成し直した結果から
    /// 基本周波数を引き直せない。**実際に踏んだ。** 声は倍音を持つ。
    fn voiced(hz: f64, secs: f64, fs: u32) -> Vec<f64> {
        let n = (secs * f64::from(fs)) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(fs);
                let mut v = 0.0;
                for k in 1..=8 {
                    v += (std::f64::consts::TAU * hz * f64::from(k) * t).sin() / f64::from(k);
                }
                v * 0.2
            })
            .collect()
    }

    /// **440 Hz の正弦波から 440 Hz が出る。** FFI の引数順が合っていることの確認。
    #[test]
    fn 有声音の基本周波数を当てられる() {
        let fs = 44_100;
        let x = voiced(440.0, 0.5, fs);
        let (f0, _t) = estimate_f0(&x, fs, F0Method::DioStoneMask, 100.0, 800.0, 5.0);
        let voiced: Vec<f64> = f0.iter().copied().filter(|v| *v > 0.0).collect();
        assert!(!voiced.is_empty(), "有声と判定されるフレームがある");
        let mean = voiced.iter().sum::<f64>() / voiced.len() as f64;
        assert!(
            (mean - 440.0).abs() < 10.0,
            "平均 {mean:.1} Hz が 440 Hz の近く"
        );
    }

    /// Harvest でも同じ答えになる（TR-SYN-22 の退避経路）。
    #[test]
    fn harvest_でも基本周波数を当てられる() {
        let fs = 44_100;
        let x = voiced(220.0, 0.5, fs);
        let (f0, _t) = estimate_f0(&x, fs, F0Method::Harvest, 100.0, 800.0, 5.0);
        let voiced: Vec<f64> = f0.iter().copied().filter(|v| *v > 0.0).collect();
        assert!(!voiced.is_empty());
        let mean = voiced.iter().sum::<f64>() / voiced.len() as f64;
        assert!((mean - 220.0).abs() < 10.0, "平均 {mean:.1} Hz");
    }

    /// **分析して合成し直すと、元に近い波形が返る。** 工程が繋がっていることの確認。
    #[test]
    fn 分析してから合成し直せる() {
        let fs = 44_100;
        let x = voiced(220.0, 0.3, fs);
        let (f0, t) = estimate_f0(&x, fs, F0Method::DioStoneMask, 100.0, 800.0, 5.0);
        let a = analyze_with_f0(&x, fs, &f0, &t, 5.0);
        assert!(a.fft_size >= 1024, "FFT サイズが取れる: {}", a.fft_size);
        assert_eq!(a.spectrogram.len(), f0.len());

        let y = synthesize(&a, &f0, x.len());
        assert_eq!(y.len(), x.len());
        let peak = y.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!(peak > 0.05, "無音ではない: ピーク {peak:.4}");

        // 合成し直した波形からも同じ基本周波数が出る
        let (f0b, _) = estimate_f0(&y, fs, F0Method::DioStoneMask, 100.0, 800.0, 5.0);
        let voiced: Vec<f64> = f0b.iter().copied().filter(|v| *v > 0.0).collect();
        let mean = voiced.iter().sum::<f64>() / voiced.len() as f64;
        assert!(
            (mean - 220.0).abs() < 15.0,
            "再合成後も 220 Hz 近辺: {mean:.1}"
        );
    }

    /// **F0 を2倍にすると1オクターブ上がる。** resampler のピッチ変更がここに乗る。
    #[test]
    fn 基本周波数を差し替えると音高が変わる() {
        let fs = 44_100;
        let x = voiced(220.0, 0.3, fs);
        let (f0, t) = estimate_f0(&x, fs, F0Method::DioStoneMask, 100.0, 800.0, 5.0);
        let a = analyze_with_f0(&x, fs, &f0, &t, 5.0);

        let doubled: Vec<f64> = f0
            .iter()
            .map(|v| if *v > 0.0 { v * 2.0 } else { 0.0 })
            .collect();
        let y = synthesize(&a, &doubled, x.len());
        let (f0b, _) = estimate_f0(&y, fs, F0Method::DioStoneMask, 60.0, 1200.0, 5.0);
        let voiced: Vec<f64> = f0b.iter().copied().filter(|v| *v > 0.0).collect();
        assert!(!voiced.is_empty());
        let mean = voiced.iter().sum::<f64>() / voiced.len() as f64;
        assert!(
            (mean - 440.0).abs() < 15.0,
            "1オクターブ上がる: {mean:.1} Hz"
        );
    }

    #[test]
    fn 空の入力で落ちない() {
        let (f0, t) = estimate_f0(&[], 44_100, F0Method::DioStoneMask, 100.0, 800.0, 5.0);
        assert!(f0.is_empty());
        assert!(t.is_empty());
    }
}
