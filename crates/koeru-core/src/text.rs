//! ディレクトリの中のファイル名を NFC に揃える（`TR-REC-32`）。
//!
//! 符号化（CP932 / UTF-8 の読み書き）は `koeru_formats::text` へ移した（`DEC-PLT-034`）。
//! 下の再輸出は既存の `koeru_core::text` の経路を通すためだけにある。 **移行中。**
//! ここに残るのはディレクトリを読み、名前を変えるものだけ。

pub use koeru_formats::text::{
    TextEncoding, TextError, cp932_fallback, decode, encode, oto_charset_declaration,
    resolve_encoding, unencodable_chars,
};
pub use koeru_model::text::to_nfc;

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
#[tracing::instrument(skip(dir))]
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
#[tracing::instrument(skip(dir))]
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
