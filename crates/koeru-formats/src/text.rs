//! UTAU 音源ファイルの文字符号化（`TR-PLT-08`, `TR-PKG-48`）。
//!
//! 黙って文字化けさせない。 推定に失敗したら失敗として返し、
//! 本人に符号化を指定させる。読めたことにして進むと、
//! エイリアスが化けたまま配布パッケージに入る。
//!
//! 書けない文字は書き出し前に見つける。 CP932 に無い絵文字や異体字を
//! 音源名やエイリアスに入れたまま書き出すと、受け手の UTAU で化ける。
//!
//! # `encoding_rs` の Shift_JIS と CP932
//!
//! ここで使うのは WHATWG の Shift_JIS で、NEC / IBM 拡張を含む点は CP932 と同じ。
//! 完全に同一ではない（未定義バイトの扱いなどが違う）が、
//! UTAU が読み書きする範囲では一致する。
//!
//! ファイル名を NFC へ揃える口はディレクトリを読むので、`koeru_core::text` に残る。

use encoding_rs::{Encoding, SHIFT_JIS, UTF_8};
use unicode_normalization::UnicodeNormalization as _;

/// 書き出す文字符号化（`TR-PLT-08`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextEncoding {
    /// CP932。既定。UTAU 本体互換。
    #[default]
    Cp932,
    /// UTF-8。OpenUtau など、対応している受け手向け。
    Utf8,
}

impl TextEncoding {
    /// `character.yaml` の `text_file_encoding` などに書く名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cp932 => "shift_jis",
            Self::Utf8 => "utf-8",
        }
    }

    /// 宣言された符号化名から読み取る。知らない名前は `None`。
    ///
    /// 知らない名前を既定へ倒すと、宣言があったこと自体が消えてしまう。
    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim().to_ascii_lowercase().replace(['-', '_'], "");
        match s.as_str() {
            "shiftjis" | "sjis" | "cp932" | "windows31j" | "ms932" => Some(Self::Cp932),
            "utf8" => Some(Self::Utf8),
            _ => None,
        }
    }
}

/// 文字符号化にまつわる失敗。
#[derive(Debug, thiserror::Error)]
pub enum TextError {
    /// CP932 で表現できない文字が入っている（`TR-PLT-08`）。
    ///
    /// どの文字が書けないかを返す。 「書けません」だけでは直しようがない。
    #[error("この符号化で表現できない文字がある")]
    Unencodable {
        /// 書けなかった文字。重複は取り除いてある。
        chars: Vec<char>,
    },

    /// 宣言された符号化では読めなかった（`TR-PLT-08`）。
    ///
    /// 文字化けした状態で黙って読み込まない。
    #[error("宣言された符号化では読めなかった")]
    Undecodable { tried: TextEncoding },
}

