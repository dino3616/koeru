//! 取り込んだ `oto.ini` を無損失で持つ（`TR-EDT-39`）。
//!
//! 行ごとに原文のバイト列を持ち、編集していない行はそのバイト列のまま書き戻す。
//! 文字列から作り直さない。 **CP932 には同じ字に2つの符号がある**（NEC 特殊文字の
//! `≒` と JIS X 0208 の `≒`、NEC 選定 IBM 拡張と IBM 拡張の `ⅰ` など）。 読んで
//! 書き直すと、字は同じでもバイトが変わる。
//!
//! 行の終わり（CRLF / LF / CR）も行ごとに持つ。 最後の行が改行で終わっていないこと、
//! UTF-8 の BOM、`#Charset:` の宣言の行（`TR-EDT-38`）もそのまま残る。
//!
//! 編集した行も、触っていない欄は原文の綴りのまま書く。 `80` を `80.000` に揃えない。
//! 変えた欄だけを小数第3位までで書く（`TR-ALN-21`）。 ただし編集した行は文字列から
//! 符号化し直すので、その行に限っては同じ字の別の符号が正規の符号に変わる。

use super::{IniError, num};
use crate::text::{self, TextEncoding};

type Result<T> = std::result::Result<T, IniError>;

const UTF8_BOM: &[u8] = b"\xEF\xBB\xBF";

/// 1行の終わり。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LineEnding {
    CrLf,
    Lf,
    /// CR だけ。 古い Mac の形。
    Cr,
    /// 改行が無い。 ファイルの最後の行だけがこれになる。
    Missing,
}

impl LineEnding {
    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::CrLf => b"\r\n",
            Self::Lf => b"\n",
            Self::Cr => b"\r",
            Self::Missing => b"",
        }
    }
}

/// 5値の欄。 `oto.ini` に並ぶ順で、3番目が右ブランク。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Offset,
    Consonant,
    Cutoff,
    Preutterance,
    Overlap,
}

impl Field {
    /// `oto.ini` に並ぶ順。
    pub const ALL: [Self; 5] = [
        Self::Offset,
        Self::Consonant,
        Self::Cutoff,
        Self::Preutterance,
        Self::Overlap,
    ];

    const fn index(self) -> usize {
        match self {
            Self::Offset => 0,
            Self::Consonant => 1,
            Self::Cutoff => 2,
            Self::Preutterance => 3,
            Self::Overlap => 4,
        }
    }
}

/// 欄の原文と、読んだ値。 空欄は `None`。
#[derive(Debug, Clone, PartialEq)]
struct Slot {
    raw: String,
    value: Option<f64>,
}

/// 解釈できた1行。 `ファイル名=エイリアス,5値`。
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    file: String,
    alias: String,
    slots: [Slot; 5],
    /// 原文から変わったか。 変わっていなければ原文のバイト列を書き戻す。
    edited: bool,
}

impl Entry {
    /// `=` の左。 原文のまま。
    #[must_use]
    pub fn file(&self) -> &str {
        &self.file
    }

    #[must_use]
    pub fn alias(&self) -> &str {
        &self.alias
    }

    /// 欄の値。 空欄は `None` で、`0` の欄とは区別する（`TR-EDT-39`）。
    #[must_use]
    pub fn get(&self, field: Field) -> Option<f64> {
        self.slots[field.index()].value
    }

    /// 欄の原文。 前後の空白も含む。
    #[must_use]
    pub fn raw(&self, field: Field) -> &str {
        &self.slots[field.index()].raw
    }

    /// 取り込んでから何かを変えたか。
    #[must_use]
    pub const fn is_edited(&self) -> bool {
        self.edited
    }

    /// エイリアスを変える。 同じ綴りなら何もしない。
    ///
    /// # Errors
    ///
    /// `,` か改行が入っている。
    pub fn set_alias(&mut self, alias: &str) -> Result<()> {
        if alias.contains([',', '\r', '\n']) {
            return Err(IniError::SeparatorInAlias);
        }
        if alias != self.alias {
            alias.clone_into(&mut self.alias);
            self.edited = true;
        }
        Ok(())
    }

