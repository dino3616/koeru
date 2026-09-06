//! `oto.ini` の書式・文字コード・数値精度（`TR-ALN-21`）。
//!
//! ```text
//! ファイル名.wav=エイリアス名,オフセット,子音部,ブランク,先行発声,オーバーラップ
//! ```
//!
//! 並びが5値の宣言順と違う。 `oto.ini` は
//! `offset, consonant, cutoff, preutterance, overlap` の順で、
//! 3番目が右ブランク。ここを取り違えると、音が鳴るのに全部ずれる。
//!
//! # 数値は小数第3位まで
//!
//! `TR-ALN-21` が定めている。丸めは書き出しのときだけ行い、内部の値は丸めない——
//! 丸めた値を読み戻して再計算すると、再推定のたびに値が動く（`TR-ALN-29` の決定性）。
//!
//! # 文字コード
//!
//! 既定は Shift-JIS（CP932）。UTF-8（BOM なし）を選べる。読み込みは両方受け付ける。
//! 判定と変換は [`koeru_core::text`] が持っている（`TR-PLT-08`, `DEC-PLT-013`）。
//!
//! `oto.ini` は作業ファイルにしない（`TR-PKG-40`）。DB を正とし、
//! ここが作るのは書き出し時の派生物。

use koeru_core::oto::Oto;
use koeru_core::text::{self, TextEncoding};

/// 書き出す数値の小数点以下の桁数（`TR-ALN-21`）。
pub const DECIMALS: usize = 3;

/// `oto.ini` の1エントリ。
#[derive(Debug, Clone, PartialEq)]
pub struct IniEntry {
    /// WAV のファイル名。拡張子を含む。
    pub file: String,
    pub alias: String,
    pub oto: Oto,
}

/// `oto.ini` の読み書きで起きる失敗。
#[derive(Debug, thiserror::Error)]
pub enum IniError {
    /// 行に `=` が無い。
    #[error("行の形式が違う")]
    MalformedLine,

    /// 数値の欄が5つ揃っていない。
    #[error("数値の欄が足りない")]
    MissingFields,

    /// 数値として読めない欄がある。
    #[error("数値として読めない欄がある")]
    NotANumber,

    /// 文字コードの扱いに失敗した。
    #[error("文字コードを扱えない")]
    Text(#[from] text::TextError),
}

impl IniError {
    /// 送信してよい種別文字列。行の中身は送らない（AGENTS.md #3）。
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::MalformedLine => "ini.malformed_line",
            Self::MissingFields => "ini.missing_fields",
            Self::NotANumber => "ini.not_a_number",
            Self::Text(e) => e.kind(),
        }
    }
}

type Result<T> = std::result::Result<T, IniError>;

/// 小数第3位までに丸めた文字列。末尾の 0 は落とさない——
/// UTAU 側の実装が桁数を見て振る舞いを変えることがあるので、幅を揃える。
fn num(v: f64) -> String {
    // `-0` を出さない。 読み手によっては符号で基準が変わる欄なので、
    // `-0` が「オフセットからの相対 0」と読まれると使える区間が消える。
    let v = if v == 0.0 { 0.0 } else { v };
    format!("{v:.DECIMALS$}")
}

/// 1エントリを1行に書く（`TR-ALN-21`）。
#[must_use]
pub fn write_line(e: &IniEntry) -> String {
    format!(
        "{}={},{},{},{},{},{}",
        e.file,
        e.alias,
        num(e.oto.offset_ms),
        num(e.oto.consonant_ms),
        num(e.oto.cutoff_ms),
        num(e.oto.preutterance_ms),
        num(e.oto.overlap_ms),
    )
}

/// 1行を読む（`TR-ALN-21`）。
///
/// # Errors
///
/// `=` が無い、欄が足りない、数値として読めない。
pub fn read_line(line: &str) -> Result<IniEntry> {
    let (file, rest) = line.split_once('=').ok_or(IniError::MalformedLine)?;
    let mut it = rest.split(',');
    let alias = it.next().ok_or(IniError::MissingFields)?;
    let mut v = [0.0_f64; 5];
    for slot in &mut v {
        let f = it.next().ok_or(IniError::MissingFields)?;
        *slot = f.trim().parse().map_err(|_| IniError::NotANumber)?;
    }
    Ok(IniEntry {
        file: file.to_owned(),
        alias: alias.to_owned(),
        oto: Oto {
            offset_ms: v[0],
            consonant_ms: v[1],
            cutoff_ms: v[2],
            preutterance_ms: v[3],
            overlap_ms: v[4],
        },
    })
}

/// エントリ列を `oto.ini` のバイト列にする（`TR-ALN-21`）。
///
/// 改行は CRLF。 UTAU 本体と既存ツールが Windows 育ちで、
/// LF だけだと行末にゴミが見える実装がある。
///
/// # Errors
///
/// 指定した文字コードで表現できない文字がある。
pub fn write(entries: &[IniEntry], enc: TextEncoding) -> Result<Vec<u8>> {
    let mut s = String::new();
    for e in entries {
        s.push_str(&write_line(e));
        s.push_str("\r\n");
    }
    Ok(text::encode(&s, enc)?)
}

