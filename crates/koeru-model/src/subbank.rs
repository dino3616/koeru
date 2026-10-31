//! サブバンクの枠（`DEC-RCL-018`）の規則。 声色の比べ方と担当音域。
//!
//! 枠そのものは台帳が持つ（`koeru_core::db`）。 ここに置くのは、枠の並びから
//! 決まることだけ。 合成側の区画（`prefix.map` を読んだもの）とは別のもの。

use crate::tone::{PREFIX_MAP_HIGH, PREFIX_MAP_LOW};

/// 声色（`TR-RCL-28`）。 本人が打つ名前で、綴りが同じなら同じ声色。
///
/// 空は「声色なし」なので、この型では持たない（[`VoiceColor::parse`] が `None` を返す）。
/// 綴りを揃えない——前後の空白も、本人が打ったとおりに比べる。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct VoiceColor(String);

impl VoiceColor {
    /// 打った名前から。 空なら声色なし。
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        (!name.is_empty()).then(|| Self(name.to_owned()))
    }

    /// 台帳へ渡す綴り。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// 担当音域（`TR-RCL-31`）。 両端を含む MIDI ノート番号。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToneRange {
    pub low: i32,
    pub high: i32,
}

/// 担当音域として受け取れない範囲。
///
/// 重なりと隙間はここで弾かない。 どの枠が歌うかの規則がまだ無い（`Q-RCL-005`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ToneRangeError {
    /// `prefix.map` が覆う範囲（C1〜B7）の外。
    #[error("担当音域が C1〜B7 の外にある")]
    OutOfRange,
    /// 下端が上端より高い。
    #[error("担当音域の下端が上端より高い")]
    Inverted,
}

impl koeru_failure::Failure for ToneRangeError {
    fn code(&self) -> &'static str {
        match self {
            Self::OutOfRange => "subbank.tone_range_out_of_range",
            Self::Inverted => "subbank.tone_range_inverted",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

impl ToneRange {
    /// 本人が直した範囲として受け取れるか見る。
    ///
    /// # Errors
    ///
    /// C1〜B7 の外、下端が上端より高い。
    pub fn edited(low: i32, high: i32) -> Result<Self, ToneRangeError> {
        let span = PREFIX_MAP_LOW..=PREFIX_MAP_HIGH;
        if !span.contains(&low) || !span.contains(&high) {
            return Err(ToneRangeError::OutOfRange);
        }
        if low > high {
            return Err(ToneRangeError::Inverted);
        }
        Ok(Self { low, high })
    }
}

/// 枠の収録音階として受け取れるか（`TR-RCL-28` の「範囲は TR-RCL-06 の C1〜B7」）。
#[must_use]
pub fn is_recordable_tone(midi: i32) -> bool {
    (PREFIX_MAP_LOW..=PREFIX_MAP_HIGH).contains(&midi)
}

/// 既定の担当音域（`TR-RCL-31`）。 `subbanks` は `(声色, 収録音階)` を枠の並びで。
///
/// 同じ声色の枠のあいだで floor 割り当て（`TR-RCL-06`）を当てる。 下端はその音階、
/// 上端は同じ声色の次に高い音階の1つ下。 いちばん低い枠は C1 まで、いちばん高い枠は
/// B7 まで受け持つ。 同じ声色に同じ音階の枠が2つあれば（方式違い）、同じ範囲になる。
///
/// 返す並びは渡した並びと同じ。
#[must_use]
pub fn default_ranges(subbanks: &[(Option<&VoiceColor>, i32)]) -> Vec<ToneRange> {
    subbanks
        .iter()
        .map(|(color, tone)| {
            let mut same: Vec<i32> = subbanks
                .iter()
                .filter(|(c, _)| c == color)
                .map(|(_, t)| *t)
                .collect();
            same.sort_unstable();
            same.dedup();
            let lowest = same.first().is_some_and(|t| t == tone);
            let low = if lowest { PREFIX_MAP_LOW } else { *tone };
            let high = same
                .iter()
                .find(|t| *t > tone)
                .map_or(PREFIX_MAP_HIGH, |next| next - 1);
            // 範囲の外の音階（作れないが、古い台帳には入りうる）でも、範囲は C1〜B7 に収める。
            let low = low.clamp(PREFIX_MAP_LOW, PREFIX_MAP_HIGH);
            ToneRange {
                low,
                high: high.clamp(low, PREFIX_MAP_HIGH),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_failure::Failure;

    fn color(s: &str) -> VoiceColor {
        VoiceColor::parse(s).expect("空でない")
    }

    #[test]
    fn 空の声色は声色なし() {
        assert_eq!(VoiceColor::parse(""), None);
        assert_eq!(VoiceColor::parse(" "), Some(VoiceColor(" ".to_owned())));
    }

    /// 同じ声色の中でだけ割り当てる（`TR-RCL-31`）。 声色が違えば、同じ音階でも互いに動かさない。
    #[test]
    fn 既定の担当音域は同じ声色の枠のあいだの_floor_割り当て() {
        let soft = color("soft");
        let ranges = default_ranges(&[
            (None, 55),
            (None, 62),
            (Some(&soft), 60),
            (None, 69),
            (Some(&soft), 55),
        ]);
        assert_eq!(
            ranges,
            [
                ToneRange { low: 24, high: 61 },
                ToneRange { low: 62, high: 68 },
                ToneRange { low: 60, high: 107 },
                ToneRange { low: 69, high: 107 },
                ToneRange { low: 24, high: 59 },
            ]
        );
    }

    #[test]
    fn 同じ声色で同じ音階の枠は同じ範囲を持つ() {
        let ranges = default_ranges(&[(None, 60), (None, 60)]);
        assert_eq!(ranges[0], ranges[1]);
        assert_eq!(ranges[0], ToneRange { low: 24, high: 107 });
    }

    #[test]
    fn 直した範囲は_c1_から_b7_の中で下端が上端を超えない() {
        assert_eq!(
            ToneRange::edited(24, 107),
            Ok(ToneRange { low: 24, high: 107 })
        );
        assert_eq!(
            ToneRange::edited(60, 60),
            Ok(ToneRange { low: 60, high: 60 })
        );
        assert_eq!(
            ToneRange::edited(23, 60).expect_err("C1 より下").code(),
            "subbank.tone_range_out_of_range"
        );
        assert_eq!(
            ToneRange::edited(60, 108).expect_err("B7 より上").code(),
            "subbank.tone_range_out_of_range"
        );
        assert_eq!(
            ToneRange::edited(61, 60).expect_err("逆さ").code(),
            "subbank.tone_range_inverted"
        );
    }
}
