//! 1つの音源の中に、方式の違う枠が並ぶとき（`DEC-RCL-018`、`TR-RCL-28`）。
//!
//! 録音リスト・被覆・歌える曲・試唱・書き出しが、同じ枠の解決（`koeru_model::subbank`、
//! `koeru_model::alias::resolve_lanes`）を通ること。 枠ごとに方式が違っても、エイリアスは
//! 音源全体で一意（`TR-PKG-19`）。
//!
//! 枠を1つしか持たない音源の書き出しが、これまでと同じファイル構成と `oto.ini` を出すこと
//! も、ここで見る。 枠を持たなかった頃の値は、枠を入れる前のコードで同じ手順を走らせて
//! 取った。
//!
//! `Studio::open` は MFA のモデルを読むので、書いていない OS 向けの組み立て
//! （`koeru_force_unsupported_backend`）では設計どおり失敗する（`DEC-ALN-016`）。

#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
#![cfg(feature = "test-hooks")]

use std::path::PathBuf;

use koeru_app_lib::Studio;
use koeru_app_lib::studio::{AddSubbank, ReviseSubbank};
use koeru_core::db::Ledger;
use koeru_formats::archive;
use koeru_model::id::SubbankId;

/// 試験ごとに独立したライブラリを作る。
fn library(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("koeru-mixed-{}-{tag}-{n}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    root
}

fn open(tag: &str, method: &str, tones: &[i32]) -> Studio {
    let mut s = Studio::open(library(tag)).expect("ライブラリを開ける");
    let id = s
        .create_project_with("まぜる", method, tones)
        .expect("作れる");
    s.open_project(id).expect("開ける");
    s
}

fn names_of(path: &std::path::Path) -> Vec<String> {
    archive::entry_names(&mut std::fs::File::open(path).expect("開ける")).expect("読める")
}

/// 枠の行を全部録った扱いにする。
fn seed_cell(s: &mut Studio, cell: SubbankId) {
    let rows: Vec<String> = {
        let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
        l.unrecorded_rows_of(cell, koeru_core::inventory::UnitSet::Core)
            .expect("引ける")
            .into_iter()
            .map(|r| r.id)
            .collect()
    };
    assert!(!rows.is_empty(), "枠に行がある");
    for r in rows {
        // 詰め直しで行が組み変わらない限り、1行ずつ置ける。 すでに録った綴りだけの行は
        // 台帳から外れていることがあるので、無いものは飛ばす。
        s.seed_material_for_test(&r).expect("素材を置ける");
    }
}

fn cells(s: &mut Studio) -> Vec<(SubbankId, i32)> {
    s.subbanks()
        .expect("読める")
        .into_iter()
        .map(|b| (b.id, b.tone))
        .collect()
}

/// Normal = CVVC、Growl = 単独音の、2つの方式が並ぶ音源。 どちらも A3。
fn normal_cvvc_and_growl_single(tag: &str) -> (Studio, SubbankId, SubbankId) {
    let mut s = open(tag, "cvvc", &[57]);
    let normal = s.subbanks().expect("読める")[0].id;
    s.revise_subbank(
        normal,
        &ReviseSubbank {
            voice_color: Some("Normal"),
            ..ReviseSubbank::default()
        },
    )
    .expect("台帳を書ける")
    .expect("付け替えられる");
    let growl = s
        .add_subbank(&AddSubbank {
            voice_color: Some("Growl"),
            preset: "single",
            tone: 57,
            after: None,
        })
        .expect("台帳を書ける")
        .expect("足せる")
        .id;
    (s, normal, growl)
}

/// 方式の違う2つの枠を持つ音源が、被覆も書き出しも通る（`DEC-RCL-018`）。
///
/// **音源の方式1つで全枠を見ていた。** 単独音の枠を CVVC の要求表で見るので、
/// どれだけ録っても「足りない」が消えず、書き出せなかった。
/// 区画は枠ごとで、エイリアスは音源全体で一意（`TR-PKG-19`）。
#[test]
fn 方式の違う枠を持つ音源が書き出せる() {
    let (mut s, normal, growl) = normal_cvvc_and_growl_single("export");
    seed_cell(&mut s, normal);
    seed_cell(&mut s, growl);

    let state = s.package_state().expect("状態を引ける");
    assert!(
        state.missing_aliases.is_empty(),
        "{:?}",
        state.missing_aliases
    );
    assert!(state.may_export(), "{:?}", state.findings);

    let out = s.export_package().expect("書き出せる");
    let names = names_of(&out.written.zip);
    // 区画は枠ごと。 声色と方式が分かれているので、どちらも名前に入る。
    for want in ["Normal_cvvc/oto.ini", "Growl_single/oto.ini", "prefix.map"] {
        assert!(
            names.iter().any(|n| n.ends_with(&format!("/{want}"))),
            "{want} が入っていない: {:?}",
            names.iter().take(8).collect::<Vec<_>>()
        );
    }
    assert!(
        names.iter().all(|n| n.is_ascii()),
        "区画の名前は ASCII（`TR-PKG-16`）"
    );

    // 配るエイリアスは、両方の枠の綴りの合計。 Growl の単独音だけで要求表の数、
    // Normal の CVVC がそれに足される。 一意性は書き出し前の検査が見ていて、通っているので
    // 重複は無い。
    let rules = koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core);
    let single = koeru_package::coverage::required(
        &rules,
        koeru_core::alias::Method::Single,
        koeru_core::inventory::UnitSet::Core,
    )
    .expect("単独音は要求表を持つ");
    let cvvc = koeru_package::coverage::required(
        &rules,
        koeru_core::alias::Method::Cvvc,
        koeru_core::inventory::UnitSet::Core,
    )
    .expect("CVVC は要求表を持つ");
    assert_eq!(
        usize::try_from(out.release.alias_count).expect("負にならない"),
        single.len() + cvvc.len(),
        "枠ごとに、その枠の方式の綴りを配る"
    );
}

