//! 歌詞（仮名列）からモーラ列へ（`TR-RCL-13`, `TR-SYN-10`）。
//!
//! 録音リスト生成・課題曲の必要単位算出・カバレッジ判定が同じ実装を使う
//! （`TR-RCL-13`）。3箇所で別々に数えると、「録ったのに歌えない」が起きる。
//!
//! # 規則
//!
//! | 入力 | 扱い |
//! |---|---|
//! | 小書き仮名（ぁぃぅぇぉゃゅょ） | 直前と結合して1モーラ |
//! | 促音「っ」 | 1モーラとして数えるが収録単位を要求しない。 直後の CV の子音部を要求する |
//! | 長音「ー」 | 直前モーラの末尾母音の継続。 新たな収録単位を要求しない |
//! | 撥音「ん」 | 母音クラス `n` を持つ独立した収録単位 |
//! | カタカナ | 対応する平仮名へ正規化。「ヴ」系は外来音 |
//! | 無声化母音 | 正規化しない（`TR-RCL-13` (f)） |

use crate::inventory::{Unit, UnitSet, units};

/// モーラ1つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mora {
    /// 元の表記（正規化後）。
    pub text: String,
    /// このモーラが要求する収録単位。要求しないなら `None`。
    ///
    /// 促音と長音は数には入るが単位を要求しない（`TR-RCL-13` (b)(c)）。
    pub unit: Option<&'static str>,
    pub kind: MoraKind,
}

/// モーラの種別（`TR-RCL-13`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoraKind {
    /// 普通の拍。収録単位を1つ要求する。
    Syllable,
    /// 促音。直後の CV の子音部を要求する。
    Geminate,
    /// 長音。直前モーラの末尾母音の継続。
    LongVowel,
    /// 撥音。
    Moraic,
}

/// 仮名列を解釈できなかった理由。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum MoraError {
    /// インベントリに無い拍。
    #[error("知らない拍がある")]
    UnknownSyllable {
        /// その拍。
        text: String,
    },
    /// 先頭に小書き仮名・長音・促音が来た。
    #[error("先頭に結合できない文字がある")]
    DanglingModifier { ch: char },
}

impl MoraError {
    /// 送信してよい種別文字列。中身の文字は送らない（歌詞の一部）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::UnknownSyllable { .. } => "mora.unknown_syllable",
            Self::DanglingModifier { .. } => "mora.dangling_modifier",
        }
    }
}

/// 小書き仮名。直前と結合して1モーラ（`TR-RCL-13` (a)）。
const SMALL: [char; 9] = ['ぁ', 'ぃ', 'ぅ', 'ぇ', 'ぉ', 'ゃ', 'ゅ', 'ょ', 'ゎ'];

/// 促音。
const GEMINATE: char = 'っ';
/// 長音。
const LONG: char = 'ー';
/// 撥音。
const MORAIC: char = 'ん';

/// カタカナを平仮名へ（`TR-RCL-13` (e)）。
///
/// 「ヴ」だけは平仮名側へ落とさない。 インベントリが `ヴぁ` の綴りで
/// 外来音を持っているので、そこへ寄せる。平仮名の「ゔ」で書かれても同じ扱いにする。
#[must_use]
pub fn to_hiragana(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            // 長音記号はそのまま。
            LONG => LONG,
            // 平仮名の「ゔ」はカタカナの「ヴ」へ寄せる。 綴りを1つに決める。
            'ゔ' => 'ヴ',
            // カタカナ（ァ〜ヶ）を平仮名へ。ヴ（U+30F4）は対象外。
            'ァ'..='ヶ' if c != 'ヴ' => char::from_u32(c as u32 - 0x60).unwrap_or(c),
            _ => c,
        })
        .collect()
}

