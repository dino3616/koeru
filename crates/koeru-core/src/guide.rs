//! ガイド音と音高提示（`TR-REC-23`, `TR-REC-24`）。
//!
//! 単独音では既定でガイドを使わない（`TR-REC-23`）。無音の中で1フレーズずつ出す。
//! ただし音高は伝える必要があるので、持続音だけを短く鳴らす経路は残る。
//! 連続音・CVVC・多音階では既定でガイドを使う。
//!
//! # 2成分に分ける
//!
//! 1. 該当音高の持続音 — 何の高さで歌うか
//! 2. 拍のクリック — どの速さで進むか
//!
//! それぞれ独立に音量を設定でき、独立に無音にできる（`TR-REC-23`）。
//! 片方だけ要る人がいる——音高は分かるが拍が要る人、その逆。
//!
//! # 同梱する
//!
//! 音源ファイルを外から入手させない（`TR-REC-23`, `TR-PLT-20`）。
//! ここで合成するので、同梱物はゼロ。

use std::f64::consts::TAU;

/// 既定のテンポ（BPM、`TR-REC-23`）。
pub const DEFAULT_TEMPO_BPM: f64 = 120.0;

/// 発声開始までの助走（ミリ秒、`TR-REC-23`）。
pub const DEFAULT_LEAD_IN_MS: f64 = 1250.0;

/// フレーズの末尾余白（ミリ秒）。
pub const DEFAULT_TAIL_MS: f64 = 500.0;

/// ガイドを使わない方式で、次のフレーズへ進むまでの長さ（ミリ秒、`TR-REC-20`）。
///
/// 発話の検出結果を条件にしない。 声を認識してから進む形にすると、
/// 小さい声・かすれた声・咳払いで挙動が変わり、何が起きたか説明できなくなる。
/// 固定長なら、遅れても早くても同じだけ待つ。
pub const AUTO_ADVANCE_MS: f64 = 3000.0;

/// クリックの長さ（ミリ秒）。短くする。 発声に被ると邪魔になる。
const CLICK_MS: f64 = 18.0;

/// クリックの基本周波数（Hz）。
const CLICK_HZ: f64 = 1800.0;

/// 小節頭のクリック（Hz）。1拍目だけ高くして、頭が分かるようにする。
const CLICK_ACCENT_HZ: f64 = 2400.0;

/// 持続音の立ち上がり・立ち下がり（ミリ秒）。ぶつっと切らない。
const FADE_MS: f64 = 25.0;

/// ガイドの作り方（`TR-REC-23`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuideSpec {
    /// テンポ（BPM）。1モーラ = 1拍。
    pub tempo_bpm: f64,
    /// このフレーズのモーラ数。録音リストの構造に一致させる（`TR-REC-23`）。
    pub moras: u32,
    /// 発声開始までの助走（ミリ秒）。
    pub lead_in_ms: f64,
    /// 末尾余白（ミリ秒）。
    pub tail_ms: f64,
    /// 持続音の音量（0.0〜1.0）。0.0 で無音。
    pub tone_level: f64,
    /// クリックの音量（0.0〜1.0）。0.0 で無音。
    pub click_level: f64,
}

impl Default for GuideSpec {
    fn default() -> Self {
        Self {
            tempo_bpm: DEFAULT_TEMPO_BPM,
            moras: 1,
            lead_in_ms: DEFAULT_LEAD_IN_MS,
            tail_ms: DEFAULT_TAIL_MS,
            tone_level: 0.25,
            click_level: 0.2,
        }
    }
}

impl GuideSpec {
    /// 単独音の音高提示（`TR-REC-23`）。
    ///
    /// ガイドは使わないが、音高は伝える。 持続音だけを、助走なしで短く。
    #[must_use]
    pub fn pitch_reference() -> Self {
        Self {
            moras: 1,
            lead_in_ms: 0.0,
            tail_ms: 0.0,
            click_level: 0.0,
            ..Self::default()
        }
    }

    /// 1拍の長さ（ミリ秒）。
    #[must_use]
    pub fn beat_ms(&self) -> f64 {
        60_000.0 / self.tempo_bpm.max(1.0)
    }

    /// フレーズ全体の長さ（ミリ秒）。
    ///
    /// 助走 + モーラ数×拍長 + 末尾余白（`TR-REC-23`）。
    #[must_use]
    pub fn total_ms(&self) -> f64 {
        self.lead_in_ms + f64::from(self.moras) * self.beat_ms() + self.tail_ms
    }

    /// 発声を始める位置（ミリ秒）。助走が終わったところ。
    #[must_use]
    pub const fn voice_start_ms(&self) -> f64 {
        self.lead_in_ms
    }

    /// どちらの成分も鳴らないか。
    #[must_use]
    pub fn is_silent(&self) -> bool {
        self.tone_level <= 0.0 && self.click_level <= 0.0
    }
}

/// 次のフレーズへ進むまでの長さ（`TR-REC-20`）。
///
/// ガイドを使うならガイド1フレーズぶん、使わないなら固定長。
/// どちらも発話の検出には依らない。
#[must_use]
pub fn advance_ms(spec: Option<&GuideSpec>) -> f64 {
    spec.map_or(AUTO_ADVANCE_MS, GuideSpec::total_ms)
}

