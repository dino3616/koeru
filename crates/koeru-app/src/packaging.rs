//! 配布パッケージの書き出し（`PROFILE-M4`）。
//!
//! 台帳と manifest から [`VoiceBank`] を組み立て、検証して包む。
//! 規則は `koeru-package` が持ち、ここはその境界。
//!
//! 順序は `packaging-export.fsl` が決めている。 被覆 → 検証 → ZIP →
//! 読み戻し。検証を通っていないものは包まない（`FB-PKG-102`）。

use std::collections::BTreeMap;

use koeru_core::db::{Distribution, Ledger};
use koeru_core::names;
use koeru_core::project::{Manifest, Method, ProjectDir};
use koeru_core::release::{NewRelease, Validation, archive_base_name, content_hash};
use koeru_package::archive::{self, Written};
use koeru_package::bank::{Character, Portrait, Readme, Sample, Subbank, VoiceBank};
use koeru_package::profile::{self, Profile};
use koeru_package::tree::{self, Content, PackagedFile};
use koeru_package::validate::{self, Finding, Unencodable};

use crate::error::{AppError, Result};

/// 書き出しの手前で分かること（`TR-PKG-49`, `TR-PKG-51`）。
#[derive(Debug, Clone)]
pub struct PackageState {
    /// 配布に出す値。まだ決めていなければ既定値。
    pub distribution: Distribution,
    /// 選んでいるプロファイル（`TR-PKG-12`）。
    pub profile: Profile,
    /// 書き出しを止めている違反。
    pub findings: Vec<Finding>,
    /// CP932 で書けない箇所（`TR-PKG-17`）。
    pub unencodable: Vec<Unencodable>,
    /// 出せるようになっている方式（`INV-PKG-105`）。
    pub exportable: Vec<Method>,
    /// 使えるプロファイル。CP932 が壊れていれば減る（`TR-PKG-13`）。
    pub available_profiles: Vec<Profile>,
    /// 配布物に入るファイルの数。
    pub file_count: usize,
    /// 書き出すエイリアスの数。
    pub alias_count: usize,
    /// 配布 WAV の名前から行 ID を引く表（`TR-PKG-51`）。
    ///
    /// 検証結果から、その行の録った回へ入れるようにするために要る。
    /// ファイル名だけを見せて終わらない。
    pub rows_by_file: BTreeMap<String, String>,
}

impl PackageState {
    /// 書き出してよいか（`REQ-PKG-104`）。
    #[must_use]
    pub fn may_export(&self) -> bool {
        self.findings.is_empty()
            && self.unencodable.is_empty()
            && self.alias_count > 0
            && profile::is_available(self.profile)
    }
}

/// 書き出した結果。
#[derive(Debug, Clone)]
pub struct Exported {
    /// ZIP と UAR の在り処。
    pub written: Written,
    /// 台帳に残した記録（`TR-PKG-44`）。
    pub release: koeru_core::release::Release,
}

/// 配布に出す値を読む。無ければ既定値を作る（`DEC-PKG-008`）。
///
/// 既定の配布名は表示名から作る。 空欄から始めさせない。
#[tracing::instrument(skip(ledger, manifest), err)]
pub fn settings(ledger: &mut Ledger, manifest: &Manifest) -> Result<Distribution> {
    Ok(ledger.distribution()?.unwrap_or_else(|| Distribution {
        distribution_name: names::default_distribution_name(&manifest.display_name),
        profile: Profile::default().as_str().to_owned(),
        portrait_opacity: 1.0,
        ..Distribution::default()
    }))
}

/// いま書き出せるかを調べる（`TR-PKG-49`）。
#[tracing::instrument(skip(dir, ledger, manifest), err)]
pub fn state(dir: &ProjectDir, ledger: &mut Ledger, manifest: &Manifest) -> Result<PackageState> {
    let distribution = settings(ledger, manifest)?;
    let profile = Profile::parse(&distribution.profile).unwrap_or_default();
    let rows_by_file = ledger
        .distribution_samples()?
        .into_iter()
        .map(|s| (format!("{}.wav", s.file_stem), s.row_id))
        .collect();
    let bank = bank_of(dir, ledger, manifest, &distribution)?;

    let report = validate::validate(&bank, profile);
    let covered = ledger.covered_units()?;
    let exportable =
        koeru_package::coverage::exportable(koeru_core::inventory::UnitSet::Core, &covered)
            .into_iter()
            .map(project_method)
            .collect();

    // 組み立てて初めて分かる数（同梱物の件数）を出す。 検証が通らない間は
    // 組み立てられないので、そのときは 0。
    let file_count = tree::build(&bank, profile).map(|f| f.len()).unwrap_or(0);

    Ok(PackageState {
        distribution,
        profile,
        findings: report.findings,
        unencodable: report.unencodable,
        exportable,
        available_profiles: [Profile::Classic, Profile::OpenUtau, Profile::Both]
            .into_iter()
            .filter(|p| profile::is_available(*p))
            .collect(),
        file_count,
        alias_count: bank.aliases().len(),
        rows_by_file,
    })
}

