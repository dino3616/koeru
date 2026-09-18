//! 書き出しの縦切り（`PROFILE-M4`）。
//!
//! 素材を置く → 検証 → ZIP と UAR → 読み戻し → 台帳へ記録、までを
//! 音声デバイス無しで1本通す。
//!
//! 形式的な契約は `specs/requirements/packaging-export.fsl`。
//! ここで見るのは、その契約どおりに実装が動くかどうか。

use std::path::PathBuf;

use koeru_app_lib::Studio;
use koeru_package::archive;

/// 試験ごとに独立したライブラリを作る。
fn library(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("koeru-export-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

/// 2行ぶんの素材を置いた音源を開く。
fn seeded(tag: &str) -> Studio {
    let mut studio = Studio::open(library(tag)).expect("ライブラリを開ける");
    let id = studio.create_project("こえるちゃん").expect("作れる");
    studio.open_project(id).expect("開ける");

    let rows = studio.rows_with_takes().expect("録音リストを引ける");
    for row in rows.iter().take(2) {
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

    let exported = studio.export_package("v1.0").expect("書き出せる");
    assert!(exported.written.zip.is_file(), "ZIP が残ること");
    assert!(exported.written.uar.is_file(), "UAR が残ること");
    assert_eq!(exported.release.seq, 1);
    assert_eq!(exported.release.archive_name, "000001-v1.0.zip");

    // 音源ルートフォルダ1つを内包する単層構造（`TR-PKG-26`）。
    let names = archive::entry_names(&exported.written.zip).expect("読める");
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
    let uar = archive::entry_names(&exported.written.uar).expect("読める");
    assert!(uar.contains(&"install.txt".to_owned()));
    assert!(!names.contains(&"install.txt".to_owned()));
    assert_eq!(names.len() + 1, uar.len(), "中身は同じ");
}

/// `TR-PKG-44`。書き出すたびに記録が1つ増え、名前は衝突しない。
#[test]
fn 書き出すたびに記録が増える() {
    let mut studio = seeded("releases");
    assert!(studio.releases().expect("引ける").is_empty());

    studio.export_package("v1").expect("書き出せる");
    studio.export_package("v1").expect("もう一度書き出せる");

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
    assert_eq!(e.kind, "name.disallowed_chars");
}

/// `TR-PKG-17`。CP932 で書けない名前は、置換せずに書き出しを止める。
#[test]
fn cp932_で書けない名前は書き出しを止める() {
    let mut studio = Studio::open(library("cp932")).expect("ライブラリを開ける");
    let id = studio.create_project("こえる🎤").expect("作れる");
    studio.open_project(id).expect("開ける");
    let rows = studio.rows_with_takes().expect("引ける");
    studio
        .seed_material_for_test(&rows[0].row_id)
        .expect("素材を置ける");

    let state = studio.package_state().expect("状態を引ける");
    assert!(!state.may_export(), "止まること");
    assert_eq!(state.unencodable.len(), 1);
    // 代替案を出す。「書けません」だけでは直しようがない。
    assert_eq!(state.unencodable[0].suggestion.as_deref(), Some("こえる"));
    assert_eq!(
        studio.export_package("v1").expect_err("止まること").kind,
        "package.validation_failed"
    );

    // UTF-8 で出すなら通る（`TR-PKG-12`）。
    let mut d = studio.package_settings().expect("引ける");
    d.profile = "openutau".to_owned();
    studio.set_package_settings(&d).expect("保存できる");
    assert!(studio.package_state().expect("引ける").may_export());
    studio.export_package("v1").expect("書き出せる");
}

/// `TR-PKG-28`、`DEC-PKG-011`。書いた節だけが説明書に出る。
#[test]
fn 規約は未記入でも書き出せる() {
    let mut studio = seeded("terms");
    let contents = studio.package_contents().expect("引ける");
    assert!(contents.iter().any(|(p, _)| p == "readme.txt"));

    studio
        .export_package("v1")
        .expect("規約が無くても書き出せる");
}
