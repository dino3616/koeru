//! 外へ出す経路（`TR-PKG-45`, `TR-PKG-47`）。
//!
//! 利用者にフォルダ操作を「要求」しない。だが到達経路は必ず残す（`TR-PKG-45`）。
//! 現役の制作者は vLabeler や OpenUtau と併用したい。WAV に外から届かないと、
//! 部分的な併用すら成立しない。
//!
//! ここが持つのは3つのうちの2つ。
//!
//! 1. プロジェクトのフォルダを OS のファイルマネージャで開く（アプリ層）
//! 2. 録音済み WAV 一式を任意のフォルダへ非破壊で書き出す（[`export_for_external_tools`]）
//! 3. その書き出し先を再取り込みする（[`scan_external_folder`] ＋ [`crate::release::detect_drift`]）
//!
//! どれも UUID ディレクトリの不変性を損なわない。 書き出しは複製で、
//! 元のプロジェクトには触れない。
//!
//! ライブラリ全体の持ち出し（`TR-PKG-47`）は [`archive_library`] と [`restore_library`]。
//! 拡張子を配布パッケージと分ける。 同じ `.zip` にすると、配る相手に
//! ライブラリごと渡してしまう事故が起きる。

use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use crate::project::{Library, ProjectDir};

/// ライブラリ書き出しの拡張子（`TR-PKG-47`）。
///
/// 配布パッケージ（`.zip`）と分ける。 取り違えを字面で止める。
pub const LIBRARY_ARCHIVE_EXT: &str = "koerulib";

/// 外へ出すときの失敗。
#[derive(Debug, thiserror::Error)]
pub enum HandoffError {
    #[error("入出力に失敗した")]
    Io(#[from] std::io::Error),

    #[error("アーカイブの操作に失敗した")]
    Zip(#[from] zip::result::ZipError),

    /// 書き出し先に既にものがある。黙って上書きしない。
    #[error("書き出し先が空でない")]
    DestinationNotEmpty,

    /// アーカイブの中に、外へ出るパスが入っていた。
    #[error("アーカイブに不正なパスが入っている")]
    UnsafePath,
}

impl HandoffError {
    /// 送信してよい種別文字列。`Display` は送らない。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Io(_) => "handoff.io",
            Self::Zip(_) => "handoff.zip",
            Self::DestinationNotEmpty => "handoff.destination_not_empty",
            Self::UnsafePath => "handoff.unsafe_path",
        }
    }
}

type Result<T> = std::result::Result<T, HandoffError>;

/// 外部ツール向けに書き出した内容。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalExport {
    pub dest: PathBuf,
    /// 置いた WAV の名前（書き出し順）。
    pub wavs: Vec<String>,
}

/// 録音済み WAV 一式を任意のフォルダへ非破壊で書き出す（`TR-PKG-45`）。
///
/// `wavs` はプロジェクト相対のパス（台帳の `rel_path`）。`oto_ini` を渡すと
/// `oto.ini` として一緒に置く。書式と符号化は呼び出し側が確定させてから渡す。
///
/// 元のプロジェクトには一切触れない。 複製するだけ。
///
/// 書き出し先が空でなければ拒む。上書きすると、そこにあった他人の作業が消える。
#[tracing::instrument(skip(project, dest, wavs, oto_ini), fields(count = wavs.len()), err)]
pub fn export_for_external_tools(
    project: &ProjectDir,
    dest: &Path,
    wavs: &[String],
    oto_ini: Option<&[u8]>,
) -> Result<ExternalExport> {
    if dest.exists() && fs::read_dir(dest)?.next().is_some() {
        return Err(HandoffError::DestinationNotEmpty);
    }
    fs::create_dir_all(dest)?;

    let mut placed = Vec::with_capacity(wavs.len());
    for rel in wavs {
        let src = project.root().join(rel);
        let Some(name) = Path::new(rel).file_name().and_then(|s| s.to_str()) else {
            continue;
        };
        fs::copy(&src, dest.join(name))?;
        placed.push(name.to_owned());
    }

    if let Some(bytes) = oto_ini {
        fs::write(dest.join("oto.ini"), bytes)?;
    }

    Ok(ExternalExport {
        dest: dest.to_path_buf(),
        wavs: placed,
    })
}

/// 外部で編集されたフォルダを見に行く（`TR-PKG-48`）。
///
/// 返るのは `oto.ini` の中身（あれば）。符号化の判定と正規化は呼び出し側が行う。
/// ここは生のバイト列だけを渡す。
///
/// 差分を取り込むか捨てるかは [`crate::release::detect_drift`] の結果を
/// 本人に見せてから決める。自動で取り込まない。
#[tracing::instrument(skip(folder), err)]
pub fn scan_external_folder(folder: &Path) -> Result<Option<Vec<u8>>> {
    let p = folder.join("oto.ini");
    if !p.is_file() {
        return Ok(None);
    }
    Ok(Some(fs::read(p)?))
}

