//! ZIP と UAR の方針（`TR-PKG-09`, `TR-PKG-14`, `TR-PKG-26`, `TR-PKG-27`, `TR-PKG-52`）。
//!
//! 何を入れ、何と突き合わせて読み戻すかを決める。 ZIP の符号化と読み戻しの手順は
//! `koeru-formats` の [`koeru_formats::archive`] が持ち、ファイルを開いて書くのは
//! 呼び出し側（`koeru-app` の `export_io`）。 ここはファイルにも ZIP にも触らない
//! （`tests/purity.rs`）。
//!
//! エントリ名は [`crate::tree`] が組み立てたものをそのまま使い、ディレクトリを
//! 列挙しない（`TR-PKG-15`）。 名前が ASCII に閉じていること（`DEC-PKG-009`）と、
//! 書いたら必ず読み戻すこと（`INV-PKG-103`）は、formats の側に書いてある。

use std::path::PathBuf;

use koeru_formats::archive::Expect;
use koeru_formats::text::{self, TextEncoding, TextError};

use crate::bank::VoiceBank;
use crate::character::{ICON_FILE, PORTRAIT_FILE};
use crate::profile::{NEWLINE, Profile};
use crate::tree::{Content, PackagedFile};

/// UAR のルート直下に置くインストール指示（`TR-PKG-09`）。
pub use koeru_formats::archive::INSTALL_FILE;

/// 配布アーカイブの拡張子。
pub const ZIP_EXT: &str = "zip";
/// UTAU 本体へのドラッグ&ドロップでインストールできる形（`DEC-PKG-010`）。
pub const UAR_EXT: &str = "uar";

/// 音源ルート直下に必ず入るファイル（`TR-PKG-01`, `TR-PKG-50`）。
pub const REQUIRED_ROOT_FILES: [&str; 3] = ["character.txt", "character.yaml", "readme.txt"];

/// 配布物に混ぜてはいけない名前（`TR-PKG-26`）。
pub const EXCLUDED: [&str; 4] = ["__MACOSX", ".DS_Store", "Thumbs.db", "desktop.ini"];

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
pub fn install_txt(bank: &VoiceBank, profile: Profile) -> Result<Vec<u8>, TextError> {
    let name = &bank.distribution_name;
    let mut body =
        format!("type=voiceset{NEWLINE}folder={name}{NEWLINE}contentsdir={name}{NEWLINE}");
    // 説明は音源名。1行に収まらないものは書かない——`install.txt` は
    // 1行1項目で、改行が入ると次の行が別の項目として読まれる。
    let description = bank.character.name.trim();
    if !description.is_empty() && !description.contains(['\r', '\n']) {
        body.push_str(&format!("description={description}{NEWLINE}"));
    }
    text::encode(&body, profile.encoding())
}

/// 読み戻しが突き合わせるもの（`TR-PKG-52`）。
///
/// 入れたはずのファイルのうち、バイト列そのものを持つものだけが中身まで突き合わせられる
/// （テキスト・BMP・PNG・`.frq`）。 マスターは書くときに 16 bit へ落とすので、ここには
/// 元のバイト列が無く、ヘッダだけが見られる。
///
/// 突き合わせるのはテキストだけ。 `.frq` と画像は、バイト列が一致するかを見ない
/// （読み戻しがテキストとして読まないものは、中身を引かない）。
#[must_use]
pub fn expect<'a>(
    files: &'a [PackagedFile],
    root: &'a str,
    install: Option<&'a [u8]>,
) -> Expect<'a> {
    Expect {
        root,
        install,
        files: files
            .iter()
            .map(|f| {
                let bytes = match &f.content {
                    Content::Bytes(b) => Some(b.as_slice()),
                    Content::Master(_) => None,
                };
                (f.path.as_str(), bytes)
            })
            .collect(),
        required: &REQUIRED_ROOT_FILES,
        excluded: &EXCLUDED,
    }
}

