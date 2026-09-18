//! 音源ルートの組み立て（`TR-PKG-01`, `TR-PKG-08`, `TR-PKG-10`, `TR-PKG-27`）。
//!
//! 出るのはファイルの一覧で、ディスクには何も書かない。 包むのは
//! [`crate::archive`] の仕事で、ここは「何がどの名前でどう入るか」だけを決める。
//!
//! # ディレクトリ列挙から名前を作らない
//!
//! `TR-PKG-15`。ファイルシステムが返す名前は正規化形が環境で変わる。
//! パスは全て [`crate::bank::VoiceBank`]（内部モデル）から組み立てる。

use std::path::PathBuf;

use koeru_align::ini::{self, IniEntry};
use koeru_core::names::{self, NameProblem};
use koeru_core::text::{TextEncoding, TextError};

use crate::bank::{Subbank, VoiceBank, decorate};
use crate::character::{self, ICON_FILE, PORTRAIT_FILE};
use crate::icon::{self, IconError};
use crate::profile::Profile;
use crate::readme;

/// 配布物に入る1ファイル。
#[derive(Debug, Clone)]
pub struct PackagedFile {
    /// 音源ルートフォルダを基点にした相対パス。区切りは `/`（`TR-PKG-26`）。
    pub path: String,
    /// 中身。
    pub content: Content,
    /// 圧縮方式（`TR-PKG-27`）。
    pub compression: Compression,
}

/// ファイルの中身。
#[derive(Debug, Clone)]
pub enum Content {
    /// 生成したバイト列（テキスト・BMP・PNG・`.frq`）。
    Bytes(Vec<u8>),
    /// マスター WAV。包むときに 16 bit へ落とす（`TR-PKG-20`）。
    ///
    /// 全テイクを一度に載せない。 3時間の収録では数百 MB になる。
    Master(PathBuf),
}

/// 圧縮方式（`TR-PKG-27`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// 無圧縮。`.wav` と `.frq`。
    Store,
    /// Deflate。テキスト・YAML・画像。
    Deflate,
}

/// 組み立てに失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    /// 生成する名前が `TR-PKG-16` の条件を満たさない。
    #[error("生成する名前が使えない")]
    Name {
        /// 音源ルートからの相対パス、または区画の名前。
        path: String,
        problems: Vec<NameProblem>,
    },

    /// 選んだプロファイルの符号化で書けない文字がある（`TR-PKG-17`）。
    #[error("この符号化で書けない文字がある")]
    Text(#[from] TextError),

    /// 音源アイコンを作れない（`TR-PKG-07`）。
    #[error("音源アイコンを作れない")]
    Icon(#[from] IconError),

    /// `oto.ini` を書けない。
    #[error("oto.ini を書けない")]
    Ini(#[from] ini::IniError),
}

impl BuildError {
    /// 送信してよい種別文字列。パスも名前も送らない（`AGENTS.md` #3）。
    #[must_use]
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Name { .. } => "package.unsafe_name",
            Self::Text(e) => e.kind(),
            Self::Icon(e) => e.kind(),
            Self::Ini(e) => e.kind(),
        }
    }
}

type Result<T> = std::result::Result<T, BuildError>;

