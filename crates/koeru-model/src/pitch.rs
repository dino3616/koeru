//! 収録した音高の分布（S06b のグラフ）。
//!
//! 描き方の正本は `docs/reports/ux/redesign-canvas/canvas.json` の付箋 `memoS06bPitch`、
//! 調べたことと考察は同じ場所の `boards/P4f-PitchChart.dc.html`。 ここは数えて並べるだけで、
//! 色・目盛り・札は画面が持つ。
//!
//! # 数えるのは使われる区間の有声フレームだけ
//!
//! 息や無音で F0 推定が拾う低い雑音は、ほとんどが原音設定の左ブランクより前か
//! 右ブランクより後ろにある。 区間で切らないと、歌っていない音高がグラフの左に
//! 裾として残る（闇音レンリの ↑ で D3 より下が 6.8 % → 0.3 %）。
//!
//! 数えるのは時間で、フレームの数ではない。 フレーム長はテイクの解析ごとに持つので、
//! 長さの違うフレームを同じ1つとして数えると、枠どうしの割合が比べられなくなる。
//!
//! # 音高は MIDI セントで持つ
//!
//! MIDI ノート番号の 100 倍（C4 = 6000、A4 = 440 Hz = 6900）。 [`crate::tone`] と同じ
//! 目盛りなので、収録音階の縦線も音名の目盛りも同じ軸に乗る。
//!
//! # 窓は重ねる枠と分け合う
//!
//! 枠を重ねると上のグラフの範囲が広がり、重ねた枠もこの枠も、広がった窓で描き直す。
//! なので [`PitchDistribution`] は数えたフレームを持ったまま返し、山は窓を受け取ってから
//! 作る（[`PitchDistribution::histogram`]）。 窓を決めてから数えると、重ねるたびに
//! 入力からやり直すことになる。

use crate::oto::Oto;
use crate::tone;

/// 山の1ビンの幅（セント）。 縦軸は「このビンで鳴っていた時間の割合」。
///
/// ビンの端は常にこの倍数に置く。 窓ごとに端をずらすと、重ねた枠どうしで
/// 同じ音高が別のビンに入り、山の高さが比べられなくなる。
pub const BIN_CENTS: f64 = 10.0;

/// ならしの幅（セント）。 1フレームを、この標準偏差の正規分布に広げてからビンに配る。
///
/// 細かすぎると、行の少ない枠の山がギザギザになる（P4f の「なめらかさ」）。
pub const SMOOTHING_CENTS: f64 = 4.5;

/// 窓の左右に足す余白（1半音）。
pub const WINDOW_MARGIN_CENTS: f64 = 100.0;

/// 窓の下端に使うパーセンタイル。
///
/// 上端（[`WINDOW_HIGH`]）と揃えない。 区間で切っても残る息や雑音は低い側に
/// 寄るので（探索の下限 55 Hz のすぐ上に固まる）、下を深く取ると窓がそれに引かれて
/// 歌った山が細くなる。 はみ出した分は割合で端に出す。
const WINDOW_LOW: f64 = 0.10;

/// 窓の上端に使うパーセンタイル。
const WINDOW_HIGH: f64 = 0.99;

/// ならしを打ち切る幅（標準偏差の倍数）。 この外の質量は 1e-9 に届かない。
const SMOOTHING_REACH: f64 = 6.0;

/// 1行ぶんの観測。 採用テイクの F0 と、その行から切り出す原音設定。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LinePitch<'a> {
    /// フレームごとの F0（Hz）。 無声は 0。
    pub f0_hz: &'a [f64],
    /// F0 1フレームの長さ（ミリ秒）。 フレーム `i` は時刻 `i × frame_ms` を表す。
    pub frame_ms: f64,
    /// 録音の長さ（ミリ秒）。 右ブランクが正（末尾から）のときに区間の終わりを決める。
    pub file_ms: f64,
    /// この行の原音設定。 連続音は1行に何本も持ち、区間が重なる。
    pub otos: &'a [Oto],
}

