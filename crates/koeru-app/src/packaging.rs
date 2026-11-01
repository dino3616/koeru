//! 配布パッケージの書き出し（`PROFILE-M4`）。
//!
//! 台帳と manifest から [`VoiceBank`] を組み立て、検証して包む。
//! 規則は `koeru-package` が持ち、ここはその境界。
//!
//! 順序は `packaging-export.fsl` が決めている。 被覆 → 検証 → ZIP →
//! 読み戻し。検証を通っていないものは包まない（`FB-PKG-102`）。

use std::collections::{BTreeMap, BTreeSet, HashSet};

use koeru_align::preset::Preset;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_core::db::{Distribution, DistributionSample, Ledger, MaterialFacts};
use koeru_core::names;
use koeru_core::presamp::Rules;
use koeru_core::project::{Manifest, Method, ProjectDir};
use koeru_core::reclist::Slot;
use koeru_core::release::{NewRelease, Validation, archive_base_name, content_hash};
use koeru_failure::Class;
use koeru_model::distribution::next_version;
use koeru_model::id::{DistributionId, RowId, SubbankId, TakeId};
use koeru_model::materials::{
    self, Adoption, Delivery, Facts, Production, Resolution, Role, UseContext,
};
use koeru_model::subbank::Cells;
use koeru_package::archive::{self, Written};
use koeru_package::bank::{Character, Portrait, Readme, Sample, Subbank, VoiceBank};
use koeru_package::coverage::Coverage;
use koeru_package::profile::{self, Profile};
use koeru_package::tree::{self, Content, PackagedFile};
use koeru_package::validate::{self, Finding, Unencodable};

use crate::error::{AppError, Result};
use crate::export_io;

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
    /// 下位方式への書き出し（`TR-PKG-24`, `TR-PKG-25`）。
    ///
    /// **出せる方式ごとに1件。** 独立した音源ルート・独立した ZIP になるので、
    /// 元パッケージとほぼ同等の容量がもう1本できる。
    pub downgrades: Vec<Downgrade>,
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
///
/// フォルダ名と版は配布物ごとの値（`DEC-PKG-018`）で、ここでは先頭の配布物（既定の「すべて」）の
/// ものを重ねて返す。 配布物が1つの音源の画面は、いまの口のまま同じに見える。
/// 音源で1つの値だけが欲しいときは [`source_settings`]。
#[tracing::instrument(skip(ledger, manifest))]
pub fn settings(ledger: &mut Ledger, manifest: &Manifest) -> Result<Distribution> {
    let mut d = source_settings(ledger, manifest)?;
    if let Some(first) = ledger.distributions()?.into_iter().next() {
        if let Some(name) = first.archive_name {
            d.distribution_name = name;
        }
        d.version = first.draft_version;
    }
    Ok(d)
}

/// 音源で1つの配布の値（画像・名義・声の提供者・web・サンプル音声・規約）。 無ければ既定値。
///
/// 配布物ごとに変わる値（フォルダ名・版・規約の上書き）は入っていない。
fn source_settings(ledger: &mut Ledger, manifest: &Manifest) -> Result<Distribution> {
    Ok(ledger.distribution()?.unwrap_or_else(|| Distribution {
        distribution_name: names::default_distribution_name(&manifest.display_name),
        profile: Profile::default().as_str().to_owned(),
        portrait_opacity: 1.0,
        ..Distribution::default()
    }))
}

/// 書き出す配布物と版（`TR-PKG-58`, `TR-PKG-60`）。
#[derive(Debug, Clone)]
pub struct Target {
    pub distribution: DistributionId,
    /// この配布物で書き出すときの値。 音源で1つの値に、配布物のフォルダ名・規約の上書き・版を
    /// 載せたもの。
    pub settings: Distribution,
}

/// 書き出す配布物を決める。 `id` を省くと先頭の配布物。 配布物が無ければ `None`。
///
/// 版は `version`、無ければ下書きの番号、下書きも無ければ最後に書き出した番号の次
/// （`TR-PKG-60`）。 規約は配布物の上書きがあればそれ、無ければ音源の規約。
///
/// # Errors
///
/// 台帳を読めない。
pub fn target_of(
    ledger: &mut Ledger,
    manifest: &Manifest,
    id: Option<DistributionId>,
    version: Option<&str>,
) -> Result<Option<Target>> {
    let records = ledger.distributions()?;
    let Some(record) = records
        .into_iter()
        .find(|r| id.is_none_or(|want| r.id == want))
    else {
        return Ok(None);
    };
    let released = ledger.released_versions(record.id)?;
    let version = match (version, &record.draft_version) {
        (Some(v), _) => v.to_owned(),
        (None, Some(draft)) => draft.clone(),
        (None, None) => {
            let taken: Vec<&str> = released.iter().map(String::as_str).collect();
            next_version(released.last().map(String::as_str), &taken)
        }
    };
    let mut settings = source_settings(ledger, manifest)?;
    if let Some(name) = record.archive_name {
        settings.distribution_name = name;
    }
    if record.terms.is_some() {
        settings.terms = record.terms;
    }
    settings.version = Some(version);
    Ok(Some(Target {
        distribution: record.id,
        settings,
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
            Class::Corrupt,
            "保存してある書き出し方が分からない",
        )
    })
}

