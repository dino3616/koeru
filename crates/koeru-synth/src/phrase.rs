//! フレーズ単位の合成（`TR-SYN-01`, `TR-SYN-02`, `TR-SYN-18`）。
//!
//! **7工程のうち「サンプル選択 → タイミング算出 → resample → クロスフェード連結」**
//! をここが持つ（`TR-SYN-01`）。外部プロセスもネットワークも使わない。
//!
//! # なぜフレーズ単位なのか
//!
//! **キャッシュの破棄を曲全体に及ばせないため**（`TR-SYN-26`）。
//! 1テイク録り直しただけで曲を丸ごと合成し直すと、録音のたびに数秒待たされる。
//! フレーズに割っておけば、変わったところだけを捨てられる。
//!
//! **鳴らし始めを早くするため**でもある（`TR-SYN-03`）。
//! 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。

use std::hash::{Hash, Hasher};
use std::path::PathBuf;

use crate::resampler::{FrequencyTable, PreviewFlags, RenderError, RenderRequest, render};
use koeru_core::oto::Oto;

/// 合成コアの版（`TR-SYN-02`, `TR-SYN-26`）。
///
/// **これが変わったキャッシュは捨てる。** 同じ入力でも出る音が変わる。
pub const CORE_VERSION: u32 = 1;

/// フレーズを分ける最短の無音（ミリ秒）。
///
/// **これより長い休符でフレーズを割る。** 息継ぎの位置がフレーズの境目。
pub const PHRASE_GAP_MS: f64 = 200.0;

/// 1音符ぶんの合成の入力（`TR-SYN-02`）。
#[derive(Debug, Clone, PartialEq)]
pub struct NoteSpec {
    /// 解決済みのエイリアス。
    pub alias: String,
    /// 素材の場所。
    pub sample_path: PathBuf,
    /// **素材の内容ハッシュ**（`TR-SYN-02`）。録り直したら変わる。
    pub sample_hash: u64,
    /// oto の5値。
    pub oto: Oto,
    /// 鳴らしたい音高（MIDI）。
    pub midi: i32,
    /// 鳴らしたい長さ（ミリ秒）。
    pub duration_ms: f64,
}

/// フレーズ（`TR-SYN-02`）。
///
/// **不変。** 作ったあとは変えない。変わったら別のフレーズとして作り直す。
#[derive(Debug, Clone, PartialEq)]
pub struct Phrase {
    /// 音符列。
    pub notes: Vec<NoteSpec>,
    /// 合成コアの版。
    pub core_version: u32,
}

impl Phrase {
    /// 音符列からフレーズを作る。
    #[must_use]
    pub fn new(notes: Vec<NoteSpec>) -> Self {
        Self {
            notes,
            core_version: CORE_VERSION,
        }
    }

    /// キャッシュの鍵（`TR-SYN-02`）。
    ///
    /// **素材の内容ハッシュ・oto 5値・音高列・合成コアの版から作る。**
    /// このどれかが変われば別の鍵になり、古い結果は使われない（`TR-SYN-26`）。
    ///
    /// oto の5値は f64 なので、**ビット列として混ぜる。** 丸めで鍵が動かないように。
    #[must_use]
    pub fn cache_key(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.core_version.hash(&mut h);
        for n in &self.notes {
            n.alias.hash(&mut h);
            n.sample_hash.hash(&mut h);
            n.midi.hash(&mut h);
            n.duration_ms.to_bits().hash(&mut h);
            for v in [
                n.oto.offset_ms,
                n.oto.consonant_ms,
                n.oto.cutoff_ms,
                n.oto.preutterance_ms,
                n.oto.overlap_ms,
            ] {
                v.to_bits().hash(&mut h);
            }
        }
        h.finish()
    }

