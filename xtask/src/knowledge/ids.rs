//! 型つきの ID。
//!
//! meta と FSL が使う ID は文字列のまま比べられてきたが、形は名前空間ごとに違う。
//! - `PREFIX-AREA-NUMBER`（`TR-REC-07`、`DEC-PLT-041`、`SUITE-XTASK-004`）。 いちばん多い形
//! - `PREFIX-NUMBER`（`CMP-081`）。 領域を持たない
//! - `PREFIX-AREANUMBER`（`PROFILE-M2`）。 領域と番号のあいだにハイフンが無い
//! - `PREFIX-OPAQUE`（`DEC-6CQ3NA`）。 目標の形。 分類（領域）を持たず、opaque は
//!   Crockford Base32 の6文字固定（`DEC-PLT-048`）。 接頭辞は [`TARGET_PREFIXES`] だけ
//!
//! 目標の形は、接頭辞が [`TARGET_PREFIXES`] にあり、`-` のあとが Crockford Base32 の6文字ちょうどの
//! ときだけ。 今の形（領域か番号を持つ）はその形に当たらないので、読みは変わらない。 `DEC-123456`
//! のように数字だけの opaque も目標の形として読む——今の名前空間に、目標の接頭辞で番号だけを持つ
//! ID は無い（実在する ID で試験している）。
//!
//! 番号は先頭 0 を持つことがあり、桁数も名前空間ごとに違う（`TR-ALN-01` は2桁、
//! `DEC-PLT-042` は3桁）。 数値として畳むと `01` と `1` が同じになり、元の表記へ
//! 書き戻せなくなるので、番号は文字列のまま持つ。

use std::fmt;
use std::str::FromStr;

/// 領域と番号のあいだのハイフンを省いてよい接頭辞。
///
/// 省いてよいのは実際にそう名乗っている名前空間だけにする。 ここを全名前空間に
/// 広げると `TR-REC02` のような打ち間違いが実在する ID として通ってしまう
/// ——`refs.rs` の `id_spans` が自由文からの拾い上げで同じ理由の線を引いている。
const HYPHENLESS_AREA: &[&str] = &["PROFILE"];

/// 目標の形（`PREFIX-OPAQUE`）で名乗れる接頭辞。 提案書 5.2 と 5.2.1、`DEC-PLT-048`。
/// 種類との対応は `knowledge::design` が持つ。 ここは字面だけ。
pub(crate) const TARGET_PREFIXES: &[&str] = &[
    "Q", "HYP", "EVID", "DEC", "REQ", "PROBE", "OBL", "INV", "FB", "ACC",
];

/// 目標の ID の opaque の長さ。 `DEC-PLT-048`。
pub(crate) const OPAQUE_LEN: usize = 6;

/// Crockford Base32 の字。 `0-9` と、I・L・O・U を除く大文字。 小文字は含まない。
pub(crate) fn is_crockford(b: u8) -> bool {
    b.is_ascii_digit() || (b.is_ascii_uppercase() && !matches!(b, b'I' | b'L' | b'O' | b'U'))
}

/// 型つきの ID。接頭辞・領域・番号に分けて持つ。
///
/// 領域が無い形（`CMP-081`）もあるので `area` は `Option`。 元の表記は `raw` に
/// 持ち、`Display` はそれをそのまま書き戻す——組み立て直すと桁の情報が失われる。
///
/// 目標の形（`DEC-6CQ3NA`）は領域を持たず、opaque を `number` の場所に持つ。 外へは
/// [`Id::opaque`] で出し、[`Id::number`] は空にする。 目標の形かどうかは持たず、形から決める
/// ——持つと `Id` が大きくなり、`NodeKey` を2つ抱える `GraphIssue::Dangling` が膨らむ。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct Id {
    raw: String,
    prefix: String,
    area: Option<String>,
    number: String,
}

impl Id {
    /// 元の表記のまま。
    pub(crate) fn as_str(&self) -> &str {
        &self.raw
    }
}

// 接頭辞・領域・番号を個別に引く口。 今の唯一の消費者（`check-profile`）は
// `as_str` の突き合わせしか要らないので、ここはまだ試験だけが呼ぶ。
// `snapshot::KnowledgeSnapshot` を引く次の消費者（X05 のグラフ構築、X06 の
// migration、D00 の legacy adapter）は接頭辞や領域で絞り込みたくなるはずで、
// その口を今のうちに用意しておく。
#[allow(dead_code)]
impl Id {
    pub(crate) fn prefix(&self) -> &str {
        &self.prefix
    }

    pub(crate) fn area(&self) -> Option<&str> {
        self.area.as_deref()
    }

    /// 先頭 0 を含めた、そのままの番号。 目標の形では空。
    pub(crate) fn number(&self) -> &str {
        if self.is_target() { "" } else { &self.number }
    }

    /// 目標の形（`PREFIX-OPAQUE`）の opaque。 今の形では `None`。
    pub(crate) fn opaque(&self) -> Option<&str> {
        self.is_target().then_some(self.number.as_str())
    }