/// 録ったままの方式で届ける用途（`DEC-PLT-052`）。
const AS_RECORDED: UseContext = UseContext::Export {
    delivery: Delivery::AsRecorded,
};

/// 書き出しが役割を見ない。 試唱が鳴らさない語尾も、録ってあれば配る。
///
/// 解決は役割を尋ねるが、書き出しの用途はその答えで候補を絞らない
/// （`koeru_model::materials::UseContext::Export`）。
fn any_role(_: &(SubbankId, String)) -> Role {
    Role::Cv
}

/// 書き出しが使う候補（`DEC-PLT-052`）。
///
/// 試唱と同じ事実を同じ解決で解く。 被覆も、配る素材も、ここから取る
/// ——別々に台帳を引くと、持ち主の決め方が経路ごとに割れる。
fn export_candidates(
    ledger: &mut Ledger,
) -> Result<(MaterialFacts, Resolution<(SubbankId, String)>)> {
    let material = ledger.material_facts()?;
    let resolution = materials::resolve(&material.facts, AS_RECORDED, any_role);
    Ok((material, resolution))
}

/// 枠ごとの、録ってある綴り（`TR-RCL-26`, `DEC-RCL-018`）。 1テイクも録っていない枠は現れない。
///
/// 鍵は枠。 同じ高さに、方式や声色の違う枠が並びうるので、高さで束ねると
/// 別の方式の綴りが録り済みに数えられる。
fn covered_by_cell(
    resolution: &Resolution<(SubbankId, String)>,
) -> BTreeMap<SubbankId, BTreeSet<String>> {
    let mut out: BTreeMap<SubbankId, BTreeSet<String>> = BTreeMap::new();
    for (cell, alias) in resolution.covered() {
        out.entry(*cell).or_default().insert(alias.clone());
    }
    out
}

/// この音源の被覆（`TR-PKG-23`, `TR-RCL-26`）。要求表を持っていなければ `None`。
///
/// **枠ごとに独立して、その枠の方式で見る**（`DEC-RCL-018`）。 一度は `covered_units()`
/// （音高をまたいだ和集合）で見ていた。多音階では区画も `oto.ini` も枠ごとに分かれる
/// （`TR-PKG-04`）ので、**1枠だけ埋めれば関門を通り、残りの区画が空のまま配れた。**
/// 受け取った側は、その音域のエイリアスを1つも引けない。 さらに音源の方式1つで全枠を見ると、
/// 単独音の枠を連続音の要求表で見て、永遠に足りないと言う（逆なら、足りないのに通す）。
///
/// `TR-RCL-26` の「進捗表示では音高を跨いだ最小値で扱う」と同じ向き。
/// 足りないエイリアスは全枠ぶんを合わせて返す——どの枠が足りないかは
/// 画面が枠ごとの消化率で別に出す。
///
/// 録ってあることは、5値の有無では決めない。 5値は解析のあとに付くので、
/// 解析が済む前の素材を「録っていない」と言うと、録ったものを録り直させる。
/// 5値が揃っているかは `otos_ready` の関門が見る。
fn coverage_of(
    by_cell: &BTreeMap<SubbankId, BTreeSet<String>>,
    cells: &Cells,
    rules: &Rules,
    manifest: &Manifest,
) -> Result<Option<Coverage>> {
    let set = koeru_core::inventory::UnitSet::Core;

    // 枠が1つも無いときは、音源の方式で、何も録っていないものとして見る。
    // 空のまま畳むと「全部揃っている」に見える。
    if cells.is_empty() {
        return Ok(koeru_package::coverage::coverage(
            rules,
            alias_method(manifest.method),
            set,
            &BTreeSet::new(),
        ));
    }

    // **枠を全部回る。** 台帳に現れるのは1テイクでも録った枠だけなので、
    // `by_cell` の値だけを見ると、まだ手を付けていない枠が判定に入らない。
    // `bank_of` はその枠の区画も作るので、空の区画を抱えたまま「完成」と言うことになる。
    let empty = BTreeSet::new();
    let mut required = 0;
    let mut provided = 0;
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for cell in cells.as_slice() {
        let covered = by_cell.get(&cell.id).unwrap_or(&empty);
        let Some(c) = koeru_package::coverage::coverage(rules, cell.method, set, covered) else {
            return Ok(None);
        };
        required += c.required;
        provided += c.provided;
        missing.extend(c.missing);
    }
    Ok(Some(Coverage {
        required,
        provided,
        missing: missing.into_iter().collect(),
    }))
}