/// 音高の幅（セント）。 両端を含む。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CentRange {
    /// 下端。
    pub low: f64,
    /// 上端。
    pub high: f64,
}

impl CentRange {
    /// 両方を覆う幅。 重ねた枠の窓を合わせるのに使う。
    #[must_use]
    pub fn union(self, other: Self) -> Self {
        Self {
            low: self.low.min(other.low),
            high: self.high.max(other.high),
        }
    }
}

/// いちばん近い音名と、そこからのずれ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PitchName {
    /// MIDI ノート番号。
    pub midi: i32,
    /// 音名からのずれ（セント）。 −50〜+49。
    pub cents: i32,
}

impl PitchName {
    /// セントを丸めて、いちばん近い音名に割り振る。
    ///
    /// ちょうど半音の中間（+50）は上の音名の −50 にする。 「A3 +50」と
    /// 「A#3 −50」の両方を出すと、同じ音高が2つの札を持つ。
    #[must_use]
    pub fn nearest(cents: f64) -> Self {
        #[allow(
            clippy::cast_possible_truncation,
            reason = "可聴域の音高は ±10^6 セントに収まる。外れても飽和するだけ"
        )]
        let total = cents.round() as i32;
        let midi = (total + 50).div_euclid(100);
        Self {
            midi,
            cents: total - midi * 100,
        }
    }

    /// 音名（`A3`、`F#4`）。 表記は [`tone::name`] が持つ。
    #[must_use]
    pub fn name(&self) -> String {
        tone::name(self.midi)
    }
}

/// Hz を MIDI セントへ写す。 A4 = 440 Hz = 6900。
#[must_use]
pub fn cents_of_hz(hz: f64) -> f64 {
    6900.0 + 1200.0 * (hz / 440.0).log2()
}

/// 数えた1フレーム。
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame {
    cents: f64,
    ms: f64,
}

/// 1つの枠（サブバンク）で数えた音高。
///
/// 数えたフレームを音高の昇順で持つ。 窓が決まってから山を作るため
/// （このモジュールの冒頭）。
#[derive(Debug, Clone, PartialEq)]
pub struct PitchDistribution {
    frames: Vec<Frame>,
    total_ms: f64,
    line_medians: Vec<Option<f64>>,
}

/// 上のグラフの数と、下の一覧の1行。
#[derive(Debug, Clone, PartialEq)]
pub struct PitchSummary {
    /// 中央値（セント）。 札の音名は [`PitchName::nearest`] で作る。
    pub median: f64,
    /// 中央 50 %（25〜75 パーセンタイル）。 下の一覧の太い帯。
    pub central: CentRange,
    /// 90 %（5〜95 パーセンタイル）。 下の一覧の細い線。
    pub outer: CentRange,
    /// 上のグラフの窓。 10〜99 パーセンタイルに左右1半音を足し、[`BIN_CENTS`] の
    /// 倍数へ外向きに揃えたもの。 重ねるときは [`CentRange::union`] で広げる。
    pub window: CentRange,
    /// 行ごとの中央値（セント）。 並びは入力の行と同じで、数えるフレームが
    /// 1つも無い行は `None`。
    pub line_medians: Vec<Option<f64>>,
}

/// 窓の中の山。
#[derive(Debug, Clone, PartialEq)]
pub struct Histogram {
    /// 最初のビンの左端（セント）。 [`BIN_CENTS`] の倍数。
    ///
    /// ビン `i` は `start + i × BIN_CENTS` から `BIN_CENTS` の幅を持つ（右端を含まない）。
    pub start: f64,
    /// ビンごとの割合。 数えた発声時間に対する比で、0〜1。
    pub shares: Vec<f64>,
    /// 窓より下へはみ出した割合（端の「← 3 %」）。
    pub below: f64,
    /// 窓より上へはみ出した割合。
    pub above: f64,
}

