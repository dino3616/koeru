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

use encoding_rs::{SHIFT_JIS, UTF_8};
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

impl TextError {
    /// 送信してよい種別文字列。
    ///
    /// `Display` も、書けなかった文字そのものも送らない。
    /// 文字は音源名や歌詞の一部でありうる。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Unencodable { .. } => "text.unencodable",
            Self::Undecodable { .. } => "text.undecodable",
        }
    }
}

type Result<T> = std::result::Result<T, TextError>;

/// 文字列をバイト列にする。
///
/// 表現できない文字があれば、代替に置き換えず失敗させる（`TR-PLT-08`）。
/// 置き換えると、受け手の UTAU で化けたエイリアスがそのまま配られる。
#[tracing::instrument(skip(s), fields(enc = enc.as_str()), err)]
pub fn encode(s: &str, enc: TextEncoding) -> Result<Vec<u8>> {
    match enc {
        TextEncoding::Utf8 => Ok(s.as_bytes().to_vec()),
        TextEncoding::Cp932 => {
            let (bytes, _, had_errors) = SHIFT_JIS.encode(s);
            if had_errors {
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
#[tracing::instrument(skip(bytes), fields(enc = enc.as_str(), len = bytes.len()), err)]
pub fn decode(bytes: &[u8], enc: TextEncoding) -> Result<String> {
    let encoding = match enc {
        TextEncoding::Cp932 => SHIFT_JIS,
        TextEncoding::Utf8 => UTF_8,
    };
    let (text, _, had_errors) = encoding.decode(bytes);
    if had_errors {
        return Err(TextError::Undecodable { tried: enc });
    }
    Ok(text.into_owned())
}

/// この符号化で書けない文字を挙げる（`TR-PLT-08`）。
///
/// 書き出し前に見せて、代替を促すために使う。 重複は取り除く。
#[must_use]
pub fn unencodable_chars(s: &str) -> Vec<char> {
    let mut seen = std::collections::BTreeSet::new();
    let mut out = Vec::new();
    for c in s.chars() {
        if seen.contains(&c) {
            continue;
        }
        let mut buf = [0_u8; 4];
        let (_, _, had_errors) = SHIFT_JIS.encode(c.encode_utf8(&mut buf));
        if had_errors {
            seen.insert(c);
            out.push(c);
        }
    }
    out
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

/// NFC へ揃える（`TR-PKG-48`）。
///
/// macOS はファイル名を NFD で返す。 揃えないと、同じ「が」が
/// 別の文字列として二重に台帳へ載る。
#[must_use]
pub fn to_nfc(s: &str) -> String {
    s.nfc().collect()
}

/// ディレクトリの中のファイル名を NFC に揃える（`TR-REC-32`）。
///
/// macOS はファイル作成後の名前を分解形で返すことがある。
/// 揃えないと、同じ「が」が別の文字列として台帳と食い違う。
///
/// 分解形だったものはリネームして直す。返るのは直した数。
///
/// # Errors
///
/// ディレクトリを読めない、またはリネームできないとき。
#[tracing::instrument(skip(dir), err)]
pub fn normalize_names_to_nfc(dir: &std::path::Path) -> std::io::Result<usize> {
    if !dir.is_dir() {
        return Ok(0);
    }
    let mut fixed = 0;
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let nfc = to_nfc(&name);
        if nfc == name {
            continue;
        }
        let from = entry.path();
        let to = dir.join(&nfc);

        // NFC 名で既に「別のファイル」が居るなら触らない。 上書きすると片方が消える。
        //
        // ただし macOS の APFS は名前の正規化の違いを無視して同じファイルを指すので、
        // `exists()` だけで判断すると自分自身とぶつかって直せない。同じファイルなら直す。
        // リネームで、`readdir` が返す綴りが NFC に変わる。
        if to.exists() && !is_same_file(&from, &to)? {
            tracing::warn!("NFC 名で別のファイルがあるので直さない");
            continue;
        }
        std::fs::rename(&from, &to)?;
        fixed += 1;
    }
    Ok(fixed)
}

/// 2つのパスが同じファイルを指しているか。
///
/// macOS では、綴りが違っても同じファイルのことがある。
fn is_same_file(a: &std::path::Path, b: &std::path::Path) -> std::io::Result<bool> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt as _;
        let (x, y) = (std::fs::metadata(a)?, std::fs::metadata(b)?);
        Ok(x.dev() == y.dev() && x.ino() == y.ino())
    }
    #[cfg(not(unix))]
    {
        // Windows は名前の正規化で同じファイルを指すことが無い。
        let _ = (a, b);
        Ok(false)
    }
}

/// ディレクトリの中に、NFC でない名前が残っていないか（`TR-REC-32`）。
///
/// 書き出しの直前に通す。 分解形のまま配ると、受け手の環境で見つからない。
///
/// # Errors
///
/// ディレクトリを読めないとき。
#[tracing::instrument(skip(dir), err)]
pub fn find_non_nfc_names(dir: &std::path::Path) -> std::io::Result<Vec<String>> {
    if !dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        if to_nfc(&name) != name {
            out.push(name);
        }
    }
    out.sort();
    Ok(out)
}

#[cfg(test)]
mod tests {
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
        assert_eq!(e.kind(), "text.undecodable");

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

    /// macOS が NFD で返すファイル名を揃える（`TR-PKG-48`）。
    #[test]
    fn nfd_and_nfc_become_the_same_string() {
        let nfd = "\u{304B}\u{3099}"; // か + 濁点
        let nfc = "が";
        assert_ne!(nfd, nfc, "元は別の文字列");
        assert_eq!(to_nfc(nfd), nfc);
        assert_eq!(to_nfc(nfc), nfc, "既に NFC なら変わらないこと");
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
            encode(&to_nfc(nfd), TextEncoding::Cp932).is_ok(),
            "NFC にすれば書ける"
        );
    }
    /// 分解形のファイル名を NFC へ直す（`TR-REC-32`）。
    #[test]
    fn 分解形のファイル名を直す() {
        let dir = std::env::temp_dir().join(format!("koeru-nfc-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("作れること");

        // 「が」を分解形で書く。
        let nfd = "\u{304B}\u{3099}.wav";
        std::fs::write(dir.join(nfd), b"x").expect("書けること");

        // macOS の APFS は書いた時点で NFC へ寄せることがある。
        // 寄っていれば直すものが無く、寄っていなければ直す。どちらでも最後は NFC。
        let before = find_non_nfc_names(&dir).expect("読めること");
        let fixed = normalize_names_to_nfc(&dir).expect("直せること");
        assert_eq!(fixed, before.len(), "見つけたぶんだけ直すこと");

        assert!(
            find_non_nfc_names(&dir).expect("読めること").is_empty(),
            "書き出しの前に分解形が残らないこと"
        );
        assert!(dir.join("が.wav").is_file(), "NFC の名前で読めること");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn ascii_の名前は触らない() {
        let dir = std::env::temp_dir().join(format!("koeru-nfc-ascii-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("作れること");
        std::fs::write(dir.join("s001_1.wav"), b"x").expect("書けること");

        assert_eq!(normalize_names_to_nfc(&dir).expect("通ること"), 0);
        assert!(find_non_nfc_names(&dir).expect("読めること").is_empty());
        assert!(dir.join("s001_1.wav").is_file());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn 無いディレクトリでも落ちない() {
        let missing = std::path::Path::new("/存在しない/場所");
        assert_eq!(normalize_names_to_nfc(missing).expect("通ること"), 0);
        assert!(find_non_nfc_names(missing).expect("通ること").is_empty());
    }
}
