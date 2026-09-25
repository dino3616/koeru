//! 文字列の正規形（`TR-PKG-48`）。
//!
//! 符号化（CP932 / UTF-8 の読み書き）は入出力の側に残る（`koeru_core::text`）。
//! ここは綴りを比べる前に揃える規則だけを持つ。

use unicode_normalization::UnicodeNormalization as _;

/// NFC へ揃える（`TR-PKG-48`）。
///
/// macOS はファイル名を NFD で返す。 揃えないと、同じ「が」が
/// 別の文字列として二重に台帳へ載る。
#[must_use]
pub fn to_nfc(s: &str) -> String {
    s.nfc().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// macOS が NFD で返すファイル名を揃える（`TR-PKG-48`）。
    #[test]
    fn nfd_and_nfc_become_the_same_string() {
        let nfd = "\u{304B}\u{3099}"; // か + 濁点
        let nfc = "が";
        assert_ne!(nfd, nfc, "元は別の文字列");
        assert_eq!(to_nfc(nfd), nfc);
        assert_eq!(to_nfc(nfc), nfc, "既に NFC なら変わらないこと");
    }
}