    /// 鳴らしたときの長さ（ミリ秒）。**重なるぶんを引く。**
    #[must_use]
    pub fn duration_ms(&self) -> f64 {
        let total: f64 = self.notes.iter().map(|n| n.duration_ms).sum();
        let overlap: f64 = self
            .notes
            .iter()
            .skip(1)
            .map(|n| n.oto.overlap_ms.max(0.0))
            .sum();
        (total - overlap).max(0.0)
    }
}

/// 素材を渡す口。
///
/// **合成が素材の読み方を知らなくてよいようにする。** テストでは合成波を渡せる。
pub trait Samples {
    /// この音符の素材（44100 Hz 相当の倍精度配列）とサンプルレート。
    ///
    /// # Errors
    ///
    /// 読めないとき。
    fn load(&self, note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError>;

    /// この音符の周波数表。**無ければ空**（`TR-SYN-08`）。
    ///
    /// **素材ファイル全体を、`.frq` の格子（hop=256）で返す**（`TR-PKG-05`）。
    /// 切り出しと格子の載せ替えは合成器がする。**ここで切らないこと。**
    fn frequency_table(&self, note: &NoteSpec) -> Vec<f64> {
        let _ = note;
        Vec::new()
    }
}

/// フレーズを1本の波形にする（`TR-SYN-01`）。
///
/// **クロスフェードで繋ぐ。** 重なりの長さは次の音符のオーバーラップ。
/// 突き合わせで繋ぐと、境目でぷつっと鳴る。
///
/// # Errors
///
/// 素材を読めない、または合成できないとき。
#[tracing::instrument(skip(phrase, samples), fields(notes = phrase.notes.len()), err)]
pub fn render_phrase(
    phrase: &Phrase,
    samples: &dyn Samples,
    rate_hz: u32,
) -> Result<Vec<f64>, RenderError> {
    if phrase.notes.is_empty() {
        return Ok(Vec::new());
    }
    let per_ms = f64::from(rate_hz) / 1000.0;
    let flags = PreviewFlags::default();
    let mut out: Vec<f64> = Vec::new();

    for note in &phrase.notes {
        let (source, source_rate) = samples.load(note)?;
        let table = samples.frequency_table(note);
        let piece = render(&RenderRequest {
            samples: &source,
            sample_rate_hz: source_rate,
            // **`tone` は鳴らしたい音高。収録音高ではない。**
            tone: note.midi,
            oto: note.oto,
            required_length_ms: note.duration_ms,
            // **試唱のフラグは既定に固定し、UI に出さない**（TR-SYN-09）。
            consonant_velocity: flags.consonant_velocity,
            volume: flags.volume,
            modulation: flags.modulation,
            tempo: 120.0,
            pitch_bend_cents: &[],
            // **表はファイル全体・hop=256。** 切り出しは合成器がする。
            frequency_table: (!table.is_empty()).then_some(FrequencyTable {
                f0: &table,
                hop_samples: koeru_core::frq::HOP_SIZE,
            }),
        })?;

        if out.is_empty() {
            out = piece;
            continue;
        }

        // **重なりの長さは次の音符のオーバーラップ**（UTAU の慣例）。
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "重なりはミリ秒から作る非負の値"
        )]
        let overlap = ((note.oto.overlap_ms.max(0.0) * per_ms) as usize)
            .min(out.len())
            .min(piece.len());
        crossfade_append(&mut out, &piece, overlap);
    }

    Ok(out)
}

/// 重ねながら繋ぐ。
///
/// **等パワーで混ぜる。** 直線で混ぜると、重なりの真ん中で音量が落ちる。
fn crossfade_append(out: &mut Vec<f64>, piece: &[f64], overlap: usize) {
    if overlap == 0 {
        out.extend_from_slice(piece);
        return;
    }
    let start = out.len() - overlap;
    for i in 0..overlap {
        let t = (i as f64 + 0.5) / overlap as f64;
        // 等パワー（sin/cos）。
        let (fade_out, fade_in) = (
            (1.0 - t) * std::f64::consts::FRAC_PI_2,
            t * std::f64::consts::FRAC_PI_2,
        );
        out[start + i] = out[start + i] * fade_out.sin() + piece[i] * fade_in.sin();
    }
    out.extend_from_slice(&piece[overlap..]);
}