impl PitchDistribution {
    /// 行ごとの観測から、使われる区間の有声フレームを数える。
    ///
    /// 1行の中で原音設定の区間が重なっても、同じフレームは1回しか数えない。
    /// 連続音は1行に区間の重なる原音設定を並べるので、重ねて数えると
    /// エイリアスの多い行ほど重くなる。
    ///
    /// フレーム長が正でない行は数えない。 長さの無いフレームは時間の割合に寄与しない。
    #[must_use]
    pub fn from_lines(lines: &[LinePitch<'_>]) -> Self {
        let mut frames = Vec::new();
        let mut line_medians = Vec::with_capacity(lines.len());
        for line in lines {
            let mut counted = counted_frames(line);
            counted.sort_by(|a, b| a.cents.total_cmp(&b.cents));
            let ms: f64 = counted.iter().map(|f| f.ms).sum();
            line_medians.push(quantile(&counted, ms, 0.5));
            frames.extend(counted);
        }
        frames.sort_by(|a, b| a.cents.total_cmp(&b.cents));
        let total_ms = frames.iter().map(|f| f.ms).sum();
        Self {
            frames,
            total_ms,
            line_medians,
        }
    }

    /// 数えたフレームが1つも無いか。 まだ録っていない枠と、区間の中が全部無声の枠。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// 中央値・幅・窓・行ごとの中央値。 数えたフレームが無ければ `None`。
    ///
    /// 空の枠に中央値を作らない。 0 セントや前の値を置くと、録っていない枠が
    /// 一覧に点を持つ。
    #[must_use]
    pub fn summary(&self) -> Option<PitchSummary> {
        let q = |p: f64| quantile(&self.frames, self.total_ms, p);
        let window = snapped(CentRange {
            low: q(WINDOW_LOW)? - WINDOW_MARGIN_CENTS,
            high: q(WINDOW_HIGH)? + WINDOW_MARGIN_CENTS,
        });
        Some(PitchSummary {
            median: q(0.5)?,
            central: CentRange {
                low: q(0.25)?,
                high: q(0.75)?,
            },
            outer: CentRange {
                low: q(0.05)?,
                high: q(0.95)?,
            },
            window,
            line_medians: self.line_medians.clone(),
        })
    }

    /// 窓の中の山を作る。 窓の端は [`BIN_CENTS`] の倍数へ外向きに揃える。
    ///
    /// ビンとはみ出しは、どちらもならした後の質量で数える。 なので
    /// `below + shares の和 + above` は 1 になる（空の枠ではすべて 0）。
    /// ならす前のフレームで数えると、窓の端のすぐ外にあるフレームが
    /// ビンにも「←」にも入り、合計が 100 % を超える。
    #[must_use]
    pub fn histogram(&self, window: CentRange) -> Histogram {
        let window = snapped(window);
        let width = (window.high - window.low) / BIN_CENTS;
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "端を揃えたので幅はビンの整数倍。 負と NaN は 0 になる"
        )]
        let bins = width.round().max(0.0) as usize;
        let mut hist = Histogram {
            start: window.low,
            shares: vec![0.0; bins],
            below: 0.0,
            above: 0.0,
        };
        if self.total_ms <= 0.0 {
            return hist;
        }

        #[allow(clippy::cast_precision_loss, reason = "ビンの数は窓の幅ぶん")]
        let end = window.low + bins as f64 * BIN_CENTS;
        let reach = SMOOTHING_REACH * SMOOTHING_CENTS;
        for f in &self.frames {
            let weight = f.ms / self.total_ms;
            let cdf = |edge: f64| normal_cdf((edge - f.cents) / SMOOTHING_CENTS);
            hist.below += weight * cdf(window.low);
            hist.above += weight * (1.0 - cdf(end));

            // 届く範囲のビンにだけ配る。 全部のビンを回すと、フレームの数 × ビンの数になる。
            let first = ((f.cents - reach - window.low) / BIN_CENTS).floor();
            let last = ((f.cents + reach - window.low) / BIN_CENTS).ceil();
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "0 で下を切り、ビンの数で上を切る"
            )]
            let (first, last) = (
                (first.max(0.0) as usize).min(bins),
                (last.max(0.0) as usize).min(bins),
            );
            for (i, share) in hist.shares.iter_mut().enumerate().take(last).skip(first) {
                #[allow(clippy::cast_precision_loss, reason = "ビンの数は窓の幅ぶん")]
                let left = window.low + i as f64 * BIN_CENTS;
                *share += weight * (cdf(left + BIN_CENTS) - cdf(left));
            }
        }
        hist
    }
}

