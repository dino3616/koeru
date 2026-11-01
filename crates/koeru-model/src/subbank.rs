//! サブバンクの枠（`DEC-RCL-018`）の規則。 声色の比べ方と担当音域。
//!
//! 枠そのものは台帳が持つ（`koeru_core::db`）。 ここに置くのは、枠の並びから
//! 決まることだけ。 合成側の区画（`prefix.map` を読んだもの）とは別のもの。

use crate::alias::Method;
use crate::id::SubbankId;
use crate::tone::{self, PREFIX_MAP_HIGH, PREFIX_MAP_LOW};

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
    /// `prefix.map` が覆う範囲（C-1〜G9）の外。
    #[error("担当音域が C-1〜G9 の外にある")]
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
    /// C-1〜G9 の外、下端が上端より高い。
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

/// 枠の収録音階として受け取れるか（`TR-RCL-28` の「範囲は TR-RCL-06 の C-1〜G9」）。
#[must_use]
pub fn is_recordable_tone(midi: i32) -> bool {
    (PREFIX_MAP_LOW..=PREFIX_MAP_HIGH).contains(&midi)
}

/// 既定の担当音域（`TR-RCL-31`）。 `subbanks` は `(声色, 収録音階)` を枠の並びで。
///
/// 同じ声色の枠のあいだで floor 割り当て（`TR-RCL-06`）を当てる。 下端はその音階、
/// 上端は同じ声色の次に高い音階の1つ下。 いちばん低い枠は C-1 まで、いちばん高い枠は
/// G9 まで受け持つ。 同じ声色に同じ音階の枠が2つあれば（方式違い）、同じ範囲になる。
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
            // 範囲の外の音階（作れないが、台帳の値としては入りうる）でも、範囲は C-1〜G9 に収める。
            let low = low.clamp(PREFIX_MAP_LOW, PREFIX_MAP_HIGH);
            ToneRange {
                low,
                high: high.clamp(low, PREFIX_MAP_HIGH),
            }
        })
        .collect()
}

/// 素材を探すときに見る枠1つ（`DEC-RCL-018`）。 声色・方式・収録音階は、枠が持つ属性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cell {
    pub id: SubbankId,
    /// `None` は声色なし。
    pub voice_color: Option<VoiceColor>,
    pub method: Method,
    /// 収録音階（MIDI）。
    pub tone: i32,
}

/// 枠の並び（`TR-RCL-28`）。 並びは台帳の並び（録る順の既定）。
///
/// カバレッジ・歌えるか・試唱・書き出しが、同じ枠の選び方（[`Cells::chain`]）を通る
/// （`TR-RCL-20`）。 素材の鍵は枠で持つので、声色や方式の違う枠が同じ高さにあっても、
/// 収録済みの数は混ざらない。
///
/// 担当音域は、直していても読まない。 どの枠が歌うかは floor 割り当て（既定の担当音域と同じ）
/// で決める。 直した範囲が重なる・隙間があるときの規則が決まっていない（`Q-RCL-005`）ので、
/// 判定・試唱・書き出しが同じ規則を読めるようになるまで、どれも直した範囲を読まない。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Cells(Vec<Cell>);

impl Cells {
    /// 台帳の並びのまま。
    #[must_use]
    pub const fn new(cells: Vec<Cell>) -> Self {
        Self(cells)
    }

    /// 並びの順に。
    #[must_use]
    pub fn as_slice(&self) -> &[Cell] {
        &self.0
    }

    /// 枠が1つも無い。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 枠を識別子で引く。
    #[must_use]
    pub fn get(&self, id: SubbankId) -> Option<&Cell> {
        self.0.iter().find(|c| c.id == id)
    }

    /// 曲を歌わせる声色の既定。 並びの先頭の枠の声色（`None` は声色なし）。
    ///
    /// 曲の判定と試唱が声色を指定されないときに使う。 声色を選ぶ口が画面に無い間は、
    /// 本人が先頭に置いた枠の声色で歌う。
    #[must_use]
    pub fn default_color(&self) -> Option<&VoiceColor> {
        self.0.first().and_then(|c| c.voice_color.as_ref())
    }

    /// その声色が持つ収録音階（昇順、重なりなし）。
    #[must_use]
    pub fn tones(&self, color: Option<&VoiceColor>) -> Vec<i32> {
        let mut tones: Vec<i32> = self
            .0
            .iter()
            .filter(|c| c.voice_color.as_ref() == color)
            .map(|c| c.tone)
            .collect();
        tones.sort_unstable();
        tones.dedup();
        tones
    }