/// ライブラリ全体を1つのアーカイブへ書き出す（`TR-PKG-47`）。
///
/// 拡張子は `.koerulib`。配布パッケージと取り違えられない。
///
/// `renders/`（試唱キャッシュ）は入れない。再生成できるものを運ばない。
#[tracing::instrument(skip(lib, dest), err)]
pub fn archive_library(lib: &Library, dest: &Path) -> Result<u64> {
    let file = fs::File::create(dest)?;
    let mut zip = zip::ZipWriter::new(file);
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    let mut count = 0_u64;
    let mut stack = vec![lib.root().to_path_buf()];
    let mut files = Vec::new();
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let path = entry?.path();
            if path.is_dir() {
                // 試唱キャッシュは運ばない。
                if path.file_name().is_some_and(|n| n == "renders") {
                    continue;
                }
                stack.push(path);
            } else {
                files.push(path);
            }
        }
    }
    // 並びを決めておく。 同じライブラリからは同じアーカイブが出るほうが、
    // 移行が通ったかを突き合わせやすい。
    files.sort();

    for path in files {
        let Ok(rel) = path.strip_prefix(lib.root()) else {
            continue;
        };
        let Some(name) = rel.to_str() else { continue };
        zip.start_file(name.replace('\\', "/"), opts)?;
        let mut f = fs::File::open(&path)?;
        let mut buf = Vec::new();
        f.read_to_end(&mut buf)?;
        zip.write_all(&buf)?;
        count += 1;
    }
    zip.finish()?;
    Ok(count)
}

