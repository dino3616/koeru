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

use crate::world;
use koeru_core::oto::Oto;

/// resampler の入力（`TR-SYN-08` の引数一式）。
///
/// **外部 resampler の呼び出し規約でもある。** 同梱コアと同じ引数で呼ぶ。
#[derive(Debug, Clone)]
pub struct RenderRequest<'a> {
    /// 素材。**44100 Hz の倍精度配列**（`TR-SYN-08`）。
    pub samples: &'a [f64],
    pub sample_rate_hz: u32,
    /// **鳴らしたい音高**（MIDI ノート番号）。収録音高ではない。
    ///
    /// 素材が何の高さで録られていたかは、ここには要らない。
    /// WORLD が F0 を直に置き換えるので、収録音高はスペクトル包絡の中に暗黙に残る。
    /// **`modulation` が 0 なら、出力の F0 はちょうどこの音高になる。**
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
    /// 素材の周波数表。**無ければ `None`**（`TR-SYN-08`）。
    /// 無い場合は合成コア側が F0 を推定する。
    pub frequency_table: Option<FrequencyTable<'a>>,
}

/// 素材の周波数表（`.frq`）。
///
/// # 何が入っているかを型に言わせる
///
/// **素材ファイル全体を、`.frq` の格子で持つ。** 合成器が見るのは
/// oto で切り出した区間を 5ms の格子で並べたものなので、**どちらも合わない。**
/// 切り出しと載せ替えは [`render`] が行う。**呼び出し側で切らないこと。**
///
/// 以前ここは `&[f64]` 1つで、上の2つがどちらも書かれていなかった。
/// **`.frq` をそのまま渡す実装が2箇所あり、声が雑音になって音高も乗らなかった**
/// （`tests/frequency_table_grid.rs`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrequencyTable<'a> {
    /// F0（Hz）。**無声は 0。**
    ///
    /// **先頭は素材ファイルの先頭。** oto の offset ではない。
    pub f0: &'a [f64],
    /// 格子の間隔（サンプル）。`.frq` は 256（`TR-PKG-05`）。
    pub hop_samples: u32,
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

    /// 素材が見つからない、または読めない。
    #[error("素材を読めない")]
    SourceUnavailable,

    /// **素材がマスターの形式で無い**（`TR-SYN-31`）。
    ///
    /// 試唱のために変換して通さないので、ここで止まる。
    /// **`RegionOutOfRange` に畳まないこと**——畳むと「oto の切り出し範囲が
    /// 素材に収まらない」と出て、**oto を疑うことになる**（踏んだ）。
    #[error("素材のサンプルレートがマスターと違う")]
    SampleRateMismatch,
}

impl RenderError {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::RegionOutOfRange => "synth.region_out_of_range",
            Self::EmptyOutput => "synth.empty_output",
            Self::SourceUnavailable => "synth.source_unavailable",
            Self::SampleRateMismatch => "synth.sample_rate_mismatch",
        }
    }
}

type Result<T> = std::result::Result<T, RenderError>;

/// 素材の F0 を探す下限（Hz）。**歌声の音域を広く取る。**
///
/// 目標音高から範囲を作ってはいけない。素材は別の音高で録られている。
const SOURCE_F0_FLOOR_HZ: f64 = 55.0;
/// 素材の F0 を探す上限（Hz）。
const SOURCE_F0_CEIL_HZ: f64 = 1100.0;

/// 試唱で固定する音色フラグ（`TR-SYN-09`）。
///
/// **UI に出さない。** 試唱は「自分の声が歌になる」を確かめる場であって、
/// 音を作り込む場ではない。ここを触れるようにすると、
/// **本人の声そのものではないものを聴いて判断することになる。**
///
/// **試唱の設定値を oto や配布パッケージへ書き込むこともしない**（`TR-SYN-09`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PreviewFlags {
    /// 子音速度。100 が等倍。
    pub consonant_velocity: f64,
    /// 音量（%）。**試唱では既定に固定する**（`TR-SYN-29`）。
    pub volume: f64,
    /// モジュレーション（%）。**0 は目標 F0 へ完全に倒す。**
    pub modulation: f64,
}