    /// その声色で、あるノートが素材を探す枠の順（`TR-RCL-20`, `DEC-SYN-014`）。
    ///
    /// floor の収録音階の枠から始めて、下の音階へ1段ずつ。 同じ音階に方式の違う枠が
    /// あれば、並びの順に1つずつ試す。 先頭が、そのノートを受け持つ枠。
    ///
    /// その声色の枠が無ければ空。
    #[must_use]
    pub fn chain(&self, color: Option<&VoiceColor>, midi: i32) -> Vec<&Cell> {
        let tones = self.tones(color);
        let Some(floor) = tone::floor_tone(&tones, midi) else {
            return Vec::new();
        };
        tones
            .iter()
            .rev()
            .filter(|t| **t <= floor)
            .flat_map(|t| {
                self.0
                    .iter()
                    .filter(move |c| c.voice_color.as_ref() == color && c.tone == *t)
            })
            .collect()
    }

    /// そのノートを受け持つ枠。 [`Cells::chain`] の先頭。
    #[must_use]
    pub fn owner(&self, color: Option<&VoiceColor>, midi: i32) -> Option<&Cell> {
        self.chain(color, midi).into_iter().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_failure::Failure;

    fn cell(id: i32, color: Option<&str>, method: Method, tone: i32) -> Cell {
        Cell {
            id: SubbankId::new(id),
            voice_color: color.and_then(VoiceColor::parse),
            method,
            tone,
        }
    }

    fn ids(cells: &[&Cell]) -> Vec<i32> {
        cells.iter().map(|c| c.id.get()).collect()
    }

    /// 声色の違う枠は、同じ高さでも互いの鎖に入らない。 同じ高さの枠は並びの順に試す。
    #[test]
    fn 鎖は声色の中で_floor_から下へ_同じ高さの枠は並びの順() {
        let cells = Cells::new(vec![
            cell(1, None, Method::Cvvc, 62),
            cell(2, Some("Growl"), Method::Single, 62),
            cell(3, None, Method::Single, 55),
            cell(4, None, Method::Sequential, 62),
            cell(5, Some("Growl"), Method::Single, 69),
        ]);
        assert_eq!(
            ids(&cells.chain(None, 70)),
            [1, 4, 3],
            "62 の2枠を並びの順に、次に 55"
        );
        assert_eq!(ids(&cells.chain(None, 61)), [3], "floor は 55");
        assert_eq!(
            ids(&cells.chain(None, 20)),
            [3],
            "最低音階より下は最低音階が受け持つ"
        );
        let growl = VoiceColor::parse("Growl");
        assert_eq!(ids(&cells.chain(growl.as_ref(), 70)), [5, 2]);
        assert_eq!(cells.owner(growl.as_ref(), 64).map(|c| c.id.get()), Some(2));
        assert!(
            cells
                .chain(VoiceColor::parse("Soft").as_ref(), 60)
                .is_empty()
        );
        assert_eq!(cells.default_color(), None, "先頭の枠は声色なし");
        assert_eq!(cells.tones(None), [55, 62]);
    }

    #[test]
    fn 先頭の枠の声色が既定() {
        let cells = Cells::new(vec![
            cell(1, Some("Growl"), Method::Single, 60),
            cell(2, None, Method::Cvvc, 60),
        ]);
        assert_eq!(cells.default_color().map(VoiceColor::as_str), Some("Growl"));
        assert!(Cells::default().default_color().is_none());
    }

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
                ToneRange { low: 0, high: 61 },
                ToneRange { low: 62, high: 68 },
                ToneRange { low: 60, high: 127 },
                ToneRange { low: 69, high: 127 },
                ToneRange { low: 0, high: 59 },
            ]
        );
    }

    #[test]
    fn 同じ声色で同じ音階の枠は同じ範囲を持つ() {
        let ranges = default_ranges(&[(None, 60), (None, 60)]);
        assert_eq!(ranges[0], ranges[1]);
        assert_eq!(ranges[0], ToneRange { low: 0, high: 127 });
    }

    #[test]
    fn 直した範囲は_c_1_から_g9_の中で下端が上端を超えない() {
        assert_eq!(
            ToneRange::edited(0, 127),
            Ok(ToneRange { low: 0, high: 127 })
        );
        assert_eq!(
            ToneRange::edited(60, 60),
            Ok(ToneRange { low: 60, high: 60 })
        );
        assert_eq!(
            ToneRange::edited(-1, 60).expect_err("C-1 より下").code(),
            "subbank.tone_range_out_of_range"
        );
        assert_eq!(
            ToneRange::edited(60, 128).expect_err("G9 より上").code(),
            "subbank.tone_range_out_of_range"
        );
        assert_eq!(
            ToneRange::edited(61, 60).expect_err("逆さ").code(),
            "subbank.tone_range_inverted"
        );
    }
}
