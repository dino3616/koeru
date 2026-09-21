//! `prefix.map` と subbank（`TR-SYN-13`, `TR-SYN-14`, `TR-SYN-16`, `TR-ALN-22`）。
//!
//! 音源ディレクトリ直下の `prefix.map` を読み、prefix と suffix の組ごとに
//! 音域レンジを持つ区画へ展開する。エイリアスの検索は素のエイリアスではなく
//! 「prefix ＋ エイリアス ＋ suffix」で行う（`TR-SYN-13`）。
//!
//! # 書き出しと読み込みで同じ形を使う
//!
//! 書き出しは [`crate::tone::prefix_map_body`]。 ここが読む側で、
//! 往復できることを試験が見ている。KOERU が作った音源を KOERU が読めないと、
//! 下位方式への書き出し（`TR-PKG-24`）で自分の出力を解釈できなくなる。
//!
//! # `prefix.map` が無い音源
//!
//! 全音域を覆う単一 subbank として扱う（`TR-SYN-13`）。単音階の音源がそれで、
//! prefix も suffix も空になる。

use std::collections::BTreeMap;

use crate::tone;

/// subbank が覆う音域の下限（`TR-SYN-13`）。
pub const RANGE_LOW: i32 = 24;
/// 上限。`prefix.map` の B7 より1つ上まで見る（`TR-SYN-13` の「MIDI 24〜108」）。
pub const RANGE_HIGH: i32 = 108;

/// 音域レンジを持つ区画（`TR-SYN-13`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subbank {
    /// エイリアスの前に付ける。
    pub prefix: String,
    /// エイリアスの後ろに付ける。
    pub suffix: String,
    /// この区画が担う MIDI ノート番号。昇順。
    pub tones: Vec<i32>,
}

impl Subbank {
    /// 検索に使う形（`TR-SYN-13` の「prefix + エイリアス + suffix」）。
    ///
    /// **素のエイリアスで探さない。** 多音階音源では、素の綴りは
    /// どの区画にも無い。
    #[must_use]
    pub fn decorate(&self, alias: &str) -> String {
        format!("{}{alias}{}", self.prefix, self.suffix)
    }

    /// 音域の下限。
    #[must_use]
    pub fn low(&self) -> Option<i32> {
        self.tones.first().copied()
    }

    /// 音域の上限。
    #[must_use]
    pub fn high(&self) -> Option<i32> {
        self.tones.last().copied()
    }

    /// 全音域を覆っているか。`prefix.map` を持たない音源がこれ。
    #[must_use]
    pub fn covers_everything(&self) -> bool {
        self.low() == Some(RANGE_LOW) && self.high() == Some(RANGE_HIGH)
    }
}

/// `prefix.map` を読んで区画へ展開する（`TR-SYN-13`）。
///
/// 1行は `音階名 \t prefix \t suffix`。列が足りない行と、読めない音階名の行は飛ばす
/// ——**行が1つ壊れているだけで音源全体を読めなくしない。**
///
/// 同じ prefix / suffix の組は1つの区画にまとまる。 `prefix.map` は
/// 半音ごとに1行あるので、畳まないと 84 個の区画ができる。
#[must_use]
pub fn parse_prefix_map(text: &str) -> Vec<Subbank> {
    let mut by_affix: BTreeMap<(String, String), Vec<i32>> = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim_end_matches(['\r', '\n']);
        if line.trim().is_empty() {
            continue;
        }
        let mut cols = line.split('\t');
        let (Some(name), prefix, suffix) = (cols.next(), cols.next(), cols.next()) else {
            continue;
        };
        let Some(midi) = tone::parse(name) else {
            continue;
        };
        by_affix
            .entry((
                prefix.unwrap_or_default().to_owned(),
                suffix.unwrap_or_default().to_owned(),
            ))
            .or_default()
            .push(midi);
    }
    let mut out: Vec<Subbank> = by_affix
        .into_iter()
        .map(|((prefix, suffix), mut tones)| {
            tones.sort_unstable();
            tones.dedup();
            Subbank {
                prefix,
                suffix,
                tones,
            }
        })
        .collect();
    // 低い区画から並べる。読み手が音域順に見られる。
    out.sort_by_key(|s| s.low().unwrap_or(i32::MAX));
    out
}