/// すべての枠で成り立つ方式（`TR-RCL-26`, `DEC-RCL-018`）。
///
/// 書き出せる方式の一覧、下位方式の一覧、下位方式の関門が、同じこれを通る。
/// **一覧と関門で別の見方をすると、出せると言ったものが押すと落ちる。**
///
/// **音高を跨いだ和集合で見ていた。** 多音階で1音高だけ録り終えても、
/// 必要な綴りが音高ごとにばらけていても「出せる」に入り、下位方式の関門も
/// 通った——`bank_of` は枠の数だけ区画を作るので、録っていない
/// 枠の区画が空のまま配られる。素の書き出しの関門（`coverage_of`）は
/// 枠ごとに見ていたので、見方が2つに割れていた。
///
/// 1テイクも録っていない枠は空集合として判定する（台帳に現れないため）。
/// 方式の違う枠が混じるときは、どの枠からも出せる方式だけが残る。
fn methods_in_every_cell(
    by_cell: &BTreeMap<SubbankId, BTreeSet<String>>,
    cells: &Cells,
    judge: impl Fn(&BTreeSet<String>) -> Vec<koeru_core::alias::Method>,
) -> Vec<koeru_core::alias::Method> {
    let empty = BTreeSet::new();
    let mut each = cells.as_slice().iter();
    let Some(first) = each.next() else {
        return Vec::new();
    };
    let mut out = judge(by_cell.get(&first.id).unwrap_or(&empty));
    for c in each {
        let here = judge(by_cell.get(&c.id).unwrap_or(&empty));
        out.retain(|m| here.contains(m));
    }
    out
}

/// いま書き出せるかを調べる（`TR-PKG-49`）。
#[tracing::instrument(skip(dir, ledger, rules, manifest))]
pub fn state(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    gates: Gates,
) -> Result<PackageState> {
    let distribution = settings(ledger, manifest)?;
    let profile = resolved_profile(&distribution)?;
    // 検証の指摘と同じ鍵で持つ（`TR-PKG-51`）。
    //
    // **素の WAV 名で持っていた。** 多音階の指摘は `G3/s001.wav` の形で
    // 来るので一度も引けず、しかも音高をまたいで同じ stem が潰れていた
    // ——指摘からその行の録った回へ入る経路が消える。
    let (material, resolution) = export_candidates(ledger)?;
    let covered = covered_by_cell(&resolution);
    // 区画の名前は書き出しと同じ決め方（`layout::labels`）。 別に作ると、指摘のパスと
    // 引く鍵がずれて、指摘から行へ戻る経路が消える。
    let folders: BTreeMap<SubbankId, String> = koeru_package::layout::labels(&material.cells)
        .into_iter()
        .flatten()
        .collect();
    let rows_by_file = ledger
        .distribution_samples()?
        .into_iter()
        .map(|s| {
            let path = match folders.get(&s.subbank) {
                Some(folder) => format!("{folder}/{}.wav", s.file_stem),
                None => format!("{}.wav", s.file_stem),
            };
            (path, s.row_id)
        })
        .collect();
    let bank = bank_of(dir, ledger, rules, manifest, &distribution, None)?;

    let coverage = coverage_of(&covered, &material.cells, rules, manifest)?;
    let report = validate::validate(&bank, profile, &export_io::master_facts(&bank));
    // 要求表は方式ごとの綴り（`coverage::required`）。 仮名で突き合わせると、
    // 単独音以外はどの方式も「出せる」に入らない。
    let set = koeru_core::inventory::UnitSet::Core;
    let exportable = methods_in_every_cell(&covered, &material.cells, |c| {
        koeru_package::coverage::exportable(rules, set, c)
    })
    .into_iter()
    .map(project_method)
    .collect();

    // 下位方式への書き出し（`TR-PKG-24`）。
    //
    // 判定はエイリアスの被覆から（`TR-PKG-22`）。 容量は「oto.ini 1ファイル分」
    // ではなく、元パッケージとほぼ同等の容量がもう1本——WAV を複製するため。
    //
    // **全素材を数えていた。** 実際に複製するのは対象方式が参照するものだけ
    // （`TR-PKG-24` の性能上の最適化）なので、出す数と作る量が食い違う。
    let mut downgrades = Vec::new();
    for m in methods_in_every_cell(&covered, &material.cells, |c| {
        koeru_package::coverage::downgradable(rules, set, c)
    }) {
        let sources = downgrade_sources(dir, ledger, rules, m)?;
        downgrades.push(Downgrade {
            method: project_method(m),
            bytes: koeru_package::downgrade::plan(m, &sources).bytes,
        });
    }

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
        downgrades,
    })
}