/// MIDI ノート番号を Hz にする。A4（69）= 440 Hz。
#[must_use]
pub fn midi_to_hz(note: i32) -> f64 {
    440.0 * 2.0_f64.powf(f64::from(note - 69) / 12.0)
}

/// ガイドを1フレーズ合成する。
///
/// 同梱物を要らなくする。 ここで作るので、外部ファイルの入手を求めない。
#[must_use]
pub fn render(spec: &GuideSpec, midi: i32, rate_hz: u32) -> Vec<f32> {
    let fs = f64::from(rate_hz);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "長さはミリ秒から作る非負の値"
    )]
    let total = ((spec.total_ms() / 1000.0) * fs) as usize;
    if total == 0 {
        return Vec::new();
    }
    let mut out = vec![0.0_f32; total];

    if spec.tone_level > 0.0 {
        add_tone(&mut out, spec, midi, fs);
    }
    if spec.click_level > 0.0 {
        add_clicks(&mut out, spec, fs);
    }

    // 足し合わせて 1.0 を超えたら全体を縮める。 割れたガイドを鳴らさない。
    let peak = out.iter().fold(0.0_f32, |m, v| m.max(v.abs()));
    if peak > 1.0 {
        let k = 1.0 / peak;
        for v in &mut out {
            *v *= k;
        }
    }
    out
}

/// 持続音を重ねる。倍音を少し足す。 純音は音高が取りにくい。
fn add_tone(out: &mut [f32], spec: &GuideSpec, midi: i32, fs: f64) {
    let hz = midi_to_hz(midi);
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "位置はミリ秒から作る非負の値"
    )]
    let (start, end) = (
        ((spec.voice_start_ms() / 1000.0) * fs) as usize,
        (((spec.voice_start_ms() + f64::from(spec.moras) * spec.beat_ms()) / 1000.0) * fs) as usize,
    );
    let end = end.min(out.len());
    if start >= end {
        return;
    }
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "フェード長はミリ秒から作る非負の値"
    )]
    let fade = ((FADE_MS / 1000.0) * fs) as usize;
    let span = end - start;

    for (i, v) in out[start..end].iter_mut().enumerate() {
        let t = i as f64 / fs;
        // 基音 + 2倍音 + 3倍音。音高が取りやすい厚みにする。
        let s = (TAU * hz * t).sin()
            + 0.4 * (TAU * hz * 2.0 * t).sin()
            + 0.2 * (TAU * hz * 3.0 * t).sin();
        let env = fade_envelope(i, span, fade);
        #[allow(
            clippy::cast_possible_truncation,
            reason = "音量は 0.0..=1.0。最後に全体を正規化する"
        )]
        let sample = (s / 1.6 * spec.tone_level * env) as f32;
        *v += sample;
    }
}

/// クリックを重ねる。
fn add_clicks(out: &mut [f32], spec: &GuideSpec, fs: f64) {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "長さはミリ秒から作る非負の値"
    )]
    let click_len = ((CLICK_MS / 1000.0) * fs) as usize;

    // 助走のあいだも鳴らす。いつ始まるかが分からないと助走の意味が無い。
    let beats_before = (spec.lead_in_ms / spec.beat_ms()).floor();
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "助走の拍数は非負"
    )]
    let lead_beats = beats_before as u32;

    for b in 0..(lead_beats + spec.moras) {
        let ms = spec.voice_start_ms() - f64::from(lead_beats - b.min(lead_beats)) * spec.beat_ms()
            + f64::from(b.saturating_sub(lead_beats)) * spec.beat_ms();
        if ms < 0.0 {
            continue;
        }
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "位置はミリ秒から作る非負の値"
        )]
        let at = ((ms / 1000.0) * fs) as usize;
        // 助走の最後の拍が「次で入る」の合図。 そこだけ高くする。
        let hz = if b == lead_beats.saturating_sub(1) || b == lead_beats {
            CLICK_ACCENT_HZ
        } else {
            CLICK_HZ
        };
        for i in 0..click_len {
            let Some(v) = out.get_mut(at + i) else { break };
            let t = i as f64 / fs;
            // 短い減衰。尾を引かせない。
            let env = (1.0 - i as f64 / click_len as f64).powi(2);
            #[allow(
                clippy::cast_possible_truncation,
                reason = "音量は 0.0..=1.0。最後に全体を正規化する"
            )]
            let sample = ((TAU * hz * t).sin() * env * spec.click_level) as f32;
            *v += sample;
        }
    }
}