    /// 目標の形か。 今の形（`DEC-ALL-001`）と区別する。
    ///
    /// 目標の接頭辞で、領域が無く、`number` が Crockford Base32 の6文字。 今の形の読みは
    /// その並びにならない（`from_str` が目標の形を先に見るので、当たるものは必ず目標の形）。
    pub(crate) fn is_target(&self) -> bool {
        self.area.is_none()
            && TARGET_PREFIXES.contains(&self.prefix.as_str())
            && self.number.len() == OPAQUE_LEN
            && self.number.bytes().all(is_crockford)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// [`Id::from_str`] が拒んだときの理由。 文言は固定で、渡した文字列だけを持つ
/// （`rust-conventions` の「パス・入力値を差し込まない」は `Display` に限った縛りで、
/// これは検査の途中経過を人が読むためのものなので、ここでは元の文字列を持たせる）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ParseIdError(String);

impl fmt::Display for ParseIdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}` は ID の形をしていない", self.0)
    }
}

impl std::error::Error for ParseIdError {}

impl FromStr for Id {
    type Err = ParseIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let reject = || ParseIdError(s.to_owned());

        // 目標の形を先に見る。 領域か番号を持つ今の形は、この形に当たらない。
        if let Some((prefix, token)) = s.split_once('-')
            && TARGET_PREFIXES.contains(&prefix)
            && token.len() == OPAQUE_LEN
            && token.bytes().all(is_crockford)
        {
            return Ok(Self {
                raw: s.to_owned(),
                prefix: prefix.to_owned(),
                area: None,
                number: token.to_owned(),
            });
        }

        let prefix_len = s.bytes().take_while(u8::is_ascii_uppercase).count();
        if prefix_len == 0 {
            return Err(reject());
        }
        let (prefix, rest) = s.split_at(prefix_len);
        let rest = rest.strip_prefix('-').ok_or_else(reject)?;
        if rest.is_empty() {
            return Err(reject());
        }

        // 領域は大文字小文字を問わない。 大文字だけに絞る意味は無い——実在する ID は
        // すべて大文字だが、それは自由文からの拾い上げ（`refs.rs` の `id_spans`）が
        // 誤検知を避けるための線であって、ここは構造化された `id` 欄の値を読むだけ。
        let area_len = rest.bytes().take_while(u8::is_ascii_alphabetic).count();
        let (area, tail) = rest.split_at(area_len);

        let (area, number) = if area.is_empty() {
            // 領域を持たない形（`CMP-081`）。
            (None, tail)
        } else if let Some(number) = tail.strip_prefix('-') {
            (Some(area), number)
        } else if HYPHENLESS_AREA.contains(&prefix) {
            (Some(area), tail)
        } else {
            return Err(reject());
        };

        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(reject());
        }

        Ok(Self {
            raw: s.to_owned(),
            prefix: prefix.to_owned(),
            area: area.map(str::to_owned),
            number: number.to_owned(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 実在する3つの形をすべて通す。
    #[test]
    fn 実在する形は通る() {
        let id: Id = "TR-REC-07".parse().expect("PREFIX-AREA-NUMBER");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("TR", Some("REC"), "07")
        );
        assert_eq!(id.as_str(), "TR-REC-07");

        let id: Id = "DEC-PLT-041".parse().expect("3桁の番号");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("DEC", Some("PLT"), "041")
        );

        let id: Id = "SUITE-XTASK-004".parse().expect("複数文字の接頭辞");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("SUITE", Some("XTASK"), "004")
        );

        let id: Id = "CMP-081".parse().expect("PREFIX-NUMBER");
        assert_eq!((id.prefix(), id.area(), id.number()), ("CMP", None, "081"));

        let id: Id = "PROFILE-M2".parse().expect("PREFIX-AREANUMBER");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("PROFILE", Some("M"), "2")
        );

        let id: Id = "BUDGET-LATENCY-001".parse().expect("複数文字の領域");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("BUDGET", Some("LATENCY"), "001")
        );

        // 試験の fixture が使う小文字の領域（`TR-fix-01` のような形。`tests/commands.rs`
        // と同じ流儀）も、構造化された `id` 欄の値としては読める。 小文字を弾いているのは
        // 自由文からの拾い上げ側（`refs.rs`）で、ここではない。
        let id: Id = "TR-fix-01".parse().expect("小文字の領域");
        assert_eq!(
            (id.prefix(), id.area(), id.number()),
            ("TR", Some("fix"), "01")
        );
    }

    /// 桁を保つ。 `01` を数値に畳むと `1` になり、元の表記へ書き戻せなくなる。
    #[test]
    fn 表示は元の桁を保つ() {
        assert_eq!("TR-ALN-01".parse::<Id>().unwrap().to_string(), "TR-ALN-01");
        assert_eq!(
            "DEC-PLT-042".parse::<Id>().unwrap().to_string(),
            "DEC-PLT-042"
        );
    }

    /// `PROFILE` 以外でハイフンを省いた形は、実在する ID ではなく打ち間違いとして拒む。
    ///
    /// `TR-REC02` を許すと、`TR-REC-02` の打ち間違いが実在する ID に化ける。
    #[test]
    fn hyphenless_area_は_profile_だけ() {
        assert!("TR-REC02".parse::<Id>().is_err());
        assert!("PROFILE-M2".parse::<Id>().is_ok());
    }

    #[test]
    fn 形の崩れたものは拒む() {
        for bad in [
            "",
            "no-prefix-here",
            "TR",
            "TR-",
            "TR-ALN-",
            "TR-ALN-abc",
            "123-ALN-01",
            "tr-aln-01",
        ] {
            assert!(bad.parse::<Id>().is_err(), "{bad} を通してしまった");
        }
    }

    /// 目標の形。 接頭辞は目標の語彙だけ、opaque は Crockford Base32 の6文字固定。
    #[test]
    fn 目標の形は_opaque_を持つ() {
        for raw in [
            "Q-4K7DP2",
            "HYP-91MTWX",
            "EVID-2V8JRF",
            "DEC-6CQ3NA",
            "REQ-5H8NQ2",
            "OBL-4K7DP2",
            "INV-4K7DP2",
            "FB-4K7DP2",
            "ACC-4K7DP2",
            "PROBE-4K7DP2",
            // 数字だけ・先頭が0でも opaque。
            "DEC-123456",
            "DEC-000000",
        ] {
            let id: Id = raw.parse().unwrap_or_else(|_| panic!("{raw}"));
            assert!(id.is_target(), "{raw}");
            assert_eq!(id.as_str(), raw);
            assert_eq!(id.to_string(), raw);
            assert_eq!(id.area(), None);
            assert_eq!(id.number(), "");
            assert_eq!(Some(id.prefix()), raw.split('-').next());
            assert_eq!(id.opaque(), raw.split('-').nth(1));
        }
    }

    /// 小文字、I・L・O・U、長さ違い、目標の語彙に無い接頭辞は、目標の形として読まない。
    /// 今の形としても読めないので、今と同じく拒む。
    #[test]
    fn 目標の形でないものは今と同じく拒む() {
        for bad in [
            "DEC-6cq3na",
            "DEC-6CQ3NI",
            "DEC-6CQ3NL",
            "DEC-6CQ3NO",
            "DEC-6CQ3NU",
            "DEC-6CQ3N",
            "DEC-6CQ3NAA",
            "DEC-6CQ3NA-",
            "DEC-6CQ 3N",
            // 接頭辞が目標の語彙に無い。
            "TR-4K7DP2",
            "CMP-4K7DP2",
            // 領域だけで番号が無い。 今の形でもない。
            "DEC-ABCDEFG",
            "DEC-fixfoo",
        ] {
            assert!(bad.parse::<Id>().is_err(), "{bad} を通してしまった");
        }
    }

    /// 今の形は1つも読みが変わらない。 目標の形には当たらない。
    #[test]
    fn 今の形の読みは変わらない() {
        for (raw, prefix, area, number) in [
            ("DEC-ALL-001", "DEC", Some("ALL"), "001"),
            ("REQ-REC-101", "REQ", Some("REC"), "101"),
            ("INV-PKG-001", "INV", Some("PKG"), "001"),
            ("FB-fix-001", "FB", Some("fix"), "001"),
            ("Q-fix-001", "Q", Some("fix"), "001"),
            ("CMP-081", "CMP", None, "081"),
            ("PROFILE-M2", "PROFILE", Some("M"), "2"),
            // 目標の接頭辞で、番号が6桁より短い。
            ("DEC-001", "DEC", None, "001"),
            ("REQ-1", "REQ", None, "1"),
        ] {
            let id: Id = raw.parse().unwrap_or_else(|_| panic!("{raw}"));
            assert!(!id.is_target(), "{raw}");
            assert_eq!(id.opaque(), None);
            assert_eq!(
                (id.prefix(), id.area(), id.number()),
                (prefix, area, number),
                "{raw}"
            );
        }
    }

    /// 今の形と目標の形は、同じ文字列にならない別の ID。
    #[test]
    fn 今の形と目標の形は別の_id() {
        let legacy: Id = "DEC-ALL-001".parse().unwrap();
        let target: Id = "DEC-6CQ3NA".parse().unwrap();
        assert_ne!(legacy, target);
        assert!(legacy < target || target < legacy);
    }

    /// 順序は `raw` の文字列順に一致する。 既存の `BTreeMap<String, _>` を
    /// 型つきの索引へ置き換えても、並びが変わらないようにするため。
    #[test]
    fn 順序は文字列と同じ() {
        let a: Id = "DEC-PLT-001".parse().unwrap();
        let b: Id = "DEC-PLT-010".parse().unwrap();
        assert!(a < b);
        assert_eq!(a < b, "DEC-PLT-001" < "DEC-PLT-010");
    }
}