/// 同じ高さ・同じ声色に、連続音と CVVC の枠が並んでも、綴りは音源全体で一意。
///
/// 連続音と CVVC はどちらも `- か` を持つ。 区画の名前（フォルダとサフィックス）が高さだけだと、
/// 2つの区画に同じ綴りが並んで `TR-PKG-19` を割る。 方式が分かれていれば、方式も名前に入る。
#[test]
fn 同じ高さの方式違いの枠でも綴りは音源全体で一意() {
    let mut s = open("same-tone", "sequential", &[57]);
    let seq = s.subbanks().expect("読める")[0].id;
    let cvvc = s
        .add_subbank(&AddSubbank {
            voice_color: None,
            preset: "cvvc",
            tone: 57,
            after: None,
        })
        .expect("台帳を書ける")
        .expect("足せる")
        .id;
    seed_cell(&mut s, seq);
    seed_cell(&mut s, cvvc);

    let state = s.package_state().expect("状態を引ける");
    assert!(state.may_export(), "{:?}", state.findings);
    let out = s.export_package().expect("書き出せる");
    let names = names_of(&out.written.zip);
    for want in ["seq/oto.ini", "cvvc/oto.ini"] {
        assert!(
            names.iter().any(|n| n.ends_with(&format!("/{want}"))),
            "{want} が入っていない"
        );
    }
}

/// 片方の枠が足りなければ、書き出さない（`TR-PKG-23`）。 足りているのは、枠が自分の方式で
/// 判定される。
#[test]
fn 片方の枠が足りなければ書き出さない() {
    let (mut s, normal, _growl) = normal_cvvc_and_growl_single("partial");
    seed_cell(&mut s, normal);

    let state = s.package_state().expect("状態を引ける");
    assert!(!state.may_export(), "Growl が空のまま出さない");
    assert!(!state.missing_aliases.is_empty(), "足りない綴りを全件出す");
    assert_eq!(
        s.export_package().expect_err("止まること").code,
        "package.incomplete_coverage"
    );
}

