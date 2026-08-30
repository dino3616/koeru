//! UTAU 互換 resampler。
//!
//! **`worldline` が OpenUtau のコードで取れないため、自前で書く**（`DEC-SYN-005`）。
//! 仕様は `TR-SYN-08` が引数一式として定義済みで、**同梱コアと外部エンジンが
//! 同じ引数で呼ばれることが、試唱と配布の音を一致させる前提**（`TR-SYN-35`）。
//!
//! ## 工程
//!
//! 1. oto の5値で切り出す（`offset` から `cutoff` まで）
//! 2. WORLD で分析（F0 / スペクトル包絡 / 非周期性指標）
//! 3. **子音部は伸縮させず、母音部だけを伸縮させて `required_length` に合わせる**
//! 4. ピッチベンドと収録音高から目標 F0 を作る
//! 5. WORLD で合成する
//!
//! ## 子音部を伸ばさない理由
//!
//! 子音は長さを変えると別の音に聞こえる。**母音は伸ばしても母音のまま。**
//! UTAU の `consonant`（固定範囲）はこのための値で、
//! **ここを伸ばすと「あー」が「あ゛ー」のように濁る。**

use crate::oto::Oto;
use crate::world;

/// resampler の入力（`TR-SYN-08` の引数一式）。
///
/// **外部 resampler の呼び出し規約でもある。** 同梱コアと同じ引数で呼ぶ。
#[derive(Debug, Clone)]
pub struct RenderRequest<'a> {
    /// 素材。**44100 Hz の倍精度配列**（`TR-SYN-08`）。
    pub samples: &'a [f64],
    pub sample_rate_hz: u32,
    /// 収録音高（MIDI ノート番号）。
    pub tone: i32,
    /// oto の5値。
    pub oto: Oto,
    /// 出したい長さ（ミリ秒）。
    pub required_length_ms: f64,
    /// 子音速度。100 が等倍で、大きいほど子音が短くなる。
    pub consonant_velocity: f64,
    /// 音量（%）。100 が等倍。
    pub volume: f64,
    /// モジュレーション（%）。**素材の F0 変化をどれだけ残すか。**
    /// 0 なら完全に目標 F0 へ倒す。
    pub modulation: f64,
    pub tempo: f64,
    /// ピッチベンド列（セント）。**目標音高からの差分。**
    pub pitch_bend_cents: &'a [f64],
    /// 周波数表。**長さ 0 で「無し」を表す**（`TR-SYN-08`）。
    /// 無い場合は合成コア側が F0 を推定する。
    pub frequency_table: &'a [f64],
}

/// 合成の失敗。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RenderError {
    /// oto の5値が素材の範囲に収まっていない。
    #[error("oto の切り出し範囲が素材に収まらない")]
    RegionOutOfRange,
    /// 出したい長さが 0 以下。
    #[error("required_length が正でない")]
    EmptyOutput,
}

impl RenderError {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::RegionOutOfRange => "synth.region_out_of_range",
            Self::EmptyOutput => "synth.empty_output",
        }
    }
}

type Result<T> = std::result::Result<T, RenderError>;

/// MIDI ノート番号を Hz にする。A4（69）= 440 Hz。
#[must_use]
pub fn midi_to_hz(note: i32) -> f64 {
    440.0 * 2.0_f64.powf(f64::from(note - 69) / 12.0)
}

