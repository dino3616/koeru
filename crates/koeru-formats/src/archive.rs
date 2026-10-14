//! ZIP の符号化と読み戻し検証（`TR-PKG-26`, `TR-PKG-27`, `TR-PKG-52`, `DEC-PKG-009`）。
//!
//! 受け取るのは `Write + Seek` / `Read + Seek` とバイト列で、パスは持たない。
//! どのファイルを入れるか、どの符号化で読むかは呼び出し側（`koeru-package`）が決める。
//!
//! # エントリ名は ASCII に閉じている
//!
//! `DEC-PKG-009`。CP932 と UTF-8 でバイト列が同じなので、プロファイルで
//! 変えるものが無い。EFS フラグも立てない——ASCII しか無いところに
//! 「UTF-8 でないと読めない」と宣言することになる。
//!
//! # 書いたら必ず読み戻す
//!
//! `TR-PKG-52` / `INV-PKG-103`。1つでも食い違えば失敗として扱う。
//! 読み戻しに通らなかったものを消すのは、書いた側（ファイルを持つ側）の仕事。
//!
//! # 同じ入力から同じバイト列が出る
//!
//! エントリの並びは渡した順で、更新時刻は `zip` の既定（時計を読まない）のまま。
//! 2回書き出した ZIP は一致する（`TR-EDT-41`）。

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};

use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::text::{self, TextEncoding};
use crate::wav;

/// UAR のルート直下に置くインストール指示のファイル名（`TR-PKG-09`）。
pub const INSTALL_FILE: &str = "install.txt";

/// 圧縮方式（`TR-PKG-27`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// 無圧縮。`.wav` と `.frq`。
    Store,
    /// Deflate。テキスト・YAML・画像。
    Deflate,
}

/// アーカイブの符号化と読み戻しが失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// 読み書きが失敗した。
    #[error("ファイル操作が失敗した")]
    Io(#[from] std::io::Error),

    /// ZIP の組み立てが失敗した。
    #[error("ZIP を組み立てられない")]
    Zip(#[from] zip::result::ZipError),

    /// 読み戻し検証に通らなかった（`TR-PKG-52`）。
    #[error("読み戻し検証に通らなかった")]
    Verification { problem: VerifyProblem },
}

impl koeru_failure::Failure for ArchiveError {
    fn code(&self) -> &'static str {
        match self {
            Self::Io(_) => "archive.io",
            Self::Zip(_) => "archive.zip",
            Self::Verification { problem } => problem.kind(),
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Io(e) | Self::Zip(zip::result::ZipError::Io(e)) => koeru_failure::io_class(e),
            // 組み立てているのは KOERU。書いたばかりの ZIP が読み戻せないのも同じ。
            Self::Zip(_) | Self::Verification { .. } => koeru_failure::Class::Internal,
        }
    }
}

/// 読み戻し検証で見つかった食い違い（`TR-PKG-52`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VerifyProblem {
    /// エントリ名が ASCII の外に出ている（`DEC-PKG-009`）。
    NonAsciiEntryName,
    /// EFS フラグが立っている（`DEC-PKG-009`）。
    UnexpectedEfsFlag,
    /// 音源ルートフォルダが1つに収まっていない（`TR-PKG-26`）。
    NotSingleRoot,
    /// エントリ名に `..` が入っている（`TR-PKG-26`）。
    TraversingEntry,
    /// 除外するはずのものが入っている（`TR-PKG-26`）。
    ExcludedEntry,
    /// 入っているはずのファイルが無い。
    MissingFile,
    /// 入れたはずのないファイルが入っている。
    UnexpectedFile,
    /// テキストを指定した符号化で読み戻せない、または内容が一致しない。
    TextMismatch,
    /// 配布 WAV が 44100 Hz / 16 bit / モノラルでない（`TR-PKG-20`）。
    WrongWavFormat,
    /// 中心ディレクトリを読めない。
    MalformedArchive,
}

impl VerifyProblem {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NonAsciiEntryName => "archive.non_ascii_entry_name",
            Self::UnexpectedEfsFlag => "archive.unexpected_efs_flag",
            Self::NotSingleRoot => "archive.not_single_root",
            Self::TraversingEntry => "archive.traversing_entry",
            Self::ExcludedEntry => "archive.excluded_entry",
            Self::MissingFile => "archive.missing_file",
            Self::UnexpectedFile => "archive.unexpected_file",
            Self::TextMismatch => "archive.text_mismatch",
            Self::WrongWavFormat => "archive.wrong_wav_format",
            Self::MalformedArchive => "archive.malformed",
        }
    }
}