/// 下位方式の書き出し1件（`TR-PKG-24`, `TR-PKG-25`）。
///
/// **素材の由来は持たない**（`DEC-RCL-015`）。 跨いだ収録セッションの数と
/// 期間を出していたが、そこから読めるのは「声が揃っていないかもしれない」だけで、
/// 声質に関与しないと言いながら判断材料を置いていたことになる。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Downgrade {
    pub method: Method,
    /// 複製される WAV の概算バイト数。
    pub bytes: u64,
}

/// 書き出す（`REQ-PKG-105`, `REQ-PKG-106`, `TR-PKG-44`）。
///
/// 検証を通らないまま包まない。 読み戻しに落ちたらファイルを残さない。
/// 台帳へ記録するのは、両方が通ってから。
///
/// 版の札は `dist` から取る（`TR-PKG-44`）。 **書き出しのときに別に打たせない**
/// ——同じ札が2つあると、配布物の `character.txt` と履歴で違う値になる。
// バージョン文字列は本人が書いた自由文。トレースへ載せない（`AGENTS.md` #3）。
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at, dist))]
pub fn export(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    released_at: &str,
    dist: &Target,
) -> Result<Exported> {
    // 部分的なパッケージを出さない（`TR-PKG-23`, `INV-PKG-102`）。
    // 画面の関門と別に見る。 コマンドを直に叩かれても素通りさせない。
    let (material, resolution) = export_candidates(ledger)?;
    let Some(coverage) = coverage_of(
        &covered_by_cell(&resolution),
        &material.cells,
        rules,
        manifest,
    )?
    else {
        return Err(AppError::new(
            "package.no_required_table",
            Class::Rejected,
            "この作り方に必要な音の表をまだ持っていない",
        ));
    };
    if !coverage.is_complete() {
        return Err(AppError::new(
            "package.incomplete_coverage",
            Class::Rejected,
            format!("まだ録れていない音が {} 件ある", coverage.missing.len()),
        ));
    }
    write_package(dir, ledger, rules, manifest, released_at, dist, None)
}

/// 下位方式へ書き出す（`TR-PKG-23`, `TR-PKG-24`, `TR-PKG-25`）。
///
/// 独立した音源ルート・独立した ZIP。 同一 ZIP へ同梱しない——接頭辞を
/// 付ければ単独音として使えなくなり、付けなければ全 `oto.ini` 横断の
/// エイリアス重複で落ちる。両立しない。
///
/// **5値は対象方式の規約プリセットで再導出する**（`TR-ALN-34`）。
/// 値を流用すると、語頭の子音区間を持ったままの oto が単独音として配られる。
///
/// WAV は対象方式が参照するものだけを複製する（`TR-PKG-24` の性能上の最適化）。
///
/// # Errors
///
/// 被覆が満ちていない、検証に通らない、包めない、台帳へ書けない。
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at, dist))]
pub fn export_downgrade(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    method: Method,
    released_at: &str,
    dist: &Target,
) -> Result<Exported> {
    let set = koeru_core::inventory::UnitSet::Core;
    let target = alias_method(method);
    // 画面が出している一覧と同じ判定を通す（`TR-PKG-22`, `TR-PKG-23`）。
    //
    // **変換後の綴りで見ていた。** 連続音の素材が持つのは `- か` で、
    // 単独音が要求するのは素の `か`。それを作り出すのがこの下の再導出
    // なのに、その手前で「`か` を持っていない」と断っていた——
    // **画面が「出せます」と言う音源が、押すと必ず落ちる。**
    //
    // 枠ごとに見る（`methods_in_every_cell`）。 一覧と同じものを通すので、
    // 出せると言ったものは出せ、出せないと言ったものは出ない。
    let (material, resolution) = export_candidates(ledger)?;
    if !methods_in_every_cell(&covered_by_cell(&resolution), &material.cells, |c| {
        koeru_package::coverage::downgradable(rules, set, c)
    })
    .contains(&target)
    {
        return Err(AppError::new(
            "package.incomplete_coverage",
            Class::Rejected,
            "その作り方では、いまの素材から出せない",
        ));
    }
    let Some(down) = downgraded_for(rules, target)? else {
        return Err(AppError::new(
            "package.no_required_table",
            Class::Rejected,
            "その作り方に必要な音の表をまだ持っていない",
        ));
    };
    write_package(dir, ledger, rules, manifest, released_at, dist, Some(&down))
}