    /// 欄の値を変える。 `None` は空欄。 同じ値なら原文の綴りを残す。
    ///
    /// # Errors
    ///
    /// 有限の数でない。 書くと読み戻せない。
    pub fn set(&mut self, field: Field, value: Option<f64>) -> Result<()> {
        if value.is_some_and(|v| !v.is_finite()) {
            return Err(IniError::NotANumber);
        }
        let slot = &mut self.slots[field.index()];
        if slot.value == value {
            return Ok(());
        }
        slot.raw = value.map(num).unwrap_or_default();
        slot.value = value;
        self.edited = true;
        Ok(())
    }

    fn render(&self) -> String {
        let [a, b, c, d, e] = &self.slots;
        format!(
            "{}={},{},{},{},{},{}",
            self.file, self.alias, a.raw, b.raw, c.raw, d.raw, e.raw
        )
    }

    /// 1行を読む。 形が合わなければ `None`——その行は原文のまま持つ。
    ///
    /// 欄は5つちょうど。 足りない行も多い行も、どう読むかを決めずに残す。
    fn parse(line: &str) -> Option<Self> {
        let (file, rest) = line.split_once('=')?;
        if file.is_empty() {
            return None;
        }
        let mut parts = rest.split(',');
        let alias = parts.next()?;
        let slots: Vec<Slot> = parts
            .by_ref()
            .take(5)
            .map(Slot::parse)
            .collect::<Option<_>>()?;
        let slots: [Slot; 5] = slots.try_into().ok()?;
        if parts.next().is_some() {
            return None;
        }
        Some(Self {
            file: file.to_owned(),
            alias: alias.to_owned(),
            slots,
            edited: false,
        })
    }
}

impl Slot {
    fn parse(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        let value = if trimmed.is_empty() {
            None
        } else {
            let v: f64 = trimmed.parse().ok()?;
            // `nan` や `inf` も f64 としては読めてしまう。 5値としては読まない。
            if !v.is_finite() {
                return None;
            }
            Some(v)
        };
        Some(Self {
            raw: raw.to_owned(),
            value,
        })
    }
}

/// 行の中身。
#[derive(Debug, Clone, PartialEq)]
pub enum LineKind {
    /// 空白だけの行。
    Blank,
    /// `#` で始まる行。 `#Charset:` の宣言もここに入る。
    Comment(String),
    /// 5つの欄の原文を持つぶん、ほかの変種より1桁大きい。 空行やコメントが同じ大きさを
    /// 取らないよう箱に入れる。
    Entry(Box<Entry>),
    /// 解釈できない行。 原文のまま持ち、同じ位置へ書き戻す（`TR-EDT-39`）。
    Uninterpretable(String),
}

/// 1行。 原文のバイト列と行の終わりを持つ。
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    raw: Vec<u8>,
    ending: LineEnding,
    kind: LineKind,
}

impl Line {
    #[must_use]
    pub const fn kind(&self) -> &LineKind {
        &self.kind
    }

    #[must_use]
    pub const fn ending(&self) -> LineEnding {
        self.ending
    }

    /// 取り込んだときの原文。 行の終わりを含まない。
    #[must_use]
    pub fn raw(&self) -> &[u8] {
        &self.raw
    }
}

/// 取り込んだ `oto.ini` の全体（`TR-EDT-39`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    encoding: TextEncoding,
    bom: bool,
    lines: Vec<Line>,
}