type Result<T, E = ArchiveError> = std::result::Result<T, E>;

/// 音源ルートフォルダを被せたエントリ名（`TR-PKG-26`）。
#[must_use]
pub fn entry_name(root: &str, path: &str) -> String {
    format!("{root}/{path}")
}

/// アーカイブに入れる1ファイル。
#[derive(Debug, Clone)]
pub struct Entry<'a> {
    /// 音源ルートフォルダを基点にした相対パス。区切りは `/`。
    pub path: &'a str,
    pub compression: Compression,
    /// 中身。 借りられるものは借りる——`.frq` を複製しない。
    pub body: Cow<'a, [u8]>,
}

/// アーカイブを1つ書く（`TR-PKG-26`, `TR-PKG-27`）。
///
/// エントリ名は `<配布名>/<相対パス>`。`install` を渡すと、その手前に
/// `install.txt` を置く（`TR-PKG-09` の二層構造）。
///
/// `entries` は1つずつ取り出して書く。 素材を載せるときは、取り出すたびに読んで
/// 渡せば、全部を一度に持たずに済む（3時間の収録では数百 MB になる）。 取り出す側の
/// 失敗は `E` のまま返る。
///
/// 書き終えた `dest` を返す。 flush や fsync が要る呼び出し側のために。
///
/// # Errors
///
/// `entries` が失敗を返した、書けない。
pub fn write<'a, W, E>(
    dest: W,
    root: &str,
    install: Option<&[u8]>,
    entries: impl IntoIterator<Item = Result<Entry<'a>, E>>,
) -> Result<W, E>
where
    W: Write + Seek,
    E: From<ArchiveError>,
{
    let mut zip = ZipWriter::new(dest);
    if let Some(bytes) = install {
        zip.start_file(INSTALL_FILE, options(Compression::Deflate))
            .map_err(ArchiveError::from)?;
        zip.write_all(bytes).map_err(ArchiveError::from)?;
    }
    for entry in entries {
        let entry = entry?;
        zip.start_file(entry_name(root, entry.path), options(entry.compression))
            .map_err(ArchiveError::from)?;
        zip.write_all(&entry.body).map_err(ArchiveError::from)?;
    }
    Ok(zip.finish().map_err(ArchiveError::from)?)
}

fn options(c: Compression) -> SimpleFileOptions {
    let method = match c {
        Compression::Store => CompressionMethod::Stored,
        Compression::Deflate => CompressionMethod::Deflated,
    };
    SimpleFileOptions::default().compression_method(method)
}

/// 読み戻しが突き合わせる、入れたはずのもの（`TR-PKG-52`）。
#[derive(Debug, Clone)]
pub struct Expect<'a> {
    /// 音源ルートフォルダ名。
    pub root: &'a str,
    /// `install.txt` のバイト列。 UAR だけが持つ。
    pub install: Option<&'a [u8]>,
    /// 入れたはずのファイル。 `Some` のバイト列は、読み戻したものと一致しなければならない。
    ///
    /// 復号できるかだけを見ない。 それでは、別の内容でも正しい CP932 で
    /// ありさえすれば通る——改行が変わった `oto.ini` も、古いままの
    /// `character.txt` も素通りする（`TR-PKG-52` の「内部モデルと一致する」）。
    pub files: Vec<(&'a str, Option<&'a [u8]>)>,
    /// 音源ルート直下に必ず入るファイル（`TR-PKG-01`, `TR-PKG-50`）。
    pub required: &'a [&'a str],
    /// 配布物に混ぜてはいけない名前（`TR-PKG-26`）。
    ///
    /// ディレクトリを列挙しないので普通は出てこない。 それでも見るのは、
    /// 素材のフォルダに OS が勝手に置くものを、将来だれかが拾ってきたときに
    /// ここで止めるため。
    pub excluded: &'a [&'a str],
}