/// `prefix.map` を持たない音源の区画（`TR-SYN-13`）。
#[must_use]
pub fn whole_range() -> Subbank {
    Subbank {
        prefix: String::new(),
        suffix: String::new(),
        tones: (RANGE_LOW..=RANGE_HIGH).collect(),
    }
}

/// 音源ディレクトリの `prefix.map` から区画を作る（`TR-SYN-13`）。
///
/// 無ければ全音域の単一区画。 空でも同じ——1行も読めない `prefix.map` は
/// 「無い」と同じ扱いにする。
#[must_use]
pub fn from_prefix_map(text: Option<&str>) -> Vec<Subbank> {
    let parsed = text.map(parse_prefix_map).unwrap_or_default();
    if parsed.is_empty() {
        vec![whole_range()]
    } else {
        parsed
    }
}

/// その音を鳴らす区画（`TR-SYN-16`）。
///
/// 音域に入っている区画を返す。 どこにも入らなければ、いちばん近い区画へ寄せる
/// ——`prefix.map` が全音域を覆っていない音源がある。
#[must_use]
pub fn for_tone(banks: &[Subbank], midi: i32) -> Option<usize> {
    if banks.is_empty() {
        return None;
    }
    banks
        .iter()
        .position(|b| b.tones.binary_search(&midi).is_ok())
        .or_else(|| {
            banks
                .iter()
                .enumerate()
                .min_by_key(|(_, b)| {
                    let lo = b.low().unwrap_or(i32::MAX);
                    let hi = b.high().unwrap_or(i32::MIN);
                    (midi - lo).abs().min((midi - hi).abs())
                })
                .map(|(i, _)| i)
        })
}

/// 区画の代表音高（`TR-SYN-14` (1)）。
///
/// 見る順は2つ。
///
/// 1. 接辞が音階名なら、それが収録音高。 `prefix.map` の suffix に音階名を書くのが
///    多音階音源の慣習で、KOERU の書き出しもそうしている（`TR-RCL-06`）
/// 2. そうでなければ音域の下限。 floor 割り当て（`TR-RCL-06`）では
///    「その音以下で最も高い収録音高」が担うので、収録音高は下限にある
///
/// **最低区画は音域からは決まらない。** 自分より下の全音域も担うので、
/// 収録音高は下限でも上限でもなく、範囲の途中にある。接辞が読めなければ
/// `None` を返して `TR-SYN-14` (2) の周波数表へ落とす。
///
/// 全音域を覆う区画にも答えない。 単一区画の音源は音域から決めようがない。
#[must_use]
pub fn representative_tone(banks: &[Subbank], index: usize) -> Option<i32> {
    let b = banks.get(index)?;
    if let Some(t) = tone::parse(&b.suffix).or_else(|| tone::parse(&b.prefix)) {
        return Some(t);
    }
    if b.covers_everything() {
        return None;
    }
    // 最低区画は下限が収録音高ではない。 接辞が読めなかった以上、ここでは決まらない。
    if b.low() == Some(RANGE_LOW) && banks.len() > 1 {
        return None;
    }
    b.low()
}

/// 平均 F0 から収録音高を推定する（`TR-SYN-14` (2)）。
///
/// A4 = 440Hz = MIDI 69。 無声フレーム（0 Hz）は平均に入れない。
#[must_use]
pub fn tone_from_f0(mean_f0_hz: f64) -> Option<i32> {
    if !mean_f0_hz.is_finite() || mean_f0_hz <= 0.0 {
        return None;
    }
    let midi = 69.0 + 12.0 * (mean_f0_hz / 440.0).log2();
    if !(f64::from(RANGE_LOW)..=f64::from(RANGE_HIGH)).contains(&midi.round()) {
        return None;
    }
    #[allow(
        clippy::cast_possible_truncation,
        reason = "直前に MIDI の範囲へ収まることを確かめている"
    )]
    Some(midi.round() as i32)
}

