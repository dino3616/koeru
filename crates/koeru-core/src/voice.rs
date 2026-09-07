//! 声から色を作る（`DEC-PLT-025`、`DEC-PLT-027`）。
//!
//! 採用テイクの観測から、色相・彩度・明度の3つを決定的に導く。
//! 録るほど入力が増えるので、色は録るほど動いて、やがて落ち着く。
//!
//! # 明度と彩度をここで確定させない
//!
//! 返すのは 0〜1 の位置で、実際の値は面が決める。 暗い面と明るい面で
//! 同じ明度を使うと片方で地に沈むし（環は非テキストの図なので 3:1 が要る、
//! `TR-PLT-28`）、明るい面で sRGB に収まる彩度は暗い面より狭い——
//! 収まらない彩度は画面で丸められ、**声ごとの彩度の差がそこで消える。**
//! 位置だけを渡し、面ごとの幅への写しは CSS が持つ（`globals.css`）。
//!
//! 色相だけは絶対値で返す。 色相は面によって変えない——同じ声が
//! 明暗で違う色に見えると、一覧と音源の面で別の声に見える。
//!
//! # 色相に禁じられた帯がある
//!
//! 危険と注意（red / amber）だけが色相を持ってよい状態色なので
//! （`docs/design/direction.md`）、声がその帯に入ると状態と見分けがつかない。
//! [`HUE_START`] から始めて、赤と橙の帯を丸ごと外す。

/// 声から測った3つ。
///
/// どれも採用テイク全体の中央値。 平均にすると、極端なテイクが1本あるだけで
/// 色が動く——録り直しても戻らない。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceTraits {
    /// 有声フレームの F0 の中央値（Hz）。音高帯。
    pub pitch_hz: f64,
    /// 倍音の重心が基本周波数の何倍か。声の明るさ。
    ///
    /// 重心をそのまま使わない。 重心は音高に引きずられるので、
    /// 生の Hz を使うと色相（音高帯）と同じことを二度測ることになる。
    pub centroid_ratio: f64,
    /// 1テイクの有声区間の中央値（ミリ秒）。発声の長さ。
    pub voiced_ms: f64,
}

/// 画面へ渡す色。
///
/// OKLCH の3成分に対応する。 彩度と明度は位置だけで、幅は面が決める
/// （このモジュールの冒頭）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VoiceColor {
    /// 色相（度）。[`HUE_START`] から [`HUE_SPAN`] のあいだ。
    pub hue: f64,
    /// 彩度の位置（0〜1）。面ごとの幅へ写す前の値。
    pub chroma_t: f64,
    /// 明度の位置（0〜1）。面ごとの幅へ写す前の値。
    pub lightness_t: f64,
}

/// 色相の始まり（度）。
///
/// red と amber を外した先。 OKLCH で red-11 は 20 度付近、amber-11 は
/// 85 度付近にいるので、そこを含む帯を丸ごと空ける。
pub const HUE_START: f64 = 110.0;

/// 色相の幅（度）。[`HUE_START`] から一周の手前まで。
pub const HUE_SPAN: f64 = 250.0;

/// 音高帯の下端と上端（Hz）。
///
/// 低い男声から高い女声までを覆う。 外れた声は端に貼り付く——
/// 貼り付いても色は付くので、見分けがつかなくなるのは同じ端どうしだけ。
const PITCH_LO_HZ: f64 = 70.0;
const PITCH_HI_HZ: f64 = 450.0;

/// 重心比の下端と上端。
///
/// 実測ではなく、母音の第1・第2フォルマントが基本周波数の何倍に来るかの範囲から置いた。
/// **`DEC-PLT-027` の撤回条件がここに掛かっている。**
const RATIO_LO: f64 = 2.0;
const RATIO_HI: f64 = 14.0;

/// 発声の長さの下端と上端（ミリ秒）。
const VOICED_LO_MS: f64 = 200.0;
const VOICED_HI_MS: f64 = 1600.0;

/// 色相の区画の数。
///
/// 音高帯で区画を選び、区画の中の位置を重心が決める（`DEC-PLT-027`）。
///
/// **音高だけを色相の入力にしない。** 同じ人が録った音源は音高がほぼ同じなので、
/// 音高だけで決めると一人の音源が全部同じ色になる。手元の6音源で色相が
/// 170〜179 度と 214〜217 度の2つに割れ、2組は画素値まで同じになっていた
/// （`EVID-PLT-002`）。
///
/// 5 にした。 区画が広すぎると音高帯の違いが読めず、狭すぎると
/// 重心の違いが隣の音高帯へはみ出して音高帯の意味が消える。
const HUE_SECTORS: usize = 5;

