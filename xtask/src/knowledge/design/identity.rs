//! 目標の ID の形と、操作上の locator の形（D00）。
//!
//! 出典は提案書 5.2（ID は分類ではなく identity だけを持つ）と、5.1.5 / 7.1 に
//! 例として書かれている locator。 書かれていない形は読まず、読めないと言う。
//!
//! 今の `Id`（`knowledge::Id`）は `PREFIX-AREA-NUMBER` の形しか読めないので、目標の ID
//! （`REQ-5H8NQ2`）はそちらでは読めない。 graph の node はまだ `Id` で持つ
//! （X03 への要求。 D00 の後の作業）。 ここはその前に、形だけを検査する。

use std::fmt;
use std::str::FromStr;

use super::ontology::Kind;

/// 目標の ID の接頭辞と、その種類。 提案書 5.2 と 5.2.1。
const PREFIXES: &[(&str, Kind)] = &[
    ("Q", Kind::Question),
    ("HYP", Kind::Hypothesis),
    ("EVID", Kind::Evidence),
    ("DEC", Kind::Decision),
    ("REQ", Kind::Requirement),
    ("PROBE", Kind::Probe),
    ("OBL", Kind::FormalContract),
    ("INV", Kind::FormalContract),
    ("FB", Kind::FormalContract),
    // FSL の `acceptance`（`DEC-PLT-048`）。
    ("ACC", Kind::FormalContract),
];

/// opaque の長さ。 `DEC-PLT-048`。
const OPAQUE_LEN: usize = 6;

/// 目標の ID。 `<接頭辞>-<opaque>`。
///
/// opaque は Crockford Base32（`0-9` と、I・L・O・U を除く `A-Z`）の6文字固定で、本文から
/// 計算する hash ではない。 作成時に乱数で一度だけ払い出し、衝突を検査し、内容が変わっても
/// 変わらない（`DEC-PLT-048`）。 払い出す生成器は将来の `new-id` が持つ。 ここが持つのは形の検査だけ。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) struct TargetId {
    kind: Kind,
    raw: String,
}

impl TargetId {
    pub(crate) fn kind(&self) -> Kind {
        self.kind
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.raw
    }
}

impl fmt::Display for TargetId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.raw)
    }
}

/// 目標の ID として読めなかった理由。 文言は固定で、入力を差し込まない。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IdentityError {
    /// `<接頭辞>-<opaque>` の形ではない。 今の `DEC-ALL-001` のような、分類を含む形もここ。
    NotTargetForm,
    /// 接頭辞が目標の語彙に無い。
    UnknownPrefix,
    /// 接頭辞が示す種類と、記録の種類が合わない。
    PrefixMismatch,
    /// ファイル名が ID と一致しない。
    StemMismatch,
}

impl fmt::Display for IdentityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NotTargetForm => "目標の ID の形（接頭辞と opaque の2つだけ）ではない",
            Self::UnknownPrefix => "接頭辞が目標の語彙に無い",
            Self::PrefixMismatch => "接頭辞が記録の種類と合わない",
            Self::StemMismatch => "ファイル名が ID と一致しない",
        })
    }
}

impl std::error::Error for IdentityError {}

impl FromStr for TargetId {
    type Err = IdentityError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (prefix, token) = s.split_once('-').ok_or(IdentityError::NotTargetForm)?;
        // 分類（領域）を ID に入れない。 token にハイフンが続くのは今の形。
        if token.len() != OPAQUE_LEN || !token.bytes().all(is_crockford) {
            return Err(IdentityError::NotTargetForm);
        }
        let kind = PREFIXES
            .iter()
            .find(|(p, _)| *p == prefix)
            .map(|(_, k)| *k)
            .ok_or(IdentityError::UnknownPrefix)?;
        Ok(Self {
            kind,
            raw: s.to_owned(),
        })
    }
}

/// Crockford Base32 の字。 小文字と、紛らわしい I・L・O・U は含まない。
fn is_crockford(b: u8) -> bool {
    b.is_ascii_digit() || (b.is_ascii_uppercase() && !matches!(b, b'I' | b'L' | b'O' | b'U'))
}