/// 下位方式の要求表と、5値を作り直す規約プリセット。 要求表が無ければ `None`。
fn downgraded_for(rules: &Rules, target: koeru_core::alias::Method) -> Result<Option<Downgraded>> {
    let set = koeru_core::inventory::UnitSet::Core;
    let Some(required) = koeru_package::coverage::required(rules, target, set) else {
        return Ok(None);
    };
    let preset = Preset::default_for(target)
        .map_err(|e| AppError::from_failure(e).saying("規約プリセットを読めない"))?;
    Ok(Some(Downgraded {
        method: target,
        required,
        preset,
    }))
}

/// 包んで、読み戻して、台帳へ残す。
///
/// 素の書き出しと下位方式で違うのは、組み立てる中身と音源ルートの名前だけ。
/// **2本書くと片方だけが直る**——検証・改名・記録の順序は1箇所に置く。
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at, dist, down))]
fn write_package(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    released_at: &str,
    dist: &Target,
    down: Option<&Downgraded>,
) -> Result<Exported> {
    let mut distribution = dist.settings.clone();
    let version = distribution.version.clone().unwrap_or_default();
    let profile = resolved_profile(&distribution)?;
    if !profile::is_available(profile) {
        return Err(AppError::new(
            "package.profile_unavailable",
            Class::Unsupported,
            "この書き出し方は、いまの環境では使えない",
        ));
    }
    // 元の名前に方式を足した別の名前（`TR-PKG-24`）。 同じ名前にすると、
    // 受け取った側のフォルダで上書きが起きる。
    if let Some(t) = down {
        let base = if distribution.distribution_name.is_empty() {
            names::default_distribution_name(&manifest.display_name)
        } else {
            distribution.distribution_name.clone()
        };
        distribution.distribution_name = koeru_package::downgrade::root_name(&base, t.method);
    }

    let bank = bank_of(dir, ledger, rules, manifest, &distribution, down)?;
    let report = validate::validate(&bank, profile, &export_io::master_facts(&bank));
    if !report.may_export() {
        // 件数を添える。 種別文字列だけだと、画面を開き直して
        // `package_state` を引くまで何が引っかかったのか分からない。
        // 中身は載せない——エイリアスもパスも、送信してよい語ではない。
        return Err(AppError::new(
            "package.validation_failed",
            Class::Rejected,
            format!(
                "書き出し前の検査に通っていない（指摘 {} 件、書けない文字 {} 件）",
                report.findings.len(),
                report.unencodable.len()
            ),
        ));
    }
    // 欄の名前は許可リストのもの（`tests/offline.rs`）。 `findings` と名付けると、
    // 次に触った人が件数ではなく中身（エイリアスとパス）を載せられる形になる。
    tracing::debug!(count = report.findings.len(), "検査を通った");

    let files = tree::build(&bank, profile).map_err(AppError::from_failure)?;
    let exports = dir.exports_dir();
    // 前の書き出しが途中で落ちていたら片付ける。 仮の名前が残っていると、
    // 次の書き出しがそれを上書きするのか作り直すのかが読めない。
    for ext in [archive::ZIP_EXT, archive::UAR_EXT] {
        let _ = std::fs::remove_file(exports.join(format!("{PENDING}.{ext}")));
    }

    let written = export_io::write_and_verify(&bank, &files, profile, &exports, PENDING)
        .map_err(AppError::from_failure)?;

    // 番号を先に見て、最終名まで作ってから台帳へ書く。
    //
    // **記録が先だと、改名に落ちた回の記録だけが残る。** 履歴には
    // 書き出したと書いてあるのに `exports/` に何も無い状態は、
    // リリースレコードが不変なので（`TR-PKG-44`）あとから消せない。
    let seq = ledger.next_release_seq()?;
    let base = archive_base_name(seq, &version);
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
            distribution: dist.distribution,
            version: version.clone(),
            // 下位方式の回は、出した方式を残す（`TR-PKG-44`）。
            // **プロジェクトの方式を書いていた。** どの回に何を配ったかが
            // 履歴から読めなくなる。
            method: down.map_or(manifest.method, |t| project_method(t.method)),
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
/// 区画は枠ごと（`DEC-RCL-018`）。 枠が1つなら区画を分けない。 区画の名前は
/// `koeru_package::layout` が枠の属性から作る。
#[tracing::instrument(skip(dir, ledger, rules, manifest, d, down))]
fn bank_of(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    d: &Distribution,
    down: Option<&Downgraded>,
) -> Result<VoiceBank> {
    let root = dir.root();
    let material = ledger.material_facts()?;
    let tones = material.tones.clone();
    let cells = &material.cells;
    let raw = adopted_samples(ledger, &material)?;

    // どの素材のどのエントリを配るかは、解決が決める（`DEC-PLT-052`）。
    //
    // 素の書き出しは、確認を通った5値（`TR-ALN-25`）のうち、綴りの持ち主のテイクのものを
    // そのまま出す（`DEC-RCL-016`）。 下位方式では、素材ごとにエントリを作り直し
    // （`TR-PKG-24`）、行の頭に近いものを選ぶ（`DEC-PKG-014`）。
    let entries_of: Vec<Vec<koeru_align::ini::IniEntry>> = match down {
        None => {
            let resolution = materials::resolve(&material.facts, AS_RECORDED, any_role);
            let used: HashSet<(i32, SubbankId, &str)> = resolution
                .used()
                .map(|c| (c.take.get(), c.key.0, c.key.1.as_str()))
                .collect();
            raw.iter()
                .map(|sample| {
                    let file = format!("{}.wav", sample.file_stem);
                    sample
                        .otos
                        .iter()
                        .filter(|(alias, _)| {
                            used.contains(&(sample.take_id, sample.subbank, alias.as_str()))
                        })
                        .map(|(alias, o)| koeru_align::ini::IniEntry {
                            file: file.clone(),
                            alias: alias.clone(),
                            oto: *o,
                        })
                        .collect()
                })
                .collect()
        }
        Some(t) => downgrade_entries(ledger, rules, cells, &raw, t)?,
    };

    // 出す素材だけの枠。 エントリを1つも持たない素材は落ちるので
    // （下位方式が参照しない WAV を複製しない、`TR-PKG-24`）、
    // **落とす前の並びで枠を持つと、区画分けが1つずつずれる。**
    let sample_cells: Vec<SubbankId> = raw
        .iter()
        .zip(&entries_of)
        .filter(|(_, e)| !e.is_empty())
        .map(|(s, _)| s.subbank)
        .collect();

    let samples = raw
        .into_iter()
        .zip(entries_of)
        .filter(|(_, entries)| !entries.is_empty())
        .map(|(s, entries)| {
            // 表を書けなかったことを `None` に畳まない（`TR-PKG-05`）。
            // 畳むと、同梱すると書いてある readme と中身が食い違ったまま出る。
            let frq = s.frq.map(|f| f.to_bytes()).transpose()?;
            Ok(Sample {
                file: format!("{}.wav", s.file_stem),
                master: root.join(&s.rel_path),
                frq,
                entries,
            })
        })
        .collect::<std::result::Result<Vec<_>, koeru_core::frq::FrqError>>()
        .map_err(AppError::from_failure)?;

    // 区画。 枠が複数なら、枠ごとに分ける（`TR-PKG-04`, `DEC-RCL-018`）。
    let subbanks = match koeru_package::layout::labels(cells) {
        // **1区画にまとめると、フォルダ間でエイリアスが衝突する。** 3音高の
        // 「か」が同じ名前で3つ並び、`TR-PKG-19` の一意性を割る。
        // 区画に分けると `oto.ini` は枠ごとになり、サフィックスが
        // 一括で付く（`tree::oto_ini` の `decorate`）。
        // 名前は枠の属性から作る（`koeru_package::layout`）。 高さだけが分かれる音源は
        // これまでどおり音名で、方式や声色が分かれると、その軸も名前に入る。
        Some(labels) => labels
            .into_iter()
            .filter_map(|(id, label)| {
                let cell = cells.get(id)?;
                Some(Subbank {
                    folder: Some(label.clone()),
                    color: koeru_package::layout::yaml_color(cell, &label),
                    voice_color: cell.voice_color.as_ref().map(|v| v.as_str().to_owned()),
                    prefix: String::new(),
                    // 音階サフィックスはここが付ける（`TR-ALN-22`）。
                    suffix: label,
                    tone: Some(cell.tone),
                    samples: samples
                        .iter()
                        .zip(&sample_cells)
                        .filter(|(_, c)| **c == id)
                        .map(|(m, _)| m.clone())
                        .collect(),
                })
            })
            .collect(),
        None => vec![Subbank {
            folder: None,
            color: String::new(),
            voice_color: None,
            prefix: String::new(),
            suffix: String::new(),
            tone: None,
            samples,
        }],
    };
    let mut methods: Vec<koeru_core::alias::Method> = Vec::new();
    match down {
        // 降りた回は降りた先の方式を名乗る（`TR-PKG-24`, `DEC-PKG-015`）。
        Some(t) => methods.push(t.method),
        None if cells.is_empty() => methods.push(alias_method(manifest.method)),
        // 方式は枠ごと。 枠の並びで最初に現れた順に、重複なく並べる。
        None => {
            for c in cells.as_slice() {
                if !methods.contains(&c.method) {
                    methods.push(c.method);
                }
            }
        }
    }

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
        methods,
        tones,
        subbanks,
        // 音素体系とエイリアス規則（`TR-RCL-24`）。受け取った側が同じ表で解決する。
        //
        // **同梱の既定を書いていた。** 音源に `presamp.ini` を置いて綴りを
        // 差し替えると（`TR-SYN-36`）、配る `presamp.ini` だけが既定のまま
        // 出て、同じ配布物の `oto.ini` と食い違う。受け取った側は
        // 1つも引けない。
        rules: rules.clone(),
    })
}

