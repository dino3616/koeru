//! ZIP と UAR（`TR-PKG-09`, `TR-PKG-14`, `TR-PKG-26`, `TR-PKG-27`, `TR-PKG-52`）。
//!
//! 包むのはここだけ。 エントリ名は [`crate::tree`] が組み立てたものを
//! そのまま使い、ディレクトリを列挙しない（`TR-PKG-15`）。
//!
//! # エントリ名は ASCII に閉じている
//!
//! `DEC-PKG-009`。CP932 と UTF-8 でバイト列が同じなので、プロファイルで
//! 変えるものが無い。EFS フラグも立てない——ASCII しか無いところに
//! 「UTF-8 でないと読めない」と宣言することになる。
//!
//! # 書いたら必ず読み戻す
//!
//! `TR-PKG-52` / `INV-PKG-103`。1つでも食い違えば失敗として扱い、
//! ファイルを残さない。半端なアーカイブが `exports/` に残ると、
//! 次に開いた人にはそれが成果物に見える。

use std::collections::BTreeMap;
use std::fs;
use std::io::{Read as _, Seek as _, SeekFrom, Write as _};
use std::path::{Path, PathBuf};

use koeru_core::text::{self, TextEncoding};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

use crate::bank::VoiceBank;
use crate::character::{ICON_FILE, PORTRAIT_FILE};
use crate::profile::{NEWLINE, Profile};
use crate::tree::{Compression, Content, PackagedFile};

/// UAR のルート直下に置くインストール指示（`TR-PKG-09`）。
pub const INSTALL_FILE: &str = "install.txt";

/// 配布アーカイブの拡張子。
pub const ZIP_EXT: &str = "zip";
/// UTAU 本体へのドラッグ&ドロップでインストールできる形（`DEC-PKG-010`）。
pub const UAR_EXT: &str = "uar";

/// 音源ルート直下に必ず入るファイル（`TR-PKG-01`, `TR-PKG-50`）。
const REQUIRED_ROOT_FILES: [&str; 3] = ["character.txt", "character.yaml", "readme.txt"];

/// 配布物に混ぜてはいけない名前（`TR-PKG-26`）。
///
/// ディレクトリを列挙しないので普通は出てこない。 それでも見るのは、
/// 素材のフォルダに OS が勝手に置くものを、将来だれかが拾ってきたときに
/// ここで止めるため。
const EXCLUDED: [&str; 4] = ["__MACOSX", ".DS_Store", "Thumbs.db", "desktop.ini"];

/// 書き出しで失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    /// ファイル操作が失敗した。
    #[error("ファイル操作が失敗した")]
    Io(#[from] std::io::Error),

    /// ZIP の組み立てが失敗した。
    #[error("ZIP を組み立てられない")]
    Zip(#[from] zip::result::ZipError),

    /// マスター WAV を読めない。
    #[error("マスター WAV を読めない")]
    Wav(#[from] koeru_audio::wav::WavError),

    /// テキストを書けない。
    #[error("この符号化で書けない文字がある")]
    Text(#[from] text::TextError),

    /// 読み戻し検証に通らなかった（`TR-PKG-52`）。
    #[error("読み戻し検証に通らなかった")]
    Verification { problem: VerifyProblem },
}

impl ArchiveError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "archive.io",
            Self::Zip(_) => "archive.zip",
            Self::Wav(e) => e.kind(),
            Self::Text(e) => e.kind(),
            Self::Verification { problem } => problem.kind(),
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

type Result<T> = std::result::Result<T, ArchiveError>;

/// 書き出した結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Written {
    /// ZIP の在り処。
    pub zip: PathBuf,
    /// UAR の在り処（`DEC-PKG-010`）。
    pub uar: PathBuf,
}

/// `install.txt` を組み立てる（`TR-PKG-09`）。
///
/// 4項目だけを書く。`folder` と `contentsdir` は同じ配布名にする——
/// アーカイブの中の名前とインストール先の名前を違えても得が無い。
///
/// # Errors
///
/// 選んだ符号化で書けない文字がある。
pub fn install_txt(bank: &VoiceBank, profile: Profile) -> Result<Vec<u8>> {
    let name = &bank.distribution_name;
    let mut body =
        format!("type=voiceset{NEWLINE}folder={name}{NEWLINE}contentsdir={name}{NEWLINE}");
    // 説明は音源名。1行に収まらないものは書かない——`install.txt` は
    // 1行1項目で、改行が入ると次の行が別の項目として読まれる。
    let description = bank.character.name.trim();
    if !description.is_empty() && !description.contains(['\r', '\n']) {
        body.push_str(&format!("description={description}{NEWLINE}"));
    }
    Ok(text::encode(&body, profile.encoding())?)
}

/// ZIP と UAR を書き、どちらも読み戻して確かめる（`TR-PKG-52`, `DEC-PKG-010`）。
///
/// 失敗したら両方消す。 片方だけ残すと、中身の違う2つが配られる。
///
/// # Errors
///
/// 素材を読めない、ZIP を組み立てられない、読み戻し検証に通らない。
// 配布名も版の札も、本人が書いた自由文。トレースへ載せない（`AGENTS.md` #3）。
#[tracing::instrument(
    skip(bank, files, dest_dir, base_name),
    fields(profile = profile.as_str(), entries = files.len()),
    err
)]
pub fn write_and_verify(
    bank: &VoiceBank,
    files: &[PackagedFile],
    profile: Profile,
    dest_dir: &Path,
    base_name: &str,
) -> Result<Written> {
    fs::create_dir_all(dest_dir)?;
    let root = &bank.distribution_name;
    let install = install_txt(bank, profile)?;

    let written = Written {
        zip: dest_dir.join(format!("{base_name}.{ZIP_EXT}")),
        uar: dest_dir.join(format!("{base_name}.{UAR_EXT}")),
    };

    let result = (|| {
        write(files, root, None, &written.zip)?;
        verify(&written.zip, files, root, profile, None)?;
        write(files, root, Some(&install), &written.uar)?;
        verify(&written.uar, files, root, profile, Some(&install))?;
        Ok(())
    })();

    if let Err(e) = result {
        // 読み戻しに通らなかったものを残さない（`INV-PKG-103`）。
        let _ = fs::remove_file(&written.zip);
        let _ = fs::remove_file(&written.uar);
        return Err(e);
    }
    Ok(written)
}