/// 方式の違う枠を持つ音源が、試唱できる。 歌わせる声色の枠が、その方式で解決する。
///
/// 枠の並びを入れ替えて、どちらの方式が先頭でも鳴る。
#[test]
fn 方式の違う枠を持つ音源を試唱できる() {
    let (mut s, normal, growl) = normal_cvvc_and_growl_single("sing");
    seed_cell(&mut s, normal);
    seed_cell(&mut s, growl);
    let song = {
        let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
        l.songs_in_bank().expect("引ける")[0].0.clone()
    };

    for order in [[normal, growl], [growl, normal]] {
        s.reorder_subbanks(&order)
            .expect("台帳を書ける")
            .expect("並べ替えられる");
        assert_eq!(cells(&mut s).iter().map(|c| c.0).collect::<Vec<_>>(), order);
        let sung = s.sing_song(&song).expect("試唱できる");
        assert!(sung.phrases > 0, "鳴るフレーズがある");
        assert_eq!(sung.dropped_phrases, 0, "素材が足りて落ちない");
    }
}

/// 枠を1つだけ持つ音源（これまでの音源）の書き出しは、ファイル構成も `oto.ini` も変わらない。
///
/// 値は、枠ごとに方式を持つ前のコードで同じ手順を走らせて取った。 変わったなら、
/// 受け取った人が持っている音源と、新しく配る音源の `oto.ini` が食い違う。
fn export_of(tag: &str, method: &str, tones: &[i32]) -> (String, usize, String) {
    use koeru_core::release::content_hash;

    let mut s = open(tag, method, tones);
    for (cell, _) in cells(&mut s) {
        seed_cell(&mut s, cell);
    }
    let out = s.export_package().expect("書き出せる");
    let root = format!(
        "{}/",
        s.package_settings().expect("引ける").distribution_name
    );
    // 音源ルートの名前は表示名から作る。 値に混ぜると、名前を変えただけで落ちる。
    let names: Vec<String> = names_of(&out.written.zip)
        .into_iter()
        .map(|n| n.strip_prefix(&root).unwrap_or(&n).to_owned())
        .collect();
    (
        content_hash(names.join("\n").as_bytes()),
        names.len(),
        out.release.oto_hash,
    )
}

/// `(エントリ名の並びのハッシュ, エントリ数, oto.ini のハッシュ)`。
type Golden = (&'static str, usize, &'static str);

fn same_as(got: (String, usize, String), want: Golden) {
    assert_eq!(
        (got.0.as_str(), got.1, got.2.as_str()),
        want,
        "枠を1つだけ持つ音源の書き出しが変わった"
    );
}

#[test]
fn 単独音の単音階の書き出しは変わらない() {
    same_as(
        export_of("g-single", "single", &[57]),
        (
            "f1dcd7d8e05104cdb7980fc9d9f838d14e2fbe439b912ea361a03af11d767e3f",
            63,
            "b6554b77e7c26276430c2f6a81d7c4b3ee512f22849752f03304396790f05a2b",
        ),
    );
}

#[test]
fn 連続音の多音階の書き出しは変わらない() {
    same_as(
        export_of("g-seq", "sequential", &[55, 62, 69]),
        (
            "e8e217f2d803475e6164a0d345f9715938590a89e685abfe13c7c017a5526170",
            1574,
            "9b32b19d8c8933c8ab3d8ce86980fb5abe924b088f5c9904fa8a3dcfbcf94195",
        ),
    );
}

#[test]
fn cvvc_の単音階の書き出しは変わらない() {
    same_as(
        export_of("g-cvvc", "cvvc", &[57]),
        (
            "8a84da4c8e38cb4bea979e9eed18e760b02d0c238554cf5d9cae5ea48d2bbe2b",
            267,
            "b4e2a9cb0d6ebabfc4afe00d2011b9a6dc4de68b1168b4640cd8eca1d5d80990",
        ),
    );
}
