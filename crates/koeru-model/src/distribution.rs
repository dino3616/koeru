//! 配布物（`DEC-PKG-018`）の規則。 版の番号の出し方、写したときの名前、入れた枠が受け持たない高さ。
//!
//! 配布物そのものは台帳が持つ（`koeru_core::db`）。 ここに置くのは、並びと文字列から
//! 決まることだけ。

use crate::names::MAX_NAME_BYTES;
use crate::subbank::{ToneRange, VoiceColor};
use crate::tone::{PREFIX_MAP_HIGH, PREFIX_MAP_LOW};

/// 書き出していない配布物の、最初の下書きの番号（`TR-PKG-60`）。
pub const FIRST_VERSION: &str = "1.0.0";

/// 前の版の末尾の数を1つ上げる（`TR-PKG-60`）。 1.0.0 の次は 1.0.1。
///
/// 末尾が数でなければ `.1` を足す（`1.0.0-beta` の次は `1.0.0-beta.1`）。 数の桁は揃える
/// （`v009` の次は `v010`）。 書き出していなければ [`FIRST_VERSION`]。
///
/// `taken` に同じ番号があれば、あいた番号まで上げる。 下書きは、同じ配布物で書き出した番号と
/// 重ねない。
#[must_use]
pub fn next_version(last: Option<&str>, taken: &[&str]) -> String {
    let Some(last) = last else {
        return avoid(FIRST_VERSION.to_owned(), taken);
    };
    avoid(bump(last), taken)
}

fn avoid(mut candidate: String, taken: &[&str]) -> String {
    while taken.contains(&candidate.as_str()) {
        candidate = bump(&candidate);
    }
    candidate
}

fn bump(version: &str) -> String {
    let digits = version.bytes().rev().take_while(u8::is_ascii_digit).count();
    if digits == 0 {
        return format!("{version}.1");
    }
    let (head, tail) = version.split_at(version.len() - digits);
    // 桁があふれる（u128 に収まらない）数は、末尾へ 1 を足した桁として扱う。
    match tail.parse::<u128>() {
        Ok(n) => format!("{head}{:0width$}", n + 1, width = digits),
        Err(_) => format!("{version}.1"),
    }
}

/// 配布物を写して足すときの名前（`TR-PKG-58`）。 「〇〇 のコピー」。 もうあれば番号を付ける。
#[must_use]
pub fn copy_name(source: &str, taken: &[&str]) -> String {
    let base = format!("{source} のコピー");
    if !taken.contains(&base.as_str()) {
        return base;
    }
    (2..)
        .map(|n| format!("{base} {n}"))
        .find(|candidate| !taken.contains(&candidate.as_str()))
        .unwrap_or(base)
}

/// 配布物を写して足すときの音源ルートフォルダ名（`TR-PKG-58`）。 元の名前に番号を付ける。
///
/// 長さの上限（[`MAX_NAME_BYTES`]）に収める。 元の名前を後ろから削り、番号は残す。
/// 元の名前が ASCII でも、そうでなくても同じ——文字の途中では切らない。
#[must_use]
pub fn copy_archive_name(source: &str, taken: &[&str]) -> String {
    for n in 2_u32.. {
        let suffix = format!("-{n}");
        let mut head = source.to_owned();
        while head.len() + suffix.len() > MAX_NAME_BYTES {
            head.pop();
        }
        let candidate = format!("{head}{suffix}");
        if !taken.contains(&candidate.as_str()) {
            return candidate;
        }
    }
    unreachable!("番号は尽きない")
}

