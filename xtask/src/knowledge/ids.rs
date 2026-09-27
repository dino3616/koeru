//! 型つきの ID。
//!
//! meta と FSL が使う ID は文字列のまま比べられてきたが、形は名前空間ごとに違う。
//! - `PREFIX-AREA-NUMBER`（`TR-REC-07`、`DEC-PLT-041`、`SUITE-XTASK-004`）。 いちばん多い形
//! - `PREFIX-NUMBER`（`CMP-081`）。 領域を持たない
//! - `PREFIX-AREANUMBER`（`PROFILE-M2`）。 領域と番号のあいだにハイフンが無い
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

/// 型つきの ID。接頭辞・領域・番号に分けて持つ。
///
/// 領域が無い形（`CMP-081`）もあるので `area` は `Option`。 元の表記は `raw` に
/// 持ち、`Display` はそれをそのまま書き戻す——組み立て直すと桁の情報が失われる。
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

    /// 先頭 0 を含めた、そのままの番号。
    pub(crate) fn number(&self) -> &str {
        &self.number
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