impl Document {
    /// バイト列を読む。 符号化は呼び出し側が決める（`TR-EDT-38` は判定を本人が
    /// 上書きできることを求めている）。
    ///
    /// 1行でも指定した符号化で読めなければ失敗させる。 その行だけを原文のまま
    /// 残すと、符号化の判定を誤ったまま黙って読み進める（`TR-EDT-38`）。
    ///
    /// # Errors
    ///
    /// 指定した符号化で読めない行がある。
    #[tracing::instrument(skip_all, fields(enc = encoding.as_str(), len = bytes.len()))]
    pub fn parse(bytes: &[u8], encoding: TextEncoding) -> Result<Self> {
        let (bom, mut rest) = match encoding {
            TextEncoding::Utf8 => bytes
                .strip_prefix(UTF8_BOM)
                .map_or((false, bytes), |b| (true, b)),
            TextEncoding::Cp932 => (false, bytes),
        };
        let mut lines = Vec::new();
        while !rest.is_empty() {
            let (raw, ending, next) = split_line(rest);
            let decoded = text::decode_exact(raw, encoding)?;
            lines.push(Line {
                raw: raw.to_vec(),
                ending,
                kind: classify(&decoded),
            });
            rest = next;
        }
        Ok(Self {
            encoding,
            bom,
            lines,
        })
    }

    /// バイト列に戻す。 編集していない行は原文のバイト列のまま出す。
    ///
    /// # Errors
    ///
    /// 編集した行に、この文書の符号化で書けない文字がある。
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut out = Vec::new();
        if self.bom {
            out.extend_from_slice(UTF8_BOM);
        }
        for line in &self.lines {
            match &line.kind {
                LineKind::Entry(e) if e.edited => {
                    out.extend(text::encode(&e.render(), self.encoding)?);
                }
                _ => out.extend_from_slice(&line.raw),
            }
            out.extend_from_slice(line.ending.bytes());
        }
        Ok(out)
    }

    #[must_use]
    pub const fn encoding(&self) -> TextEncoding {
        self.encoding
    }

    /// 先頭に UTF-8 の BOM があったか。
    #[must_use]
    pub const fn has_bom(&self) -> bool {
        self.bom
    }

    /// 1行目の `#Charset:` が宣言している名前（`TR-EDT-38`）。
    #[must_use]
    pub fn charset_declaration(&self) -> Option<&str> {
        match &self.lines.first()?.kind {
            LineKind::Comment(c) => c.trim().strip_prefix("#Charset:").map(str::trim),
            _ => None,
        }
    }

    #[must_use]
    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    /// エントリと、それが何行目か（0 始まり）。
    pub fn entries(&self) -> impl Iterator<Item = (usize, &Entry)> {
        self.lines
            .iter()
            .enumerate()
            .filter_map(|(i, l)| match &l.kind {
                LineKind::Entry(e) => Some((i, e.as_ref())),
                _ => None,
            })
    }

    /// `index` 行目のエントリ。 エントリでない行なら `None`。
    pub fn entry_mut(&mut self, index: usize) -> Option<&mut Entry> {
        match &mut self.lines.get_mut(index)?.kind {
            LineKind::Entry(e) => Some(e.as_mut()),
            _ => None,
        }
    }
}

/// 先頭の1行を切り出す。 CR・LF・CRLF のどれでも行を終える。
///
/// バイト列のまま切る。 CR と LF は CP932 の2バイト目にも UTF-8 の続きのバイトにも
/// 現れないので、符号化を決める前に切ってよい。
fn split_line(bytes: &[u8]) -> (&[u8], LineEnding, &[u8]) {
    let Some(at) = bytes.iter().position(|b| matches!(b, b'\r' | b'\n')) else {
        return (bytes, LineEnding::Missing, &[]);
    };
    let (line, rest) = bytes.split_at(at);
    match rest {
        [b'\r', b'\n', next @ ..] => (line, LineEnding::CrLf, next),
        [b'\r', next @ ..] => (line, LineEnding::Cr, next),
        [_, next @ ..] => (line, LineEnding::Lf, next),
        [] => unreachable!("位置は区切りの文字を指している"),
    }
}