/// アーカイブを1つ書く（`TR-PKG-26`, `TR-PKG-27`）。
///
/// エントリ名は `<配布名>/<相対パス>`。`install` を渡すと、その手前に
/// `install.txt` を置く（`TR-PKG-09` の二層構造）。
///
/// # Errors
///
/// 素材を読めない、書けない。
pub fn write(
    files: &[PackagedFile],
    root: &str,
    install: Option<&[u8]>,
    dest: &Path,
) -> Result<()> {
    let file = fs::File::create(dest)?;
    let mut zip = ZipWriter::new(file);

    if let Some(bytes) = install {
        zip.start_file(INSTALL_FILE, options(Compression::Deflate))?;
        zip.write_all(bytes)?;
    }
    for f in files {
        zip.start_file(entry_name(root, &f.path), options(f.compression))?;
        match &f.content {
            Content::Bytes(b) => zip.write_all(b)?,
            Content::Master(path) => {
                // マスターは 32 bit float。配布は 16 bit（`TR-PKG-20`）。
                // レートは `distribution_bytes` が 44100 で固定する。
                //
                // **ディザを入れる。** `TR-REC-37` が「TPDF ディザ（振幅 1 LSB）
                // のみを適用する」と定めている。切ると、静かなところの量子化
                // 誤差が信号と相関して歪みとして聞こえる。
                let wav = koeru_audio::wav::read(path)?;
                zip.write_all(&koeru_audio::wav::distribution_bytes(&wav.samples, true))?;
            }
        }
    }
    zip.finish()?;
    Ok(())
}