/// 書いたアーカイブを読み戻して確かめる（`TR-PKG-52`）。
///
/// 見るのは5点。 エントリ名の符号化、EFS フラグ、階層、必須ファイル、
/// テキストの再デコード。加えて配布 WAV のヘッダを見る——
/// `TR-PKG-49` の「サンプリング周波数 ≠ 44100 / 量子化ビット数 ≠ 16」は、
/// 素材ではなく出したものについての条件。
///
/// `text_encoding` はエントリ名（`<配布名>/` を含む）から、テキストとして
/// 読み戻すときの符号化を返す。 テキストでなければ `None` で、中身は読まない。
///
/// # Errors
///
/// 読めない、または5点のどれかが食い違う。
pub fn verify<R: Read + Seek>(
    mut source: R,
    expect: &Expect<'_>,
    text_encoding: impl Fn(&str) -> Option<TextEncoding>,
) -> Result<()> {
    let root = expect.root;
    let expect_install = expect.install.is_some();
    // (1)(2) 中心ディレクトリを直接見る。 crate の解釈ではなく、
    // 実際に書いたバイト列でフラグと名前を確かめる。
    for (name, flags) in central_directory(&mut source)? {
        if !name.is_ascii() {
            return Err(fail(VerifyProblem::NonAsciiEntryName));
        }
        if flags & EFS_FLAG != 0 {
            return Err(fail(VerifyProblem::UnexpectedEfsFlag));
        }
    }

    // 入れたはずのバイト列。 読み戻しはこれと突き合わせる。
    let mut expected: BTreeMap<String, &[u8]> = BTreeMap::new();
    for (path, bytes) in &expect.files {
        if let Some(bytes) = bytes {
            expected.insert(entry_name(root, path), bytes);
        }
    }
    if let Some(bytes) = expect.install {
        expected.insert(INSTALL_FILE.to_owned(), bytes);
    }

    // **アーカイブを丸ごと読み込まない。** 275MB の配布物で ZIP と UAR を
    // 順に検証すると、そのぶんが常駐に乗る（`BUDGET-MEMORY-001`）。
    // 突き合わせが要るのはテキストだけで、WAV は先頭 44 バイトで足りる。
    source.seek(SeekFrom::Start(0))?;
    let mut archive = ZipArchive::new(BufReader::new(source))?;
    let mut seen: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name()?.into_owned();
        seen.push(name.clone());

        if name.contains("..") {
            return Err(fail(VerifyProblem::TraversingEntry));
        }
        if name.split('/').any(|seg| expect.excluded.contains(&seg)) {
            return Err(fail(VerifyProblem::ExcludedEntry));
        }
        // (3) 単層構造。 install.txt だけがルート直下に出る。
        let is_install = expect_install && name == INSTALL_FILE;
        if !is_install && !name.starts_with(&format!("{root}/")) {
            return Err(fail(VerifyProblem::NotSingleRoot));
        }

        // 読む量をエントリごとに決める。 WAV はヘッダだけ、テキストは全部、
        // `.frq` と画像は読まない。
        if name.ends_with(".wav") {
            let mut head = [0_u8; wav::HEADER_LEN];
            entry
                .read_exact(&mut head)
                .map_err(|_| fail(VerifyProblem::WrongWavFormat))?;
            check_wav_header(&head)?;
            continue;
        }
        let Some(enc) = text_encoding(&name) else {
            continue;
        };

        let mut body = Vec::new();
        std::io::copy(&mut entry, &mut body)?;
        // (5) テキストを指定した符号化で読み戻し、内部モデルと一致するか。
        //
        // バイト列の一致と復号の両方を見る。 一致だけでは「宣言した符号化で
        // 読めるか」が分からず、復号だけでは「同じ内容か」が分からない。
        if let Some(bytes) = expected.get(name.as_str())
            && *bytes != body.as_slice()
        {
            return Err(fail(VerifyProblem::TextMismatch));
        }
        if text::decode(&body, enc).is_err() {
            return Err(fail(VerifyProblem::TextMismatch));
        }
    }

    // (4) 必須ファイルと、入れたものが全部入っているか。
    let mut want: Vec<String> = expect
        .files
        .iter()
        .map(|(path, _)| entry_name(root, path))
        .collect();
    if expect_install {
        want.push(INSTALL_FILE.to_owned());
    }
    for w in &want {
        if !seen.contains(w) {
            return Err(fail(VerifyProblem::MissingFile));
        }
    }
    for s in &seen {
        if !want.contains(s) {
            return Err(fail(VerifyProblem::UnexpectedFile));
        }
    }
    for r in expect.required {
        if !seen.contains(&entry_name(root, r)) {
            return Err(fail(VerifyProblem::MissingFile));
        }
    }
    Ok(())
}