/// 書けなかった文字そのものは code にも文言にも入れない。 文字は音源名や歌詞の一部でありうる。
impl koeru_failure::Failure for TextError {
    fn code(&self) -> &'static str {
        match self {
            Self::Unencodable { .. } => "text.unencodable",
            Self::Undecodable { .. } => "text.undecodable",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

type Result<T> = std::result::Result<T, TextError>;

/// 文字列をバイト列にする。
///
/// 表現できない文字があれば、代替に置き換えず失敗させる（`TR-PLT-08`）。
/// 置き換えると、受け手の UTAU で化けたエイリアスがそのまま配られる。
///
/// **往復で見る。失敗の報告だけを信じない。** WHATWG の Shift_JIS は
/// `¥`（U+00A5）を `0x5C` へ、`‾`（U+203E）を `0x7E` へ写す。どちらも
/// 「書けなかった」とは報告されないのに、読み戻すと `\` と `~` になる。
/// **踏むのは書き出したあと**——読み戻し検証は符号化済みのバイト列と
/// 突き合わせるので、この取り違えを見つけられない（`TR-PKG-13` の
/// 「不可逆な変換が起きた位置を呼び出し側へ返し」）。
#[tracing::instrument(skip(s), fields(enc = enc.as_str()))]
pub fn encode(s: &str, enc: TextEncoding) -> Result<Vec<u8>> {
    match enc {
        TextEncoding::Utf8 => Ok(s.as_bytes().to_vec()),
        TextEncoding::Cp932 => {
            let (bytes, _, had_errors) = SHIFT_JIS.encode(s);
            let (back, _, _) = SHIFT_JIS.decode(&bytes);
            if had_errors || back != s {
                return Err(TextError::Unencodable {
                    chars: unencodable_chars(s),
                });
            }
            Ok(bytes.into_owned())
        }
    }
}

/// バイト列を文字列にする。
///
/// 置換文字が出たら失敗として返す（`TR-PLT-08`）。読めたことにして進むと、
/// 化けたまま書き出しへ流れる。
#[tracing::instrument(skip(bytes), fields(enc = enc.as_str(), len = bytes.len()))]
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Result<String> {
    let (text, _, had_errors) = encoding_of(enc).decode(bytes);
    if had_errors {
        return Err(TextError::Undecodable { tried: enc });
    }
    Ok(text.into_owned())
}

/// 指定した符号化だけで読む。 BOM を剥がさず、BOM を見て符号化を替えもしない。
///
/// [`decode`] は `encoding_rs` の BOM 判定を通すので、CP932 を指定しても先頭が
/// `EF BB BF` なら UTF-8 として読む。 どの符号化で読んだかが食い違うと、編集した行を
/// 書き戻すときに別の符号化で書いてしまう。 無損失の往復（`TR-EDT-39`）ではこちらを使う。
pub(crate) fn decode_exact(bytes: &[u8], enc: TextEncoding) -> Result<String> {
    encoding_of(enc)
        .decode_without_bom_handling_and_without_replacement(bytes)
        .map(std::borrow::Cow::into_owned)
        .ok_or(TextError::Undecodable { tried: enc })
}

const fn encoding_of(enc: TextEncoding) -> &'static Encoding {
    match enc {
        TextEncoding::Cp932 => SHIFT_JIS,
        TextEncoding::Utf8 => UTF_8,
    }
}

/// この符号化で書けない文字を挙げる（`TR-PLT-08`, `TR-PKG-13`）。
///
/// 書き出し前に見せて、代替を促すために使う。 重複は取り除く。
///
/// 往復しない文字も挙げる。 `¥` は `0x5C` として書けてしまい、読み戻すと
/// `\` になる。書けたかどうかだけを見ると、別の字に変わったことを見逃す。
#[must_use]
pub fn unencodable_chars(s: &str) -> Vec<char> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for c in s.chars() {
        if seen.contains(&c) {
            continue;
        }
        let mut buf = [0_u8; 4];
        let one: &str = c.encode_utf8(&mut buf);
        let (bytes, _, had_errors) = SHIFT_JIS.encode(one);
        let (back, _, _) = SHIFT_JIS.decode(&bytes);
        if had_errors || back != one {
            seen.insert(c);
            out.push(c);
        }
    }
    out
}