/// 読み戻しで再デコードするときの符号化。テキストでなければ `None`。
///
/// `character.yaml` だけは常に UTF-8（`TR-PKG-12`）。 プロファイルの符号化で
/// 読もうとすると、両対応で書いた配布物が読み戻し検証に落ちる。
#[must_use]
pub fn text_encoding_of(name: &str, profile: Profile) -> Option<TextEncoding> {
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

/// 宣言した符号化。`character.yaml` と突き合わせるために外へ出す。
#[must_use]
pub const fn declared(profile: Profile) -> TextEncoding {
    profile.encoding()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Character, Readme, Sample, Subbank};
    use crate::tree::{self, Compression};
    use koeru_failure::Failure;
    use koeru_formats::archive::{self, ArchiveError, Entry};
    use koeru_formats::oto_ini::IniEntry;
    use koeru_formats::wav;
    use koeru_model::alias::Method;
    use koeru_model::oto::Oto;
    use std::borrow::Cow;
    use std::io::Cursor;

    fn bank() -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            method: Method::Single,
            tones: vec![57],
            subbanks: vec![Subbank {
                folder: None,
                color: String::new(),
                prefix: String::new(),
                suffix: String::new(),
                tone: None,
                samples: vec![Sample {
                    file: "s001.wav".to_owned(),
                    master: PathBuf::from("/virtual/s001.wav"),
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
            rules: koeru_model::presamp::Rules::builtin(koeru_model::inventory::UnitSet::Core),
        }
    }

    /// マスターを開かずに ZIP へ詰める。 マスターは決まった配布 WAV に置き換える。
    fn pack(files: &[PackagedFile], root: &str, install: Option<&[u8]>) -> Vec<u8> {
        let wav = wav::distribution_bytes(&[0.1; 4410], false);
        let entries = files.iter().map(|f| {
            let body: Cow<'_, [u8]> = match &f.content {
                Content::Bytes(b) => Cow::Borrowed(b),
                Content::Master(_) => Cow::Borrowed(&wav),
            };
            Ok::<_, ArchiveError>(Entry {
                path: &f.path,
                compression: f.compression,
                body,
            })
        });
        archive::write(Cursor::new(Vec::new()), root, install, entries)
            .expect("書けること")
            .into_inner()
    }

    fn verify(
        bytes: &[u8],
        files: &[PackagedFile],
        profile: Profile,
        install: Option<&[u8]>,
    ) -> Result<(), ArchiveError> {
        archive::verify(Cursor::new(bytes), &expect(files, "koeru", install), |n| {
            text_encoding_of(n, profile)
        })
    }

    #[test]
    fn 組み立てた中身は詰めて読み戻せる() {
        let b = bank();
        for p in [Profile::Classic, Profile::OpenUtau, Profile::Both] {
            let files = tree::build(&b, p).expect("組み立てられること");
            let install = install_txt(&b, p).expect("書けること");
            let zip = pack(&files, "koeru", None);
            verify(&zip, &files, p, None).expect("ZIP は通ること");
            let uar = pack(&files, "koeru", Some(&install));
            verify(&uar, &files, p, Some(&install)).expect("UAR は通ること");
        }
    }

    /// `TR-PKG-26`。音源ルートフォルダ1つを内包する単層構造で、区切りは斜線。
    #[test]
    fn 単層構造で区切りは斜線() {
        let files = tree::build(&bank(), Profile::Both).expect("組み立てられること");
        let zip = pack(&files, "koeru", None);
        for name in archive::entry_names(&mut Cursor::new(&zip)).expect("読めること") {
            assert!(name.starts_with("koeru/"), "{name}");
            assert!(!name.contains('\\'), "{name}");
        }
    }

    /// `TR-PKG-09`。UAR だけが install.txt を持つ。
    #[test]
    fn uar_だけが_install_txt_を持つ() {
        let b = bank();
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let install = install_txt(&b, Profile::Both).expect("書けること");

        let zip = archive::entry_names(&mut Cursor::new(pack(&files, "koeru", None)))
            .expect("読めること");
        let uar = archive::entry_names(&mut Cursor::new(pack(&files, "koeru", Some(&install))))
            .expect("読めること");
        assert!(!zip.contains(&INSTALL_FILE.to_owned()));
        assert!(uar.contains(&INSTALL_FILE.to_owned()));
        assert_eq!(zip.len() + 1, uar.len(), "他の中身は同じ");
    }

    /// プロファイルを変えてもエントリ名のバイト列は変わらない（`DEC-PKG-009`）。
    #[test]
    fn エントリ名はプロファイルで変わらない() {
        let b = bank();
        let mut names = Vec::new();
        for p in [Profile::Classic, Profile::OpenUtau, Profile::Both] {
            let files = tree::build(&b, p).expect("組み立てられること");
            names.push(
                archive::entry_names(&mut Cursor::new(pack(&files, "koeru", None)))
                    .expect("読めること"),
            );
        }
        assert_eq!(names[0], names[1]);
        assert_eq!(names[1], names[2]);
    }

    /// `TR-PKG-27`。音は無圧縮、テキストは Deflate。 圧縮方式は `tree` が決め、
    /// 詰めるときにそのまま使われる。
    #[test]
    fn 圧縮方式が種別で分かれている() {
        let files = tree::build(&bank(), Profile::Both).expect("組み立てられること");
        for f in &files {
            let want = if f.path.ends_with(".wav") || f.path.ends_with(".frq") {
                Compression::Store
            } else {
                Compression::Deflate
            };
            assert_eq!(f.compression, want, "{}", f.path);
        }
    }

    /// `INV-PKG-103`。必須ファイルが抜けていれば、読み戻しが落ちる。
    #[test]
    fn 必須ファイルが抜ければ読み戻しで落ちる() {
        let mut files = tree::build(&bank(), Profile::Both).expect("組み立てられること");
        files.retain(|f| f.path != "character.txt");
        let zip = pack(&files, "koeru", None);
        let e = verify(&zip, &files, Profile::Both, None).expect_err("落ちること");
        assert_eq!(e.code(), "archive.missing_file");
    }

    /// `TR-PKG-52` (5)。入れたはずのテキストと違えば落ちる。
    #[test]
    fn 入れたテキストと違えば読み戻しで落ちる() {
        let files = tree::build(&bank(), Profile::Both).expect("組み立てられること");
        let zip = pack(&files, "koeru", None);

        let mut tampered = files.clone();
        for f in &mut tampered {
            if f.path == "character.txt" {
                f.content = Content::Bytes(b"name=\xe3\x81\xa0\xe3\x82\x8c\r\n".to_vec());
            }
        }
        let e = verify(&zip, &tampered, Profile::Both, None).expect_err("落ちること");
        assert_eq!(e.code(), "archive.text_mismatch");
    }

    #[test]
    fn マスターは中身を持たず_frq_と画像は読まない() {
        let files = tree::build(&bank(), Profile::Both).expect("組み立てられること");
        let e = expect(&files, "koeru", None);
        for (path, bytes) in &e.files {
            let master = path.ends_with(".wav");
            assert_eq!(bytes.is_none(), master, "{path}");
        }
        assert_eq!(text_encoding_of("koeru/s001.wav", Profile::Both), None);
        assert_eq!(text_encoding_of("koeru/s001.frq", Profile::Both), None);
        assert_eq!(text_encoding_of("koeru/icon.bmp", Profile::Both), None);
        assert_eq!(
            text_encoding_of("koeru/character.yaml", Profile::Both),
            Some(TextEncoding::Utf8),
            "character.yaml は常に UTF-8"
        );
        assert_eq!(
            text_encoding_of("koeru/oto.ini", Profile::Both),
            Some(Profile::Both.encoding())
        );
    }

    #[test]
    fn install_txt_は4項目だけ() {
        let bytes = install_txt(&bank(), Profile::Both).expect("書けること");
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
        let mut b = bank();
        b.character.name = "こえる\nちゃん".to_owned();
        let bytes = install_txt(&b, Profile::Both).expect("書けること");
        let text = text::decode(&bytes, Profile::Both.encoding()).expect("読めること");
        assert!(!text.contains("description="));
    }
}