/// 仮名列をモーラ列にする。
///
/// 空白は区切りとして落とす。 録音リストの行は空白で単位を区切っている。
#[tracing::instrument(skip(kana), err)]
pub fn parse(kana: &str, set: UnitSet) -> Result<Vec<Mora>, MoraError> {
    let table = units(set);
    let normalized = to_hiragana(&crate::text::to_nfc(kana));
    let chars: Vec<char> = normalized.chars().filter(|c| !c.is_whitespace()).collect();

    let mut out: Vec<Mora> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];

        if c == GEMINATE {
            // 数には入るが単位を要求しない（`TR-RCL-13` (b)）。
            // 直後の CV の子音部を要求するが、その子音は次のモーラが持っている。
            out.push(Mora {
                text: c.to_string(),
                unit: None,
                kind: MoraKind::Geminate,
            });
            i += 1;
            continue;
        }

        if c == LONG {
            // 直前モーラの末尾母音の継続。新たな単位を要求しない（`TR-RCL-13` (c)）。
            if out.is_empty() {
                return Err(MoraError::DanglingModifier { ch: c });
            }
            out.push(Mora {
                text: c.to_string(),
                unit: None,
                kind: MoraKind::LongVowel,
            });
            i += 1;
            continue;
        }

        if c == MORAIC {
            // 母音クラス n を持つ独立した収録単位（`TR-RCL-13` (d)）。
            let unit = find(&table, "ん").ok_or_else(|| MoraError::UnknownSyllable {
                text: "ん".to_owned(),
            })?;
            out.push(Mora {
                text: c.to_string(),
                unit: Some(unit.kana),
                kind: MoraKind::Moraic,
            });
            i += 1;
            continue;
        }

        if SMALL.contains(&c) {
            // 直前と結合できなかった小書き仮名。
            return Err(MoraError::DanglingModifier { ch: c });
        }

        // 2文字（拗音）を先に試す。 「きゃ」を「き」+「ゃ」に割らない。
        let two = chars
            .get(i + 1)
            .filter(|n| SMALL.contains(n))
            .map(|n| format!("{c}{n}"));
        if let Some(pair) = two
            && let Some(unit) = find(&table, &pair)
        {
            out.push(Mora {
                text: pair,
                unit: Some(unit.kana),
                kind: MoraKind::Syllable,
            });
            i += 2;
            continue;
        }

        let one = c.to_string();
        let unit = find(&table, &one).ok_or(MoraError::UnknownSyllable { text: one.clone() })?;
        out.push(Mora {
            text: one,
            unit: Some(unit.kana),
            kind: MoraKind::Syllable,
        });
        i += 1;
    }

    Ok(out)
}

/// モーラ列が要求する収録単位の集合。
///
/// 促音と長音は入らない（`TR-RCL-13` (b)(c)）。
#[must_use]
pub fn required_units(moras: &[Mora]) -> std::collections::BTreeSet<String> {
    moras
        .iter()
        .filter_map(|m| m.unit.map(str::to_owned))
        .collect()
}