/// 入れた枠のどれも受け持たない高さ（`TR-PKG-59`）。
///
/// `members` は、入れた枠の `(声色, その配布物で書く担当音域)`。 声色ごとに、C-1〜G9 のうち
/// どの範囲にも入らない高さを返す。 声色の並びは、最初に現れた順。 隙間が無い声色は現れない。
///
/// 重なりは隙間にしない。 どの枠が歌うかの規則はまだ無い（`Q-RCL-005`）。
#[must_use]
pub fn uncovered(
    members: &[(Option<&VoiceColor>, ToneRange)],
) -> Vec<(Option<VoiceColor>, ToneRange)> {
    let mut colors: Vec<Option<&VoiceColor>> = Vec::new();
    for (color, _) in members {
        if !colors.contains(color) {
            colors.push(*color);
        }
    }
    let mut out = Vec::new();
    for color in colors {
        let mut ranges: Vec<ToneRange> = members
            .iter()
            .filter(|(c, _)| *c == color)
            .map(|(_, r)| *r)
            .collect();
        ranges.sort_unstable_by_key(|r| (r.low, r.high));
        let mut next = PREFIX_MAP_LOW;
        for r in &ranges {
            if r.low > next {
                out.push((
                    color.cloned(),
                    ToneRange {
                        low: next,
                        high: r.low - 1,
                    },
                ));
            }
            next = next.max(r.high + 1);
        }
        if next <= PREFIX_MAP_HIGH {
            out.push((
                color.cloned(),
                ToneRange {
                    low: next,
                    high: PREFIX_MAP_HIGH,
                },
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 書き出していなければ最初の番号() {
        assert_eq!(next_version(None, &[]), "1.0.0");
    }

    #[test]
    fn 末尾の数を1つ上げる() {
        assert_eq!(next_version(Some("1.0.0"), &[]), "1.0.1");
        assert_eq!(next_version(Some("v1.9"), &[]), "v1.10");
        assert_eq!(next_version(Some("v009"), &[]), "v010");
        assert_eq!(next_version(Some("1.0.0-beta"), &[]), "1.0.0-beta.1");
        assert_eq!(next_version(Some("初版"), &[]), "初版.1");
    }

    #[test]
    fn 書き出した番号とは重ねない() {
        assert_eq!(next_version(Some("1.0.0"), &["1.0.0", "1.0.1"]), "1.0.2");
        assert_eq!(next_version(None, &["1.0.0"]), "1.0.1");
    }

    #[test]
    fn 数があふれても落ちない() {
        let huge = "9".repeat(60);
        assert_eq!(next_version(Some(&huge), &[]), format!("{huge}.1"));
    }

    #[test]
    fn 写した名前はもうあれば番号を付ける() {
        assert_eq!(copy_name("すべて", &["すべて"]), "すべて のコピー");
        assert_eq!(
            copy_name("すべて", &["すべて", "すべて のコピー"]),
            "すべて のコピー 2"
        );
    }

    #[test]
    fn 写したフォルダ名は重ならず長さに収まる() {
        assert_eq!(copy_archive_name("Koeru", &["Koeru"]), "Koeru-2");
        assert_eq!(copy_archive_name("Koeru", &["Koeru-2"]), "Koeru-3");
        let long = "a".repeat(MAX_NAME_BYTES);
        let copy = copy_archive_name(&long, &[]);
        assert_eq!(copy.len(), MAX_NAME_BYTES);
        assert!(copy.ends_with("-2"));
    }

    fn range(low: i32, high: i32) -> ToneRange {
        ToneRange { low, high }
    }

    #[test]
    fn 外した枠が受け持っていた高さが空く() {
        // 55 / 62 / 69 の3枠の既定は 0..61 / 62..68 / 69..127。 真ん中を外す。
        let low = (None, range(0, 61));
        let high = (None, range(69, 127));
        let got = uncovered(&[(low.0, low.1), (high.0, high.1)]);
        assert_eq!(got, [(None, range(62, 68))]);
    }

    #[test]
    fn 端の枠を外すと端が空く() {
        assert_eq!(
            uncovered(&[(None, range(0, 61)), (None, range(62, 68))]),
            [(None, range(69, 127))]
        );
        assert_eq!(uncovered(&[(None, range(62, 127))]), [(None, range(0, 61))]);
    }

    #[test]
    fn 隙間が無ければ何も出さず_重なりは隙間にしない() {
        assert!(uncovered(&[(None, range(0, 127))]).is_empty());
        assert!(uncovered(&[(None, range(0, 80)), (None, range(60, 127))]).is_empty());
        assert!(uncovered(&[]).is_empty());
    }

    #[test]
    fn 隙間は声色ごとに見て_最初に現れた順に出す() {
        let soft = VoiceColor::parse("soft").expect("空でない");
        let got = uncovered(&[
            (Some(&soft), range(0, 59)),
            (None, range(0, 127)),
            (Some(&soft), range(70, 127)),
        ]);
        assert_eq!(got, [(Some(soft), range(60, 69))]);
    }
}