/// 目標の形で書かれた1件1ファイルの記録の、ID を検査する。
///
/// ID の形、接頭辞と記録の種類の一致、ファイル名と ID の一致の3つ。 どれも
/// 内容を見ずに決まる——内容が変わっても ID が変わらないための前提。
///
/// 「内容を直したら ID が変わった」は、この関数では見えない。 2つの版の差分で、git の改名検出
/// を使い、同じ種類のファイルの改名として出す（`DEC-PLT-048`）。 `RepoView` が改名の情報を出す口
/// （X02）が要るので、D00 の後の作業で行う。
pub(crate) fn check_identity(
    expected: Kind,
    file_stem: &str,
    id: &str,
) -> Result<TargetId, IdentityError> {
    let parsed: TargetId = id.parse()?;
    if parsed.kind != expected {
        return Err(IdentityError::PrefixMismatch);
    }
    if file_stem != id {
        return Err(IdentityError::StemMismatch);
    }
    Ok(parsed)
}

/// 操作上の locator。 提案書が書式を示しているものだけ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Locator {
    /// `github:pr/123@<rev>`。 `@<rev>` は省ける。
    PullRequest { number: u64, rev: Option<String> },
    /// `workload:<slug>`、または `verification/workloads/<slug>.toml`。 同じ workload の2つの書き方。
    Workload { slug: String },
    /// リポジトリの根からの相対パス。
    Source { path: String },
}

/// locator として読めなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LocatorError {
    /// 提案書が書式を示していない scheme（issue・commit・story・dependency・artifact・receipt
    /// のどれも）。 推測で読まない。
    UnspecifiedForm,
    Malformed,
}

impl fmt::Display for LocatorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::UnspecifiedForm => "書式が決まっていない locator",
            Self::Malformed => "locator の形が崩れている",
        })
    }
}

impl std::error::Error for LocatorError {}

const WORKLOAD_DIR: &str = "verification/workloads/";

impl Locator {
    pub(crate) fn kind(&self) -> Kind {
        match self {
            Self::PullRequest { .. } => Kind::PullRequest,
            Self::Workload { .. } => Kind::Workload,
            Self::Source { .. } => Kind::Source,
        }
    }

    pub(crate) fn parse(raw: &str) -> Result<Self, LocatorError> {
        if let Some(rest) = raw.strip_prefix("github:") {
            let rest = rest
                .strip_prefix("pr/")
                .ok_or(LocatorError::UnspecifiedForm)?;
            let (number, rev) = match rest.split_once('@') {
                Some((n, rev)) if !rev.is_empty() => (n, Some(rev.to_owned())),
                Some(_) => return Err(LocatorError::Malformed),
                None => (rest, None),
            };
            let number = number.parse().map_err(|_| LocatorError::Malformed)?;
            return Ok(Self::PullRequest { number, rev });
        }
        if let Some(slug) = raw.strip_prefix("workload:") {
            return slug_of(slug).map(|slug| Self::Workload { slug });
        }
        if raw.contains(':') {
            return Err(LocatorError::UnspecifiedForm);
        }
        if let Some(file) = raw.strip_prefix(WORKLOAD_DIR) {
            let slug = file.strip_suffix(".toml").ok_or(LocatorError::Malformed)?;
            return slug_of(slug).map(|slug| Self::Workload { slug });
        }
        if raw.is_empty()
            || raw.starts_with('/')
            || raw.split('/').any(|c| c.is_empty() || c == "..")
        {
            return Err(LocatorError::Malformed);
        }
        Ok(Self::Source {
            path: raw.to_owned(),
        })
    }
}