fn find<'a>(table: &'a [Unit], kana: &str) -> Option<&'a Unit> {
    table.iter().find(|u| u.kana == kana)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn texts(ms: &[Mora]) -> Vec<&str> {
        ms.iter().map(|m| m.text.as_str()).collect()
    }

    fn parse_core(s: &str) -> Vec<Mora> {
        parse(s, UnitSet::Core).expect("読めること")
    }

    /// 小書き仮名は直前と結合して1モーラ（`TR-RCL-13` (a)）。
    #[test]
    fn 拗音は一モーラ() {
        let m = parse_core("きゃきゅきょ");
        assert_eq!(texts(&m), ["きゃ", "きゅ", "きょ"]);
        assert_eq!(m.len(), 3, "6文字だが3モーラ");
    }

    /// 促音は数に入るが単位を要求しない（`TR-RCL-13` (b)）。
    #[test]
    fn 促音は数に入るが単位を要求しない() {
        let m = parse_core("きって");
        assert_eq!(texts(&m), ["き", "っ", "て"]);
        assert_eq!(m[1].kind, MoraKind::Geminate);
        assert_eq!(m[1].unit, None);
        assert_eq!(required_units(&m).len(), 2, "き と て だけ");
    }

    /// 長音は直前母音の継続で、新たな単位を要求しない（`TR-RCL-13` (c)）。
    #[test]
    fn 長音は単位を要求しない() {
        let m = parse_core("かー");
        assert_eq!(texts(&m), ["か", "ー"]);
        assert_eq!(m[1].kind, MoraKind::LongVowel);
        assert_eq!(m[1].unit, None);
        assert_eq!(required_units(&m), ["か".to_owned()].into());
    }

    /// 撥音は独立した収録単位（`TR-RCL-13` (d)）。
    #[test]
    fn 撥音は独立した単位() {
        let m = parse_core("ほん");
        assert_eq!(texts(&m), ["ほ", "ん"]);
        assert_eq!(m[1].kind, MoraKind::Moraic);
        assert_eq!(m[1].unit, Some("ん"));
        assert_eq!(required_units(&m).len(), 2);
    }

    /// カタカナは平仮名へ（`TR-RCL-13` (e)）。
    #[test]
    fn カタカナを平仮名へ正規化する() {
        assert_eq!(to_hiragana("アイウエオ"), "あいうえお");
        assert_eq!(to_hiragana("キャット"), "きゃっと");
        // 長音記号は残る。
        assert_eq!(to_hiragana("ドーナツ"), "どーなつ");
        // ヴは平仮名へ落とさない。 外来音として `ヴぁ` の綴りに寄せる。
        assert_eq!(to_hiragana("ヴァイオリン"), "ヴぁいおりん");
        assert_eq!(to_hiragana("ゔぁ"), "ヴぁ", "平仮名のゔも同じ綴りへ");
    }

    #[test]
    fn カタカナの歌詞をそのまま読める() {
        let m = parse_core("サクラ");
        assert_eq!(texts(&m), ["さ", "く", "ら"]);
    }

    /// ヴ系は 168 音セットにのみ含める（`TR-RCL-13` (e)）。
    #[test]
    fn 外来音は拡張セットでだけ読める() {
        // 平仮名でもカタカナでも同じ単位を指す。綴りを1つに寄せてある。
        for s in ["ゔぁ", "ヴぁ", "ヴァ"] {
            let m = parse(s, UnitSet::Extended).unwrap_or_else(|e| panic!("{s}: {e}"));
            assert_eq!(m[0].unit, Some("ヴぁ"), "{s}");
        }
        let e = parse("ゔぁ", UnitSet::Core).expect_err("中核には無いこと");
        assert_eq!(e.kind(), "mora.unknown_syllable");
    }

    #[test]
    fn 空白は落とす() {
        let m = parse_core("あ い う え お");
        assert_eq!(texts(&m), ["あ", "い", "う", "え", "お"]);
    }

    #[test]
    fn 先頭の長音と小書き仮名を拒む() {
        assert!(matches!(
            parse("ーあ", UnitSet::Core),
            Err(MoraError::DanglingModifier { ch: 'ー' })
        ));
        assert!(matches!(
            parse("ゃあ", UnitSet::Core),
            Err(MoraError::DanglingModifier { ch: 'ゃ' })
        ));
    }

    #[test]
    fn 知らない文字を拒む() {
        let e = parse("あXう", UnitSet::Core).expect_err("拒むこと");
        assert_eq!(e.kind(), "mora.unknown_syllable");
    }

    /// 録音リストの行も同じ実装で読める（`TR-RCL-13` の「同一の実装」）。
    #[test]
    fn 録音リストの行を読める() {
        let list = crate::reclist::generate_single(UnitSet::Core, 5).expect("生成できる");
        for row in &list {
            let m = parse(&row.text, UnitSet::Core)
                .unwrap_or_else(|e| panic!("{} を読めること: {e}", row.text));
            let got = required_units(&m);
            let want: std::collections::BTreeSet<String> =
                row.units.iter().map(|u| u.kana.to_owned()).collect();
            assert_eq!(got, want, "行 {} の単位が一致すること", row.id);
        }
    }

    /// 歌詞の1フレーズを通す。
    #[test]
    fn 歌詞を通せる() {
        let m = parse_core("さくらさくら やよいのそらは");
        assert_eq!(m.len(), 13);
        let units = required_units(&m);
        assert!(units.contains("さ"));
        assert!(!units.contains("ん"), "ん は無い");
    }
}
