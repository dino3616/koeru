//! 書き出しのファイル入出力（`TR-PKG-52`、`INV-PKG-103`）。
//!
//! 方針は `koeru-package`（何を入れ、何と突き合わせるか）、符号化は `koeru-formats`
//! （WAV と ZIP）が持ち、どちらもファイルを開かない。 ここがファイルを開く側で、
//! 書き出し先の作成、素材を開いて 16 bit へ落とすこと、書いた ZIP を開き直して
//! 読み戻すこと、通らなければ消すこと、を受け持つ。 `koeru-audio` は引かない。
//!
//! **移行中。** `koeru-runtime` へ移す（T08e-3）。 そのときは、この層の外（`packaging.rs`）
//! が引いている3つの入口（[`master_facts`]・[`write_and_verify`]・[`ExportError`]）が
//! 動かないまま、ここの中身だけが動く形にしてある。

use std::borrow::Cow;
use std::fs::{self, File};
use std::path::Path;

use koeru_formats::archive::{self, ArchiveError, Entry};
use koeru_formats::text::TextError;
use koeru_formats::wav::{self, WavError};
use koeru_package::archive::{UAR_EXT, Written, ZIP_EXT, expect, install_txt, text_encoding_of};
use koeru_package::bank::VoiceBank;
use koeru_package::profile::Profile;
use koeru_package::tree::{Content, PackagedFile};
use koeru_package::validate::{self, MasterFact, MasterFacts};

/// 書き出しのファイル入出力が失敗した理由。
///
/// 下の層の code と分類は、そのまま通す。 どの層で落ちたかで code が変わらない。
#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// ZIP の組み立て、読み戻し検証、書き出し先の操作。
    #[error("アーカイブを書けない")]
    Archive(#[from] ArchiveError),

    /// マスター WAV を読めない。
    #[error("マスター WAV を読めない")]
    Wav(#[from] WavError),

    /// テキストを書けない。
    #[error("この符号化で書けない文字がある")]
    Text(#[from] TextError),
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        Self::Archive(e.into())
    }
}

impl koeru_failure::Failure for ExportError {
    fn code(&self) -> &'static str {
        match self {
            Self::Archive(e) => e.code(),
            Self::Wav(e) => e.code(),
            Self::Text(e) => e.code(),
        }
    }

    fn class(&self) -> koeru_failure::Class {
        match self {
            Self::Archive(e) => e.class(),
            Self::Wav(e) => e.class(),
            Self::Text(e) => e.class(),
        }
    }
}

type Result<T> = std::result::Result<T, ExportError>;

/// マスター1本のヘッダを読んで、検証が要る事実にする（`TR-PKG-49`）。
///
/// サンプルは読まない。 画面の状態を引くたびに全テイクを読み直さないため。
/// ヘッダが宣言した長さにファイルが足りているかまでは見る。
#[must_use]
pub fn probe_master(path: &Path) -> MasterFact {
    if !path.is_file() {
        return MasterFact::Missing;
    }
    let header = File::open(path)
        .map_err(|source| WavError::Io { op: "open", source })
        .and_then(|mut file| wav::inspect(&mut file));
    MasterFact::from_header(&header)
}

/// `bank` の素材それぞれの事実を、ファイルから集める（`validate::validate` へ渡す）。
#[must_use]
pub fn master_facts(bank: &VoiceBank) -> MasterFacts {
    validate::collect_facts(bank, probe_master)
}