/// 書き出す（`REQ-PKG-105`, `REQ-PKG-106`, `TR-PKG-44`）。
///
/// 検証を通らないまま包まない。 読み戻しに落ちたらファイルを残さない。
/// 台帳へ記録するのは、両方が通ってから。
// バージョン文字列は本人が書いた自由文。トレースへ載せない（`AGENTS.md` #3）。
#[tracing::instrument(skip(dir, ledger, manifest, version, released_at), err)]
pub fn export(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    manifest: &Manifest,
    version: &str,
    released_at: &str,
) -> Result<Exported> {
    let distribution = settings(ledger, manifest)?;
    let profile = Profile::parse(&distribution.profile).unwrap_or_default();
    if !profile::is_available(profile) {
        return Err(AppError::new(
            "package.profile_unavailable",
            "この書き出し方は、いまの環境では使えない",
        ));
    }

    let bank = bank_of(dir, ledger, manifest, &distribution)?;
    let report = validate::validate(&bank, profile);
    if !report.may_export() {
        return Err(AppError::new(
            "package.validation_failed",
            "書き出し前の検査に通っていない",
        ));
    }

    let files = tree::build(&bank, profile).map_err(|e| AppError::new(e.kind(), e))?;
    let exports = dir.exports_dir();
    // 前の書き出しが途中で落ちていたら片付ける。 仮の名前が残っていると、
    // 次の書き出しがそれを上書きするのか作り直すのかが読めない。
    for ext in [archive::ZIP_EXT, archive::UAR_EXT] {
        let _ = std::fs::remove_file(exports.join(format!("{PENDING}.{ext}")));
    }

    let written = archive::write_and_verify(&bank, &files, profile, &exports, PENDING)
        .map_err(|e| AppError::new(e.kind(), e))?;

    let release = ledger.record_release(
        &NewRelease {
            version: version.to_owned(),
            method: manifest.method,
            alias_count: i32::try_from(bank.aliases().len()).unwrap_or(i32::MAX),
            validation: Validation::Passed,
            oto_hash: content_hash(&oto_bytes(&files)),
            terms_hash: content_hash(distribution.terms.unwrap_or_default().as_bytes()),
            released_at: released_at.to_owned(),
        },
        archive::ZIP_EXT,
    )?;

    // 番号が決まってから最終名にする。 先に名前を決めると、書き出しに
    // 落ちたときだけ番号が飛ぶ。
    let base = archive_base_name(release.seq, version);
    let final_written = Written {
        zip: exports.join(format!("{base}.{}", archive::ZIP_EXT)),
        uar: exports.join(format!("{base}.{}", archive::UAR_EXT)),
    };
    std::fs::rename(&written.zip, &final_written.zip)?;
    std::fs::rename(&written.uar, &final_written.uar)?;

    Ok(Exported {
        written: final_written,
        release,
    })
}

/// 番号が決まるまでの仮の名前。
const PENDING: &str = "pending";