/// 汎用フラグの 11 ビット目。エントリ名が UTF-8 であることの宣言。
pub const EFS_FLAG: u16 = 1 << 11;

/// 書き出したアーカイブのエントリ名を、外から確かめられるようにしておく。
///
/// 試験と、既に配ったものを見直すときに使う。
///
/// # Errors
///
/// 読めない、中心ディレクトリが壊れている。
pub fn entry_names<R: Read + Seek>(source: &mut R) -> Result<Vec<String>> {
    Ok(central_directory(source)?
        .into_iter()
        .map(|(n, _)| n)
        .collect())
}

/// 中心ディレクトリから、エントリ名と汎用フラグを取り出す。
///
/// 局所ヘッダを頭から探さない。 `PK\x03\x04` は無圧縮で入れた WAV の
/// 中にも現れうるので、署名を探して歩くと別のところで止まる。
///
/// **末尾と中心ディレクトリだけを読む。** アーカイブ全体を載せると、
/// 275MB の配布物でそのぶんが常駐に乗る。EOCD はコメントを含めても
/// 末尾 64KiB + 22 バイトに収まる。
///
/// # Errors
///
/// 読めない、中心ディレクトリが壊れている。
pub fn central_directory<R: Read + Seek>(file: &mut R) -> Result<Vec<(String, u16)>> {
    const EOCD: &[u8; 4] = b"PK\x05\x06";
    const CD: &[u8; 4] = b"PK\x01\x02";
    /// EOCD の固定長 22 バイトと、コメントの上限 65535。
    const EOCD_SEARCH: u64 = 22 + 65_535;

    let len = file.seek(SeekFrom::End(0))?;
    let tail_at = len.saturating_sub(EOCD_SEARCH);
    file.seek(SeekFrom::Start(tail_at))?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail)?;

    let eocd = tail
        .windows(4)
        .rposition(|w| w == EOCD)
        .ok_or_else(malformed)?;
    let u16_at = |buf: &[u8], o: usize| -> Result<u16> {
        Ok(u16::from_le_bytes([
            *buf.get(o).ok_or_else(malformed)?,
            *buf.get(o + 1).ok_or_else(malformed)?,
        ]))
    };
    let u32_at = |buf: &[u8], o: usize| -> Result<u32> {
        Ok(u32::from_le_bytes([
            *buf.get(o).ok_or_else(malformed)?,
            *buf.get(o + 1).ok_or_else(malformed)?,
            *buf.get(o + 2).ok_or_else(malformed)?,
            *buf.get(o + 3).ok_or_else(malformed)?,
        ]))
    };
    let count = u16_at(&tail, eocd + 10)?;
    let cd_size = u32_at(&tail, eocd + 12)? as usize;
    let cd_at = u64::from(u32_at(&tail, eocd + 16)?);

    file.seek(SeekFrom::Start(cd_at))?;
    let mut cd = vec![0_u8; cd_size];
    file.read_exact(&mut cd).map_err(|_| malformed())?;

    let mut out = Vec::with_capacity(count as usize);
    let mut at = 0_usize;
    for _ in 0..count {
        if cd.get(at..at + 4) != Some(CD) {
            return Err(malformed());
        }
        let flags = u16_at(&cd, at + 8)?;
        let name_len = u16_at(&cd, at + 28)? as usize;
        let extra_len = u16_at(&cd, at + 30)? as usize;
        let comment_len = u16_at(&cd, at + 32)? as usize;
        let name = cd.get(at + 46..at + 46 + name_len).ok_or_else(malformed)?;
        out.push((String::from_utf8_lossy(name).into_owned(), flags));
        at += 46 + name_len + extra_len + comment_len;
    }
    Ok(out)
}

fn malformed() -> ArchiveError {
    fail(VerifyProblem::MalformedArchive)
}

const fn fail(problem: VerifyProblem) -> ArchiveError {
    ArchiveError::Verification { problem }
}