/// 収録音高を決める（`TR-SYN-14`）。
///
/// 区画の音域が先、周波数表の平均 F0 が次。 ユーザーに入力を求めない。
#[must_use]
pub fn decide_tone(banks: &[Subbank], index: usize, mean_f0_hz: Option<f64>) -> Option<i32> {
    representative_tone(banks, index).or_else(|| mean_f0_hz.and_then(tone_from_f0))
}

/// フレーズ内で区画が切り替わった位置（`TR-SYN-16`）。
///
/// 原音設定側の確認対象として参照できるようにするために記録する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switch {
    /// 切り替わった音符の添字。ここから新しい区画になる。
    pub note_index: usize,
    /// 前の区画。
    pub from: usize,
    /// 次の区画。
    pub to: usize,
}

/// 音符ごとに区画を引き当てる（`TR-SYN-16`）。
///
/// **切り替えは音符境界でのみ。** 1音符の途中では切り替えない——
/// 途中で素材が変わると、1つの音の中で声が別人になる。
///
/// 返すのは音符と同じ長さの区画の添字と、切り替わった位置。
#[must_use]
pub fn assign(banks: &[Subbank], notes: &[i32]) -> (Vec<usize>, Vec<Switch>) {
    let mut chosen = Vec::with_capacity(notes.len());
    let mut switches = Vec::new();
    let mut prev: Option<usize> = None;
    for (i, midi) in notes.iter().enumerate() {
        let Some(b) = for_tone(banks, *midi) else {
            continue;
        };
        if let Some(p) = prev
            && p != b
        {
            switches.push(Switch {
                note_index: i,
                from: p,
                to: b,
            });
        }
        prev = Some(b);
        chosen.push(b);
    }
    (chosen, switches)
}