/// 音源ルートの中身を組み立てる（`TR-PKG-01`）。
///
/// 出る順は決まっている。 同じ音源からは同じ並びが出るので、
/// 2回書き出したときに ZIP のエントリ順が動かない。
///
/// # Errors
///
/// 生成する名前が `TR-PKG-16` を満たさない、選んだ符号化で書けない文字が
/// ある、アイコンを変換できない。
#[tracing::instrument(skip(bank), fields(profile = profile.as_str()), err)]
pub fn build(bank: &VoiceBank, profile: Profile) -> Result<Vec<PackagedFile>> {
    check_names(bank)?;

    let icon = bank
        .character
        .icon
        .as_deref()
        .map(icon::to_bmp)
        .transpose()?;
    let mut files = Vec::new();

    // 素材を先に積む。 readme の同梱物一覧に何が入るかは、これが決める。
    for s in &bank.subbanks {
        let prefix = s.path_prefix();
        for m in &s.samples {
            files.push(PackagedFile {
                path: format!("{prefix}{}", m.file),
                content: Content::Master(m.master.clone()),
                compression: Compression::Store,
            });
            if let Some(frq) = &m.frq {
                files.push(PackagedFile {
                    path: format!("{prefix}{}", frq_name(&m.file)),
                    content: Content::Bytes(frq.clone()),
                    compression: Compression::Store,
                });
            }
        }
        // WAV を含むフォルダごとに oto.ini を1つ（`TR-PKG-01`）。
        if !s.samples.is_empty() {
            files.push(PackagedFile {
                path: format!("{prefix}oto.ini"),
                content: Content::Bytes(oto_ini(s, profile)?),
                compression: Compression::Deflate,
            });
        }
    }

    let mut root = Vec::new();
    root.push(text_file(
        "character.txt",
        &character::character_txt(&bank.character, icon.is_some()),
        profile,
    )?);
    // `character.yaml` だけは常に UTF-8（`TR-PKG-12`）。
    // (a) が CP932 にすると挙げているのは character.txt / oto.ini /
    // prefix.map / readme.txt の4つで、ここは入っていない。読むのは
    // OpenUtau だけで、`text_file_encoding` はこのファイル自身ではなく
    // **他のテキストの符号化**を宣言する欄。
    root.push(encoded_file(
        "character.yaml",
        &character::character_yaml(bank, profile, icon.is_some()),
        TextEncoding::Utf8,
    )?);
    if let Some(map) = character::prefix_map(bank) {
        root.push(text_file("prefix.map", &map, profile)?);
    }
    if let Some(bmp) = icon {
        root.push(PackagedFile {
            path: ICON_FILE.to_owned(),
            content: Content::Bytes(bmp),
            compression: Compression::Deflate,
        });
    }
    if let Some(p) = &bank.character.portrait {
        root.push(PackagedFile {
            path: PORTRAIT_FILE.to_owned(),
            content: Content::Bytes(p.png.clone()),
            compression: Compression::Deflate,
        });
    }

    let summary = contents_summary(&root, &files);
    root.push(text_file(
        "readme.txt",
        &readme::readme_txt(bank, &summary),
        profile,
    )?);

    root.extend(files);
    Ok(root)
}

/// 音源ルート直下に出しうるファイル。
///
/// ここに並んでいないものは出ない。 `$read` を生成しないこと（`TR-PKG-10`）を、
/// 一覧として確かめられるようにしておく。
pub const ROOT_FILES: [&str; 6] = [
    "character.txt",
    "character.yaml",
    "readme.txt",
    "prefix.map",
    ICON_FILE,
    PORTRAIT_FILE,
];

/// WAV 名から `.frq` 名を作る（`TR-PKG-05`）。
fn frq_name(wav: &str) -> String {
    let stem = wav.strip_suffix(".wav").unwrap_or(wav);
    format!("{stem}_wav.frq")
}

/// 区画の `oto.ini`（`TR-PKG-08`, `TR-PKG-19`）。
///
/// `<wav>` 欄は同一フォルダ内のファイル名だけ。 区画の prefix / suffix を
/// 付けたエイリアスで書く——付けないと、フォルダ間で同名になる。
fn oto_ini(s: &Subbank, profile: Profile) -> Result<Vec<u8>> {
    let entries: Vec<IniEntry> = s
        .samples
        .iter()
        .flat_map(|m| {
            m.entries.iter().map(|e| IniEntry {
                file: m.file.clone(),
                alias: decorate(s, &e.alias),
                oto: e.oto,
            })
        })
        .collect();
    Ok(ini::write(&entries, profile.encoding())?)
}