/// 有効な採用テイクを持つ行の素材（`DEC-PLT-052`）。
///
/// 採用が有効かどうかは、解決の事実（[`MaterialFacts`]）が決める。 配る素材の一覧を
/// 台帳から別に引くと、解決が候補にしない行の素材まで配ることになる。
fn adopted_samples(
    ledger: &mut Ledger,
    material: &MaterialFacts,
) -> Result<Vec<DistributionSample>> {
    Ok(ledger
        .distribution_samples()?
        .into_iter()
        .filter(|s| {
            material
                .facts
                .adopted
                .contains_key(&RowId::new(s.row_id.clone()))
        })
        .collect())
}

/// 下位方式が参照する素材と、その大きさ（`TR-PKG-24`）。
///
/// 書き出しが実際に複製する素材だけを数える。 **同じ解決を通す**（[`downgrade_entries`]）
/// ——一覧は「対象方式の綴りを1つでも作れる素材」を数えていたので、行の頭に近い別の素材に
/// 負けて1つも配られない素材まで入り、書き出し前に出す容量が実際より大きくなりえた。
fn downgrade_sources(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    target: koeru_core::alias::Method,
) -> Result<Vec<(String, u64)>> {
    let Some(down) = downgraded_for(rules, target)? else {
        return Ok(Vec::new());
    };
    let material = ledger.material_facts()?;
    let raw = adopted_samples(ledger, &material)?;
    let entries = downgrade_entries(ledger, rules, &material.cells, &raw, &down)?;
    let root = dir.root();
    Ok(raw
        .into_iter()
        .zip(entries)
        .filter(|(_, e)| !e.is_empty())
        .map(|(s, _)| {
            let bytes = std::fs::metadata(root.join(&s.rel_path)).map_or(0, |m| m.len());
            (s.rel_path, bytes)
        })
        .collect())
}