impl VoiceColor {
    /// 観測から色を作る。
    #[must_use]
    pub fn from_traits(t: &VoiceTraits) -> Self {
        let centroid_t = log_position(t.centroid_ratio, RATIO_LO, RATIO_HI);
        Self {
            hue: hue_of(t.pitch_hz, centroid_t),
            chroma_t: centroid_t,
            lightness_t: log_position(t.voiced_ms, VOICED_LO_MS, VOICED_HI_MS),
        }
    }
}

/// 色相（度）。音高帯が区画を選び、重心が区画の中の位置を決める。
///
/// 重心は彩度にも効く（[`VoiceColor::chroma_t`]）。 1つの観測が2つの成分を
/// 動かすので、明るい声は「色相が進んで、かつ彩度が上がる」形で揃って動く。
/// 承知のうえで選んだ——**入力が3つしか無いので、色相を音高から離すには
/// どれかを二度使うほかない**（`DEC-PLT-027`）。
fn hue_of(pitch_hz: f64, centroid_t: f64) -> f64 {
    #[allow(clippy::cast_precision_loss, reason = "区画は 5 個")]
    let sectors = HUE_SECTORS as f64;
    let width = HUE_SPAN / sectors;
    let pitch_t = log_position(pitch_hz, PITCH_LO_HZ, PITCH_HI_HZ);

    // 上端はちょうど 1.0 になる。 最後の区画へ入れないと幅を1つ飛び出す。
    #[allow(clippy::cast_possible_truncation, reason = "0〜5 に収まる")]
    #[allow(clippy::cast_sign_loss, reason = "`log_position` は 0 以上")]
    let sector = ((pitch_t * sectors) as usize).min(HUE_SECTORS - 1);

    #[allow(clippy::cast_precision_loss, reason = "区画は 5 個")]
    let base = sector as f64 * width;
    HUE_START + base + width * centroid_t
}

/// 値を対数の目盛りで 0〜1 へ写す。
///
/// 線形で割らない。 音高も長さも比で効くもので、100Hz と 150Hz の違いは
/// 300Hz と 350Hz の違いより大きい。線形に割ると、低い声どうしが同じ色になる。
fn log_position(value: f64, lo: f64, hi: f64) -> f64 {
    if !value.is_finite() || value <= 0.0 || lo <= 0.0 || hi <= lo {
        return 0.0;
    }
    ((value.ln() - lo.ln()) / (hi.ln() - lo.ln())).clamp(0.0, 1.0)
}

/// 1テイクぶんの観測。台帳から引いた生の値。
#[derive(Debug, Clone, PartialEq)]
pub struct TakeVoice {
    /// フレームごとの F0（Hz）。無声は 0。
    pub f0: Vec<f64>,
    /// 倍音の重心（Hz）。測っていなければ `None`。
    pub centroid_hz: Option<f64>,
    /// F0 1フレームぶんの長さ（ミリ秒）。
    pub frame_ms: f64,
}

/// 採用テイクの束から、声の3つを測る。
///
/// 有声フレームが1つも無ければ `None`。 まだ録っていない音源に声は無いので、
/// 色も無い——空いた席に色を付けない（`docs/design/direction.md` の「欠け」）。
///
/// 重心が1つも取れなくても色は作る。 その場合の彩度は中央に落ちる。
/// 色を出さないほうが情報を失う——古いテイクにだけ重心が無い状態がありうる。
#[must_use]
pub fn traits_of(takes: &[TakeVoice]) -> Option<VoiceTraits> {
    let mut pitches: Vec<f64> = Vec::new();
    let mut ratios: Vec<f64> = Vec::new();
    let mut lengths: Vec<f64> = Vec::new();

    for t in takes {
        let voiced: Vec<f64> = t.f0.iter().copied().filter(|v| *v > 0.0).collect();
        if voiced.is_empty() {
            continue;
        }
        #[allow(clippy::cast_precision_loss, reason = "フレーム数はテイク長ぶん")]
        let length = voiced.len() as f64 * t.frame_ms;
        let pitch = median(&voiced)?;
        pitches.push(pitch);
        lengths.push(length);
        if let Some(c) = t.centroid_hz
            && c > 0.0
            && pitch > 0.0
        {
            ratios.push(c / pitch);
        }
    }

    Some(VoiceTraits {
        pitch_hz: median(&pitches)?,
        // 重心が無ければ中央へ置く。 対数の目盛りで中央になるのは両端の積の平方根。
        centroid_ratio: median(&ratios).unwrap_or((RATIO_LO * RATIO_HI).sqrt()),
        voiced_ms: median(&lengths)?,
    })
}