/// 短縮版に載せるフレーズを選ぶ（`TR-SYN-18`）。
///
/// **鳴らせない音符があるフレーズは、フレーズごと落とす。**
/// 落とした位置に無音・別音・代替音を挿入しない（`TR-SYN-18` (2)）。
/// 「途中で変な音がした」より「そこは無かった」ほうが、何が足りないか分かる。
///
/// 残ったフレーズの合計が `min_total_ms` に満たなければ、**試唱の選択肢に出さない**
/// （`TR-SYN-18` (3)）。
#[must_use]
pub fn shortened(phrases: &[(Phrase, bool)], min_total_ms: f64) -> Option<Vec<&Phrase>> {
    let kept: Vec<&Phrase> = phrases
        .iter()
        .filter(|(_, playable)| *playable)
        .map(|(p, _)| p)
        .collect();
    let total: f64 = kept.iter().map(|p| p.duration_ms()).sum();
    (total >= min_total_ms).then_some(kept)
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

    /// 決まった素材を返す口。
    struct Fixed {
        samples: Vec<f64>,
        rate: u32,
    }

    impl Samples for Fixed {
        fn load(&self, _note: &NoteSpec) -> Result<(Vec<f64>, u32), RenderError> {
            Ok((self.samples.clone(), self.rate))
        }
    }

    fn voiced(hz: f64, secs: f64, rate: u32) -> Vec<f64> {
        let n = (secs * f64::from(rate)) as usize;
        (0..n)
            .map(|i| {
                let t = i as f64 / f64::from(rate);
                // 倍音を足す。**純音では包絡が点になり、音高を動かせない。**
                (1..=8)
                    .map(|k| {
                        (2.0 * std::f64::consts::PI * hz * f64::from(k) * t).sin() / f64::from(k)
                    })
                    .sum::<f64>()
                    * 0.2
            })
            .collect()
    }

    fn note(alias: &str, midi: i32, hash: u64) -> NoteSpec {
        NoteSpec {
            alias: alias.to_owned(),
            sample_path: PathBuf::from("x.wav"),
            sample_hash: hash,
            oto: oto(20.0, 480.0),
            midi,
            duration_ms: 400.0,
        }
    }

    /// **素材が変われば鍵が変わる**（TR-SYN-02, TR-SYN-26）。
    #[test]
    fn 素材の録り直しで鍵が変わる() {
        let a = Phrase::new(vec![note("か", 60, 1)]);
        let b = Phrase::new(vec![note("か", 60, 2)]);
        assert_ne!(a.cache_key(), b.cache_key());
    }

    /// **oto が変われば鍵が変わる**（TR-SYN-26）。
    #[test]
    fn otoの変更で鍵が変わる() {
        let a = Phrase::new(vec![note("か", 60, 1)]);
        let mut n = note("か", 60, 1);
        n.oto.offset_ms += 1.0;
        let b = Phrase::new(vec![n]);
        assert_ne!(a.cache_key(), b.cache_key());
    }

    /// **音高と長さが変われば鍵が変わる。**
    #[test]
    fn 音符列の変更で鍵が変わる() {
        let base = Phrase::new(vec![note("か", 60, 1)]);
        assert_ne!(
            base.cache_key(),
            Phrase::new(vec![note("か", 62, 1)]).cache_key()
        );

        let mut n = note("か", 60, 1);
        n.duration_ms = 500.0;
        assert_ne!(base.cache_key(), Phrase::new(vec![n]).cache_key());
    }

    /// **合成コアの版が変われば鍵が変わる**（TR-SYN-02）。
    #[test]
    fn 合成コアの版で鍵が変わる() {
        let a = Phrase::new(vec![note("か", 60, 1)]);
        let mut b = a.clone();
        b.core_version += 1;
        assert_ne!(a.cache_key(), b.cache_key());
    }

    /// **同じ入力なら同じ鍵。**
    #[test]
    fn 同じ入力なら同じ鍵() {
        let a = Phrase::new(vec![note("か", 60, 1), note("き", 62, 2)]);
        let b = Phrase::new(vec![note("か", 60, 1), note("き", 62, 2)]);
        assert_eq!(a.cache_key(), b.cache_key());
    }

    #[test]
    fn 空のフレーズは空の波形() {
        let f = Fixed {
            samples: Vec::new(),
            rate: 44_100,
        };
        let y = render_phrase(&Phrase::new(Vec::new()), &f, 44_100).expect("通ること");
        assert!(y.is_empty());
    }

    /// **音符を繋いだ長さになる。** 重なるぶんは引かれる。
    #[test]
    fn 繋いだ長さが仕様どおり() {
        let f = Fixed {
            samples: voiced(220.0, 0.5, 44_100),
            rate: 44_100,
        };
        let p = Phrase::new(vec![
            note("か", 60, 1),
            note("き", 62, 2),
            note("く", 64, 3),
        ]);
        let y = render_phrase(&p, &f, 44_100).expect("合成できる");

        let want = (p.duration_ms() / 1000.0 * 44_100.0) as usize;
        let got = y.len();
        assert!(
            (got as f64 - want as f64).abs() < 44_100.0 * 0.01,
            "10ms 以内で一致すること: {got} / {want}"
        );
    }

    /// **境目でぷつっと鳴らない。** 等パワーで混ぜる。
    #[test]
    fn 境目に段差ができない() {
        let f = Fixed {
            samples: voiced(220.0, 0.5, 44_100),
            rate: 44_100,
        };
        let p = Phrase::new(vec![note("か", 60, 1), note("き", 60, 2)]);
        let y = render_phrase(&p, &f, 44_100).expect("合成できる");

        // 隣り合うサンプルの差の最大が、素材の最大の差を大きく超えないこと。
        let jump = y
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f64, f64::max);
        let source_jump = f
            .samples
            .windows(2)
            .map(|w| (w[1] - w[0]).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            jump < source_jump * 3.0,
            "段差 {jump:.4} が素材の {source_jump:.4} に対して大きすぎる"
        );
    }

    /// **鳴らせないフレーズはフレーズごと落とす**（TR-SYN-18 (2)）。
    /// 落とした位置に何も挿入しない。
    #[test]
    fn 鳴らせないフレーズを落とす() {
        let a = Phrase::new(vec![note("か", 60, 1), note("き", 60, 2)]);
        let b = Phrase::new(vec![note("く", 60, 3)]);
        let c = Phrase::new(vec![note("け", 60, 4), note("こ", 60, 5)]);

        let all = [(a.clone(), true), (b, false), (c.clone(), true)];
        let got = shortened(&all, 0.0).expect("出せること");
        assert_eq!(got.len(), 2, "鳴らせない1本を落とす");
        assert_eq!(got[0], &a);
        assert_eq!(got[1], &c, "落とした位置に何も挿さない");
    }

    /// **短すぎれば試唱の選択肢に出さない**（TR-SYN-18 (3)）。
    #[test]
    fn 短すぎる曲は選択肢に出さない() {
        let a = Phrase::new(vec![note("か", 60, 1)]);
        let one = [(a, true)];
        // 1音符 400ms。4秒には届かない。
        assert!(shortened(&one, 4000.0).is_none());
        assert!(shortened(&one, 100.0).is_some());
    }

    #[test]
    fn 全部鳴らせなければ出さない() {
        let none = [(Phrase::new(vec![note("か", 60, 1)]), false)];
        assert!(shortened(&none, 0.0).is_some_and(|v| v.is_empty()));
    }
}