/// 立ち上がり・立ち下がりの包絡。
fn fade_envelope(i: usize, span: usize, fade: usize) -> f64 {
    if fade == 0 || span <= fade * 2 {
        return 1.0;
    }
    if i < fade {
        i as f64 / fade as f64
    } else if i + fade >= span {
        (span - i) as f64 / fade as f64
    } else {
        1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peak(x: &[f32]) -> f32 {
        x.iter().fold(0.0, |m, v| m.max(v.abs()))
    }

    #[test]
    fn 長さは助走とモーラと余白の合計() {
        let s = GuideSpec {
            moras: 8,
            ..GuideSpec::default()
        };
        // 120 BPM → 1拍 500ms。1250 + 8*500 + 500 = 5750ms。
        assert!((s.total_ms() - 5750.0).abs() < 1e-9);
        assert!((s.beat_ms() - 500.0).abs() < 1e-9);
        assert!((s.voice_start_ms() - 1250.0).abs() < 1e-9);
    }

    #[test]
    fn 合成した長さが仕様と一致する() {
        let s = GuideSpec {
            moras: 4,
            ..GuideSpec::default()
        };
        let y = render(&s, 60, 44_100);
        let want = (s.total_ms() / 1000.0 * 44_100.0) as usize;
        assert_eq!(y.len(), want);
    }

    /// 2成分を独立に無音にできる（`TR-REC-23`）。
    #[test]
    fn 成分ごとに無音にできる() {
        let base = GuideSpec {
            moras: 2,
            ..GuideSpec::default()
        };

        let both = render(&base, 60, 44_100);
        assert!(peak(&both) > 0.0);

        let no_click = render(
            &GuideSpec {
                click_level: 0.0,
                ..base
            },
            60,
            44_100,
        );
        let no_tone = render(
            &GuideSpec {
                tone_level: 0.0,
                ..base
            },
            60,
            44_100,
        );
        assert!(peak(&no_click) > 0.0, "持続音だけでも鳴る");
        assert!(peak(&no_tone) > 0.0, "クリックだけでも鳴る");
        assert_ne!(no_click, no_tone);

        let silent = render(
            &GuideSpec {
                tone_level: 0.0,
                click_level: 0.0,
                ..base
            },
            60,
            44_100,
        );
        assert!(peak(&silent).abs() < 1e-9, "両方切れば完全に無音");
        assert!(
            GuideSpec {
                tone_level: 0.0,
                click_level: 0.0,
                ..base
            }
            .is_silent()
        );
    }

    /// 助走のあいだは持続音が鳴らない。 発声位置から鳴る。
    #[test]
    fn 持続音は助走のあとから鳴る() {
        let s = GuideSpec {
            moras: 2,
            click_level: 0.0,
            ..GuideSpec::default()
        };
        let y = render(&s, 60, 44_100);
        let lead = (s.lead_in_ms / 1000.0 * 44_100.0) as usize;
        // 助走の途中（フェードの手前）は無音。
        assert!(peak(&y[..lead - 2000]).abs() < 1e-6);
        assert!(peak(&y[lead + 2000..lead + 10_000]) > 0.0);
    }

    /// クリックは助走のあいだも鳴る。 いつ始まるか分からないと助走の意味が無い。
    #[test]
    fn クリックは助走のあいだも鳴る() {
        let s = GuideSpec {
            moras: 2,
            tone_level: 0.0,
            ..GuideSpec::default()
        };
        let y = render(&s, 60, 44_100);
        let lead = (s.lead_in_ms / 1000.0 * 44_100.0) as usize;
        assert!(peak(&y[..lead]) > 0.0, "助走の中で鳴ること");
    }

    /// 割れたガイドを鳴らさない。
    #[test]
    fn 足し合わせて割れない() {
        let y = render(
            &GuideSpec {
                moras: 4,
                tone_level: 1.0,
                click_level: 1.0,
                ..GuideSpec::default()
            },
            60,
            44_100,
        );
        assert!(peak(&y) <= 1.0);
    }

    /// 単独音はガイドを使わず、音高だけ伝える（`TR-REC-23`）。
    #[test]
    fn 単独音の音高提示はクリックも助走も無い() {
        let s = GuideSpec::pitch_reference();
        assert!((s.click_level - 0.0).abs() < f64::EPSILON);
        assert!((s.lead_in_ms - 0.0).abs() < f64::EPSILON);
        assert!((s.total_ms() - s.beat_ms()).abs() < 1e-9);

        let y = render(&s, 57, 44_100);
        assert!(peak(&y) > 0.0);
    }

    #[test]
    fn 音高が変われば波形が変わる() {
        let s = GuideSpec::pitch_reference();
        let lo = render(&s, 48, 44_100);
        let hi = render(&s, 72, 44_100);
        assert_eq!(lo.len(), hi.len());
        assert!(lo.iter().zip(&hi).any(|(a, b)| (a - b).abs() > 1e-6));
    }

    #[test]
    fn midi_を_hz_に変換できる() {
        assert!((midi_to_hz(69) - 440.0).abs() < 1e-9);
        assert!((midi_to_hz(57) - 220.0).abs() < 1e-9);
    }
    /// 発話の検出結果を条件にしない（`TR-REC-20`）。
    #[test]
    fn 次へ進む長さは固定かガイド1フレーズ分() {
        assert!((advance_ms(None) - AUTO_ADVANCE_MS).abs() < f64::EPSILON);

        let s = GuideSpec {
            moras: 8,
            ..GuideSpec::default()
        };
        assert!((advance_ms(Some(&s)) - s.total_ms()).abs() < 1e-9);
    }
}