/// 1行のうち、使われる区間に入る有声フレーム。
fn counted_frames(line: &LinePitch<'_>) -> Vec<Frame> {
    if !line.frame_ms.is_finite() || line.frame_ms <= 0.0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for (start, end) in used_regions(line) {
        let index = |ms: f64| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "0 で下を切り、フレームの数で上を切る"
            )]
            let i = (ms / line.frame_ms).ceil().max(0.0) as usize;
            i.min(line.f0_hz.len())
        };
        // 区間は左ブランクを含み、右ブランクを含まない。 隣り合う区間で同じフレームを2度数えない。
        for &hz in &line.f0_hz[index(start)..index(end)] {
            if hz.is_finite() && hz > 0.0 {
                out.push(Frame {
                    cents: cents_of_hz(hz),
                    ms: line.frame_ms,
                });
            }
        }
    }
    out
}

/// 原音設定ごとの左ブランク〜右ブランクを、重なりを畳んで昇順に並べる（ミリ秒）。
fn used_regions(line: &LinePitch<'_>) -> Vec<(f64, f64)> {
    let mut regions: Vec<(f64, f64)> = line
        .otos
        .iter()
        .map(|o| (o.offset_ms.max(0.0), o.cutoff_position_ms(line.file_ms)))
        .filter(|(start, end)| start.is_finite() && end.is_finite() && end > start)
        .collect();
    regions.sort_by(|a, b| a.0.total_cmp(&b.0));

    let mut merged: Vec<(f64, f64)> = Vec::with_capacity(regions.len());
    for (start, end) in regions {
        match merged.last_mut() {
            Some(last) if start <= last.1 => last.1 = last.1.max(end),
            _ => merged.push((start, end)),
        }
    }
    merged
}

/// 時間で重みを付けたパーセンタイル。 空なら `None`。
///
/// 補間しない。 累積が `p × 全体` に届いた最初のフレームの音高を返す
/// （経験分布の逆関数）。 5 ms のフレームが数百秒ぶん並ぶので補間の差は
/// 1フレームに満たず、補間しないほうが、返る音高が実際に鳴った音高になる。
fn quantile(sorted: &[Frame], total_ms: f64, p: f64) -> Option<f64> {
    let target = p * total_ms;
    let mut cum = 0.0;
    for f in sorted {
        cum += f.ms;
        if cum >= target {
            return Some(f.cents);
        }
    }
    sorted.last().map(|f| f.cents)
}

/// 窓の端を [`BIN_CENTS`] の倍数へ外向きに揃える。
fn snapped(window: CentRange) -> CentRange {
    CentRange {
        low: (window.low / BIN_CENTS).floor() * BIN_CENTS,
        high: (window.high / BIN_CENTS).ceil() * BIN_CENTS,
    }
}

