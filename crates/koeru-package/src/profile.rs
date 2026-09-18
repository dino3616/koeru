//! 書き出しプロファイル（`TR-PKG-12`, `TR-PKG-13`）。
//!
//! 3つのプロファイルが変えるのはテキストの符号化と `text_file_encoding` の
//! 宣言だけ。 エントリ名は ASCII 固定なので、どれを選んでもバイト列が同じ
//! （`DEC-PKG-009`）。
//!
//! # 改行はどのプロファイルでも CRLF
//!
//! `TR-PKG-12` が課しているのは classic 互換だけだが、他でも壊れない。
//! `oto.ini` は `koeru_align::ini` が既に CRLF で書いており、
//! プロファイルごとに改行を変えると、同じ配布物の中で行末が2種類になる。

use koeru_core::text::{self, TextEncoding};

/// 生成するテキストの改行。
pub const NEWLINE: &str = "\r\n";

/// 書き出しプロファイル（`TR-PKG-12`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Profile {
    /// classic 互換。テキストは CP932。
    Classic,
    /// OpenUtau 互換。テキストは UTF-8。
    OpenUtau,
    /// 両対応。テキストは CP932 で、`character.yaml` にそう宣言する。既定。
    #[default]
    Both,
}

impl Profile {
    /// 保存と受け渡しに使う名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::OpenUtau => "openutau",
            Self::Both => "both",
        }
    }

    /// 保存した名前から戻す。知らない名前は `None`。
    ///
    /// 既定へ倒さない。 知らない名前を両対応と読むと、classic だけを
    /// 選んだつもりの書き出しが黙って別のものになる。
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "classic" => Some(Self::Classic),
            "openutau" => Some(Self::OpenUtau),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// 生成するテキストの符号化（`TR-PKG-12`）。
    #[must_use]
    pub const fn encoding(self) -> TextEncoding {
        match self {
            Self::Classic | Self::Both => TextEncoding::Cp932,
            Self::OpenUtau => TextEncoding::Utf8,
        }
    }

    /// `character.yaml` の `text_file_encoding` に書く名前（`TR-PKG-12`）。
    #[must_use]
    pub const fn declared_encoding(self) -> &'static str {
        self.encoding().as_str()
    }

    /// CP932 で書き出すか。 変換できない文字の検査が要るかの判定に使う。
    #[must_use]
    pub const fn needs_cp932(self) -> bool {
        matches!(self.encoding(), TextEncoding::Cp932)
    }
}

/// CP932 の往復が健全かを起動時に確かめる（`TR-PKG-13`）。
///
/// 失敗したら classic 互換と両対応の書き出しを無効化する。 符号化が
/// 壊れた状態で書き出すと、化けた配布物が検証を通ってしまう。
///
/// `encoding_rs` はライブラリへ静的に含まれるので、通常は失敗しない。
/// それでも見るのは、**無効化の経路を持っていること自体が要件だから。**
#[must_use]
pub fn cp932_is_healthy() -> bool {
    // NEC / IBM 拡張と、CP932 固有の並びを含める。
    // ASCII だけで往復させても、CP932 が生きているかは分からない。
    const PROBES: [&str; 4] = ["あいうえお", "ガギグゲゴ", "髙﨑", "①②③"];
    PROBES.iter().all(|p| {
        text::encode(p, TextEncoding::Cp932)
            .ok()
            .and_then(|bytes| text::decode(&bytes, TextEncoding::Cp932).ok())
            .is_some_and(|back| back == *p)
    })
}

/// このプロファイルで書き出せるか（`TR-PKG-13`）。
#[must_use]
pub fn is_available(p: Profile) -> bool {
    !p.needs_cp932() || cp932_is_healthy()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定は両対応() {
        assert_eq!(Profile::default(), Profile::Both);
        assert_eq!(Profile::Both.declared_encoding(), "shift_jis");
        assert_eq!(Profile::OpenUtau.declared_encoding(), "utf-8");
    }

    #[test]
    fn 名前を往復できる() {
        for p in [Profile::Classic, Profile::OpenUtau, Profile::Both] {
            assert_eq!(Profile::parse(p.as_str()), Some(p));
        }
        assert_eq!(Profile::parse("なんだこれ"), None);
    }

    #[test]
    fn cp932_の往復が通る() {
        assert!(cp932_is_healthy());
        assert!(is_available(Profile::Classic));
        assert!(is_available(Profile::OpenUtau));
    }
}