fn text_file(path: &str, body: &str, profile: Profile) -> Result<PackagedFile> {
    encoded_file(path, body, profile.encoding())
}

fn encoded_file(path: &str, body: &str, enc: TextEncoding) -> Result<PackagedFile> {
    Ok(PackagedFile {
        path: path.to_owned(),
        content: Content::Bytes(koeru_core::text::encode(body, enc)?),
        compression: Compression::Deflate,
    })
}

/// 生成する名前を全部見る（`TR-PKG-16`）。
///
/// 区画のフォルダ名と WAV 名だけでなく、配布名（音源ルートフォルダ名）も
/// 見る。ルート名が使えないと、ZIP のエントリが全部使えない。
fn check_names(bank: &VoiceBank) -> Result<()> {
    let problems = names::check_segment(&bank.distribution_name);
    if !problems.is_empty() {
        return Err(BuildError::Name {
            path: bank.distribution_name.clone(),
            problems,
        });
    }
    for s in &bank.subbanks {
        if let Some(folder) = &s.folder {
            let problems = names::check_segment(folder);
            if !problems.is_empty() {
                return Err(BuildError::Name {
                    path: folder.clone(),
                    problems,
                });
            }
        }
        for m in &s.samples {
            for name in [m.file.clone(), frq_name(&m.file)] {
                let problems = names::check_file_name(&name);
                if !problems.is_empty() {
                    return Err(BuildError::Name {
                        path: format!("{}{name}", s.path_prefix()),
                        problems,
                    });
                }
            }
        }
    }
    Ok(())
}