/// 標準正規分布の累積分布関数。
///
/// 標準ライブラリに `erf` が無いので、Abramowitz–Stegun 7.1.26 で近似する
/// （誤差 1.5e-7）。 0 を挟んで対称に組み、Φ(−z) = 1 − Φ(z) を崩さない——
/// 崩れると、真ん中にいるフレームの山が左右で違う高さになる。
fn normal_cdf(z: f64) -> f64 {
    const P: f64 = 0.327_591_1;
    const A: [f64; 5] = [
        0.254_829_592,
        -0.284_496_736,
        1.421_413_741,
        -1.453_152_027,
        1.061_405_429,
    ];
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + P * x);
    let poly = A.iter().rev().fold(0.0, |acc, a| acc * t + a) * t;
    let tail = 0.5 * poly * (-x * x).exp();
    if z < 0.0 { tail } else { 1.0 - tail }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME_MS: f64 = 5.0;

    /// セントから Hz へ。 試験の入力を音高で書くためだけに使う。
    fn hz(cents: f64) -> f64 {
        440.0 * 2.0_f64.powf((cents - 6900.0) / 1200.0)
    }

    /// 左ブランク `start_ms` から右ブランク `end_ms` までを使う原音設定。
    fn oto(start_ms: f64, end_ms: f64) -> Oto {
        Oto {
            offset_ms: start_ms,
            consonant_ms: 0.0,
            cutoff_ms: -(end_ms - start_ms),
            preutterance_ms: 0.0,
            overlap_ms: 0.0,
        }
    }

    fn length_ms(f0: &[f64]) -> f64 {
        #[allow(clippy::cast_precision_loss, reason = "試験のフレームは数千")]
        let frames = f0.len() as f64;
        frames * FRAME_MS
    }

    fn line<'a>(f0: &'a [f64], otos: &'a [Oto]) -> LinePitch<'a> {
        LinePitch {
            f0_hz: f0,
            frame_ms: FRAME_MS,
            file_ms: length_ms(f0),
            otos,
        }
    }

    /// 全部のフレームを使う原音設定。
    fn whole(f0: &[f64]) -> [Oto; 1] {
        [oto(0.0, length_ms(f0))]
    }

    const fn range(low: f64, high: f64) -> CentRange {
        CentRange { low, high }
    }

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    fn total(h: &Histogram) -> f64 {
        h.below + h.shares.iter().sum::<f64>() + h.above
    }

    /// 音高 `cents` が入るビンの番号。
    fn bin_of(h: &Histogram, cents: f64) -> usize {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "試験では窓の中の音高しか引かない"
        )]
        let i = ((cents - h.start) / BIN_CENTS).floor() as usize;
        i
    }

    #[test]
    fn hz_を_midi_セントへ写す() {
        assert!(close(cents_of_hz(440.0), 6900.0, 1e-9));
        assert!(close(cents_of_hz(220.0), 5700.0, 1e-9), "A3");
        assert!(close(cents_of_hz(261.625_565), 6000.0, 1e-4), "C4");
    }

    /// 札は「A3 +9 ¢」の形。 半音の中間は上の音名の −50 に寄せる。
    #[test]
    fn 音名とずれ() {
        let a = PitchName::nearest(5709.4);
        assert_eq!((a.name().as_str(), a.cents), ("A3", 9));
        let g = PitchName::nearest(5645.0);
        assert_eq!((g.name().as_str(), g.cents), ("G#3", 45));
        let below = PitchName::nearest(5691.0);
        assert_eq!((below.name().as_str(), below.cents), ("A3", -9));
        let mid = PitchName::nearest(5750.0);
        assert_eq!((mid.name().as_str(), mid.cents), ("A#3", -50));
        let just = PitchName::nearest(5749.0);
        assert_eq!((just.name().as_str(), just.cents), ("A3", 49));
    }

    /// 一定の音高は、中央値も幅もその音高になり、窓は左右1半音になる。
    #[test]
    fn 一定の音高() {
        let f0 = vec![hz(5705.0); 400];
        let otos = whole(&f0);
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let s = d.summary().expect("数えたフレームがある");

        assert!(close(s.median, 5705.0, 1e-6), "{}", s.median);
        for r in [s.central, s.outer] {
            assert!(
                close(r.low, 5705.0, 1e-6) && close(r.high, 5705.0, 1e-6),
                "{r:?}"
            );
        }
        assert_eq!(s.window, range(5600.0, 5810.0));
        assert_eq!(s.line_medians.len(), 1);
        assert!(close(
            s.line_medians[0].expect("行に中央値がある"),
            5705.0,
            1e-6
        ));
        let name = PitchName::nearest(s.median);
        assert_eq!((name.name().as_str(), name.cents), ("A3", 5));

        let h = d.histogram(s.window);
        assert_eq!(h.start, 5600.0);
        assert_eq!(h.shares.len(), 21);
        assert!(close(total(&h), 1.0, 1e-9), "{}", total(&h));
        assert!(h.below < 1e-9 && h.above < 1e-9, "{} {}", h.below, h.above);
    }

    /// ±4.5 ¢ でならす。 ビンの真ん中にいる音高は、そのビンに 73 %、両隣に 13 % ずつ、
    /// 2つ隣に 0.04 % 配られ、その先には届かない。
    #[test]
    fn ならしの幅() {
        let f0 = vec![hz(5705.0); 400];
        let otos = whole(&f0);
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let h = d.histogram(range(5600.0, 5810.0));
        let i = bin_of(&h, 5705.0);

        // 期待値は erf から直接計算した値（Φ(5/4.5) − Φ(−5/4.5) など）。
        assert!(close(h.shares[i], 0.733_479, 1e-5), "{}", h.shares[i]);
        assert!(
            close(h.shares[i - 1], 0.132_831, 1e-5),
            "{}",
            h.shares[i - 1]
        );
        assert!(
            close(h.shares[i + 1], 0.132_831, 1e-5),
            "{}",
            h.shares[i + 1]
        );
        assert!(
            close(h.shares[i - 2], 0.000_429, 1e-5),
            "{}",
            h.shares[i - 2]
        );
        assert!(
            close(h.shares[i + 2], 0.000_429, 1e-5),
            "{}",
            h.shares[i + 2]
        );
        assert!(h.shares[i - 4] < 1e-9 && h.shares[i + 4] < 1e-9);
        assert!(
            close(h.shares[i - 1], h.shares[i + 1], 1e-12),
            "真ん中の音高は左右対称に広がる"
        );

        // ビンの境目にいる音高は、両側に半分ずつ（Φ(0) − Φ(−10/4.5)）。
        // 220 Hz はちょうど 5700 ¢ になる。 往復で丸めると境目のどちらへ落ちるかが揺れる。
        let f0 = vec![220.0; 400];
        let edge = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let h = edge.histogram(range(5600.0, 5810.0));
        assert!(close(h.shares[9], 0.486_866, 1e-4), "{}", h.shares[9]);
        assert!(close(h.shares[10], 0.486_866, 1e-4), "{}", h.shares[10]);
    }

    /// 左ブランクより前と右ブランクより後ろの雑音、区間の中の無声は数えない。
    #[test]
    fn 区間の外の雑音は数えない() {
        // 0〜200 ms は息（55 Hz 付近の雑音）、200〜1200 ms は歌、1200〜1500 ms はまた雑音。
        // 歌の中に無声のフレームを混ぜる。
        let mut f0 = vec![58.0; 40];
        f0.extend((0..200).map(|i| if i % 10 == 0 { 0.0 } else { hz(5705.0) }));
        f0.extend(vec![61.0; 60]);
        let otos = [oto(200.0, 1200.0)];
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let s = d.summary().expect("歌の区間がある");

        assert!(close(s.median, 5705.0, 1e-6));
        assert!(
            close(s.outer.low, 5705.0, 1e-6),
            "雑音が 5 % 点を引き下げない"
        );
        assert_eq!(s.window, range(5600.0, 5810.0));
        let h = d.histogram(s.window);
        assert!(h.below < 1e-9, "雑音が「←」に数えられた: {}", h.below);

        // 同じ録音で区間を全体にすると、雑音が数えられて左へはみ出す。
        let all = whole(&f0);
        let noisy = PitchDistribution::from_lines(&[line(&f0, &all)]);
        let h = noisy.histogram(s.window);
        assert!(close(h.below, 100.0 / 280.0, 1e-6), "{}", h.below);
    }

    /// はみ出した割合を左右それぞれ出す。 下は 10 %、上は 99 % より外なら窓に入れない。
    #[test]
    fn はみ出しの割合() {
        // 1000 フレームのうち、3 % は 12 半音下、0.5 % は 12 半音上。
        let mut f0 = vec![hz(4505.0); 30];
        f0.extend(vec![hz(5705.0); 965]);
        f0.extend(vec![hz(6905.0); 5]);
        let otos = whole(&f0);
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let s = d.summary().expect("数えたフレームがある");

        assert_eq!(s.window, range(5600.0, 5810.0));
        let h = d.histogram(s.window);
        assert!(close(h.below, 0.03, 1e-9), "{}", h.below);
        assert!(close(h.above, 0.005, 1e-9), "{}", h.above);
        assert!(close(total(&h), 1.0, 1e-9));
    }

    /// 25〜75 と 5〜95 のパーセンタイルは補間せず、実際に鳴った音高を返す。
    #[test]
    fn 中央_50_と_90_の幅() {
        let f0: Vec<f64> = (0..100).map(|i| hz(5600.0 + f64::from(i))).collect();
        let otos = whole(&f0);
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let s = d.summary().expect("数えたフレームがある");

        assert!(close(s.median, 5649.0, 1e-6), "{}", s.median);
        assert!(close(s.central.low, 5624.0, 1e-6), "{:?}", s.central);
        assert!(close(s.central.high, 5674.0, 1e-6), "{:?}", s.central);
        assert!(close(s.outer.low, 5604.0, 1e-6), "{:?}", s.outer);
        assert!(close(s.outer.high, 5694.0, 1e-6), "{:?}", s.outer);
        // 10 % 点 5609 − 100、99 % 点 5698 + 100 を、10 ¢ の倍数へ外向きに。
        assert_eq!(s.window, range(5500.0, 5800.0));
    }

    /// 行ごとの中央値は入力の行の順に並び、数えるものが無い行は `None`。
    #[test]
    fn 行ごとの中央値() {
        let a = vec![hz(5700.0); 100];
        let b = vec![0.0; 100];
        let c = vec![hz(5800.0); 100];
        let (oa, ob, oc) = (whole(&a), whole(&b), whole(&c));
        let d = PitchDistribution::from_lines(&[line(&a, &oa), line(&b, &ob), line(&c, &oc)]);
        let s = d.summary().expect("数えたフレームがある");

        assert_eq!(s.line_medians.len(), 3);
        assert!(close(s.line_medians[0].expect("1行目"), 5700.0, 1e-6));
        assert_eq!(s.line_medians[1], None, "全部無声の行");
        assert!(close(s.line_medians[2].expect("3行目"), 5800.0, 1e-6));
    }

    /// 連続音は区間の重なる原音設定を並べる。 重なったフレームを2度数えない。
    #[test]
    fn 重なる区間を二度数えない() {
        let mut f0 = vec![hz(5700.0); 100];
        f0.extend(vec![hz(5900.0); 100]);
        // 前半だけを3本の原音設定が覆い、後半は1本だけが覆う。
        let overlapping = [oto(0.0, 500.0), oto(100.0, 400.0), oto(200.0, 1000.0)];
        let d = PitchDistribution::from_lines(&[line(&f0, &overlapping)]);
        let h = d.histogram(range(5600.0, 6000.0));

        let low: f64 = h.shares[..20].iter().sum();
        let high: f64 = h.shares[20..].iter().sum();
        assert!(
            close(low, 0.5, 1e-6) && close(high, 0.5, 1e-6),
            "{low} {high}"
        );
    }

    /// 割合は時間で数える。 フレーム長が違う行を、フレームの数で比べない。
    #[test]
    fn フレームの数ではなく時間で数える() {
        // 5 ms × 100 = 500 ms と、10 ms × 60 = 600 ms。 数では前、時間では後ろが多い。
        let short = vec![hz(5700.0); 100];
        let long = vec![hz(5900.0); 60];
        let (os, ol) = (whole(&short), [oto(0.0, 600.0)]);
        let lines = [
            line(&short, &os),
            LinePitch {
                f0_hz: &long,
                frame_ms: 10.0,
                file_ms: 600.0,
                otos: &ol,
            },
        ];
        let d = PitchDistribution::from_lines(&lines);
        let s = d.summary().expect("数えたフレームがある");
        assert!(close(s.median, 5900.0, 1e-6), "{}", s.median);

        let h = d.histogram(range(5600.0, 6000.0));
        let high: f64 = h.shares[20..].iter().sum();
        assert!(close(high, 600.0 / 1100.0, 1e-6), "{high}");
    }

    /// 重ねた枠の窓で描き直しても、同じ音高は同じビンに同じ高さで入る。
    #[test]
    fn 重ねると窓が広がる() {
        let a = vec![hz(5705.0); 200];
        let b = vec![hz(6205.0); 200];
        let (oa, ob) = (whole(&a), whole(&b));
        let da = PitchDistribution::from_lines(&[line(&a, &oa)]);
        let db = PitchDistribution::from_lines(&[line(&b, &ob)]);
        let wa = da.summary().expect("A").window;
        let wb = db.summary().expect("B").window;
        let shared = wa.union(wb);
        assert_eq!(shared, range(5600.0, 6310.0));

        let alone = da.histogram(wa);
        let overlaid = da.histogram(shared);
        let i = bin_of(&alone, 5705.0);
        let j = bin_of(&overlaid, 5705.0);
        assert!(close(alone.shares[i], overlaid.shares[j], 1e-12));
        assert!(close(total(&overlaid), 1.0, 1e-9));
    }

    /// 窓の端が 10 ¢ の倍数でなくても、外向きに揃えてから数える。
    #[test]
    fn 窓の端はビンの倍数へ揃える() {
        let f0 = vec![hz(5705.0); 100];
        let otos = whole(&f0);
        let d = PitchDistribution::from_lines(&[line(&f0, &otos)]);
        let h = d.histogram(range(5603.0, 5797.0));
        assert_eq!(h.start, 5600.0);
        assert_eq!(h.shares.len(), 20);
    }

    /// 空の枠には中央値も窓も作らない。 山はすべて 0。
    #[test]
    fn 空の入力() {
        let empty = PitchDistribution::from_lines(&[]);
        assert!(empty.is_empty());
        assert_eq!(empty.summary(), None);
        let h = empty.histogram(range(5600.0, 5800.0));
        assert_eq!(h.shares, vec![0.0; 20]);
        assert_eq!((h.below, h.above), (0.0, 0.0));

        // 録ってはあるが数えるものが無い: 全部無声、原音設定が無い、区間が録音の外、
        // フレーム長が壊れている。
        let unvoiced = vec![0.0; 100];
        let sung = vec![hz(5705.0); 100];
        let ou = whole(&unvoiced);
        let outside = [oto(1000.0, 2000.0)];
        let os = whole(&sung);
        let lines = [
            line(&unvoiced, &ou),
            line(&sung, &[]),
            line(&sung, &outside),
            LinePitch {
                frame_ms: 0.0,
                ..line(&sung, &os)
            },
        ];
        let d = PitchDistribution::from_lines(&lines);
        assert!(d.is_empty());
        assert_eq!(d.summary(), None);
    }

    /// 近似した Φ が対称で、既知の値に合う。
    #[test]
    fn 正規分布の累積は対称() {
        assert!(close(normal_cdf(0.0), 0.5, 1e-9));
        assert!(close(normal_cdf(1.0), 0.841_344_746, 2e-7));
        assert!(close(normal_cdf(-1.96), 0.024_997_895, 2e-7));
        for z in [0.1, 0.7, 1.3, 2.9, 5.0] {
            assert!(close(normal_cdf(-z), 1.0 - normal_cdf(z), 1e-15), "{z}");
        }
    }
}