/// 配布 WAV のヘッダを見る（`TR-PKG-20`, `TR-PKG-49`）。
fn check_wav_header(head: &[u8; wav::HEADER_LEN]) -> Result<()> {
    match wav::parse_header(head) {
        Ok(info) if info.is_distribution() => Ok(()),
        _ => Err(fail(VerifyProblem::WrongWavFormat)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::TextEncoding::{Cp932, Utf8};
    use koeru_failure::Failure as _;
    use std::io::Cursor;

    const REQUIRED: [&str; 3] = ["character.txt", "character.yaml", "readme.txt"];
    const EXCLUDED: [&str; 2] = ["__MACOSX", ".DS_Store"];

    fn entry<'a>(path: &'a str, compression: Compression, body: &'a [u8]) -> Entry<'a> {
        Entry {
            path,
            compression,
            body: Cow::Borrowed(body),
        }
    }

    fn encoding(name: &str) -> Option<TextEncoding> {
        if name.ends_with(".wav") || name.ends_with(".frq") {
            None
        } else if name.ends_with("character.yaml") {
            Some(Utf8)
        } else {
            Some(Cp932)
        }
    }

    /// 必須の3つと WAV と `.frq`。 `character.txt` は CP932 の「あ」を含む。
    fn files() -> Vec<(String, Compression, Vec<u8>)> {
        let wav = wav::distribution_bytes(&[0.1; 4410], false);
        vec![
            ("a.wav".to_owned(), Compression::Store, wav),
            ("a.frq".to_owned(), Compression::Store, vec![0_u8; 40]),
            (
                "character.txt".to_owned(),
                Compression::Deflate,
                b"name=\x82\xa0\r\n".to_vec(),
            ),
            (
                "character.yaml".to_owned(),
                Compression::Deflate,
                b"a: 1\n".to_vec(),
            ),
            (
                "readme.txt".to_owned(),
                Compression::Deflate,
                b"readme\r\n".to_vec(),
            ),
        ]
    }

    fn zip_of(files: &[(String, Compression, Vec<u8>)], install: Option<&[u8]>) -> Vec<u8> {
        let entries = files
            .iter()
            .map(|(p, c, b)| Ok::<_, ArchiveError>(entry(p, *c, b)));
        write(Cursor::new(Vec::new()), "koeru", install, entries)
            .expect("書ける")
            .into_inner()
    }

    fn expect_of<'a>(
        files: &'a [(String, Compression, Vec<u8>)],
        install: Option<&'a [u8]>,
    ) -> Expect<'a> {
        Expect {
            root: "koeru",
            install,
            files: files
                .iter()
                .map(|(p, _, b)| {
                    let text = encoding(&format!("koeru/{p}")).map(|_| b.as_slice());
                    (p.as_str(), text)
                })
                .collect(),
            required: &REQUIRED,
            excluded: &EXCLUDED,
        }
    }

    fn check(bytes: &[u8], expect: &Expect<'_>) -> Result<()> {
        verify(Cursor::new(bytes), expect, encoding)
    }

    #[test]
    fn 書いて読み戻せる() {
        let f = files();
        let bytes = zip_of(&f, None);
        check(&bytes, &expect_of(&f, None)).expect("通る");
        let names = entry_names(&mut Cursor::new(&bytes)).expect("読める");
        assert_eq!(names.len(), f.len());
        assert!(names.iter().all(|n| n.starts_with("koeru/")));
    }

    /// 同じ入力からは同じバイト列が出る（`TR-EDT-41`）。
    #[test]
    fn 同じ入力から同じバイト列が出る() {
        let f = files();
        assert_eq!(
            zip_of(&f, Some(b"type=voiceset\r\n")),
            zip_of(&f, Some(b"type=voiceset\r\n"))
        );
    }

    #[test]
    fn install_は手前に置かれる() {
        let f = files();
        let install = b"type=voiceset\r\n";
        let bytes = zip_of(&f, Some(install));
        let names = entry_names(&mut Cursor::new(&bytes)).expect("読める");
        assert_eq!(names[0], INSTALL_FILE);
        check(&bytes, &expect_of(&f, Some(install))).expect("通る");
    }

    /// `TR-PKG-27`。音は無圧縮、テキストは Deflate。
    #[test]
    fn 圧縮方式は指定どおりになる() {
        let f = files();
        let bytes = zip_of(&f, None);
        let mut zip = ZipArchive::new(Cursor::new(bytes)).expect("読める");
        for i in 0..zip.len() {
            let e = zip.by_index(i).expect("エントリを読める");
            let name = e.name().expect("名前を読める");
            let want = if name.ends_with(".wav") || name.ends_with(".frq") {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            assert_eq!(e.compression(), want, "{name}");
        }
    }

    /// `DEC-PKG-009`。エントリ名は ASCII で、EFS フラグを立てない。
    #[test]
    fn efs_フラグを立てない() {
        let f = files();
        let bytes = zip_of(&f, None);
        for (name, flags) in central_directory(&mut Cursor::new(&bytes)).expect("読める") {
            assert!(name.is_ascii(), "{name}");
            assert_eq!(flags & EFS_FLAG, 0, "{name}");
        }
    }

    /// 取り出す側の失敗は、そのまま返る。
    #[test]
    fn 取り出す側の失敗は変えずに返す() {
        #[derive(Debug)]
        struct Stop;
        impl From<ArchiveError> for Stop {
            fn from(_: ArchiveError) -> Self {
                Self
            }
        }
        let body = [0_u8];
        let entries = vec![Ok(entry("a.wav", Compression::Store, &body)), Err(Stop)];
        let e = write(Cursor::new(Vec::new()), "koeru", None, entries).expect_err("止まる");
        assert!(matches!(e, Stop));
    }

    #[test]
    fn 必須ファイルが無ければ落ちる() {
        let mut f = files();
        f.retain(|(p, _, _)| p != "character.txt");
        let e = check(&zip_of(&f, None), &expect_of(&f, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.missing_file");
    }

    /// `TR-PKG-52` (5)。読み戻したテキストが、入れたものと違えば落ちる。
    ///
    /// 復号できるかだけを見ていると、別の内容でも正しい CP932 なら通る。
    #[test]
    fn 中身が違えば読み戻しで落ちる() {
        let f = files();
        let bytes = zip_of(&f, None);

        let mut tampered = f.clone();
        for t in &mut tampered {
            if t.0 == "character.txt" {
                t.2 = b"name=\xe3\x81\xa0\xe3\x82\x8c\r\n".to_vec();
            }
        }
        let e = check(&bytes, &expect_of(&tampered, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.text_mismatch");
        check(&bytes, &expect_of(&f, None)).expect("入れたものと突き合わせれば通る");
    }

    #[test]
    fn 入れていないものが入っていれば落ちる() {
        let f = files();
        let bytes = zip_of(&f, None);
        let mut fewer = f.clone();
        fewer.retain(|(p, _, _)| p != "a.frq");
        let e = check(&bytes, &expect_of(&fewer, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.unexpected_file");
    }

    #[test]
    fn 配布_wav_の形が違えば落ちる() {
        let mut f = files();
        // 32 bit float のマスターをそのまま入れてしまった形。
        let mut master = Vec::new();
        wav::write_master_header(&mut master, 44_100, 16).expect("書ける");
        master.extend_from_slice(&[0_u8; 16]);
        f[0].2 = master;
        let e = check(&zip_of(&f, None), &expect_of(&f, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.wrong_wav_format");
    }

    #[test]
    fn 除外する名前が入っていれば落ちる() {
        let mut f = files();
        f.push((".DS_Store".to_owned(), Compression::Store, vec![0_u8; 4]));
        let e = check(&zip_of(&f, None), &expect_of(&f, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.excluded_entry");
    }

    #[test]
    fn ルートの外に出ていれば落ちる() {
        let f = files();
        let entries = [entry("character.txt", Compression::Deflate, b"x")];
        let stray = write(
            Cursor::new(Vec::new()),
            "other",
            None,
            entries.into_iter().map(Ok::<_, ArchiveError>),
        )
        .expect("書ける")
        .into_inner();
        let e = check(&stray, &expect_of(&f, None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.not_single_root");
    }

    #[test]
    fn 壊れた中心ディレクトリは落ちる() {
        let e = check(b"not a zip at all", &expect_of(&files(), None)).expect_err("落ちる");
        assert_eq!(e.code(), "archive.malformed");
    }
}
