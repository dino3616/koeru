//! 書き出し単位のバージョン管理（`TR-PKG-44`, `TR-PKG-46`, `TR-PKG-48`）。
//!
//! バージョンは ZIP の書き出し単位で刻む。 プロジェクトの保存回数でも、
//! 録音した本数でもない。受け手の手元にあるのは ZIP なので、
//! 突き合わせられる単位はそこしかない。
//!
//! リリースレコードは不変で、これはスキーマのトリガが止める
//! （`migrations/2026-08-30-020000_releases`）。規律に頼らない。

use sha2::{Digest as _, Sha256};

use crate::project::Method;

/// 書き出し前検証の結果（`TR-PKG-44`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Validation {
    /// 全 oto が検証を通った。
    Passed,
    /// 通らないものがあるまま書き出した（本人が承知のうえ）。
    PassedWithWarnings,
    /// 検証を実行していない。
    NotRun,
}

impl Validation {
    /// 保存する名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::PassedWithWarnings => "passed_with_warnings",
            Self::NotRun => "not_run",
        }
    }

    /// 保存した名前から戻す。知らない値は `NotRun` に倒す。
    /// 未知の検証結果を「通った」と読むほうが危ない。
    #[must_use]
    pub fn parse(s: &str) -> Self {
        match s {
            "passed" => Self::Passed,
            "passed_with_warnings" => Self::PassedWithWarnings,
            _ => Self::NotRun,
        }
    }
}

/// 書き出し1回ぶんの記録。一度書いたら変わらない（`TR-PKG-44`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    /// 単調増加する連番。採番は DB が持つ。
    pub seq: i32,
    /// ユーザーが付けたバージョン文字列。
    pub version: String,
    /// 含めた方式。
    pub method: Method,
    /// 含めたエイリアス数。
    pub alias_count: i32,
    /// 書き出し前検証の結果。
    pub validation: Validation,
    /// 生成した `oto.ini` の内容ハッシュ。
    pub oto_hash: String,
    /// 規約本文のハッシュ。
    pub terms_hash: String,
    /// `exports/` 配下の名前。
    pub archive_name: String,
    /// 書き出した時刻。
    pub released_at: String,
}

/// 書き出す前に用意する内容。`seq` と `archive_name` は DB が決める。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NewRelease {
    /// ユーザーが付けたバージョン文字列。
    pub version: String,
    /// 含めた方式。
    pub method: Method,
    /// 含めたエイリアス数。
    pub alias_count: i32,
    /// 書き出し前検証の結果。
    pub validation: Validation,
    /// 生成した `oto.ini` の内容ハッシュ。
    pub oto_hash: String,
    /// 規約本文のハッシュ。
    pub terms_hash: String,
    /// 書き出した時刻。
    pub released_at: String,
}

/// バイト列の SHA-256 を小文字16進で返す。
///
/// `oto.ini` は改行と符号化を確定させてからここへ渡す。 同じ内容でも
/// CRLF と LF で違うハッシュになるので、比較の前提が崩れる。
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for b in d {
        use std::fmt::Write as _;
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// 書き出し先の名前を決める（`TR-PKG-44`）。
///
/// 過去のバージョンを上書きしない。 連番を先頭に付けるので、同じ
/// バージョン文字列を二度使っても別のファイルになる。
///
/// バージョン文字列は FS 上安全な字だけを残す。残らなかった場合でも
/// 連番があるので名前は必ず一意。
#[must_use]
pub fn archive_name(seq: i32, version: &str, ext: &str) -> String {
    let mut safe = String::with_capacity(version.len());
    for c in version.chars() {
        if c.is_ascii_alphanumeric() || matches!(c, '.' | '_') {
            safe.push(c);
        } else if !safe.ends_with('-') {
            // 連続する置換を1つにまとめる。`a----b` のような名前を作らない。
            safe.push('-');
        }
    }
    let safe = safe.trim_matches('-');
    if safe.is_empty() {
        format!("{seq:06}.{ext}")
    } else {
        format!("{seq:06}-{safe}.{ext}")
    }
}

/// 書き出し済みパッケージを取り込むときの判定（`TR-PKG-48`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OtoDrift {
    /// 書き出したときのままで、外部で編集されていない。
    Unchanged,
    /// 外部ツールで編集されている。差分を取り込むか捨てるかは本人が選ぶ。
    EditedExternally,
    /// 対応するリリースレコードが無く、比較できない。
    UnknownRelease,
}