/// エイリアスへ音階サフィックスを一括で付ける（`TR-ALN-22`）。
///
/// > エイリアスの音階サフィックス付与を一括操作として提供する
///
/// 並びは入力のまま。 台帳へ書き戻す側が順序で突き合わせる。
#[must_use]
pub fn decorate_all(bank: &Subbank, aliases: &[String]) -> Vec<String> {
    aliases.iter().map(|a| bank.decorate(a)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tone::{DEFAULT_TONES_FEMALE, default_affix, prefix_map_body};

    fn written() -> String {
        prefix_map_body(&DEFAULT_TONES_FEMALE, default_affix, "\n")
    }

    /// 書き出した `prefix.map` を読み戻せる（`TR-SYN-13`, `TR-RCL-06`）。
    ///
    /// 自分の出力を自分で解釈できないと、下位方式への書き出しが成り立たない。
    #[test]
    fn 書き出した_prefix_map_を読み戻せる() {
        let banks = parse_prefix_map(&written());
        assert_eq!(banks.len(), 3, "収録音高の数だけ区画ができる");
        assert_eq!(banks[0].suffix, "G3");
        assert_eq!(banks[1].suffix, "D4");
        assert_eq!(banks[2].suffix, "A4");
        // 半音ごとの行が畳まれている。
        assert_eq!(
            banks.iter().map(|b| b.tones.len()).sum::<usize>(),
            84,
            "C1 から B7 まで"
        );
    }

    /// 検索は prefix ＋ エイリアス ＋ suffix（`TR-SYN-13`）。
    #[test]
    fn 検索の形に綴りを畳む() {
        let b = Subbank {
            prefix: "↑".to_owned(),
            suffix: "C4".to_owned(),
            tones: vec![60],
        };
        assert_eq!(b.decorate("か"), "↑かC4");
        assert_eq!(
            decorate_all(&b, &["か".to_owned(), "き".to_owned()]),
            ["↑かC4", "↑きC4"]
        );
    }

    /// `prefix.map` が無ければ全音域の単一区画（`TR-SYN-13`）。
    #[test]
    fn prefix_map_が無ければ単一区画() {
        let banks = from_prefix_map(None);
        assert_eq!(banks.len(), 1);
        assert!(banks[0].covers_everything());
        assert_eq!(banks[0].decorate("か"), "か", "素のエイリアスのまま");
        // 読めない中身も同じ扱い。
        assert_eq!(from_prefix_map(Some("なにこれ\n")), banks);
    }

    /// 壊れた行で音源全体を読めなくしない。
    #[test]
    fn 壊れた行は飛ばす() {
        let banks = parse_prefix_map("C4\t\tC4\nなにこれ\nG4\t\tG4\n");
        assert_eq!(banks.len(), 2);
    }

    /// 音符ごとに区画を引き当てる（`TR-SYN-16`）。
    #[test]
    fn 音符ごとに区画を引き当てる() {
        let banks = parse_prefix_map(&written());
        // G3(55) / D4(62) / A4(69) の担当へ落ちる。
        assert_eq!(for_tone(&banks, 55), Some(0));
        assert_eq!(for_tone(&banks, 61), Some(0));
        assert_eq!(for_tone(&banks, 62), Some(1));
        assert_eq!(for_tone(&banks, 69), Some(2));
        assert_eq!(for_tone(&[], 60), None);
    }

    /// 切り替わった位置を記録する（`TR-SYN-16`）。
    #[test]
    fn 切り替わった位置を記録する() {
        let banks = parse_prefix_map(&written());
        let (chosen, switches) = assign(&banks, &[55, 56, 62, 63, 69]);
        assert_eq!(chosen, [0, 0, 1, 1, 2]);
        assert_eq!(switches.len(), 2);
        assert_eq!(switches[0].note_index, 2);
        assert_eq!((switches[0].from, switches[0].to), (0, 1));
        assert_eq!(switches[1].note_index, 4);
    }

    /// 同じ区画の中では切り替わらない。
    #[test]
    fn 同じ区画なら切り替えない() {
        let banks = parse_prefix_map(&written());
        let (_, switches) = assign(&banks, &[55, 56, 57, 58]);
        assert!(switches.is_empty());
    }

    /// 代表音高は floor 割り当ての収録音高（`TR-SYN-14` (1)）。
    #[test]
    fn 代表音高は収録音高に当たる() {
        let banks = parse_prefix_map(&written());
        // 接辞が音階名なので、そこから読める。
        assert_eq!(representative_tone(&banks, 0), Some(55));
        assert_eq!(representative_tone(&banks, 1), Some(62));
        assert_eq!(representative_tone(&banks, 2), Some(69));
    }

    /// 接辞が音階名でない最低区画は、音域からは決まらない（`TR-SYN-14`）。
    ///
    /// 自分より下の全音域も担うので、収録音高は範囲の途中にある。
    #[test]
    fn 接辞が読めない最低区画は周波数表へ落ちる() {
        let banks = parse_prefix_map("C1\t\tlow\nC4\t\tlow\nC5\t\thigh\n");
        assert_eq!(
            representative_tone(&banks, 0),
            None,
            "下限は収録音高ではない"
        );
        assert_eq!(representative_tone(&banks, 1), Some(72), "C5");
        assert_eq!(decide_tone(&banks, 0, Some(220.0)), Some(57));
    }

    /// 全音域の単一区画には答えない。周波数表へ落ちる（`TR-SYN-14` (2)）。
    #[test]
    fn 単一区画は周波数表で決める() {
        let banks = from_prefix_map(None);
        assert_eq!(representative_tone(&banks, 0), None);
        assert_eq!(decide_tone(&banks, 0, Some(440.0)), Some(69), "A4");
        assert_eq!(decide_tone(&banks, 0, None), None);
    }

    /// 平均 F0 から音高を出す（`TR-SYN-14` (2)）。
    #[test]
    fn 平均_f0_から音高を出す() {
        assert_eq!(tone_from_f0(440.0), Some(69));
        assert_eq!(tone_from_f0(220.0), Some(57), "A3");
        // 無声フレームや壊れた値は答えない。
        assert_eq!(tone_from_f0(0.0), None);
        assert_eq!(tone_from_f0(f64::NAN), None);
        assert_eq!(tone_from_f0(1.0), None, "音域の外");
    }
}