/// 下位方式で配るエントリを、素材ごとに（`TR-PKG-24`, `DEC-PKG-014`）。
///
/// 素材ごとに作り直したエントリ（[`rederived_entries`]）を事実として、書き出しと同じ
/// 解決（[`UseContext::Export`] の [`Delivery::Downgrade`]）で、綴りごとに配る素材を1つ選ぶ。
/// 同じ綴りを複数の素材が出せるので、全部配ると `oto.ini` 横断でエイリアスが重なり、
/// `TR-PKG-19` に反する（書き出しの検査が `DuplicateAlias` で止める）。
/// 選び方は行の頭に近いモーラから、同じ深さなら素材の名前で、枠ごと
/// （`koeru_model::materials`）。
///
/// 作り直しの入力になる「元の綴り」は、素材が属する枠の方式で引く（`DEC-RCL-018`）。
/// 音源の方式1つで引くと、別の方式の枠の素材は境界を1つも見つけられず、黙って落ちる。
///
/// 返す並びは `raw` と同じ。 1つも選ばれなかった素材は空になる。
fn downgrade_entries(
    ledger: &mut Ledger,
    rules: &Rules,
    cells: &Cells,
    raw: &[DistributionSample],
    target: &Downgraded,
) -> Result<Vec<Vec<koeru_align::ini::IniEntry>>> {
    let mut facts: Facts<(SubbankId, String)> = Facts::empty();
    let mut offered = Vec::with_capacity(raw.len());
    for sample in raw {
        let file = format!("{}.wav", sample.file_stem);
        // 枠が見つからない素材は、作り直しの元が分からない。 配らない（空）。
        let Some(source) = cells.get(sample.subbank).map(|c| c.method) else {
            offered.push(Vec::new());
            continue;
        };
        let entries = rederived_entries(ledger, rules, source, sample, &file, target)?;
        let row = RowId::new(sample.row_id.clone());
        facts.adopted.insert(
            row.clone(),
            Adoption {
                take: TakeId::new(sample.take_id),
                name: sample.file_stem.clone(),
            },
        );
        facts
            .produced
            .extend(entries.iter().map(|(e, depth)| Production {
                key: (sample.subbank, e.alias.clone()),
                row: row.clone(),
                depth: *depth,
            }));
        offered.push(entries);
    }
    let resolution = materials::resolve(
        &facts,
        UseContext::Export {
            delivery: Delivery::Downgrade {
                target: target.method,
            },
        },
        any_role,
    );
    let chosen: HashSet<(&str, &str)> = resolution
        .used()
        .map(|c| (c.row.as_str(), c.key.1.as_str()))
        .collect();
    Ok(raw
        .iter()
        .zip(offered)
        .map(|(sample, entries)| {
            entries
                .into_iter()
                .filter(|(e, _)| chosen.contains(&(sample.row_id.as_str(), e.alias.as_str())))
                .map(|(e, _)| e)
                .collect()
        })
        .collect())
}