/// 取り込もうとしている `oto.ini` が、書き出したときと同じかを判定する。
#[must_use]
pub fn detect_drift(release: Option<&Release>, incoming: &[u8]) -> OtoDrift {
    match release {
        None => OtoDrift::UnknownRelease,
        Some(r) if r.oto_hash == content_hash(incoming) => OtoDrift::Unchanged,
        Some(_) => OtoDrift::EditedExternally,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_lowercase_hex_sha256() {
        // 空文字列の SHA-256（既知の値）。
        assert_eq!(
            content_hash(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn hash_distinguishes_line_endings() {
        assert_ne!(content_hash(b"a\nb"), content_hash(b"a\r\nb"));
    }

    /// 過去のバージョンを上書きしない（`TR-PKG-44`）。
    /// 同じバージョン文字列を二度使っても、名前は衝突しない。
    #[test]
    fn archive_names_never_collide_on_repeated_versions() {
        assert_eq!(archive_name(1, "v1.0", "zip"), "000001-v1.0.zip");
        assert_eq!(archive_name(2, "v1.0", "zip"), "000002-v1.0.zip");
    }

    #[test]
    fn archive_names_stay_filesystem_safe() {
        assert_eq!(archive_name(3, "1.0/正式版", "zip"), "000003-1.0.zip");
        assert_eq!(
            archive_name(6, "a  //  b", "zip"),
            "000006-a-b.zip",
            "置換をまとめること"
        );
        // 安全な字が1つも残らなくても、連番だけで名前になる。
        assert_eq!(archive_name(4, "正式版", "zip"), "000004.zip");
        assert_eq!(archive_name(5, "", "zip"), "000005.zip");
    }

    #[test]
    fn archive_names_sort_chronologically() {
        let mut names = [
            archive_name(10, "b", "zip"),
            archive_name(2, "a", "zip"),
            archive_name(1, "c", "zip"),
        ];
        names.sort();
        assert_eq!(names, ["000001-c.zip", "000002-a.zip", "000010-b.zip"]);
    }

    fn release(hash: &str) -> Release {
        Release {
            seq: 1,
            version: "v1".into(),
            method: Method::Single,
            alias_count: 102,
            validation: Validation::Passed,
            oto_hash: hash.into(),
            terms_hash: content_hash(b"terms"),
            archive_name: "000001-v1.zip".into(),
            released_at: "2026-08-30T12:00:00Z".into(),
        }
    }

    /// 外部ツールでの編集をハッシュの不一致から検出する（`TR-PKG-48`）。
    #[test]
    fn drift_is_detected_from_the_hash() {
        let oto = b"[a.wav]\na=1";
        let r = release(&content_hash(oto));
        assert_eq!(detect_drift(Some(&r), oto), OtoDrift::Unchanged);
        assert_eq!(
            detect_drift(Some(&r), b"[a.wav]\na=2"),
            OtoDrift::EditedExternally
        );
        assert_eq!(detect_drift(None, oto), OtoDrift::UnknownRelease);
    }

    /// 知らない検証結果を「通った」と読まない。
    #[test]
    fn unknown_validation_falls_back_to_not_run() {
        assert_eq!(Validation::parse("passed"), Validation::Passed);
        assert_eq!(Validation::parse("なんだこれ"), Validation::NotRun);
        for v in [
            Validation::Passed,
            Validation::PassedWithWarnings,
            Validation::NotRun,
        ] {
            assert_eq!(Validation::parse(v.as_str()), v, "往復すること");
        }
    }
}