/// 中央値。空なら `None`。
fn median(values: &[f64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_by(f64::total_cmp);
    v.get(v.len() / 2).copied()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn take(pitch: f64, centroid: f64, frames: usize) -> TakeVoice {
        TakeVoice {
            f0: vec![pitch; frames],
            centroid_hz: Some(centroid),
            frame_ms: 5.0,
        }
    }

    #[test]
    fn 声が無ければ色も無い() {
        assert!(traits_of(&[]).is_none());
        assert!(
            traits_of(&[TakeVoice {
                f0: vec![0.0; 100],
                centroid_hz: Some(1500.0),
                frame_ms: 5.0,
            }])
            .is_none()
        );
    }

    #[test]
    fn 同じ観測からは同じ色が出る() {
        let takes = [take(180.0, 1800.0, 120), take(200.0, 2000.0, 100)];
        let a = VoiceColor::from_traits(&traits_of(&takes).expect("声がある"));
        let b = VoiceColor::from_traits(&traits_of(&takes).expect("声がある"));
        assert_eq!(a, b);
    }

    /// 色相が red / amber の帯へ入らない。 入ると状態色と見分けがつかない。
    #[test]
    fn 色相は禁じられた帯を外れる() {
        for hz in [40.0, 70.0, 120.0, 220.0, 450.0, 900.0] {
            let c = VoiceColor::from_traits(&VoiceTraits {
                pitch_hz: hz,
                centroid_ratio: 6.0,
                voiced_ms: 600.0,
            });
            assert!(
                (HUE_START..=HUE_START + HUE_SPAN).contains(&c.hue),
                "{hz} Hz が {} 度になった",
                c.hue
            );
        }
    }

    /// 高い声ほど色相が進む。 逆転すると、隣の声と入れ替わって見える。
    ///
    /// 重心比を揃えて比べる。 揃えないと区画の中の位置まで動くので、
    /// 何が色相を進めたのかが分からない。
    #[test]
    fn 音高が高いほど色相が進む() {
        let low = VoiceColor::from_traits(&traits_of(&[take(110.0, 1210.0, 100)]).expect("声"));
        let high = VoiceColor::from_traits(&traits_of(&[take(330.0, 3630.0, 100)]).expect("声"));
        assert!(low.hue < high.hue, "{} < {}", low.hue, high.hue);
    }

    /// 音高が同じでも、声色が違えば色相が動く。
    ///
    /// 同じ人が録った音源は音高がほぼ同じなので、ここが動かないと
    /// 一人の音源が全部同じ色になる（`EVID-PLT-002`）。
    #[test]
    fn 同じ音高でも重心が違えば色相が動く() {
        let dull = VoiceColor::from_traits(&traits_of(&[take(200.0, 800.0, 100)]).expect("声"));
        let bright = VoiceColor::from_traits(&traits_of(&[take(200.0, 2600.0, 100)]).expect("声"));
        assert!(
            (dull.hue - bright.hue).abs() > 10.0,
            "{} と {} が近すぎる",
            dull.hue,
            bright.hue
        );
    }

    /// 区画を飛び出さない。 飛び出すと red / amber の帯へ入る。
    #[test]
    fn 区画の端でも帯を外れない() {
        for pitch in [70.0, 90.0, 150.0, 250.0, 400.0, 450.0] {
            for ratio in [RATIO_LO, 5.0, RATIO_HI] {
                let c = VoiceColor::from_traits(&VoiceTraits {
                    pitch_hz: pitch,
                    centroid_ratio: ratio,
                    voiced_ms: 600.0,
                });
                assert!(
                    (HUE_START..=HUE_START + HUE_SPAN).contains(&c.hue),
                    "{pitch} Hz / 比 {ratio} が {} 度になった",
                    c.hue
                );
            }
        }
    }

    /// 重心が取れなくても色は出る。 古いテイクだけが重心を持たない状態がある。
    #[test]
    fn 重心が無くても色は出る() {
        let t = traits_of(&[TakeVoice {
            f0: vec![200.0; 100],
            centroid_hz: None,
            frame_ms: 5.0,
        }])
        .expect("声がある");
        let c = VoiceColor::from_traits(&t);
        assert!((0.0..=1.0).contains(&c.chroma_t));
    }

    /// 中央値を採る。 1本の極端なテイクで色を動かさない。
    #[test]
    fn 極端な一本で色が動かない() {
        let calm = [take(200.0, 2000.0, 100), take(205.0, 2050.0, 100)];
        let with_outlier = [
            take(200.0, 2000.0, 100),
            take(205.0, 2050.0, 100),
            take(440.0, 8000.0, 100),
        ];
        let a = traits_of(&calm).expect("声").pitch_hz;
        let b = traits_of(&with_outlier).expect("声").pitch_hz;
        assert!((a - b).abs() < 10.0, "{a} と {b}");
    }
}
