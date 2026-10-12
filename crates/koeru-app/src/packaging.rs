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
use koeru_model::id::{RowId, TakeId};
use koeru_model::materials::{
    self, Adoption, Delivery, Facts, Production, Resolution, Role, UseContext,
};
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
#[tracing::instrument(skip(ledger, manifest))]
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
fn any_role(_: &(i32, String)) -> Role {
    Role::Cv
}

/// 書き出しが使う候補（`DEC-PLT-052`）。
///
/// 試唱と同じ事実を同じ解決で解く。 被覆も、配る素材も、ここから取る
/// ——別々に台帳を引くと、持ち主の決め方が経路ごとに割れる。
fn export_candidates(ledger: &mut Ledger) -> Result<(MaterialFacts, Resolution<(i32, String)>)> {
    let material = ledger.material_facts()?;
    let resolution = materials::resolve(&material.facts, AS_RECORDED, any_role);
    Ok((material, resolution))
}

/// 音高ごとの、録ってある綴り（`TR-RCL-26`）。 1テイクも録っていない音高は現れない。
fn covered_by_tone(resolution: &Resolution<(i32, String)>) -> BTreeMap<i32, BTreeSet<String>> {
    let mut out: BTreeMap<i32, BTreeSet<String>> = BTreeMap::new();
    for (tone, alias) in resolution.covered() {
        out.entry(*tone).or_default().insert(alias.clone());
    }
    out
}