fn classify(line: &str) -> LineKind {
    if line.trim().is_empty() {
        LineKind::Blank
    } else if line.starts_with('#') {
        LineKind::Comment(line.to_owned())
    } else if let Some(e) = Entry::parse(line) {
        LineKind::Entry(Box::new(e))
    } else {
        LineKind::Uninterpretable(line.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 行はcrとlfとcrlfのどれでも終わる() {
        assert_eq!(
            split_line(b"a\r\nb"),
            (&b"a"[..], LineEnding::CrLf, &b"b"[..])
        );
        assert_eq!(split_line(b"a\nb"), (&b"a"[..], LineEnding::Lf, &b"b"[..]));
        assert_eq!(split_line(b"a\rb"), (&b"a"[..], LineEnding::Cr, &b"b"[..]));
        assert_eq!(split_line(b"a"), (&b"a"[..], LineEnding::Missing, &b""[..]));
        // CR の直後の LF だけを対にする。 LF CR は2つの行の終わり。
        assert_eq!(split_line(b"\n\r"), (&b""[..], LineEnding::Lf, &b"\r"[..]));
    }

    #[test]
    fn 行の種類を分ける() {
        assert_eq!(classify(" \t"), LineKind::Blank);
        assert_eq!(classify("#x"), LineKind::Comment("#x".to_owned()));
        assert!(matches!(classify("a.wav=あ,1,2,3,4,5"), LineKind::Entry(_)));
        for bad in [
            "[a.wav]",
            "=あ,1,2,3,4,5",
            "a.wav=あ,1,2,3,4",
            "a.wav=あ,1,2,3,4,5,6",
            "a.wav=あ,1,2,x,4,5",
            "a.wav=あ,nan,0,0,0,0",
            "a.wav=あ,inf,0,0,0,0",
        ] {
            assert_eq!(
                classify(bad),
                LineKind::Uninterpretable(bad.to_owned()),
                "{bad}"
            );
        }
    }

    /// 空欄と `0` を区別する（`TR-EDT-39`）。 空白だけの欄も空欄。
    #[test]
    fn 空欄と_0_を区別する() {
        let e = Entry::parse("a.wav=,0,, ,-0,").expect("読める");
        assert_eq!(e.alias(), "");
        assert_eq!(e.get(Field::Offset), Some(0.0));
        assert_eq!(e.get(Field::Consonant), None);
        assert_eq!(e.get(Field::Cutoff), None);
        assert_eq!(e.raw(Field::Cutoff), " ");
        assert_eq!(e.get(Field::Overlap), None);
    }

    /// 変えた欄だけを小数第3位で書き、`-0` を出さない。 触っていない欄の綴りは残す。
    #[test]
    fn 変えた欄だけを書き直す() {
        let mut e = Entry::parse("a.wav=あ,80,100,-520,70,23.3333333").expect("読める");
        e.set(Field::Overlap, Some(-0.0)).expect("変えられる");
        e.set(Field::Consonant, None).expect("変えられる");
        assert_eq!(e.render(), "a.wav=あ,80,,-520,70,0.000");
    }

    #[test]
    fn 同じ値を入れても編集にしない() {
        let mut e = Entry::parse("a.wav=あ,80,,-520,70,23").expect("読める");
        e.set(Field::Offset, Some(80.0)).expect("変えられる");
        e.set(Field::Consonant, None).expect("変えられる");
        e.set_alias("あ").expect("変えられる");
        assert!(!e.is_edited());
    }

    #[test]
    fn 読み戻せない値は入れない() {
        let mut e = Entry::parse("a.wav=あ,1,2,3,4,5").expect("読める");
        for v in [f64::NAN, f64::INFINITY] {
            assert!(matches!(
                e.set(Field::Offset, Some(v)),
                Err(IniError::NotANumber)
            ));
        }
        for alias in ["あ,い", "あ\r", "あ\n"] {
            assert!(matches!(
                e.set_alias(alias),
                Err(IniError::SeparatorInAlias)
            ));
        }
        assert!(!e.is_edited(), "拒んだ変更は残さない");
    }
}