fn slug_of(slug: &str) -> Result<String, LocatorError> {
    let ok = !slug.is_empty()
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
    if ok {
        Ok(slug.to_owned())
    } else {
        Err(LocatorError::Malformed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 提案書の例の_id_を目標の形として読める() {
        for (raw, kind) in [
            ("Q-4K7DP2", Kind::Question),
            ("HYP-91MTWX", Kind::Hypothesis),
            ("EVID-2V8JRF", Kind::Evidence),
            ("DEC-6CQ3NA", Kind::Decision),
            ("REQ-5H8NQ2", Kind::Requirement),
        ] {
            let id: TargetId = raw.parse().expect("目標の形");
            assert_eq!(id.kind(), kind);
            assert_eq!(id.to_string(), raw);
        }
    }

    /// 今の ID は分類（領域）を含むので、目標の形としては読まない。
    #[test]
    fn 今の_id_の形は目標の形ではない() {
        for raw in [
            "DEC-ALL-001",
            "TR-REC-03",
            "REQ-REC-101",
            "CMP-081",
            "DEC-",
            "DEC",
            "-6CQ3NA",
        ] {
            assert!(raw.parse::<TargetId>().is_err(), "{raw}");
        }
        assert_eq!(
            "TR-4K7DP2".parse::<TargetId>(),
            Err(IdentityError::UnknownPrefix)
        );
        assert_eq!(
            "DEC-ALL-001".parse::<TargetId>(),
            Err(IdentityError::NotTargetForm)
        );
    }

    /// opaque は Crockford Base32 の6文字固定。 小文字、I・L・O・U、長さの違いは落とす。
    #[test]
    fn opaque_は_crockford_base32_の6文字固定() {
        for raw in [
            "DEC-6CQ3NA",
            "DEC-000000",
            "DEC-ZZZZZZ",
            "ACC-4K7DP2",
            "FB-4K7DP2",
            "OBL-4K7DP2",
            "INV-4K7DP2",
        ] {
            assert!(raw.parse::<TargetId>().is_ok(), "{raw}");
        }
        for raw in [
            "Q-4k7dp2",
            "DEC-6cq3na",
            "DEC-6CQ3NI",
            "DEC-6CQ3NL",
            "DEC-6CQ3NO",
            "DEC-6CQ3NU",
            "DEC-6CQ3N",
            "DEC-6CQ3NAA",
            "DEC-6CQ3N-",
            "DEC-6CQ 3N",
        ] {
            assert_eq!(
                raw.parse::<TargetId>(),
                Err(IdentityError::NotTargetForm),
                "{raw}"
            );
        }
    }

    #[test]
    fn 接頭辞が記録の種類と合わなければ落ちる() {
        assert!(check_identity(Kind::Decision, "DEC-6CQ3NA", "DEC-6CQ3NA").is_ok());
        assert_eq!(
            check_identity(Kind::Decision, "REQ-5H8NQ2", "REQ-5H8NQ2"),
            Err(IdentityError::PrefixMismatch)
        );
    }

    /// ファイル名と ID が食い違う記録は、どちらが正しいのか分からないので落とす。
    #[test]
    fn ファイル名と_id_が食い違えば落ちる() {
        assert_eq!(
            check_identity(Kind::Decision, "DEC-6CQ3NB", "DEC-6CQ3NA"),
            Err(IdentityError::StemMismatch)
        );
    }

    #[test]
    fn 提案書が書式を示した_locator_だけを読む() {
        assert_eq!(
            Locator::parse("github:pr/123@abc123"),
            Ok(Locator::PullRequest {
                number: 123,
                rev: Some("abc123".to_owned())
            })
        );
        assert_eq!(
            Locator::parse("github:pr/7"),
            Ok(Locator::PullRequest {
                number: 7,
                rev: None
            })
        );
        let w = Locator::Workload {
            slug: "continuous-8mora-3pitch".to_owned(),
        };
        assert_eq!(
            Locator::parse("workload:continuous-8mora-3pitch"),
            Ok(w.clone())
        );
        assert_eq!(
            Locator::parse("verification/workloads/continuous-8mora-3pitch.toml"),
            Ok(w)
        );
        let src = Locator::parse("crates/koeru-app/ui/src/main.tsx").expect("パス");
        assert_eq!(src.kind(), Kind::Source);
    }

    /// 書式が決まっていないものを、推測で読まない。
    #[test]
    fn 書式が決まっていない_locator_は読めないと言う() {
        for raw in [
            "github:issue/5",
            "github:commit/abc",
            "story:button--primary",
            "receipt:abc",
        ] {
            assert_eq!(
                Locator::parse(raw),
                Err(LocatorError::UnspecifiedForm),
                "{raw}"
            );
        }
        for raw in [
            "github:pr/x",
            "github:pr/1@",
            "workload:",
            "workload:Upper",
            "",
            "/abs/path",
            "a/../b",
            "verification/workloads/x.md",
        ] {
            assert_eq!(Locator::parse(raw), Err(LocatorError::Malformed), "{raw}");
        }
    }
}