/// 下位方式で組み立て直すときの上書き（`TR-PKG-24`）。
#[derive(Debug)]
struct Downgraded {
    /// 書き出す方式。
    method: koeru_core::alias::Method,
    /// その方式の要求表。 ここに無い綴りは出さない。
    required: BTreeSet<String>,
    /// 5値を作り直すときの規約プリセット（`TR-ALN-34`）。
    preset: Preset,
}

/// 素材1つぶんのエントリを、下位方式の規約で作り直す（`TR-PKG-24`）。
///
/// **値を流用しない。** 連続音の `- CV` を素の `CV` として複製すると、
/// 語頭の子音区間を持ったままの oto が単独音として配られる（`TR-RCL-21`）。
/// 入力は保存した境界（`TR-ALN-34`）と対象方式の規約プリセットだけで、
/// アライナは呼ばない。
///
/// 境界は行の CV エイリアスで引く。 保存側も同じ名前で置いている
/// （`Studio::finish_take`）ので、元の方式の綴りから並べ直せる。
///
/// 返るのは `(エントリ, 何モーラ目から取ったか)`。 **同じ綴りを複数の素材が
/// 出せる**ので（連続音の行はどれも複数のモーラを含む）、どれを配るかは
/// 呼び出し側が全体を見て決める（[`pick_sources`]）。
fn rederived_entries(
    ledger: &mut Ledger,
    rules: &Rules,
    source_method: koeru_core::alias::Method,
    sample: &koeru_core::db::DistributionSample,
    file: &str,
    target: &Downgraded,
) -> Result<Vec<(koeru_align::ini::IniEntry, usize)>> {
    let set = koeru_core::inventory::UnitSet::Core;
    let line = ledger.row_units_of(&sample.row_id, set)?;
    let saved: BTreeMap<String, koeru_core::oto::Boundary> = ledger
        .boundaries_for_take(sample.take_id)?
        .into_iter()
        .collect();

    // モーラ順に戻す。 元の方式の CV の綴りが、そのモーラの境界の名前。
    let source = koeru_core::reclist::row_entries(rules, source_method, &line);
    let mut boundaries: Vec<koeru_core::oto::Boundary> = Vec::with_capacity(line.len());
    for mora in 0..line.len() {
        let found = source.iter().find_map(|(a, slot)| match *slot {
            Slot::Cv { mora: m } if m == mora => saved.get(a),
            _ => None,
        });
        // 1つでも欠ければ、この素材からは作り直せない。
        // **部分的に作らない**——欠けたモーラより後ろが全部ずれる。
        let Some(b) = found else {
            return Ok(Vec::new());
        };
        boundaries.push(*b);
    }

    // マスターは常に 44100（`TR-REC-02`）。素材の長さはフレーム数から出す。
    #[allow(
        clippy::cast_precision_loss,
        reason = "収録の長さは 2^53 サンプルに届かない"
    )]
    let file_len_ms = sample.frames as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);

    let entries = koeru_core::reclist::row_entries(rules, target.method, &line);
    // その綴りが何モーラ目から出るか。 渡りは入っていく側ではなく、
    // 乗っている直前のモーラで数える（`derive_row` と同じ見方）。
    let depth = |alias: &str| {
        entries.iter().find_map(|(a, slot)| {
            (a == alias).then_some(match *slot {
                Slot::Cv { mora } | Slot::Ending { mora } => mora,
                Slot::Vc { prev, .. } => prev,
            })
        })
    };
    Ok(
        koeru_align::derive::derive_row(&entries, &boundaries, &line, file_len_ms, &target.preset)
            .into_iter()
            .filter(|(alias, _)| target.required.contains(alias))
            .filter_map(|(alias, oto)| {
                let d = depth(&alias)?;
                Some((
                    koeru_align::ini::IniEntry {
                        file: file.to_owned(),
                        alias,
                        oto,
                    },
                    d,
                ))
            })
            .collect(),
    )
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
        return Err(AppError::new(
            p.kind(),
            Class::InvalidInput,
            "その配布名は使えない",
        ));
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
    rules: &Rules,
    manifest: &Manifest,
) -> Result<Vec<(String, u64)>> {
    let d = settings(ledger, manifest)?;
    let profile = resolved_profile(&d)?;
    let bank = bank_of(dir, ledger, rules, manifest, &d, None)?;
    let files = tree::build(&bank, profile).map_err(AppError::from_failure)?;
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
