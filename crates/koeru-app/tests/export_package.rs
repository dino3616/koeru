//! 書き出しの縦切り（`PROFILE-M4`）。
//!
//! 素材を置く → 検証 → ZIP と UAR → 読み戻し → 台帳へ記録、までを
//! 音声デバイス無しで1本通す。
//!
//! 形式的な契約は `specs/requirements/packaging-export.fsl`。
//! ここで見るのは、その契約どおりに実装が動くかどうか。

// MFA を組んでいない OS では KOERU が起動しない（`DEC-ALN-016`）。
// `Studio::open` が設計どおり失敗するので、ここは走らせない——
// その契約そのものは `align.rs` の単体試験が見ている。
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

use std::path::PathBuf;

use koeru_app_lib::Studio;
use koeru_formats::archive;

/// 書き出した ZIP / UAR のエントリ名。
fn entry_names(path: &std::path::Path) -> Result<Vec<String>, archive::ArchiveError> {
    archive::entry_names(&mut std::fs::File::open(path)?)
}

/// 試験ごとに独立したライブラリを作る。
fn library(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("koeru-export-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 全行ぶんの素材を置いた音源を開く。
///
/// **一部だけでは書き出せない**（`TR-PKG-23`、`INV-PKG-102`）。
/// 被覆が満ちていない音源は関門で止まるので、書き出しの試験は全行要る。
fn seeded(tag: &str) -> Studio {
    let mut studio = partial(tag, usize::MAX);
    assert!(
        studio
            .package_state()
            .expect("状態を引ける")
            .missing_aliases
            .is_empty(),
        "全行置いたら被覆が満ちること"
    );
    studio
}

/// 先頭 `rows` 行だけ素材を置いた音源を開く。
fn partial(tag: &str, rows: usize) -> Studio {
    let mut studio = Studio::open(library(tag)).expect("ライブラリを開ける");
    let id = studio.create_project("こえるちゃん").expect("作れる");
    studio.open_project(id).expect("開ける");

    let all = studio.rows_with_takes().expect("録音リストを引ける");
    for row in all.iter().take(rows) {
        studio
            .seed_material_for_test(&row.row_id)
            .expect("素材を置ける");
    }
    studio
}

#[test]
fn 検証を通って_zip_と_uar_が出る() {
    let mut studio = seeded("ok");

    let state = studio.package_state().expect("状態を引ける");
    assert!(state.may_export(), "{:?}", state.findings);
    assert!(state.alias_count > 0);

    let mut d = studio.package_settings().expect("引ける");
    d.version = Some("v1.0".to_owned());
    studio.set_package_settings(&d).expect("保存できる");

    let exported = studio.export_package().expect("書き出せる");
    assert!(exported.written.zip.is_file(), "ZIP が残ること");
    assert!(exported.written.uar.is_file(), "UAR が残ること");
    assert_eq!(exported.release.seq, 1);
    assert_eq!(exported.release.archive_name, "000001-v1.0.zip");

    // 音源ルートフォルダ1つを内包する単層構造（`TR-PKG-26`）。
    let names = entry_names(&exported.written.zip).expect("読める");
    let root = format!(
        "{}/",
        studio.package_settings().expect("引ける").distribution_name
    );
    for name in &names {
        assert!(name.starts_with(&root), "{name}");
        assert!(name.is_ascii(), "{name}");
    }
    for want in ["character.txt", "character.yaml", "readme.txt", "oto.ini"] {
        assert!(
            names.contains(&format!("{root}{want}")),
            "{want} が入っていない"
        );
    }

    // UAR だけが install.txt を持つ（`TR-PKG-09`、`DEC-PKG-010`）。
    let uar = entry_names(&exported.written.uar).expect("読める");
    assert!(uar.contains(&"install.txt".to_owned()));
    assert!(!names.contains(&"install.txt".to_owned()));
    assert_eq!(names.len() + 1, uar.len(), "中身は同じ");
}

/// `TR-PKG-44`。書き出すたびに記録が1つ増え、名前は衝突しない。
///
/// 版の札は配布に出す値として保存してあるもの1つだけ（`DEC-PKG-010`）。
#[test]
fn 書き出すたびに記録が増える() {
    let mut studio = seeded("releases");
    assert!(studio.releases().expect("引ける").is_empty());

    studio.export_package().expect("書き出せる");
    studio.export_package().expect("もう一度書き出せる");

    let releases = studio.releases().expect("引ける");
    assert_eq!(releases.len(), 2);
    assert_ne!(
        releases[0].archive_name, releases[1].archive_name,
        "同じ呼び名でも名前は衝突しない"
    );
}

/// `TR-PKG-16`、`DEC-PKG-008`。使えない配布名は保存の時点で止める。
#[test]
fn 使えない配布名は保存させない() {
    let mut studio = seeded("name");
    let mut d = studio.package_settings().expect("引ける");
    d.distribution_name = "こえる".to_owned();

    let e = studio.set_package_settings(&d).expect_err("止まること");
    assert_eq!(e.code, "name.disallowed_chars");
}

/// `TR-PKG-23`、`INV-PKG-102`。部分的なパッケージを出さない。
#[test]
fn 録りきっていないと書き出せない() {
    let mut studio = partial("partial", 2);

    let state = studio.package_state().expect("状態を引ける");
    assert!(!state.may_export(), "止まること");
    assert!(
        !state.missing_aliases.is_empty(),
        "足りない分を全件出すこと"
    );
    assert_eq!(
        studio.export_package().expect_err("止まること").code,
        "package.incomplete_coverage"
    );
}

/// `TR-PKG-12`。知らない書き出し方を既定へ倒さない。
#[test]
fn 知らない書き出し方は断る() {
    let mut studio = partial("profile", 1);
    let mut d = studio.package_settings().expect("引ける");
    d.profile = "なんだこれ".to_owned();

    // 保存の時点で断る。呼び出し口は文字列で受けるので、ここが関門。
    assert_eq!(
        studio
            .set_package_settings(&d)
            .expect_err("止まること")
            .code,
        "package.unknown_profile"
    );
}

/// `TR-PKG-17`。CP932 で書けない名前は、置換せずに書き出しを止める。
#[test]
fn cp932_で書けない名前は書き出しを止める() {
    let mut studio = seeded("cp932");
    let id = studio.project_dir().expect("開いている").id();
    studio.rename_project(id, "こえる🎤").expect("改名できる");

    let state = studio.package_state().expect("状態を引ける");
    assert!(!state.may_export(), "止まること");
    assert_eq!(state.unencodable.len(), 1);
    // 代替案を出す。「書けません」だけでは直しようがない。
    assert_eq!(state.unencodable[0].suggestion.as_deref(), Some("こえる"));
    assert_eq!(
        studio.export_package().expect_err("止まること").code,
        "package.validation_failed"
    );

    // UTF-8 で出すなら通る（`TR-PKG-12`）。
    let mut d = studio.package_settings().expect("引ける");
    d.profile = "openutau".to_owned();
    studio.set_package_settings(&d).expect("保存できる");
    assert!(studio.package_state().expect("引ける").may_export());
    studio.export_package().expect("書き出せる");
}

/// `TR-PKG-28`、`DEC-PKG-011`。書いた節だけが説明書に出る。
#[test]
fn 規約は未記入でも書き出せる() {
    let mut studio = seeded("terms");
    let contents = studio.package_contents().expect("引ける");
    assert!(contents.iter().any(|(p, _)| p == "readme.txt"));

    studio.export_package().expect("規約が無くても書き出せる");
}

/// 連続音で録った音源から、単独音の配布物が出る（`TR-PKG-22`〜`24`）。
///
/// **可否を変換後の綴りで見ていた。** 連続音の素材が持つのは `- か` で、
/// 単独音が要求するのは素の `か`。それを作り出すのが再導出なのに、
/// その手前で「持っていない」と断っていた——画面が「出せます」と
/// 言う音源が、押すと必ず `package.incomplete_coverage` で落ちる。
#[test]
fn 連続音から単独音へ降りて書き出せる() {
    use koeru_core::project::Method;

    // 連続音で全行録る。 語頭 CV が全部揃うので、単独音へ降りられる。
    let mut studio = Studio::open(library("downgrade")).expect("ライブラリを開ける");
    let id = studio
        .create_project_with("こえるちゃん", "sequential", &[57])
        .expect("作れる");
    studio.open_project(id).expect("開ける");
    for row in studio.rows_with_takes().expect("引ける") {
        studio
            .seed_material_for_test(&row.row_id)
            .expect("素材を置ける");
    }

    let mut d = studio.package_settings().expect("引ける");
    d.version = Some("v1".to_owned());
    studio.set_package_settings(&d).expect("保存できる");

    // 画面が出す一覧に単独音が並ぶ（`TR-PKG-22`）。
    let state = studio.package_state().expect("引ける");
    assert!(
        state.downgrades.iter().any(|x| x.method == Method::Single),
        "単独音へ降りられる: {:?}",
        state.downgrades
    );
    let estimated = state
        .downgrades
        .iter()
        .find(|x| x.method == Method::Single)
        .map(|x| x.bytes)
        .expect("単独音がある");

    // **2つの関門で止まっていた。** 語頭 CV の重複で `review.conflicting_alias`、
    // 越えたあとは綴りの重なりで `package.duplicate_alias`。
    // 前者は綴りの持ち主を音高ごとに1行へ決めて（`DEC-ALN-017`、いまは `DEC-RCL-016`）、
    // 後者は配る素材を綴りごとに1つ選んで（`DEC-PKG-014`）解いた。
    let out = studio
        .export_downgrade(Method::Single)
        .expect("単独音へ降りて書き出せる");

    // **綴りごとに1つ**（`TR-PKG-19`, `DEC-PKG-014`）。 同じ綴りを出せる素材は
    // 30 以上あるので、選ばなければエイリアス数がその倍数に膨らむ。
    // 単独音の要求表と同じ数なら、1綴り1件で収まっている。
    let want = koeru_package::coverage::required(
        &koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
        koeru_core::alias::Method::Single,
        koeru_core::inventory::UnitSet::Core,
    )
    .expect("単独音は要求表を持つ");
    assert_eq!(
        usize::try_from(out.release.alias_count).expect("負にならない"),
        want.len(),
        "配るのは綴りごとに1つ"
    );
    assert_eq!(out.release.method, Method::Single);

    // 押す前に出す容量は、書き出しが実際に複製する素材の大きさ（`DEC-PLT-052`）。
    // 一覧と書き出しが別の選び方をすると、出す数と作る量が食い違う。
    let root = studio
        .project_dir()
        .expect("開いている")
        .root()
        .to_path_buf();
    let copied: u64 = entry_names(&out.written.zip)
        .expect("読める")
        .iter()
        .filter(|n| n.ends_with(".wav"))
        .map(|n| {
            let stem = std::path::Path::new(n)
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("WAV の名前");
            std::fs::metadata(root.join(format!("audio/{stem}_1.wav")))
                .expect("素材がある")
                .len()
        })
        .sum();
    assert_eq!(estimated, copied, "概算は実際に複製する素材と同じ");
}

// ## 配布物と版（`DEC-PKG-018`）

use koeru_app_lib::studio::{DistributionDenied, PackagedSubbankChange, ReviseDistribution};
use koeru_core::db::shelf::DistributionRefusal;

/// 配布物が1つだった頃の画面は、フォルダ名と版を音源の設定として読み書きする。
/// それが先頭の配布物（既定の「すべて」）の値として残る。
#[test]
fn 旧い設定の口は先頭の配布物へ書く() {
    let mut studio = seeded("shelf-legacy");
    let before = studio.distributions().expect("引ける");
    assert_eq!(before.len(), 1, "増やさない人の画面は今と同じ");
    assert_eq!(before[0].name, "すべて");
    assert_eq!(before[0].draft_version.as_deref(), Some("1.0.0"));

    let mut d = studio.package_settings().expect("引ける");
    d.distribution_name = "Koeru-A".to_owned();
    d.version = Some("v2".to_owned());
    studio.set_package_settings(&d).expect("保存できる");

    let after = studio.distributions().expect("引ける");
    assert_eq!(after[0].archive_name, "Koeru-A");
    assert_eq!(after[0].draft_version.as_deref(), Some("v2"));
    let again = studio.package_settings().expect("引ける");
    assert_eq!(again.distribution_name, "Koeru-A");
    assert_eq!(again.version.as_deref(), Some("v2"));
}

/// `TR-PKG-60`。版は配布物ごとに数え、書き出し済みの番号を選ぶと、同じ番号で記録が1件増える。
/// 前の ZIP は消えない（`TR-PKG-44`）。
#[test]
fn 版は配布物ごとに数え_書き出し済みの番号を選ぶと記録が1件増える() {
    let mut studio = seeded("shelf-versions");
    let all = studio.distributions().expect("引ける")[0].id;
    let copy = studio
        .duplicate_distribution(all)
        .expect("書ける")
        .expect("写せる")
        .id;

    let first = studio
        .export_distribution(Some(copy), None, None)
        .expect("台帳を書ける")
        .expect("書き出せる");
    assert_eq!(first.release.distribution, copy);
    assert_eq!(first.release.version, "1.0.0", "写した配布物は最初の版から");
    let shelf = studio.distributions().expect("引ける");
    assert_eq!(shelf[1].draft_version.as_deref(), Some("1.0.1"));
    assert_eq!(
        shelf[0].draft_version.as_deref(),
        Some("1.0.0"),
        "別の配布物の版は動かない"
    );
    assert_eq!(shelf[1].releases.len(), 1);
    assert!(shelf[0].releases.is_empty());

    // 書き出し済みの番号を選んで出し直す。 下書きは動かない。
    let again = studio
        .export_distribution(Some(copy), Some("1.0.0"), None)
        .expect("台帳を書ける")
        .expect("書き出せる");
    assert_eq!(again.release.version, "1.0.0");
    assert_ne!(again.release.seq, first.release.seq);
    assert!(first.written.zip.is_file(), "前の ZIP は消えない");
    assert!(again.written.zip.is_file());
    let shelf = studio.distributions().expect("引ける");
    assert_eq!(shelf[1].releases.len(), 2);
    assert_eq!(shelf[1].draft_version.as_deref(), Some("1.0.1"));
}

/// 配布物を消しても、書き出しの記録は残る。 最後の1つは消せない。
#[test]
fn 配布物を消しても記録は残り_最後の1つは消せない() {
    let mut studio = seeded("shelf-remove");
    let all = studio.distributions().expect("引ける")[0].id;
    assert_eq!(
        studio.remove_distribution(all).expect("台帳を書ける"),
        Err(DistributionDenied::Refused(DistributionRefusal::LastOne))
    );

    let copy = studio
        .duplicate_distribution(all)
        .expect("書ける")
        .expect("写せる")
        .id;
    studio
        .export_distribution(Some(copy), None, None)
        .expect("台帳を書ける")
        .expect("書き出せる");
    studio
        .remove_distribution(copy)
        .expect("台帳を書ける")
        .expect("消せる");

    assert_eq!(studio.distributions().expect("引ける").len(), 1);
    let kept = studio.releases().expect("引ける");
    assert_eq!(kept.len(), 1, "記録は残る");
    assert_eq!(kept[0].distribution, copy);
}

/// 指した配布物が無ければ何も書かない。
#[test]
fn 無い配布物へは書き出さない() {
    let mut studio = seeded("shelf-unknown");
    let ghost = koeru_model::id::DistributionId::new(99);
    let denied = studio
        .export_distribution(Some(ghost), None, None)
        .expect("台帳を読める")
        .expect_err("断ること");
    assert_eq!(
        denied,
        DistributionDenied::Refused(DistributionRefusal::UnknownDistribution)
    );
    assert!(studio.releases().expect("引ける").is_empty());
}

/// `TR-PKG-61`。書き出していなければ「分からない」、書き出した直後は0。
#[test]
fn 書き出したあとに採用が変わった行は_書き出すまで数えない() {
    let mut studio = seeded("shelf-changed");
    assert_eq!(
        studio.distributions().expect("引ける")[0].rows_changed_since_release,
        None
    );
    studio.export_package().expect("書き出せる");
    assert_eq!(
        studio.distributions().expect("引ける")[0].rows_changed_since_release,
        Some(0)
    );
}

/// `TR-PKG-59`。入れる枠を外すと、外した枠が受け持っていた高さが空く。 直した範囲はその配布物にだけ
/// 効き、枠の担当音域は変わらない。
#[test]
fn 担当音域は配布物の書き出しに限って直せる() {
    let mut studio = Studio::open(library("shelf-ranges")).expect("ライブラリを開ける");
    let id = studio
        .create_project_with("多音階", "single", &[55, 62, 69])
        .expect("作れる");
    studio.open_project(id).expect("開ける");
    let subbanks = studio.subbanks().expect("読める");
    let all = studio.distributions().expect("引ける")[0].id;
    assert_eq!(studio.distributions().expect("引ける")[0].subbanks.len(), 3);
    assert!(
        studio.distributions().expect("引ける")[0]
            .uncovered
            .is_empty()
    );

    let copy = studio
        .duplicate_distribution(all)
        .expect("書ける")
        .expect("写せる")
        .id;
    // 中の枠を外す。
    let outer = [
        PackagedSubbankChange {
            subbank: subbanks[0].id,
            range: None,
        },
        PackagedSubbankChange {
            subbank: subbanks[2].id,
            range: None,
        },
    ];
    let trimmed = studio
        .revise_distribution(
            copy,
            &ReviseDistribution {
                subbanks: Some(&outer),
                ..ReviseDistribution::default()
            },
        )
        .expect("書ける")
        .expect("直せる");
    assert_eq!(trimmed.uncovered.len(), 1, "真ん中の高さが空く");
    assert_eq!(
        (trimmed.uncovered[0].1.low, trimmed.uncovered[0].1.high),
        (subbanks[1].range.low, subbanks[1].range.high)
    );

    // 下の枠の範囲を伸ばして空きを埋める。 その配布物にだけ効く。
    let stretched = [
        PackagedSubbankChange {
            subbank: subbanks[0].id,
            range: Some((subbanks[0].range.low, subbanks[1].range.high)),
        },
        PackagedSubbankChange {
            subbank: subbanks[2].id,
            range: None,
        },
    ];
    let filled = studio
        .revise_distribution(
            copy,
            &ReviseDistribution {
                subbanks: Some(&stretched),
                ..ReviseDistribution::default()
            },
        )
        .expect("書ける")
        .expect("直せる");
    assert!(filled.uncovered.is_empty());
    assert!(filled.subbanks[0].range_edited);
    assert!(!filled.subbanks[1].range_edited);

    let still = studio.subbanks().expect("読める");
    assert_eq!(
        still[0].range, subbanks[0].range,
        "枠の担当音域は変わらない"
    );
    assert!(still.iter().all(|s| !s.range_edited));
    let untouched = studio.distributions().expect("引ける");
    assert!(untouched[0].subbanks.iter().all(|m| !m.range_edited));
    assert!(untouched[0].uncovered.is_empty());
}

/// 配布物の入力の誤りと断りは、何も書かずに値で返る（`DEC-PLT-038`）。
#[test]
fn 配布物の直しの誤りは何も書かずに断る() {
    let mut studio = seeded("shelf-denied");
    let all = studio.distributions().expect("引ける")[0].id;
    let copy = studio
        .duplicate_distribution(all)
        .expect("書ける")
        .expect("写せる");
    let first_name = studio.distributions().expect("引ける")[0]
        .archive_name
        .clone();

    let empty = studio
        .revise_distribution(
            copy.id,
            &ReviseDistribution {
                subbanks: Some(&[]),
                ..ReviseDistribution::default()
            },
        )
        .expect("台帳を書ける");
    assert_eq!(
        empty.expect_err("断ること"),
        DistributionDenied::Invalid(koeru_app_lib::studio::Violation {
            field: "subbanks",
            code: "distribution.no_subbanks",
        })
    );

    let bad = studio
        .revise_distribution(
            copy.id,
            &ReviseDistribution {
                archive_name: Some("こえる🎤"),
                ..ReviseDistribution::default()
            },
        )
        .expect("台帳を書ける")
        .expect_err("断ること");
    assert!(matches!(bad, DistributionDenied::Invalid(v) if v.field == "archiveName"));

    // 自分のフォルダ名へ書き直すのは重なりではない。 別の配布物のものは重なる。
    studio
        .revise_distribution(
            copy.id,
            &ReviseDistribution {
                archive_name: Some(&copy.archive_name),
                ..ReviseDistribution::default()
            },
        )
        .expect("台帳を書ける")
        .expect("通る");
    let taken = studio
        .revise_distribution(
            copy.id,
            &ReviseDistribution {
                name: Some("変えた名前"),
                archive_name: Some(&first_name),
                ..ReviseDistribution::default()
            },
        )
        .expect("台帳を書ける")
        .expect_err("断ること");
    assert_eq!(
        taken,
        DistributionDenied::Refused(DistributionRefusal::Conflict { existing: all })
    );
    assert_ne!(
        studio.distributions().expect("引ける")[1].name,
        "変えた名前",
        "断るときは何も書かない"
    );
}