/// ZIP と UAR を書き、どちらも読み戻して確かめる（`TR-PKG-52`, `DEC-PKG-010`）。
///
/// 失敗したら両方消す。 片方だけ残すと、中身の違う2つが配られる（`INV-PKG-103`）。
///
/// # Errors
///
/// 素材を読めない、ZIP を組み立てられない、読み戻し検証に通らない。
// 配布名も版の札も、本人が書いた自由文。トレースへ載せない（`AGENTS.md` #3）。
#[tracing::instrument(
    skip(bank, files, dest_dir, base_name),
    fields(profile = profile.as_str(), entries = files.len())
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
/// マスターは取り出すたびに開いて 16 bit へ落とす。 全部を一度に持たない。
fn write(files: &[PackagedFile], root: &str, install: Option<&[u8]>, dest: &Path) -> Result<()> {
    let entries = files.iter().map(|f| {
        let body = match &f.content {
            Content::Bytes(b) => Cow::Borrowed(b.as_slice()),
            Content::Master(path) => Cow::Owned(distribution_wav(path)?),
        };
        Ok::<_, ExportError>(Entry {
            path: &f.path,
            compression: f.compression,
            body,
        })
    });
    archive::write(File::create(dest)?, root, install, entries)?;
    Ok(())
}

/// マスターを開き、配布用の WAV のバイト列にする（`TR-PKG-20`）。
///
/// マスターは 32 bit float。配布は 16 bit。 レートは `distribution_bytes` が 44100 で
/// 固定する。
///
/// **ディザを入れる。** `TR-REC-37` が「TPDF ディザ（振幅 1 LSB）のみを適用する」と
/// 定めている。切ると、静かなところの量子化誤差が信号と相関して歪みとして聞こえる。
fn distribution_wav(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path).map_err(|source| WavError::Io { op: "open", source })?;
    let master = wav::read(&mut file)?;
    Ok(wav::distribution_bytes(&master.samples, true))
}