/// 台帳と manifest から音源1本を組み立てる。
///
/// 区画は1つだけ。 音階の軸は録音リストのプリセットが持つもので、
/// いまあるのは単一音階のプリセットだけ（`PROFILE-M5`）。
/// `koeru-package` の側は多音階を扱えるので、ここが増えるときに繋ぐ。
#[tracing::instrument(skip(dir, ledger, manifest, d), err)]
fn bank_of(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    manifest: &Manifest,
    d: &Distribution,
) -> Result<VoiceBank> {
    let root = dir.root();
    let samples = ledger
        .distribution_samples()?
        .into_iter()
        .map(|s| Sample {
            file: format!("{}.wav", s.file_stem),
            master: root.join(&s.rel_path),
            frq: s.frq.and_then(|f| f.to_bytes().ok()),
            entries: s
                .otos
                .into_iter()
                .map(|(alias, o)| koeru_align::ini::IniEntry {
                    file: format!("{}.wav", s.file_stem),
                    alias,
                    oto: koeru_core::oto::Oto {
                        offset_ms: o.offset_ms,
                        consonant_ms: o.consonant_ms,
                        cutoff_ms: o.cutoff_ms,
                        preutterance_ms: o.preutterance_ms,
                        overlap_ms: o.overlap_ms,
                    },
                })
                .collect(),
        })
        .collect();

    Ok(VoiceBank {
        distribution_name: if d.distribution_name.is_empty() {
            names::default_distribution_name(&manifest.display_name)
        } else {
            d.distribution_name.clone()
        },
        character: Character {
            name: manifest.display_name.clone(),
            localized_names: Vec::new(),
            author: d.author.clone(),
            voice: d.voice.clone(),
            sample: d.sample.clone(),
            web: d.web.clone(),
            version: d.version.clone(),
            icon: d.icon.clone(),
            portrait: d.portrait.clone().map(|png| Portrait {
                png,
                opacity: d.portrait_opacity,
                #[allow(
                    clippy::cast_sign_loss,
                    reason = "負の高さは入力側で弾く。ここでは 0 に倒す"
                )]
                height: d.portrait_height.max(0) as u32,
            }),
        },
        readme: Readme {
            tone_range_note: d.tone_range_note.clone(),
            terms: d.terms.clone(),
            credit_example: d.credit_example.clone(),
            contact: d.contact.clone(),
            disclaimer: d.disclaimer.clone(),
            character_note: d.character_note.clone(),
        },
        method: manifest.method,
        subbanks: vec![Subbank {
            folder: None,
            color: String::new(),
            prefix: String::new(),
            suffix: String::new(),
            tones: Vec::new(),
            samples,
        }],
    })
}

/// 生成した `oto.ini` を1つに繋いだバイト列（`TR-PKG-44`）。
///
/// リリースレコードのハッシュに使う。 取り込み時の差分検出（`TR-PKG-48`）が
/// 同じ並びを前提にしているので、[`tree::build`] が出す順のまま繋ぐ。
fn oto_bytes(files: &[PackagedFile]) -> Vec<u8> {
    let mut out = Vec::new();
    for f in files {
        if !f.path.ends_with("oto.ini") {
            continue;
        }
        if let Content::Bytes(b) = &f.content {
            out.extend_from_slice(b);
        }
    }
    out
}

/// 被覆の判定が使う方式を、manifest の方式へ写す。
const fn project_method(m: koeru_core::alias::Method) -> Method {
    match m {
        koeru_core::alias::Method::Single => Method::Single,
        koeru_core::alias::Method::Sequential => Method::Sequential,
        koeru_core::alias::Method::Cvvc => Method::Cvvc,
    }
}

/// 配布に出す値を保存する前に、名前を確かめる（`TR-PKG-16`）。
///
/// 使えない配布名を保存させない。 保存できてしまうと、書き出しの直前まで
/// 気づかない。
///
/// # Errors
///
/// 配布名が `TR-PKG-16` の条件を満たさない。
pub fn check_distribution_name(name: &str) -> Result<()> {
    let problems = names::check_segment(name);
    match problems.first() {
        None => Ok(()),
        Some(p) => Err(AppError::new(p.kind(), "その配布名は使えない")),
    }
}

/// 音源ルートの中身を、包まずに一覧する。
///
/// 書き出す前に「何が入るか」を見せるために使う（`TR-PKG-28` の同梱物一覧と
/// 同じもの）。
///
/// # Errors
///
/// 組み立てに失敗する条件は [`tree::build`] と同じ。
pub fn preview(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    manifest: &Manifest,
) -> Result<Vec<(String, u64)>> {
    let d = settings(ledger, manifest)?;
    let profile = Profile::parse(&d.profile).unwrap_or_default();
    let bank = bank_of(dir, ledger, manifest, &d)?;
    let files = tree::build(&bank, profile).map_err(|e| AppError::new(e.kind(), e))?;
    Ok(files.iter().map(|f| (f.path.clone(), size_of(f))).collect())
}

/// 配布物に入るときの大きさ（バイト）。
///
/// マスターは 32 bit float なので、16 bit にすると半分になる。
/// ヘッダぶんは数えない——一覧の桁が変わるほどの差ではない。
fn size_of(f: &PackagedFile) -> u64 {
    match &f.content {
        Content::Bytes(b) => b.len() as u64,
        Content::Master(p) => std::fs::metadata(p).map_or(0, |m| m.len() / 2),
    }
}