/// この音源の方式の被覆（`TR-PKG-23`, `TR-RCL-26`）。要求表を持っていなければ `None`。
///
/// **音高ごとに独立して見る。** 一度は `covered_units()`（音高をまたいだ和集合）で
/// 見ていた。多音階では区画も `oto.ini` も音高ごとに分かれる（`TR-PKG-04`）ので、
/// **1音高だけ埋めれば関門を通り、残りの区画が空のまま配れた。**
/// 受け取った側は、その音域のエイリアスを1つも引けない。
///
/// `TR-RCL-26` の「進捗表示では音高を跨いだ最小値で扱う」と同じ向き。
/// 足りないエイリアスは全音高ぶんを合わせて返す——どの音高が足りないかは
/// 画面が音高ごとの消化率（`progress_by_tone`）で別に出す。
///
/// 録ってあることは、5値の有無では決めない。 5値は解析のあとに付くので、
/// 解析が済む前の素材を「録っていない」と言うと、録ったものを録り直させる。
/// 5値が揃っているかは `otos_ready` の関門が見る。
fn coverage_of(
    by_tone: &BTreeMap<i32, BTreeSet<String>>,
    configured: &[i32],
    rules: &Rules,
    manifest: &Manifest,
) -> Result<Option<Coverage>> {
    let method = alias_method(manifest.method);
    let set = koeru_core::inventory::UnitSet::Core;

    // 1音高も録っていないときは、和集合（＝空）で見る。
    // `by_tone` が空なので、そのまま畳むと「全部揃っている」に見える。
    if by_tone.is_empty() {
        return Ok(koeru_package::coverage::coverage(
            rules,
            method,
            set,
            &BTreeSet::new(),
        ));
    }

    // **設定した音高を全部回る。** 台帳に現れるのは1テイクでも録った音高
    // だけなので、`by_tone` の値だけを見ると、まだ手を付けていない音高が
    // 判定に入らない。`bank_of` はその音高の区画も作るので、空の区画を
    // 抱えたまま「完成」と言うことになる。
    let empty = BTreeSet::new();
    let mut required = 0;
    let mut provided = 0;
    let mut missing: BTreeSet<String> = BTreeSet::new();
    for tone in configured {
        let covered = by_tone.get(tone).unwrap_or(&empty);
        let Some(c) = koeru_package::coverage::coverage(rules, method, set, covered) else {
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

/// 設定した音高のすべてで成り立つ方式（`TR-RCL-26`）。
///
/// 書き出せる方式の一覧、下位方式の一覧、下位方式の関門が、同じこれを通る。
/// **一覧と関門で別の見方をすると、出せると言ったものが押すと落ちる。**
///
/// **音高を跨いだ和集合で見ていた。** 多音階で1音高だけ録り終えても、
/// 必要な綴りが音高ごとにばらけていても「出せる」に入り、下位方式の関門も
/// 通った——`bank_of` は設定した音高の数だけ区画を作るので、録っていない
/// 音高の区画が空のまま配られる。素の書き出しの関門（`coverage_of`）は
/// 音高ごとに見ていたので、見方が2つに割れていた。
///
/// 1テイクも録っていない音高は空集合として判定する（台帳に現れないため）。
fn methods_in_every_tone(
    by_tone: &BTreeMap<i32, BTreeSet<String>>,
    configured: &[i32],
    judge: impl Fn(&BTreeSet<String>) -> Vec<koeru_core::alias::Method>,
) -> Vec<koeru_core::alias::Method> {
    let empty = BTreeSet::new();
    let mut tones = configured.iter();
    let Some(first) = tones.next() else {
        return Vec::new();
    };
    let mut out = judge(by_tone.get(first).unwrap_or(&empty));
    for t in tones {
        let here = judge(by_tone.get(t).unwrap_or(&empty));
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
    let covered = covered_by_tone(&resolution);
    let multi = material.tones.len() > 1;
    let rows_by_file = ledger
        .distribution_samples()?
        .into_iter()
        .map(|s| {
            let path = if multi {
                format!("{}/{}.wav", koeru_core::tone::name(s.tone), s.file_stem)
            } else {
                format!("{}.wav", s.file_stem)
            };
            (path, s.row_id)
        })
        .collect();
    let bank = bank_of(dir, ledger, rules, manifest, &distribution, None)?;

    let coverage = coverage_of(&covered, &material.tones, rules, manifest)?;
    let report = validate::validate(&bank, profile);
    // 要求表は方式ごとの綴り（`coverage::required`）。 仮名で突き合わせると、
    // 単独音以外はどの方式も「出せる」に入らない。
    let set = koeru_core::inventory::UnitSet::Core;
    let exportable = methods_in_every_tone(&covered, &material.tones, |c| {
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
    for m in methods_in_every_tone(&covered, &material.tones, |c| {
        koeru_package::coverage::downgradable(rules, set, c)
    }) {
        let sources = downgrade_sources(dir, ledger, rules, manifest, m)?;
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
/// 版の札は設定から取る（`TR-PKG-44`）。 **書き出しのときに別に打たせない**
/// ——同じ札が2つあると、配布物の `character.txt` と履歴で違う値になる。
// バージョン文字列は本人が書いた自由文。トレースへ載せない（`AGENTS.md` #3）。
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at))]
pub fn export(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    released_at: &str,
) -> Result<Exported> {
    // 部分的なパッケージを出さない（`TR-PKG-23`, `INV-PKG-102`）。
    // 画面の関門と別に見る。 コマンドを直に叩かれても素通りさせない。
    let (material, resolution) = export_candidates(ledger)?;
    let Some(coverage) = coverage_of(
        &covered_by_tone(&resolution),
        &material.tones,
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
    write_package(dir, ledger, rules, manifest, released_at, None)
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
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at))]
pub fn export_downgrade(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    method: Method,
    released_at: &str,
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
    // 音高ごとに見る（`methods_in_every_tone`）。 一覧と同じものを通すので、
    // 出せると言ったものは出せ、出せないと言ったものは出ない。
    let (material, resolution) = export_candidates(ledger)?;
    if !methods_in_every_tone(&covered_by_tone(&resolution), &material.tones, |c| {
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
    write_package(dir, ledger, rules, manifest, released_at, Some(&down))
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
#[tracing::instrument(skip(dir, ledger, rules, manifest, released_at, down))]
fn write_package(
    dir: &ProjectDir,
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    released_at: &str,
    down: Option<&Downgraded>,
) -> Result<Exported> {
    let mut distribution = settings(ledger, manifest)?;
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
    let report = validate::validate(&bank, profile);
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

    let written = archive::write_and_verify(&bank, &files, profile, &exports, PENDING)
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
/// 区画は1つだけ。 音階の軸は録音リストのプリセットが持つもので、
/// いまあるのは単一音階のプリセットだけ（`PROFILE-M5`）。
/// `koeru-package` の側は多音階を扱えるので、ここが増えるときに繋ぐ。
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
    let multi = tones.len() > 1;
    let raw = adopted_samples(ledger, &material)?;

    // どの素材のどのエントリを配るかは、解決が決める（`DEC-PLT-052`）。
    //
    // 素の書き出しは、確認を通った5値（`TR-ALN-25`）のうち、綴りの持ち主のテイクのものを
    // そのまま出す（`DEC-RCL-016`）。 下位方式では、素材ごとにエントリを作り直し
    // （`TR-PKG-24`）、行の頭に近いものを選ぶ（`DEC-PKG-014`）。
    let entries_of: Vec<Vec<koeru_align::ini::IniEntry>> = match down {
        None => {
            let resolution = materials::resolve(&material.facts, AS_RECORDED, any_role);
            let used: HashSet<(i32, i32, &str)> = resolution
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
                            used.contains(&(sample.take_id, sample.tone, alias.as_str()))
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
        Some(t) => downgrade_entries(ledger, rules, manifest, &raw, t)?,
    };

    // 出す素材だけの音高。 エントリを1つも持たない素材は落ちるので
    // （下位方式が参照しない WAV を複製しない、`TR-PKG-24`）、
    // **落とす前の並びで音高を持つと、区画分けが1つずつずれる。**
    let sample_tones: Vec<i32> = raw
        .iter()
        .zip(&entries_of)
        .filter(|(_, e)| !e.is_empty())
        .map(|(s, _)| s.tone)
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
        // 降りた回は降りた先の方式を名乗る（`TR-PKG-24`, `DEC-PKG-015`）。
        method: down.map_or_else(|| alias_method(manifest.method), |t| t.method),
        tones: tones.clone(),
        // 多音階は収録音高ごとに区画を分ける（`TR-ALN-22`, `TR-PKG-04`）。
        //
        // **1区画にまとめると、フォルダ間でエイリアスが衝突する。** 3音高の
        // 「か」が同じ名前で3つ並び、`TR-PKG-19` の一意性を割る。
        // 区画に分けると `oto.ini` は音高ごとになり、サフィックスが
        // 一括で付く（`tree::oto_ini` の `decorate`）。
        subbanks: if multi {
            tones
                .iter()
                .map(|t| {
                    let name = koeru_core::tone::name(*t);
                    Subbank {
                        folder: Some(name.clone()),
                        color: name.clone(),
                        prefix: String::new(),
                        // 音階サフィックスはここが付ける（`TR-ALN-22`）。
                        suffix: name,
                        tone: Some(*t),
                        samples: samples
                            .iter()
                            .zip(&sample_tones)
                            .filter(|(_, st)| *st == t)
                            .map(|(m, _)| m.clone())
                            .collect(),
                    }
                })
                .collect()
        } else {
            vec![Subbank {
                folder: None,
                color: String::new(),
                prefix: String::new(),
                suffix: String::new(),
                tone: None,
                samples,
            }]
        },
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
    manifest: &Manifest,
    target: koeru_core::alias::Method,
) -> Result<Vec<(String, u64)>> {
    let Some(down) = downgraded_for(rules, target)? else {
        return Ok(Vec::new());
    };
    let material = ledger.material_facts()?;
    let raw = adopted_samples(ledger, &material)?;
    let entries = downgrade_entries(ledger, rules, manifest, &raw, &down)?;
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
/// 選び方は行の頭に近いモーラから、同じ深さなら素材の名前で、音高ごと
/// （`koeru_model::materials`）。
///
/// 返す並びは `raw` と同じ。 1つも選ばれなかった素材は空になる。
fn downgrade_entries(
    ledger: &mut Ledger,
    rules: &Rules,
    manifest: &Manifest,
    raw: &[DistributionSample],
    target: &Downgraded,
) -> Result<Vec<Vec<koeru_align::ini::IniEntry>>> {
    let mut facts: Facts<(i32, String)> = Facts::empty();
    let mut offered = Vec::with_capacity(raw.len());
    for sample in raw {
        let file = format!("{}.wav", sample.file_stem);
        let entries = rederived_entries(ledger, rules, manifest, sample, &file, target)?;
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
                key: (sample.tone, e.alias.clone()),
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
    manifest: &Manifest,
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
    let source = koeru_core::reclist::row_entries(rules, alias_method(manifest.method), &line);
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