/// 1音を合成する。
#[tracing::instrument(skip(req), fields(tone = req.tone, len_ms = req.required_length_ms), err)]
pub fn render(req: &RenderRequest<'_>) -> Result<Vec<f64>> {
    if req.required_length_ms <= 0.0 {
        return Err(RenderError::EmptyOutput);
    }
    let fs = req.sample_rate_hz;
    let per_ms = f64::from(fs) / 1000.0;
    let file_len_ms = req.samples.len() as f64 / per_ms;

    // ── 1. oto の5値で切り出す ──────────────────────────
    let usable_ms = req.oto.usable_ms(file_len_ms);
    if usable_ms <= 0.0 {
        return Err(RenderError::RegionOutOfRange);
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let start = (req.oto.offset_ms * per_ms) as usize;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let end = ((req.oto.offset_ms + usable_ms) * per_ms) as usize;
    if start >= req.samples.len() || end > req.samples.len() || start >= end {
        return Err(RenderError::RegionOutOfRange);
    }
    let region = &req.samples[start..end];

    // ── 2. 分析 ─────────────────────────────────────────
    let frame_ms = world::DEFAULT_FRAME_PERIOD_MS;
    let base_hz = midi_to_hz(req.tone);
    // 探索範囲は収録音高の上下1オクターブ。**話者音域が分かっているので絞れる。**
    let (src_f0, time_axis) = if req.frequency_table.is_empty() {
        world::estimate_f0(
            region,
            fs,
            world::F0Method::DioStoneMask,
            (base_hz / 2.0).max(40.0),
            base_hz * 2.0,
            frame_ms,
        )
    } else {
        // **周波数表があれば推定を省く**（TR-SYN-08）。録音時に作ってある（TR-SYN-21）。
        let n = req.frequency_table.len();
        let t: Vec<f64> = (0..n).map(|i| i as f64 * frame_ms / 1000.0).collect();
        (req.frequency_table.to_vec(), t)
    };
    let analysis = world::analyze_with_f0(region, fs, &src_f0, &time_axis, frame_ms);

    // ── 3. 子音部は伸縮させず、母音部だけを伸ばす ────────
    // 子音速度は 100 が等倍。**大きいほど子音が短くなる**（UTAU の慣例）。
    let consonant_scale = 100.0 / req.consonant_velocity.max(1.0);
    let consonant_src_ms = req.oto.consonant_ms.clamp(0.0, usable_ms);
    let consonant_out_ms = consonant_src_ms * consonant_scale;
    let vowel_src_ms = usable_ms - consonant_src_ms;
    let vowel_out_ms = (req.required_length_ms - consonant_out_ms).max(0.0);

    let out_frames = ((req.required_length_ms / frame_ms).round() as usize).max(1);
    let src_frames = analysis.f0.len();
    if src_frames == 0 {
        return Err(RenderError::RegionOutOfRange);
    }

    // 出力フレーム → 素材フレームの対応を作る。
    let mut map = Vec::with_capacity(out_frames);
    for i in 0..out_frames {
        let out_ms = i as f64 * frame_ms;
        let src_ms = if out_ms < consonant_out_ms {
            // 子音部。**伸縮率だけを掛ける。**
            out_ms / consonant_scale
        } else if vowel_out_ms > 0.0 && vowel_src_ms > 0.0 {
            // 母音部。**required_length に合わせて伸ばす。**
            let t = (out_ms - consonant_out_ms) / vowel_out_ms;
            consonant_src_ms + t * vowel_src_ms
        } else {
            consonant_src_ms
        };
        let idx = ((src_ms / frame_ms).round() as usize).min(src_frames - 1);
        map.push(idx);
    }

    let stretched = Stretched::new(&analysis, &map);

    // ── 4. 目標 F0 を作る ───────────────────────────────
    // 素材の F0 と目標音高の比を取り、モジュレーションで混ぜる。
    let src_mean = mean_voiced(&analysis.f0).unwrap_or(base_hz);
    let mod_ratio = (req.modulation / 100.0).clamp(0.0, 1.0);
    let mut target = Vec::with_capacity(out_frames);
    for (i, &src_idx) in map.iter().enumerate() {
        let src = analysis.f0[src_idx];
        if src <= 0.0 {
            target.push(0.0);
            continue;
        }
        // モジュレーション 0 なら完全に base_hz へ、100 なら素材の変化をそのまま比で移す。
        let shaped = base_hz * (src / src_mean).powf(mod_ratio);
        let cents = pitch_bend_at(req.pitch_bend_cents, i, out_frames);
        target.push(shaped * 2.0_f64.powf(cents / 1200.0));
    }

    // ── 5. 合成 ─────────────────────────────────────────
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let out_len = (req.required_length_ms * per_ms) as usize;
    if out_len == 0 {
        return Err(RenderError::EmptyOutput);
    }
    let mut y = world::synthesize(&stretched.analysis, &target, out_len);

    // 音量。100 が等倍。
    let gain = req.volume / 100.0;
    if (gain - 1.0).abs() > f64::EPSILON {
        for v in &mut y {
            *v *= gain;
        }
    }
    Ok(y)
}

/// 伸縮した分析結果。**フレームを付け替えるだけで、包絡そのものは作り直さない。**
struct Stretched {
    analysis: world::Analysis,
}

impl Stretched {
    fn new(src: &world::Analysis, map: &[usize]) -> Self {
        let spectrogram = map.iter().map(|&i| src.spectrogram[i].clone()).collect();
        let aperiodicity = map.iter().map(|&i| src.aperiodicity[i].clone()).collect();
        let f0 = map.iter().map(|&i| src.f0[i]).collect();
        let time_axis = (0..map.len())
            .map(|i| i as f64 * src.frame_period_ms / 1000.0)
            .collect();
        Self {
            analysis: world::Analysis {
                f0,
                time_axis,
                spectrogram,
                aperiodicity,
                fft_size: src.fft_size,
                frame_period_ms: src.frame_period_ms,
                sample_rate_hz: src.sample_rate_hz,
            },
        }
    }
}

fn mean_voiced(f0: &[f64]) -> Option<f64> {
    let v: Vec<f64> = f0.iter().copied().filter(|x| *x > 0.0).collect();
    if v.is_empty() {
        return None;
    }
    Some(v.iter().sum::<f64>() / v.len() as f64)
}

/// ピッチベンド列から、出力フレーム `i` にあたるセント値を引く。
///
/// **列の長さは出力フレーム数と一致しないので、線形に対応させる。**
fn pitch_bend_at(bend: &[f64], i: usize, out_frames: usize) -> f64 {
    if bend.is_empty() || out_frames == 0 {
        return 0.0;
    }
    if bend.len() == 1 {
        return bend[0];
    }
    let t = i as f64 / (out_frames.max(2) - 1) as f64;
    let pos = t * (bend.len() - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(bend.len() - 1);
    let frac = pos - lo as f64;
    bend[lo] * (1.0 - frac) + bend[hi] * frac
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::oto::{Oto, OtoPreset, derive_cv};

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

    fn req<'a>(samples: &'a [f64], oto: Oto, len_ms: f64, bend: &'a [f64]) -> RenderRequest<'a> {
        RenderRequest {
            samples,
            sample_rate_hz: 44_100,
            tone: 60, // C4 ≒ 261.6 Hz
            oto,
            required_length_ms: len_ms,
            consonant_velocity: 100.0,
            volume: 100.0,
            modulation: 0.0,
            tempo: 120.0,
            pitch_bend_cents: bend,
            frequency_table: &[],
        }
    }

    fn analyze_hz(y: &[f64]) -> f64 {
        let (f0, _) =
            world::estimate_f0(y, 44_100, world::F0Method::DioStoneMask, 60.0, 1200.0, 5.0);
        let v: Vec<f64> = f0.iter().copied().filter(|x| *x > 0.0).collect();
        if v.is_empty() {
            return 0.0;
        }
        v.iter().sum::<f64>() / v.len() as f64
    }

    #[test]
    fn midi_を_hz_に変換できる() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1e-9, "A4 = 440 Hz");
        assert!((midi_to_hz(81) - 880.0).abs() < 1e-6, "A5 = 880 Hz");
        assert!((midi_to_hz(60) - 261.6256).abs() < 0.001, "C4");
    }

    /// **収録音高と違う音高で鳴らせる。** resampler の中核。
    #[test]
    fn 目標音高で鳴る() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        // tone=60（C4 ≒ 261.6 Hz）を指定する
        let y = render(&req(&src, o, 300.0, &[])).expect("合成できる");
        assert!(!y.is_empty());
        let hz = analyze_hz(&y);
        assert!(
            (hz - 261.6).abs() < 20.0,
            "指定した C4 の近くで鳴る: {hz:.1} Hz"
        );
    }

    /// **required_length のとおりの長さが返る。**
    #[test]
    fn 指定した長さで返る() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        for len_ms in [100.0_f64, 300.0, 800.0] {
            let y = render(&req(&src, o, len_ms, &[])).expect("合成できる");
            let got_ms = y.len() as f64 / 44.1;
            assert!(
                (got_ms - len_ms).abs() < 2.0,
                "{len_ms}ms を頼んで {got_ms:.1}ms"
            );
        }
    }

    /// **素材より長く伸ばせる。** 母音部だけが伸びる。
    #[test]
    fn 素材より長く伸ばせる() {
        let src = voiced(220.0, 0.2, 44_100); // 200ms
        let o = derive_cv(0.0, 10.0, 190.0, 200.0, &OtoPreset::default(), false);
        let y = render(&req(&src, o, 1000.0, &[])).expect("合成できる");
        let got_ms = y.len() as f64 / 44.1;
        assert!((got_ms - 1000.0).abs() < 2.0, "5倍に伸びる: {got_ms:.1}ms");
        let hz = analyze_hz(&y);
        assert!(
            (hz - 261.6).abs() < 25.0,
            "伸ばしても音高は保つ: {hz:.1} Hz"
        );
    }

    /// **ピッチベンドが効く。** +1200 セント = 1オクターブ上。
    #[test]
    fn ピッチベンドで音高が動く() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        let y = render(&req(&src, o, 300.0, &[1200.0])).expect("合成できる");
        let hz = analyze_hz(&y);
        assert!(
            (hz - 523.3).abs() < 40.0,
            "1オクターブ上の C5 近辺: {hz:.1} Hz"
        );
    }

    /// **音量が掛かる。**
    #[test]
    fn 音量が効く() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        let full = render(&req(&src, o, 300.0, &[])).expect("合成できる");
        let mut half_req = req(&src, o, 300.0, &[]);
        half_req.volume = 50.0;
        let half = render(&half_req).expect("合成できる");
        let pf = full.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        let ph = half.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
        assert!((ph / pf - 0.5).abs() < 0.01, "半分になる: {:.3}", ph / pf);
    }

    /// **周波数表があれば推定を省く**（TR-SYN-08）。
    #[test]
    fn 周波数表を渡すと推定を省く() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        // 使う区間ぶんの F0 を一定値で渡す
        let frames = (480.0 / 5.0) as usize;
        let table = vec![220.0_f64; frames];
        let mut r = req(&src, o, 300.0, &[]);
        r.frequency_table = &table;
        let y = render(&r).expect("合成できる");
        assert!(!y.is_empty());
        let hz = analyze_hz(&y);
        assert!(
            (hz - 261.6).abs() < 25.0,
            "表を使っても目標音高で鳴る: {hz:.1}"
        );
    }

    #[test]
    fn 長さが正でなければ弾く() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = derive_cv(0.0, 20.0, 480.0, 500.0, &OtoPreset::default(), false);
        assert_eq!(
            render(&req(&src, o, 0.0, &[])),
            Err(RenderError::EmptyOutput)
        );
    }

    #[test]
    fn 範囲外の切り出しは弾く() {
        let src = voiced(220.0, 0.1, 44_100); // 100ms
        let o = Oto {
            offset_ms: 500.0, // ファイルより後ろ
            consonant_ms: 10.0,
            cutoff_ms: -50.0,
            preutterance_ms: 10.0,
            overlap_ms: 0.0,
        };
        assert_eq!(
            render(&req(&src, o, 300.0, &[])),
            Err(RenderError::RegionOutOfRange)
        );
    }

    #[test]
    fn ピッチベンドの補間は端で飽和する() {
        assert_eq!(pitch_bend_at(&[], 0, 10), 0.0);
        assert_eq!(pitch_bend_at(&[100.0], 5, 10), 100.0);
        assert!((pitch_bend_at(&[0.0, 100.0], 0, 11) - 0.0).abs() < 1e-9);
        assert!((pitch_bend_at(&[0.0, 100.0], 10, 11) - 100.0).abs() < 1e-9);
        assert!((pitch_bend_at(&[0.0, 100.0], 5, 11) - 50.0).abs() < 1e-9);
    }
}
