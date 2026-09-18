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
use koeru_package::coverage::Coverage;
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
    /// この音源の方式に足りていないエイリアス（`TR-PKG-23`）。
    ///
    /// 全件持つ。 「あと3件」だけでは何を録ればよいか分からない。
    pub missing_aliases: Vec<String>,
    /// この方式の必要エイリアス表を持っているか（`TR-RCL-02`）。
    ///
    /// CVVC は VC 単位をインベントリが持っていないので表が無い。
    /// **無いことを「足りている」と読まない**——被覆を確かめずに出すことになる。
    pub required_table_known: bool,
    /// 原音設定の確認が済んでいるか（`INV-ALN-003`）。
    ///
    /// 関門そのものは `Studio` が持つが、判定はここへ畳む。
    /// **押せるのに必ず失敗する的を出さない。**
    ///
    /// 素材の名前（`TR-REC-32`）はここに入れない。 あれを判定するには
    /// 先に直しを走らせる必要があり（`preflight` が名前を付け替える）、
    /// 状態を引くだけのものが書き換えることになる。**同じことを2箇所で
    /// 判定すると、どちらが先に走ったかで答えが変わる。** 名前は
    /// `preflight` が正本で、画面はその答えと突き合わせる。
    pub otos_ready: bool,
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
    /// 書き出してよいか（`REQ-PKG-104`, `INV-PKG-102`）。
    ///
    /// **被覆が満ちていなければ出さない。** `TR-PKG-23` が「部分的な
    /// パッケージを出さない」と定めていて、`INV-PKG-002` は「書き出せるのは
    /// 完成しているときだけ」。半分録ったところで作れると、受け手には
    /// 「ほとんどの音が無い音源」が渡る。
    #[must_use]
    pub fn may_export(&self) -> bool {
        self.findings.is_empty()
            && self.unencodable.is_empty()
            && self.required_table_known
            && self.missing_aliases.is_empty()
            && self.otos_ready
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

/// 書き出しの手前にある、この層の外の関門。
///
/// 原音設定の確認（`INV-ALN-003`）は `Studio` が持っている。判定だけを
/// 渡してもらい、[`PackageState`] に畳む——**画面が「押せるのに必ず失敗する
/// 的」を出さないため。**
#[derive(Debug, Clone, Copy)]
pub struct Gates {
    /// 原音設定の確認が済んでいるか。
    pub otos_ready: bool,
}

/// 保存してあるプロファイルを解く（`TR-PKG-12`）。
///
/// **知らない名前を既定へ倒さない。** `Profile::parse` が `None` を返すのは
/// 「その名前を知らない」という意味で、両対応のつもりで CP932 を出すのとは
/// 違う。新しい版が書いた値や、打ち間違えた値が黙って別の符号化になる。
///
/// # Errors
///
/// 保存してある名前が3つのどれでもない。
fn resolved_profile(d: &Distribution) -> Result<Profile> {
    Profile::parse(&d.profile).ok_or_else(|| {
        AppError::new(
            "package.unknown_profile",
            "保存してある書き出し方が分からない",
        )
    })
}

/// この音源の方式の被覆（`TR-PKG-23`）。要求表を持っていなければ `None`。
fn coverage_of(ledger: &mut Ledger, manifest: &Manifest) -> Result<Option<Coverage>> {
    let covered = ledger.covered_units()?;
    Ok(koeru_package::coverage::coverage(
        alias_method(manifest.method),
        koeru_core::inventory::UnitSet::Core,
        &covered,
    ))
}

/// いま書き出せるかを調べる（`TR-PKG-49`）。
#[tracing::instrument(skip(dir, ledger, manifest), err)]
pub fn state(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    manifest: &Manifest,
    gates: Gates,
) -> Result<PackageState> {
    let distribution = settings(ledger, manifest)?;
    let profile = resolved_profile(&distribution)?;
    let rows_by_file = ledger
        .distribution_samples()?
        .into_iter()
        .map(|s| (format!("{}.wav", s.file_stem), s.row_id))
        .collect();
    let bank = bank_of(dir, ledger, manifest, &distribution)?;

    let coverage = coverage_of(ledger, manifest)?;
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
        missing_aliases: coverage
            .as_ref()
            .map(|c| c.missing.clone())
            .unwrap_or_default(),
        required_table_known: coverage.is_some(),
        otos_ready: gates.otos_ready,
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
///
/// `version` は NFC 済みで渡す（`TR-PKG-11`）。正規化は呼び出し口が持つ。
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
    let profile = resolved_profile(&distribution)?;
    if !profile::is_available(profile) {
        return Err(AppError::new(
            "package.profile_unavailable",
            "この書き出し方は、いまの環境では使えない",
        ));
    }

    // 部分的なパッケージを出さない（`TR-PKG-23`, `INV-PKG-102`）。
    // 画面の関門と別に見る。 コマンドを直に叩かれても素通りさせない。
    let Some(coverage) = coverage_of(ledger, manifest)? else {
        return Err(AppError::new(
            "package.no_required_table",
            "この作り方に必要な音の表をまだ持っていない",
        ));
    };
    if !coverage.is_complete() {
        return Err(AppError::new(
            "package.incomplete_coverage",
            format!("まだ録れていない音が {} 件ある", coverage.missing.len()),
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

    // 番号を先に見て、最終名まで作ってから台帳へ書く。
    //
    // **記録が先だと、改名に落ちた回の記録だけが残る。** 履歴には
    // 書き出したと書いてあるのに `exports/` に何も無い状態は、
    // リリースレコードが不変なので（`TR-PKG-44`）あとから消せない。
    let seq = ledger.next_release_seq()?;
    let base = archive_base_name(seq, version);
    let final_written = Written {
        zip: exports.join(format!("{base}.{}", archive::ZIP_EXT)),
        uar: exports.join(format!("{base}.{}", archive::UAR_EXT)),
    };
    // 同じ名前の置き土産があれば先に捨てる。
    //
    // **記録の無いファイルは孤児。** 連番は台帳の最大値の次なので、
    // そこに既にファイルがあるということは、前の回が改名まで進んで記録の
    // 手前で落ちたということ。黙って上書きせず、捨ててから置き直す。
    discard(&final_written);
    if let Err(e) = rename_both(&written, &final_written) {
        // 片方だけ動いた状態を残さない。 どちらの名前も当てにならなくなる。
        discard(&written);
        discard(&final_written);
        return Err(e.into());
    }

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
    );
    let release = match release {
        Ok(r) => r,
        Err(e) => {
            // 記録の無いファイルを残さない。 次の回が同じ番号を採って、
            // 別の内容が同じ名前で並ぶ（`TR-PKG-46`）。
            discard(&final_written);
            return Err(e.into());
        }
    };

    Ok(Exported {
        written: final_written,
        release,
    })
}

/// ZIP と UAR を、同じ回の名前へまとめて移す。
fn rename_both(from: &Written, to: &Written) -> std::io::Result<()> {
    std::fs::rename(&from.zip, &to.zip)?;
    std::fs::rename(&from.uar, &to.uar)
}

/// 中途半端に残ったものを片付ける。
fn discard(w: &Written) {
    let _ = std::fs::remove_file(&w.zip);
    let _ = std::fs::remove_file(&w.uar);
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
        .map(|s| {
            // 表を書けなかったことを `None` に畳まない（`TR-PKG-05`）。
            // 畳むと、同梱すると書いてある readme と中身が食い違ったまま出る。
            let frq = s.frq.map(|f| f.to_bytes()).transpose()?;
            Ok(Sample {
                file: format!("{}.wav", s.file_stem),
                master: root.join(&s.rel_path),
                frq,
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
        })
        .collect::<std::result::Result<Vec<_>, koeru_core::frq::FrqError>>()
        .map_err(|e| AppError::new(e.kind(), e))?;

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

/// manifest の方式を、被覆の判定が使う方式へ写す。
///
/// 多音階連続音は連続音と同じエイリアス表を要求する。 音階数は独立した軸で、
/// 方式の一員ではない（`DEC-PKG-005`）。
const fn alias_method(m: Method) -> koeru_core::alias::Method {
    match m {
        Method::Single => koeru_core::alias::Method::Single,
        Method::Sequential | Method::MultiPitchSequential => koeru_core::alias::Method::Sequential,
        Method::Cvvc => koeru_core::alias::Method::Cvvc,
    }
}

/// 被覆の判定が使う方式を、manifest の方式へ写す。
const fn project_method(m: koeru_core::alias::Method) -> Method {
    match m {
        koeru_core::alias::Method::Single => Method::Single,
        koeru_core::alias::Method::Sequential => Method::Sequential,
        koeru_core::alias::Method::Cvvc => Method::Cvvc,
    }
}

/// 配布に出す値を保存する前に確かめる（`TR-PKG-16`, `TR-PKG-12`）。
///
/// 使えない配布名と、知らない書き出し方を保存させない。 保存できてしまうと、
/// 書き出しの直前まで気づかない。呼び出し口は文字列で受けるので
/// （`set_package_settings`）、ここが最後の関門。
///
/// # Errors
///
/// 配布名が `TR-PKG-16` の条件を満たさない、書き出し方が3つのどれでもない。
pub fn check_settings(d: &Distribution) -> Result<()> {
    if let Some(p) = names::check_segment(&d.distribution_name).first() {
        return Err(AppError::new(p.kind(), "その配布名は使えない"));
    }
    resolved_profile(d)?;
    Ok(())
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
    let profile = resolved_profile(&d)?;
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