/// readme の同梱物一覧（`TR-PKG-28`）。
///
/// 1ファイル1行にしない。 102 本の WAV 名を並べても受け手は読まないし、
/// 名前が変わるたびに readme の差分が 100 行出る。
fn contents_summary(root: &[PackagedFile], samples: &[PackagedFile]) -> Vec<String> {
    let count = |ext: &str| samples.iter().filter(|f| f.path.ends_with(ext)).count();
    let mut out: Vec<String> = root.iter().map(|f| f.path.clone()).collect();
    // readme 自身はまだ積んでいないので、ここで足す。
    out.push("readme.txt".to_owned());
    out.sort();

    let wavs = count(".wav");
    if wavs > 0 {
        out.push(format!("音声ファイル（.wav） {wavs} 本"));
    }
    let frqs = count(".frq");
    if frqs > 0 {
        out.push(format!("周波数表（.frq） {frqs} 本"));
    }
    let otos = samples
        .iter()
        .filter(|f| f.path.ends_with("oto.ini"))
        .count();
    if otos > 0 {
        out.push(format!("oto.ini {otos} 個"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Character, Readme, Sample};
    use koeru_core::oto::Oto;
    use koeru_core::project::Method;

    fn oto() -> Oto {
        Oto {
            offset_ms: 100.0,
            consonant_ms: 60.0,
            cutoff_ms: -300.0,
            preutterance_ms: 40.0,
            overlap_ms: 20.0,
        }
    }

    fn sample(file: &str, alias: &str, frq: bool) -> Sample {
        Sample {
            file: file.to_owned(),
            master: PathBuf::from(file),
            frq: frq.then(|| vec![0_u8; 8]),
            entries: vec![IniEntry {
                file: file.to_owned(),
                alias: alias.to_owned(),
                oto: oto(),
            }],
        }
    }

    fn subbank(folder: Option<&str>, prefix: &str, samples: Vec<Sample>) -> Subbank {
        Subbank {
            folder: folder.map(str::to_owned),
            color: folder.unwrap_or_default().to_owned(),
            prefix: prefix.to_owned(),
            suffix: String::new(),
            tones: if folder.is_some() {
                vec![60]
            } else {
                Vec::new()
            },
            samples,
        }
    }

    fn bank(subbanks: Vec<Subbank>) -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            method: Method::Single,
            subbanks,
        }
    }

    fn paths(files: &[PackagedFile]) -> Vec<&str> {
        files.iter().map(|f| f.path.as_str()).collect()
    }

    #[test]
    fn 必須の成果物が出る() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", true)],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let p = paths(&files);
        for want in [
            "character.txt",
            "character.yaml",
            "readme.txt",
            "oto.ini",
            "s001.wav",
            "s001_wav.frq",
        ] {
            assert!(p.contains(&want), "{want} が無い");
        }
    }

    /// `TR-PKG-10` と `TR-PKG-06`。作らないものは一覧に出てこない。
    #[test]
    fn 作らない形式は出ない() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", true)],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        for ng in ["$read", ".frc", ".llsm", ".pmk", ".vs4ufrq", "desc.mrq"] {
            assert!(
                !files.iter().any(|f| f.path.contains(ng)),
                "{ng} を出してはいけない"
            );
        }
    }

    /// `TR-PKG-27`。音は無圧縮、テキストは Deflate。
    #[test]
    fn 圧縮方式を種別で分ける() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", true)],
        )]);
        for f in build(&b, Profile::Both).expect("組み立てられること") {
            let want = if f.path.ends_with(".wav") || f.path.ends_with(".frq") {
                Compression::Store
            } else {
                Compression::Deflate
            };
            assert_eq!(f.compression, want, "{}", f.path);
        }
    }

    /// `TR-PKG-01`。WAV を含むフォルダごとに oto.ini が1つ。
    #[test]
    fn 多音階ではフォルダごとに_oto_ini_が出る() {
        let b = bank(vec![
            subbank(Some("C4"), "", vec![sample("s001.wav", "あ", false)]),
            subbank(Some("G4"), "↑", vec![sample("s001.wav", "あ", false)]),
        ]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let p = paths(&files);
        assert!(p.contains(&"C4/oto.ini"));
        assert!(p.contains(&"G4/oto.ini"));
        assert!(p.contains(&"prefix.map"));
        assert!(!p.contains(&"oto.ini"), "ルート直下には出さない");
    }

    /// `TR-PKG-08`。`<wav>` 欄はフォルダ内の相対名だけ。
    #[test]
    fn oto_ini_の_wav_欄に区切りを入れない() {
        let b = bank(vec![subbank(
            Some("C4"),
            "",
            vec![sample("s001.wav", "あ", false)],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let ini = files
            .iter()
            .find(|f| f.path == "C4/oto.ini")
            .expect("oto.ini があること");
        let Content::Bytes(bytes) = &ini.content else {
            panic!("生成したバイト列であること");
        };
        let text = koeru_core::text::decode(bytes, Profile::Both.encoding()).expect("読めること");
        assert!(text.starts_with("s001.wav="), "{text}");
        assert!(!text.contains('/') && !text.contains('\\'));
    }

    /// `TR-PKG-12`。`character.yaml` だけは、どのプロファイルでも UTF-8。
    #[test]
    fn character_yaml_は常に_utf8() {
        let mut b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", false)],
        )]);
        b.character.name = "こえる".to_owned();
        for p in [Profile::Classic, Profile::OpenUtau, Profile::Both] {
            let files = build(&b, p).expect("組み立てられること");
            let yaml = files
                .iter()
                .find(|f| f.path == "character.yaml")
                .expect("character.yaml があること");
            let Content::Bytes(bytes) = &yaml.content else {
                panic!("生成したバイト列であること");
            };
            assert!(
                std::str::from_utf8(bytes).is_ok(),
                "{} で UTF-8 になっていない",
                p.as_str()
            );

            // 宣言する符号化は、他のテキストのもの。ここ自身ではない。
            let txt = files
                .iter()
                .find(|f| f.path == "character.txt")
                .expect("character.txt があること");
            let Content::Bytes(txt_bytes) = &txt.content else {
                panic!("生成したバイト列であること");
            };
            let declared = std::str::from_utf8(bytes)
                .expect("UTF-8")
                .contains(&format!("text_file_encoding: {}", p.declared_encoding()));
            assert!(declared, "{} の宣言が無い", p.as_str());
            assert!(
                koeru_core::text::decode(txt_bytes, p.encoding()).is_ok(),
                "宣言どおりに読めること"
            );
        }
    }

    /// `TR-PKG-21`。生成する `oto.ini` に `#Charset:` を書かない。
    #[test]
    fn oto_ini_に_charset_行を書かない() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", false)],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let ini = files
            .iter()
            .find(|f| f.path == "oto.ini")
            .expect("oto.ini があること");
        let Content::Bytes(bytes) = &ini.content else {
            panic!("生成したバイト列であること");
        };
        let text = koeru_core::text::decode(bytes, Profile::Both.encoding()).expect("読めること");
        assert!(!text.contains("#Charset:"), "{text}");
    }

    /// `TR-PKG-19`。区画をまたいでエイリアスが衝突しない。
    #[test]
    fn 多音階の_oto_には接頭辞が付く() {
        let b = bank(vec![subbank(
            Some("G4"),
            "↑",
            vec![sample("s001.wav", "あ", false)],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let ini = files
            .iter()
            .find(|f| f.path == "G4/oto.ini")
            .expect("oto.ini があること");
        let Content::Bytes(bytes) = &ini.content else {
            panic!("生成したバイト列であること");
        };
        let text = koeru_core::text::decode(bytes, Profile::Both.encoding()).expect("読めること");
        assert!(text.contains("=↑あ,"), "{text}");
    }

    #[test]
    fn 使えない名前は組み立てで止まる() {
        let b = bank(vec![subbank(None, "", vec![sample("あ.wav", "あ", false)])]);
        let e = build(&b, Profile::Both).expect_err("止まること");
        assert_eq!(e.kind(), "package.unsafe_name");
    }

    #[test]
    fn 配布名も名前の検査を通る() {
        let mut b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", false)],
        )]);
        b.distribution_name = "こえる".to_owned();
        assert_eq!(
            build(&b, Profile::Both).expect_err("止まること").kind(),
            "package.unsafe_name"
        );
    }

    /// `TR-PKG-17`。CP932 で書けない名前は、置換せずに止める。
    #[test]
    fn cp932_で書けない音源名は止まる() {
        let mut b = bank(vec![subbank(
            None,
            "",
            vec![sample("s001.wav", "あ", false)],
        )]);
        b.character.name = "こえる🎤".to_owned();
        assert_eq!(
            build(&b, Profile::Classic).expect_err("止まること").kind(),
            "text.unencodable"
        );
        build(&b, Profile::OpenUtau).expect("UTF-8 なら通ること");
    }

    #[test]
    fn 並びは決まっている() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![
                sample("s001.wav", "あ", true),
                sample("s002.wav", "い", true),
            ],
        )]);
        let a = paths(&build(&b, Profile::Both).expect("組み立てられること")).join(",");
        let c = paths(&build(&b, Profile::Both).expect("組み立てられること")).join(",");
        assert_eq!(a, c);
    }

    #[test]
    fn 同梱物一覧は数でまとめる() {
        let b = bank(vec![subbank(
            None,
            "",
            vec![
                sample("s001.wav", "あ", true),
                sample("s002.wav", "い", true),
            ],
        )]);
        let files = build(&b, Profile::Both).expect("組み立てられること");
        let r = files
            .iter()
            .find(|f| f.path == "readme.txt")
            .expect("readme があること");
        let Content::Bytes(bytes) = &r.content else {
            panic!("生成したバイト列であること");
        };
        let text = koeru_core::text::decode(bytes, Profile::Both.encoding()).expect("読めること");
        assert!(text.contains("音声ファイル（.wav） 2 本"), "{text}");
        assert!(!text.contains("s001.wav"), "個々の名前は並べない");
    }
}