/// 書いたアーカイブを開き直して読み戻す（`TR-PKG-52`）。
fn verify(
    path: &Path,
    files: &[PackagedFile],
    root: &str,
    profile: Profile,
    install: Option<&[u8]>,
) -> Result<()> {
    archive::verify(File::open(path)?, &expect(files, root, install), |name| {
        text_encoding_of(name, profile)
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_failure::Failure as _;
    use koeru_formats::oto_ini::IniEntry;
    use koeru_model::alias::Method;
    use koeru_model::oto::Oto;
    use koeru_package::bank::{Character, Readme, Sample, Subbank};
    use koeru_package::tree;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn tmp(tag: &str) -> PathBuf {
        static N: AtomicU32 = AtomicU32::new(0);
        let n = N.fetch_add(1, Ordering::Relaxed);
        let d =
            std::env::temp_dir().join(format!("koeru-export-io-{}-{tag}-{n}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
        d
    }

    /// 疑似乱数と正弦波の入力。 ディザの効きが見え、サンプルごとに値が違う。
    fn signal(seed: u32, n: usize) -> Vec<f32> {
        let mut s = seed;
        (0..n)
            .map(|i| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                let noise = (s >> 16) as f32 / 65536.0 - 0.5;
                (i as f32 * 0.05).sin() * 0.4 + noise * 0.05
            })
            .collect()
    }

    /// `samples` 点のマスターを覆う周波数表。 検証（`TR-PKG-05`、`TR-PKG-49`）を通る形。
    fn frq_for(samples: usize) -> Vec<u8> {
        let frames = samples.div_ceil(koeru_formats::frq::HOP_SIZE as usize);
        let mut out = Vec::new();
        out.extend_from_slice(b"FREQ0003");
        out.extend_from_slice(&koeru_formats::frq::HOP_SIZE.to_le_bytes());
        out.extend_from_slice(&440.0_f64.to_le_bytes());
        out.extend_from_slice(&[0_u8; 16]);
        out.extend_from_slice(&u32::try_from(frames).unwrap_or(0).to_le_bytes());
        for _ in 0..frames {
            out.extend_from_slice(&220.0_f64.to_le_bytes());
            out.extend_from_slice(&0.1_f64.to_le_bytes());
        }
        out
    }

    /// マスター（32 bit float）を置く。 録音と同じ口で作る。
    fn master(dir: &Path, name: &str, rate_hz: u32, samples: &[f32]) -> PathBuf {
        let path = dir.join(name);
        let mut part = koeru_audio::wav::PartialTake::create(&path, rate_hz).expect("開けること");
        part.write(samples).expect("書けること");
        part.finalize().expect("確定できること");
        path
    }

    /// 2区画・3本ずつのマスターを持つ音源。
    fn bank(dir: &Path) -> VoiceBank {
        let subbanks = ["C4", "G4"]
            .iter()
            .enumerate()
            .map(|(k, folder)| Subbank {
                folder: Some((*folder).to_owned()),
                color: String::new(),
                voice_color: None,
                prefix: String::new(),
                suffix: String::new(),
                tone: Some(60 + i32::try_from(k).unwrap_or(0)),
                samples: (0..3_usize)
                    .map(|i| {
                        let file = format!("s{k}{i}.wav");
                        Sample {
                            master: master(
                                dir,
                                &format!("master-{file}"),
                                44_100,
                                &signal(
                                    u32::try_from(k * 10 + i).unwrap_or(0) + 1,
                                    44_100 + i * 1000,
                                ),
                            ),
                            frq: Some(frq_for(44_100 + i * 1000)),
                            entries: vec![IniEntry {
                                file: file.clone(),
                                alias: format!("あ{k}{i}"),
                                oto: Oto {
                                    offset_ms: 10.0,
                                    consonant_ms: 20.0,
                                    cutoff_ms: -50.0,
                                    preutterance_ms: 15.0,
                                    overlap_ms: 5.0,
                                },
                            }],
                            file,
                        }
                    })
                    .collect(),
            })
            .collect();
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            methods: vec![Method::Single],
            tones: vec![60, 61],
            subbanks,
            rules: koeru_model::presamp::Rules::builtin(koeru_model::inventory::UnitSet::Core),
        }
    }

    fn names(path: &Path) -> Vec<String> {
        archive::entry_names(&mut File::open(path).expect("開けること")).expect("読めること")
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

        // UAR だけが install.txt を持つ（`TR-PKG-09`）。
        let (zip, uar) = (names(&w.zip), names(&w.uar));
        assert!(!zip.contains(&"install.txt".to_owned()));
        assert!(uar.contains(&"install.txt".to_owned()));
        assert_eq!(zip.len() + 1, uar.len(), "他の中身は同じ");
        for name in &zip {
            assert!(name.starts_with("koeru/"), "{name}");
            assert!(name.is_ascii(), "{name}");
        }
    }

    /// 同じ入力から同じ ZIP が出る（`TR-EDT-41`）。 2回書き出して、どちらもバイト単位で一致する。
    #[test]
    fn 同じ入力を2回書き出すと_zip_も_uar_もバイト単位で一致する() {
        let d = tmp("golden");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let a =
            write_and_verify(&b, &files, Profile::Both, &d.join("first"), "x").expect("書けること");
        let c = write_and_verify(&b, &files, Profile::Both, &d.join("second"), "x")
            .expect("書けること");
        assert_eq!(
            fs::read(&a.zip).expect("読めること"),
            fs::read(&c.zip).expect("読めること"),
            "ZIP"
        );
        assert_eq!(
            fs::read(&a.uar).expect("読めること"),
            fs::read(&c.uar).expect("読めること"),
            "UAR"
        );
    }

    /// 配布 WAV は、マスターを 16 bit へ落としたものとバイト単位で一致する
    /// （`TR-PKG-20`、`TR-REC-37`）。 WAV は無圧縮で入るので、ZIP の中にそのまま現れる。
    #[test]
    fn zip_の中の配布_wav_はマスターから作った16ビットそのもの() {
        let d = tmp("bits");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        let w = write_and_verify(&b, &files, Profile::Both, &d.join("exports"), "x")
            .expect("書けること");
        let zip = fs::read(&w.zip).expect("読めること");

        for s in b.subbanks.iter().flat_map(|s| &s.samples) {
            let read = koeru_audio::wav::read(&s.master).expect("マスターを読めること");
            let want = wav::distribution_bytes(&read.samples, true);
            let info = wav::parse_header(&want).expect("ヘッダを読めること");
            assert!(info.is_distribution(), "44100 Hz / 16 bit / モノラル");
            assert!(
                zip.windows(want.len()).any(|w| w == want.as_slice()),
                "{} が ZIP の中にそのまま入っている",
                s.file
            );
        }
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
        assert_eq!(e.code(), "archive.missing_file");
        assert!(!dest.join("x.zip").exists());
        assert!(!dest.join("x.uar").exists());
    }

    /// 書いている途中でマスターが読めなくなっても、半端な ZIP を残さない。
    #[test]
    fn 素材が読めなくなったらファイルを残さない() {
        let d = tmp("vanished");
        let b = bank(&d);
        let files = tree::build(&b, Profile::Both).expect("組み立てられること");
        // 組み立てたあとで、マスターの1本が消える。
        fs::remove_file(&b.subbanks[1].samples[2].master).expect("消せること");

        let dest = d.join("exports");
        let e = write_and_verify(&b, &files, Profile::Both, &dest, "x").expect_err("落ちること");
        assert_eq!(e.code(), "wav.io_failed");
        assert!(!dest.join("x.zip").exists());
        assert!(!dest.join("x.uar").exists());
    }

    #[test]
    fn ヘッダの事実はファイルの在り方を言い分ける() {
        let d = tmp("probe");
        let ok = master(&d, "ok.wav", 44_100, &signal(1, 4410));
        let slow = master(&d, "slow.wav", 22_050, &signal(2, 2205));
        let empty = master(&d, "empty.wav", 44_100, &[]);
        let garbage = d.join("garbage.wav");
        fs::write(&garbage, b"RIFF....").expect("書けること");
        let cut = d.join("cut.wav");
        let mut bytes = fs::read(&ok).expect("読めること");
        bytes.truncate(bytes.len() - 8);
        fs::write(&cut, bytes).expect("書けること");

        assert_eq!(
            probe_master(&ok),
            MasterFact::Wav {
                rate_hz: 44_100,
                samples: 4410
            }
        );
        assert_eq!(
            probe_master(&slow),
            MasterFact::Wav {
                rate_hz: 22_050,
                samples: 2205
            }
        );
        assert_eq!(
            probe_master(&empty),
            MasterFact::Wav {
                rate_hz: 44_100,
                samples: 0
            }
        );
        assert_eq!(probe_master(&garbage), MasterFact::Unreadable);
        assert_eq!(probe_master(&cut), MasterFact::Unreadable, "宣言より短い");
        assert_eq!(probe_master(&d.join("nothing.wav")), MasterFact::Missing);
        assert_eq!(
            probe_master(&d),
            MasterFact::Missing,
            "ディレクトリは素材ではない"
        );
    }

    /// 検証が見る事実は、全部読んだときと同じ答えになる。 ヘッダだけにしても、
    /// 44100 でないマスターを書き出しの手前で止める（`TR-REC-02`）。
    #[test]
    fn 検証はヘッダの事実で_44100_でないマスターを止める() {
        let d = tmp("rate");
        let mut b = bank(&d);
        let ok = validate::validate(&b, Profile::Both, &master_facts(&b));
        assert!(ok.may_export(), "{:?}", ok.findings);

        let slow = master(&d, "slow.wav", 22_050, &signal(9, 22_050));
        b.subbanks[0].samples[0].master = slow;

        let report = validate::validate(&b, Profile::Both, &master_facts(&b));
        let kinds: Vec<&str> = report.findings.iter().map(|f| f.problem.kind()).collect();
        assert_eq!(kinds, ["package.wrong_sample_rate"]);
        assert!(!report.may_export());
    }

    /// 検証が要る事実は、全部読んだときのレートと長さと同じ。
    #[test]
    fn ヘッダの事実は全部読んだ結果と一致する() {
        let d = tmp("consistent");
        let b = bank(&d);
        for s in b.subbanks.iter().flat_map(|s| &s.samples) {
            let full = koeru_audio::wav::read(&s.master).expect("読めること");
            assert_eq!(
                probe_master(&s.master),
                MasterFact::Wav {
                    rate_hz: full.rate_hz,
                    samples: full.samples.len() as u64
                }
            );
        }
    }
}