impl Default for PreviewFlags {
    fn default() -> Self {
        Self {
            consonant_velocity: 100.0,
            volume: 100.0,
            modulation: 0.0,
        }
    }
}

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
    // **探索範囲は目標音高から作らない。** 素材は別の音高で録られている
    // ——それを別の音高で鳴らすのが resampler の役目なので、目標から範囲を引くと
    // 素材の基本周波数が範囲外に落ちる。**実際に踏んだ。**
    // 歌声の音域を広く取る。**周波数表があれば、そもそも推定しない**（TR-SYN-08）。
    let (src_f0, time_axis) = match req.frequency_table {
        None => world::estimate_f0(
            region,
            fs,
            world::F0Method::DioStoneMask,
            SOURCE_F0_FLOOR_HZ,
            SOURCE_F0_CEIL_HZ,
            frame_ms,
        ),
        // **周波数表があれば推定を省く**（TR-SYN-08）。録音時に作ってある（TR-SYN-21）。
        // **ただし格子も範囲も違うので、載せ替える**（下の `resample_table`）。
        Some(t) => resample_table(&t, req.oto.offset_ms, region.len(), fs, frame_ms),
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

/// 周波数表を、切り出した区間の 5ms 格子へ載せ替える（`TR-SYN-08`, `TR-PKG-05`）。
///
/// # なぜ要るか
///
/// `.frq` は **ファイル全体**を **hop=256 サンプル**の格子で持つ（`TR-PKG-05`）。
/// 44100 Hz では 5.805ms なので、**5ms の格子とは1フレームあたり 16% ずれる。**
/// さらに先頭が違う——表はファイルの先頭から、区間は `offset_ms` から始まる。
///
/// **そのまま渡すと、offset 手前の無音（F0=0）が発声の先頭に当たる。**
/// F0=0 のフレームを WORLD は無声として合成するので、**声が雑音になり、
/// 目標音高も乗らない。** 実測で1オクターブ下・周期性 0.31 になった。
///
/// # 補間しない
///
/// **0 は「無声」であって「0 Hz」ではない。** 有声と無声の間を線形に混ぜると、
/// どちらでもない値ができる。**いちばん近い格子点をそのまま採る。**
fn resample_table(
    table: &FrequencyTable<'_>,
    offset_ms: f64,
    region_samples: usize,
    sample_rate_hz: u32,
    frame_ms: f64,
) -> (Vec<f64>, Vec<f64>) {
    let per_ms = f64::from(sample_rate_hz) / 1000.0;
    // WORLD がこの長さの波形に対して作るフレーム数と揃える。
    #[allow(clippy::cast_precision_loss)]
    let region_ms = region_samples as f64 / per_ms;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (region_ms / frame_ms) as usize + 1;
    let hop = f64::from(table.hop_samples.max(1));

    let mut f0 = Vec::with_capacity(frames);
    let mut axis = Vec::with_capacity(frames);
    for i in 0..frames {
        #[allow(clippy::cast_precision_loss)]
        let into_region_ms = i as f64 * frame_ms;
        // **表の索引はファイルの先頭から数える。**
        let at_sample = (offset_ms + into_region_ms) * per_ms;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let idx = (at_sample / hop).round() as usize;
        // 表の外は無声として扱う。
        f0.push(table.f0.get(idx).copied().unwrap_or(0.0));
        axis.push(into_region_ms / 1000.0);
    }
    (f0, axis)
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

    /// 素材の切り出し位置を、導出規約を通さずに直に置く。
    ///
    /// **`koeru-align` の `derive_cv` を呼ばない**（`DEC-ALN-009` で crate が分かれた）。
    /// resampler の試験は「この5値ならこう鳴る」を見るもので、
    /// **規約が変わるたびに落ちてはいけない。**
    const fn oto(preutterance_ms: f64, usable_ms: f64) -> Oto {
        Oto {
            offset_ms: 0.0,
            consonant_ms: preutterance_ms + 30.0,
            cutoff_ms: -usable_ms,
            preutterance_ms,
            overlap_ms: preutterance_ms / 3.0,
        }
    }

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
            frequency_table: None,
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
        let o = oto(20.0, 480.0);
        // tone=60（C4 ≒ 261.6 Hz）を指定する
        let y = render(&req(&src, o, 300.0, &[])).expect("合成できる");
        assert!(!y.is_empty());
        let hz = analyze_hz(&y);
        assert!(
            (hz - 261.6).abs() < 20.0,
            "指定した C4 の近くで鳴る: {hz:.1} Hz"
        );
    }

    /// **別の音高を頼めば、別の音が返る。**
    ///
    /// `tone` は「鳴らしたい音高」であって収録音高ではない。ここに収録音高を渡すと、
    /// **どの音高を選んでも同じ高さで鳴る。** アプリ側で実際にそう書いて、
    /// 「3音とも鳴った」まで通ってしまった。
    ///
    /// 実機ハーネスでは検出できない。素材に有声フレームが無いと目標 F0 が全部 0 になり、
    /// どの音高でも同じ波形が返るので、静かな部屋では毎回すり抜ける。
    /// **だからここに置く。**
    #[test]
    fn 別の音高を頼めば別の音が返る() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = oto(20.0, 480.0);

        let mut req_lo = req(&src, o, 300.0, &[]);
        req_lo.tone = 55; // G3 ≒ 196 Hz
        let mut req_hi = req(&src, o, 300.0, &[]);
        req_hi.tone = 67; // G4 ≒ 392 Hz

        let lo = render(&req_lo).expect("合成できる");
        let hi = render(&req_hi).expect("合成できる");
        assert_ne!(lo.len(), 0);
        assert_eq!(lo.len(), hi.len(), "長さは音高で変わらない");
        assert!(
            lo.iter().zip(&hi).any(|(a, b)| (a - b).abs() > 1e-9),
            "**同じ波形が返っている。tone が使われていない**"
        );

        let (lo_hz, hi_hz) = (analyze_hz(&lo), analyze_hz(&hi));
        assert!(
            (lo_hz - midi_to_hz(55)).abs() < 15.0,
            "G3 の近くで鳴る: {lo_hz:.1} Hz"
        );
        assert!(
            (hi_hz - midi_to_hz(67)).abs() < 25.0,
            "G4 の近くで鳴る: {hi_hz:.1} Hz"
        );
        assert!(hi_hz > lo_hz * 1.8, "1オクターブぶん離れていること");
    }

    /// **試唱のフラグは既定に固定する**（TR-SYN-09）。
    ///
    /// ここが動くと、聴いているのが本人の声そのものではなくなる。
    #[test]
    fn 試唱のフラグは既定に固定される() {
        let f = PreviewFlags::default();
        assert!((f.consonant_velocity - 100.0).abs() < f64::EPSILON, "等倍");
        assert!((f.volume - 100.0).abs() < f64::EPSILON, "等倍");
        assert!(
            (f.modulation - 0.0).abs() < f64::EPSILON,
            "目標音高へ完全に倒す"
        );
    }

    /// **required_length のとおりの長さが返る。**
    #[test]
    fn 指定した長さで返る() {
        let src = voiced(220.0, 0.5, 44_100);
        let o = oto(20.0, 480.0);
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
        let o = oto(10.0, 190.0);
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
        let o = oto(20.0, 480.0);
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
        let o = oto(20.0, 480.0);
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
        let o = oto(20.0, 480.0);
        // **`.frq` と同じ形で渡す**——ファイル全体を hop=256 の格子で。
        let table = vec![220.0_f64; src.len().div_ceil(256)];
        let mut r = req(&src, o, 300.0, &[]);
        r.frequency_table = Some(FrequencyTable {
            f0: &table,
            hop_samples: 256,
        });
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
        let o = oto(20.0, 480.0);
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