/// 書いたアーカイブを読み戻して確かめる（`TR-PKG-52`）。
///
/// 見るのは5点。 エントリ名の符号化、EFS フラグ、階層、必須ファイル、
/// テキストの再デコード。加えて配布 WAV のヘッダを見る——
/// `TR-PKG-49` の「サンプリング周波数 ≠ 44100 / 量子化ビット数 ≠ 16」は、
/// 素材ではなく出したものについての条件。
///
/// # Errors
///
/// ファイルを読めない、または5点のどれかが食い違う。
// **`root` は配布名、`install` は install.txt のバイト列で、どちらにも
// 音源名が入る。** skip し忘れると、そのままスパンに載る（`AGENTS.md` #3）。
#[tracing::instrument(
    skip(path, files, root, install),
    fields(entries = files.len()),
    err
)]
pub fn verify(
    path: &Path,
    files: &[PackagedFile],
    root: &str,
    profile: Profile,
    install: Option<&[u8]>,
) -> Result<()> {
    let expect_install = install.is_some();
    // (1)(2) 中心ディレクトリを直接見る。 crate の解釈ではなく、
    // 実際に書いたバイト列でフラグと名前を確かめる。
    for (name, flags) in central_directory(&mut fs::File::open(path)?)? {
        if !name.is_ascii() {
            return Err(fail(VerifyProblem::NonAsciiEntryName));
        }
        if flags & EFS_FLAG != 0 {
            return Err(fail(VerifyProblem::UnexpectedEfsFlag));
        }
    }

    // 入れたはずのバイト列。 読み戻しはこれと突き合わせる。
    //
    // 復号できるかだけを見ない。 それでは、別の内容でも正しい CP932 で
    // ありさえすれば通る——改行が変わった `oto.ini` も、古いままの
    // `character.txt` も素通りする（`TR-PKG-52` の「内部モデルと一致する」）。
    let mut expected: BTreeMap<String, &[u8]> = BTreeMap::new();
    for f in files {
        if let Content::Bytes(b) = &f.content {
            expected.insert(entry_name(root, &f.path), b);
        }
    }
    if let Some(bytes) = install {
        expected.insert(INSTALL_FILE.to_owned(), bytes);
    }

    // **アーカイブを丸ごと読み込まない。** 275MB の配布物で ZIP と UAR を
    // 順に検証すると、そのぶんが常駐に乗る（`BUDGET-MEMORY-001`）。
    // 突き合わせが要るのはテキストだけで、WAV は先頭 44 バイトで足りる。
    let mut archive = zip::ZipArchive::new(std::io::BufReader::new(fs::File::open(path)?))?;
    let mut seen: Vec<String> = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_owned();
        seen.push(name.clone());

        if name.contains("..") {
            return Err(fail(VerifyProblem::TraversingEntry));
        }
        if name.split('/').any(|seg| EXCLUDED.contains(&seg)) {
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
            let mut head = [0_u8; WAV_HEADER_LEN];
            std::io::Read::read_exact(&mut entry, &mut head)
                .map_err(|_| fail(VerifyProblem::WrongWavFormat))?;
            check_wav_header(&head)?;
            continue;
        }
        let Some(enc) = text_encoding_of(&name, profile) else {
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
    let mut want: Vec<String> = files.iter().map(|f| entry_name(root, &f.path)).collect();
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
    for r in REQUIRED_ROOT_FILES {
        if !seen.contains(&entry_name(root, r)) {
            return Err(fail(VerifyProblem::MissingFile));
        }
    }
    Ok(())
}

/// 汎用フラグの 11 ビット目。エントリ名が UTF-8 であることの宣言。
const EFS_FLAG: u16 = 1 << 11;

/// 中心ディレクトリから、エントリ名と汎用フラグを取り出す。
///
/// 局所ヘッダを頭から探さない。 `PK\x03\x04` は無圧縮で入れた WAV の
/// 中にも現れうるので、署名を探して歩くと別のところで止まる。
///
/// **末尾と中心ディレクトリだけを読む。** アーカイブ全体を載せると、
/// 275MB の配布物でそのぶんが常駐に乗る。EOCD はコメントを含めても
/// 末尾 64KiB + 22 バイトに収まる。
fn central_directory(file: &mut fs::File) -> Result<Vec<(String, u16)>> {
    const EOCD: &[u8; 4] = b"PK\x05\x06";
    const CD: &[u8; 4] = b"PK\x01\x02";
    /// EOCD の固定長 22 バイトと、コメントの上限 65535。
    const EOCD_SEARCH: u64 = 22 + 65_535;

    let len = file.metadata()?.len();
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

/// 配布 WAV のヘッダの長さ。RIFF + fmt + data のヘッダまで。
const WAV_HEADER_LEN: usize = 44;

/// 配布 WAV のヘッダを見る（`TR-PKG-20`, `TR-PKG-49`）。
fn check_wav_header(body: &[u8]) -> Result<()> {
    let ok = body.len() >= WAV_HEADER_LEN
        && &body[0..4] == b"RIFF"
        && &body[8..12] == b"WAVE"
        && u16::from_le_bytes([body[20], body[21]]) == 1
        && u16::from_le_bytes([body[22], body[23]]) == 1
        && u32::from_le_bytes([body[24], body[25], body[26], body[27]])
            == koeru_audio::wav::DISTRIBUTION_RATE_HZ
        && u16::from_le_bytes([body[34], body[35]]) == 16;
    if ok {
        Ok(())
    } else {
        Err(fail(VerifyProblem::WrongWavFormat))
    }
}

/// 読み戻しで再デコードするときの符号化。テキストでなければ `None`。
///
/// `character.yaml` だけは常に UTF-8（`TR-PKG-12`）。 プロファイルの符号化で
/// 読もうとすると、両対応で書いた配布物が読み戻し検証に落ちる。
fn text_encoding_of(name: &str, profile: Profile) -> Option<TextEncoding> {
    if name.ends_with(".wav")
        || name.ends_with(".frq")
        || name.ends_with(ICON_FILE)
        || name.ends_with(PORTRAIT_FILE)
    {
        return None;
    }
    if name.ends_with("character.yaml") {
        return Some(TextEncoding::Utf8);
    }
    Some(profile.encoding())
}

/// 音源ルートフォルダを被せたエントリ名（`TR-PKG-26`）。
fn entry_name(root: &str, path: &str) -> String {
    format!("{root}/{path}")
}

fn options(c: Compression) -> SimpleFileOptions {
    let method = match c {
        Compression::Store => CompressionMethod::Stored,
        Compression::Deflate => CompressionMethod::Deflated,
    };
    SimpleFileOptions::default().compression_method(method)
}

/// 書き出したアーカイブの符号化を、外から確かめられるようにしておく。
///
/// 試験と、既に配ったものを見直すときに使う。
///
/// # Errors
///
/// ファイルを読めない、中心ディレクトリが壊れている。
pub fn entry_names(path: &Path) -> Result<Vec<String>> {
    Ok(central_directory(&mut fs::File::open(path)?)?
        .into_iter()
        .map(|(n, _)| n)
        .collect())
}

/// 宣言した符号化。`character.yaml` と突き合わせるために外へ出す。
#[must_use]
pub const fn declared(profile: Profile) -> TextEncoding {
    profile.encoding()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Character, Readme, Sample, Subbank};
    use crate::tree;
    use koeru_align::ini::IniEntry;
    use koeru_core::oto::Oto;
    use koeru_core::project::Method;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("koeru-archive-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
        d
    }

    fn wav(dir: &Path, name: &str) -> PathBuf {
        let path = dir.join(name);
        koeru_audio::wav::write_distribution(&path, &vec![0.1_f32; 4410], false)
            .expect("書けること");
        path
    }

    fn bank(dir: &Path) -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            method: Method::Single,
            subbanks: vec![Subbank {
                folder: None,
                color: String::new(),
                prefix: String::new(),
                suffix: String::new(),
                tone: None,
                samples: vec![Sample {
                    file: "s001.wav".to_owned(),
                    master: wav(dir, "s001.wav"),
                    frq: Some(vec![0_u8; 40]),
                    entries: vec![IniEntry {
                        file: "s001.wav".to_owned(),
                        alias: "あ".to_owned(),
                        oto: Oto {
                            offset_ms: 10.0,
                            consonant_ms: 20.0,
                            cutoff_ms: -50.0,
                            preutterance_ms: 15.0,
                            overlap_ms: 5.0,
                        },
                    }],
                }],
            }],
            rules: koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
        }
    }

    #[test]
    fn zip_と_uar_を書いて読み戻せる() {
        let d = tmp("ok");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "000001-v1")
            .expect("書けること");
        assert!(w.zip.is_file());
        assert!(w.uar.is_file());
    }

    /// `TR-PKG-26`。音源ルートフォルダ1つを内包する単層構造。
    #[test]
    fn 単層構造で区切りは斜線() {
        let d = tmp("layout");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");
        for name in entry_names(&w.zip).expect("読めること") {
            assert!(name.starts_with("koeru/"), "{name}");
            assert!(!name.contains('\\'), "{name}");
        }
    }

    /// `TR-PKG-09`。UAR だけが install.txt を持つ。
    #[test]
    fn uar_だけが_install_txt_を持つ() {
        let d = tmp("uar");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");

        let zip = entry_names(&w.zip).expect("読めること");
        let uar = entry_names(&w.uar).expect("読めること");
        assert!(!zip.contains(&INSTALL_FILE.to_owned()));
        assert!(uar.contains(&INSTALL_FILE.to_owned()));
        assert_eq!(zip.len() + 1, uar.len(), "他の中身は同じ");
    }

    /// `DEC-PKG-009`。エントリ名は ASCII で、EFS フラグを立てない。
    #[test]
    fn efs_フラグを立てない() {
        let d = tmp("efs");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");
        let mut file = fs::File::open(&w.zip).expect("開けること");
        for (name, flags) in central_directory(&mut file).expect("中心ディレクトリを読めること")
        {
            assert!(name.is_ascii(), "{name}");
            assert_eq!(flags & EFS_FLAG, 0, "{name}");
        }
    }

    /// プロファイルを変えてもエントリ名のバイト列は変わらない（`DEC-PKG-009`）。
    #[test]
    fn エントリ名はプロファイルで変わらない() {
        let d = tmp("profiles");
        let b = bank(&d);
        let mut names = Vec::new();
        for p in [Profile::Classic, Profile::OpenUtau, Profile::Both] {
            let files = tree::build(&b, p).expect("組み立てられること");
            let w = write_and_verify(&b, &files, p, &d.join(p.as_str()), "x").expect("書けること");
            names.push(entry_names(&w.zip).expect("読めること"));
        }
        assert_eq!(names[0], names[1]);
        assert_eq!(names[1], names[2]);
    }

    /// `TR-PKG-27`。音は無圧縮、テキストは Deflate。
    #[test]
    fn 圧縮方式が種別で分かれている() {
        let d = tmp("compress");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");

        let mut zip =
            zip::ZipArchive::new(fs::File::open(&w.zip).expect("開けること")).expect("読めること");
        for i in 0..zip.len() {
            let e = zip.by_index(i).expect("エントリを読めること");
            let want = if e.name().ends_with(".wav") || e.name().ends_with(".frq") {
                CompressionMethod::Stored
            } else {
                CompressionMethod::Deflated
            };
            assert_eq!(e.compression(), want, "{}", e.name());
        }
    }

    /// `TR-PKG-20`。配布 WAV は 44100 Hz / 16 bit / モノラル。
    #[test]
    fn 配布_wav_は_16_bit_になる() {
        let d = tmp("bits");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");

        let mut zip =
            zip::ZipArchive::new(fs::File::open(&w.zip).expect("開けること")).expect("読めること");
        let mut body = Vec::new();
        std::io::copy(
            &mut zip.by_name("koeru/s001.wav").expect("WAV があること"),
            &mut body,
        )
        .expect("読めること");
        assert_eq!(u16::from_le_bytes([body[34], body[35]]), 16);
        assert_eq!(
            u32::from_le_bytes([body[24], body[25], body[26], body[27]]),
            44_100
        );
    }

    /// `INV-PKG-103`。読み戻しに通らないものを残さない。
    #[test]
    fn 検証に落ちたらファイルを残さない() {
        let d = tmp("discard");
        let b = bank(&d);
        let mut files = tree::build(&b, Profile::Both).expect("組み立てられること");
        // 必須ファイルを1つ抜く。読み戻しで MissingFile になる。
        files.retain(|f| f.path != "character.txt");

        let dest = d.join("exports");
        let e = write_and_verify(&b, &files, Profile::Both, &dest, "x").expect_err("落ちること");
        assert_eq!(e.kind(), "archive.missing_file");
        assert!(!dest.join("x.zip").exists());
        assert!(!dest.join("x.uar").exists());
    }

    /// `TR-PKG-52` (5)。読み戻したテキストが、入れたものと違えば落ちる。
    ///
    /// 復号できるかだけを見ていると、別の内容でも正しい CP932 なら通る。
    #[test]
    fn 中身が違えば読み戻しで落ちる() {
        let d = tmp("mismatch");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let dest = d.join("exports").join("x.zip");
        fs::create_dir_all(d.join("exports")).expect("作れること");
        write(&files, "koeru", None, &dest).expect("書けること");

        // 同じ名前で、中身だけ違うものを期待させる。
        let mut tampered = files.clone();
        for f in &mut tampered {
            if f.path == "character.txt" {
                f.content = Content::Bytes(b"name=\xe3\x81\xa0\xe3\x82\x8c\r\n".to_vec());
            }
        }
        let e = verify(&dest, &tampered, "koeru", Profile::Both, None).expect_err("落ちること");
        assert_eq!(e.kind(), "archive.text_mismatch");

        // 入れたものと突き合わせれば通る。
        verify(&dest, &files, "koeru", Profile::Both, None).expect("通ること");
    }

    #[test]
    fn install_txt_は4項目だけ() {
        let d = tmp("install");
        let b = bank(&d);
        let bytes = install_txt(&b, Profile::Both).expect("書けること");
        let text = text::decode(&bytes, Profile::Both.encoding()).expect("読めること");
        let keys: Vec<&str> = text
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k))
            .collect();
        assert_eq!(keys, ["type", "folder", "contentsdir", "description"]);
        assert!(text.contains("type=voiceset"));
        assert!(text.contains("folder=koeru"));
    }

    #[test]
    fn 改行を含む音源名は説明にしない() {
        let d = tmp("install-nl");
        let mut b = bank(&d);
        b.character.name = "こえる\nちゃん".to_owned();
        let bytes = install_txt(&b, Profile::Both).expect("書けること");
        let text = text::decode(&bytes, Profile::Both.encoding()).expect("読めること");
        assert!(!text.contains("description="));
    }
}