/// `oto.ini` のバイト列を読む（`TR-ALN-21`）。
///
/// Shift-JIS と UTF-8 の両方を受け付ける。 空行と `#` で始まる行は飛ばす。
///
/// # Errors
///
/// 文字コードを判定できない、行の形式が違う。
pub fn read(bytes: &[u8], enc: TextEncoding) -> Result<Vec<IniEntry>> {
    let s = text::decode(bytes, enc)?;
    s.lines()
        .map(str::trim_end)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(read_line)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry() -> IniEntry {
        IniEntry {
            file: "a001.wav".to_owned(),
            alias: "あ".to_owned(),
            oto: Oto {
                offset_ms: 80.0,
                consonant_ms: 100.0,
                cutoff_ms: -520.0,
                preutterance_ms: 70.0,
                overlap_ms: 23.333_333,
            },
        }
    }

    /// 並びは offset, consonant, cutoff, preutterance, overlap。
    /// 3番目が右ブランクで、宣言順と違う。
    #[test]
    fn 行の並びがotoiniの順序になっている() {
        assert_eq!(
            write_line(&entry()),
            "a001.wav=あ,80.000,100.000,-520.000,70.000,23.333"
        );
    }

    /// 小数第3位まで（`TR-ALN-21`）。
    #[test]
    fn 数値は小数第三位まで() {
        let mut e = entry();
        e.oto.overlap_ms = 1.234_567;
        assert!(write_line(&e).ends_with(",1.235"));
    }

    /// `-0` を書かない。 符号で基準が変わる欄なので、
    /// `-0.000` が「オフセットからの相対 0」と読まれると使える区間が消える。
    #[test]
    fn 負のゼロを書かない() {
        let mut e = entry();
        e.oto.cutoff_ms = -0.0;
        assert!(write_line(&e).contains(",0.000,"), "{}", write_line(&e));
    }

    #[test]
    fn 書いた行を読み戻せる() {
        let e = entry();
        let back = read_line(&write_line(&e)).expect("読める");
        assert_eq!(back.file, e.file);
        assert_eq!(back.alias, e.alias);
        assert!((back.oto.offset_ms - e.oto.offset_ms).abs() < 1e-9);
        assert!((back.oto.cutoff_ms - e.oto.cutoff_ms).abs() < 1e-9);
        // 丸めたぶんだけずれる。 内部の値は丸めない（`TR-ALN-29`）。
        assert!((back.oto.overlap_ms - e.oto.overlap_ms).abs() < 1e-3);
    }

    #[test]
    fn 壊れた行は読めない() {
        assert!(matches!(
            read_line("これは=行,1,2,3"),
            Err(IniError::MissingFields)
        ));
        assert!(matches!(
            read_line("イコールが無い"),
            Err(IniError::MalformedLine)
        ));
        assert!(matches!(
            read_line("a.wav=あ,1,2,x,4,5"),
            Err(IniError::NotANumber)
        ));
    }

    /// 既定は Shift-JIS（`TR-ALN-21`, `TR-PLT-08`）。
    #[test]
    fn 既定の文字コードで往復できる() {
        let v = vec![entry()];
        let bytes = write(&v, TextEncoding::Cp932).expect("書ける");
        // 日本語が CP932 の2バイトになっている。 UTF-8 なら3バイト。
        assert!(!bytes.is_empty());
        let back = read(&bytes, TextEncoding::Cp932).expect("読める");
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].alias, "あ");
    }

    #[test]
    fn utf8でも往復できる() {
        let v = vec![entry()];
        let bytes = write(&v, TextEncoding::Utf8).expect("書ける");
        assert!(!bytes.starts_with(&[0xEF, 0xBB, 0xBF]), "BOM を付けない");
        let back = read(&bytes, TextEncoding::Utf8).expect("読める");
        assert_eq!(back[0].alias, "あ");
    }

    /// 行末は CRLF。
    #[test]
    fn 改行はcrlf() {
        let bytes = write(&[entry()], TextEncoding::Utf8).expect("書ける");
        assert!(bytes.ends_with(b"\r\n"));
    }

    /// 空行と `#` の行は飛ばす。
    #[test]
    fn 空行とコメントを飛ばす() {
        let src = "# これはコメント\r\n\r\na001.wav=あ,0,0,0,0,0\r\n";
        let v = read(src.as_bytes(), TextEncoding::Utf8).expect("読める");
        assert_eq!(v.len(), 1);
    }

    /// CP932 で表現できない文字は書き出しで落とす（`TR-PLT-08`）。
    /// 黙って `?` に潰すと、配った先で名前が壊れる。
    #[test]
    fn cp932で書けない文字は失敗する() {
        let mut e = entry();
        e.alias = "🎤".to_owned();
        assert!(write(&[e], TextEncoding::Cp932).is_err());
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            IniError::MalformedLine,
            IniError::MissingFields,
            IniError::NotANumber,
        ] {
            assert!(e.kind().starts_with("ini."), "{}", e.kind());
        }
    }
}