/// 別の PC で取り込む（`TR-PKG-47`）。
///
/// 取り込み先が空でなければ拒む。既にあるライブラリへ混ぜない。
/// 同じ UUID のプロジェクトが両方にあると、どちらが本物か決められなくなる。
#[tracing::instrument(skip(archive, dest), err)]
pub fn restore_library(archive: &Path, dest: &Path) -> Result<Library> {
    if dest.exists() && fs::read_dir(dest)?.next().is_some() {
        return Err(HandoffError::DestinationNotEmpty);
    }
    fs::create_dir_all(dest)?;

    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i)?;
        // アーカイブの中の名前を信用しない。 `../` で外へ出られては困る。
        let rel = entry.enclosed_name().ok_or(HandoffError::UnsafePath)?;
        let out = dest.join(&rel);
        if !out.starts_with(dest) {
            return Err(HandoffError::UnsafePath);
        }
        if entry.is_dir() {
            fs::create_dir_all(&out)?;
            continue;
        }
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut f = fs::File::create(&out)?;
        std::io::copy(&mut entry, &mut f)?;
    }

    Library::open(dest).map_err(|e| match e {
        crate::project::ProjectError::Io(io) => HandoffError::Io(io),
        // Library::open は作るだけなので、他の失敗は起きない。
        _ => HandoffError::UnsafePath,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::{Manifest, Method};

    fn tmp(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("koeru-handoff-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
        d
    }

    fn manifest() -> Manifest {
        Manifest {
            display_name: "こえるちゃん".to_owned(),
            method: Method::Single,
            item_count: 2,
            derived_from: None,
        }
    }

    fn project_with_audio(tag: &str) -> (Library, ProjectDir) {
        let lib = Library::open(tmp(tag)).expect("開けること");
        let p = lib.create(&manifest()).expect("作れること");
        fs::write(p.db_path(), b"db").expect("書けること");
        fs::write(p.audio_dir().join("R001_1.wav"), b"wav-a").expect("書けること");
        fs::write(p.audio_dir().join("R002_1.wav"), b"wav-b").expect("書けること");
        fs::write(p.renders_dir().join("cache.wav"), b"cache").expect("書けること");
        (lib, p)
    }

    /// 書き出しは非破壊。元のプロジェクトに触れない（`TR-PKG-45`）。
    #[test]
    fn external_export_does_not_touch_the_project() {
        let (_lib, p) = project_with_audio("ext");
        let dest = tmp("ext-dest").join("out");

        let got = export_for_external_tools(
            &p,
            &dest,
            &["audio/R001_1.wav".into(), "audio/R002_1.wav".into()],
            Some(b"[R001_1.wav]\nR001_1.wav=,0,0,0,0,0"),
        )
        .expect("書き出せること");

        assert_eq!(got.wavs, ["R001_1.wav", "R002_1.wav"]);
        assert_eq!(fs::read(dest.join("R001_1.wav")).expect("読める"), b"wav-a");
        assert!(dest.join("oto.ini").is_file());

        // 元はそのまま。
        assert!(p.audio_dir().join("R001_1.wav").is_file());
        assert!(p.manifest_path().is_file());
    }

    /// 書き出し先が空でなければ拒む。 上書きすると他人の作業が消える。
    #[test]
    fn external_export_refuses_a_non_empty_destination() {
        let (_lib, p) = project_with_audio("ext2");
        let dest = tmp("ext2-dest");
        fs::write(dest.join("大事なもの.txt"), b"x").expect("書けること");

        let e = export_for_external_tools(&p, &dest, &[], None).expect_err("拒むこと");
        assert_eq!(e.kind(), "handoff.destination_not_empty");
        assert!(dest.join("大事なもの.txt").is_file(), "消えないこと");
    }

    #[test]
    fn scanning_returns_the_oto_bytes_verbatim() {
        let dir = tmp("scan");
        assert!(scan_external_folder(&dir).expect("見られること").is_none());

        // CP932 で書かれたものも、判定せずそのまま返す。
        let raw = [0x82, 0xA0, b'\r', b'\n'];
        fs::write(dir.join("oto.ini"), raw).expect("書けること");
        assert_eq!(
            scan_external_folder(&dir)
                .expect("見られること")
                .expect("あること"),
            raw
        );
    }

    /// ライブラリごと持ち出して、別の場所へ戻せる（`TR-PKG-47`）。
    #[test]
    fn library_round_trips_through_an_archive() {
        let (lib, p) = project_with_audio("arc");
        let archive = tmp("arc-out").join(format!("backup.{LIBRARY_ARCHIVE_EXT}"));

        let n = archive_library(&lib, &archive).expect("書き出せること");
        assert!(n >= 4, "manifest と db と WAV 2本は入ること");

        let dest = tmp("arc-restore").join("library");
        let restored = restore_library(&archive, &dest).expect("戻せること");

        let listed = restored.list().expect("挙げられること");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].0.id(), p.id(), "UUID が変わらないこと");
        assert_eq!(
            listed[0].1.as_ref().expect("読めること").display_name,
            "こえるちゃん"
        );
        assert_eq!(
            fs::read(listed[0].0.audio_dir().join("R001_1.wav")).expect("読める"),
            b"wav-a"
        );
    }

    /// 再生成できるものは運ばない。
    #[test]
    fn archive_leaves_out_the_render_cache() {
        let (lib, _p) = project_with_audio("arc2");
        let archive = tmp("arc2-out").join(format!("backup.{LIBRARY_ARCHIVE_EXT}"));
        archive_library(&lib, &archive).expect("書き出せること");

        let dest = tmp("arc2-restore").join("library");
        let restored = restore_library(&archive, &dest).expect("戻せること");
        let p = &restored.list().expect("挙げられること")[0].0;
        assert!(
            !p.renders_dir().join("cache.wav").exists(),
            "試唱キャッシュを運ばないこと"
        );
    }

    /// 既にあるライブラリへ混ぜない（同じ UUID が2つになる）。
    #[test]
    fn restore_refuses_a_non_empty_destination() {
        let (lib, _p) = project_with_audio("arc3");
        let archive = tmp("arc3-out").join(format!("backup.{LIBRARY_ARCHIVE_EXT}"));
        archive_library(&lib, &archive).expect("書き出せること");

        let dest = tmp("arc3-restore");
        fs::write(dest.join("既にある"), b"x").expect("書けること");
        let e = restore_library(&archive, &dest).expect_err("拒むこと");
        assert_eq!(e.kind(), "handoff.destination_not_empty");
    }

    /// アーカイブの中の名前を信用しない。
    #[test]
    fn restore_refuses_paths_that_escape_the_destination() {
        let archive = tmp("evil").join("evil.koerulib");
        {
            let mut z = zip::ZipWriter::new(fs::File::create(&archive).expect("作れること"));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default();
            z.start_file("../../逃げ出す.txt", opts)
                .expect("書けること");
            z.write_all(b"x").expect("書けること");
            z.finish().expect("閉じられること");
        }
        let dest = tmp("evil-dest").join("library");
        let e = restore_library(&archive, &dest).expect_err("拒むこと");
        assert_eq!(e.kind(), "handoff.unsafe_path");
    }

    /// 配布パッケージと拡張子が違う（`TR-PKG-47`）。
    #[test]
    fn the_library_archive_is_not_a_zip_by_name() {
        assert_ne!(LIBRARY_ARCHIVE_EXT, "zip");
    }
}