/// CP932 で書ける、いちばん近い形を探す（`TR-PKG-17`）。
///
/// 「書けません」だけでは直しようがない。 書けない字だけを、互換分解して
/// 結合文字を落とした形へ置き換える。それでも書けなければ取り除く。
/// 元と同じなら `None`——直す必要が無い。何も残らなければ `None`——
/// 空の名前を代替案として出さない。
///
/// **書ける字には触らない。** 文字列ごと分解すると、`ガ🎤` が `カ` になる
/// ——`ガ` は `カ` + U+3099 に分解され、結合文字を落とす段で濁点が消える。
/// `①` のような互換文字も別の字に書き換わる。直すべきでないものまで
/// 直した案を出すと、本人はそれが提案だと気づけない。
///
/// **これは提案であって、置換ではない。** 採るかどうかは本人が決める
/// （`TR-PKG-17` が暗黙置換を禁じている）。
#[must_use]
pub fn cp932_fallback(s: &str) -> Option<String> {
    let encodable = |t: &str| unencodable_chars(t).is_empty();
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        let one = c.to_string();
        if encodable(&one) {
            out.push(c);
            continue;
        }
        // 結合文字（濁点・アクセント）を落とす。`é` は `e` になる。
        let folded: String = one
            .nfkd()
            .filter(|c| !matches!(*c as u32, 0x0300..=0x036F | 0x3099 | 0x309A))
            .collect();
        if !folded.is_empty() && encodable(&folded) {
            out.push_str(&folded);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() || trimmed == s {
        None
    } else {
        Some(trimmed.to_owned())
    }
}

/// 取り込むときに使う符号化を決める（`TR-PKG-48`）。
///
/// 順序は `character.yaml` の `text_file_encoding` → `oto.ini` の `#Charset:`
/// → 既定 `shift_jis`。上位で宣言があればそれに従う。
///
/// 宣言はあるが読めない名前だった場合も、既定へ倒す。 ただし
/// 復号に失敗すれば [`decode`] が止めるので、化けたまま進むことはない。
#[must_use]
pub fn resolve_encoding(
    character_yaml_declared: Option<&str>,
    oto_charset_declared: Option<&str>,
) -> TextEncoding {
    character_yaml_declared
        .and_then(TextEncoding::parse)
        .or_else(|| oto_charset_declared.and_then(TextEncoding::parse))
        .unwrap_or(TextEncoding::Cp932)
}

/// `oto.ini` の先頭から `#Charset:` 宣言を拾う（`TR-PKG-48`）。
///
/// バイト列のまま見る。符号化が決まる前なので、文字列にはできない。
/// 宣言は ASCII なので、これで足りる。
#[must_use]
pub fn oto_charset_declaration(bytes: &[u8]) -> Option<String> {
    // 先頭の数行だけを見る。ファイル全体を走査しない。
    let head = &bytes[..bytes.len().min(256)];
    let text = String::from_utf8_lossy(head);
    for line in text.lines().take(4) {
        if let Some(rest) = line.trim().strip_prefix("#Charset:") {
            return Some(rest.trim().to_owned());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    /// 書けたと報告されるのに、読み戻すと別の字になるものがある（`TR-PKG-13`）。
    ///
    /// WHATWG の Shift_JIS は `¥` を `0x5C` へ写す。`had_errors` は立たない。
    #[test]
    fn 往復しない文字を書けないものとして挙げる() {
        for c in ['¥', '‾'] {
            let one = c.to_string();
            assert_eq!(unencodable_chars(&one), vec![c], "{c}");
            assert!(
                matches!(
                    encode(&one, TextEncoding::Cp932),
                    Err(TextError::Unencodable { .. })
                ),
                "{c} を黙って別の字にしない"
            );
        }
    }

    /// 書ける字には触らない（`TR-PKG-17`）。
    ///
    /// 文字列ごと分解すると `ガ` の濁点が消える。**直すべきでないものまで
    /// 直した案を出すと、本人はそれが提案だと気づけない。**
    #[test]
    fn 代替案は書けない字だけを置き換える() {
        assert_eq!(cp932_fallback("ガ🎤").as_deref(), Some("ガ"));
        assert_eq!(cp932_fallback("①🎤").as_deref(), Some("①"));
        assert_eq!(cp932_fallback("café").as_deref(), Some("cafe"));
        assert_eq!(cp932_fallback("こえる🎤").as_deref(), Some("こえる"));
        // 直すところが無ければ提案しない。
        assert_eq!(cp932_fallback("こえる"), None);
        // 何も残らないなら提案しない。空の名前を勧めない。
        assert_eq!(cp932_fallback("🎤"), None);
    }

    /// 往復する字は通す。全部を弾いてしまわないこと。
    #[test]
    fn 往復する字はそのまま通る() {
        for s in ["こえる", "abc", "髙﨑", "①②③", "ガギグゲゴ"] {
            assert!(unencodable_chars(s).is_empty(), "{s}");
            let bytes = encode(s, TextEncoding::Cp932).expect("書けること");
            assert_eq!(decode(&bytes, TextEncoding::Cp932).expect("読めること"), s);
        }
    }

    use super::*;

    #[test]
    fn cp932_round_trips_japanese() {
        let s = "あかさたな";
        let b = encode(s, TextEncoding::Cp932).expect("書けること");
        assert_ne!(b, s.as_bytes(), "UTF-8 とは別のバイト列であること");
        assert_eq!(decode(&b, TextEncoding::Cp932).expect("読めること"), s);
    }

    /// CP932 に無い文字は代替に置き換えず、失敗させる（`TR-PLT-08`）。
    #[test]
    fn unencodable_characters_are_refused_not_substituted() {
        let e = encode("こえる🎤ちゃん", TextEncoding::Cp932).expect_err("拒むこと");
        let TextError::Unencodable { chars } = e else {
            panic!("Unencodable であること");
        };
        assert_eq!(chars, ['🎤'], "どの文字が書けないかを返すこと");
    }

    /// どの文字が書けないかを全部挙げる。 直しようがある形で返す。
    #[test]
    fn every_unencodable_character_is_reported_once() {
        // 𠮷（サロゲートペアの異体字）と絵文字。同じ絵文字を二度入れる。
        let got = unencodable_chars("𠮷🎤野家🎤");
        assert!(got.contains(&'🎤'));
        assert!(got.contains(&'𠮷'));
        assert_eq!(
            got.iter().filter(|c| **c == '🎤').count(),
            1,
            "重複を取り除くこと"
        );
    }

    #[test]
    fn ascii_is_always_encodable() {
        assert!(unencodable_chars("a_1-.wav").is_empty());
    }

    /// 文字化けした状態で黙って読み込まない（`TR-PLT-08`）。
    #[test]
    fn undecodable_bytes_are_refused() {
        // UTF-8 の「あ」を CP932 として読むと壊れる。
        let utf8 = "あ".as_bytes();
        let e = decode(utf8, TextEncoding::Cp932).expect_err("拒むこと");
        assert_eq!(koeru_failure::Failure::code(&e), "text.undecodable");

        // 逆向きも。CP932 の「あ」を UTF-8 として読む。
        let sjis = encode("あ", TextEncoding::Cp932).expect("書ける");
        assert!(decode(&sjis, TextEncoding::Utf8).is_err());
    }

    #[test]
    fn encoding_names_are_read_leniently() {
        for n in [
            "shift_jis",
            "Shift-JIS",
            "SJIS",
            "cp932",
            "MS932",
            "windows-31j",
        ] {
            assert_eq!(TextEncoding::parse(n), Some(TextEncoding::Cp932), "{n}");
        }
        for n in ["utf-8", "UTF8", "utf_8"] {
            assert_eq!(TextEncoding::parse(n), Some(TextEncoding::Utf8), "{n}");
        }
        // 知らない名前は既定へ倒さない。 宣言があったことを消さない。
        assert_eq!(TextEncoding::parse("euc-jp"), None);
    }

    /// 判定の順序は character.yaml → #Charset: → 既定（`TR-PKG-48`）。
    #[test]
    fn encoding_resolution_follows_the_declared_order() {
        assert_eq!(
            resolve_encoding(Some("utf-8"), Some("shift_jis")),
            TextEncoding::Utf8,
            "character.yaml が優先されること"
        );
        assert_eq!(
            resolve_encoding(None, Some("utf-8")),
            TextEncoding::Utf8,
            "次に #Charset:"
        );
        assert_eq!(
            resolve_encoding(None, None),
            TextEncoding::Cp932,
            "既定は shift_jis"
        );
        assert_eq!(
            resolve_encoding(Some("euc-jp"), None),
            TextEncoding::Cp932,
            "読めない宣言も既定へ倒す"
        );
    }

    #[test]
    fn charset_declaration_is_read_from_the_head() {
        let b = b"#Charset:UTF-8\r\n[a.wav]\na=1\n";
        assert_eq!(oto_charset_declaration(b).as_deref(), Some("UTF-8"));
        assert_eq!(oto_charset_declaration(b"[a.wav]\na=1\n"), None);
    }

    /// ファイル全体を走査しない。 何万行もある oto.ini がありうる。
    #[test]
    fn charset_declaration_ignores_the_body() {
        let mut b = b"[a.wav]\n".to_vec();
        b.extend(std::iter::repeat_n(b'x', 10_000));
        b.extend_from_slice(b"\n#Charset:UTF-8\n");
        assert_eq!(
            oto_charset_declaration(&b),
            None,
            "本文の宣言らしき行を拾わないこと"
        );
    }

    /// NFD のままだと CP932 へ書けない（＝揃える必要がある）。
    #[test]
    fn nfd_must_be_normalized_before_encoding() {
        let nfd = "\u{304B}\u{3099}";
        assert!(
            encode(nfd, TextEncoding::Cp932).is_err(),
            "NFD のままでは書けない"
        );
        assert!(
            encode(&koeru_model::text::to_nfc(nfd), TextEncoding::Cp932).is_ok(),
            "NFC にすれば書ける"
        );
    }

    /// 指定した符号化から外れない。 [`decode`] は BOM を見て UTF-8 へ替える。
    #[test]
    fn 厳密な読みは_bom_を剥がさず符号化も替えない() {
        let bom_utf8 = b"\xEF\xBB\xBFa";
        assert_eq!(
            decode(bom_utf8, TextEncoding::Cp932).expect("読める"),
            "a",
            "BOM 判定で UTF-8 に替わる（この差がある）"
        );
        assert_eq!(
            decode_exact(bom_utf8, TextEncoding::Utf8).expect("読める"),
            "\u{FEFF}a",
            "BOM を剥がさない"
        );
        assert!(
            decode_exact(b"\x82", TextEncoding::Cp932).is_err(),
            "途中で切れた2バイト文字を置換文字にしない"
        );
    }
}
