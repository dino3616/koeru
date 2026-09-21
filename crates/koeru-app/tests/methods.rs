//! 方式プリセットを選んでプロジェクトを作る（`TR-RCL-01`, `TR-REC-36`, `TR-RCL-26`）。
//!
//! 音を鳴らさないので、どの OS でも通る。

use koeru_app_lib::Studio;
use koeru_core::db::Ledger;

fn manifest_of(s: &Studio, id: uuid::Uuid) -> koeru_core::project::Manifest {
    s.projects()
        .expect("引ける")
        .into_iter()
        .find_map(|(got, m)| (got == id).then_some(m))
        .expect("ある")
        .expect("読める")
}

fn studio(tag: &str) -> (Studio, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("koeru-methods-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let s = Studio::open(root.clone()).expect("開ける");
    (s, root)
}

/// 既定は単独音のまま（`TR-RCL-01`）。
#[test]
fn 既定は単独音() {
    let (mut s, _root) = studio("default");
    let id = s.create_project("既定").expect("作れる");
    let m = manifest_of(&s, id);
    assert_eq!(m.method, koeru_core::project::Method::Single);
    assert_eq!(m.preset_id.as_deref(), Some("single"));
    assert_eq!(
        m.inventory_version,
        Some(koeru_core::inventory::INVENTORY_VERSION),
        "作成時の版を記録する（`TR-RCL-02`）"
    );
}

/// 連続音を選ぶと、台帳がエイリアスで被覆を持つ（`TR-PKG-22`）。
#[test]
fn 連続音のプロジェクトが作れる() {
    let (mut s, _root) = studio("seq");
    let id = s
        .create_project_with("連続音", "sequential")
        .expect("作れる");
    s.open_project(id).expect("開ける");
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    assert_eq!(l.recording_tones().expect("引ける").len(), 1);
    // 語頭 CV が台帳に入っている（`TR-RCL-21`）。
    let aliases = l.all_aliases().expect("引ける");
    assert!(aliases.contains("- あ"), "語頭 CV がある");
    assert!(aliases.contains("a か"), "VCV がある");
}

/// 多音階は音高ごとにディレクトリと行集合を持つ（`TR-REC-36`, `TR-RCL-26`）。
#[test]
fn 多音階は音高ごとに分かれる() {
    let (mut s, _root) = studio("multi");
    let id = s
        .create_project_with("多音階", "multi-pitch-sequential")
        .expect("作れる");
    let manifest = manifest_of(&s, id);
    s.open_project(id).expect("開ける");
    let dir = s.project_dir().expect("開いている").clone();
    assert_eq!(
        manifest.method,
        koeru_core::project::Method::MultiPitchSequential
    );

    // 音高ごとのサブディレクトリ。名前は ASCII の英語音名（`TR-REC-36`）。
    for name in ["G3", "D4", "A4"] {
        let d = dir.audio_dir().join(name);
        assert!(d.is_dir(), "{name} のディレクトリが無い");
        assert!(name.is_ascii());
    }

    let mut l = Ledger::open(dir.db_path()).expect("開ける");
    assert_eq!(l.recording_tones().expect("引ける"), [55, 62, 69]);
    // 行 ID は音高ごとに分かれる。 共有すると、どの音高を録ったか分からない。
    let rows = l.rows_with_takes().expect("引ける");
    assert!(rows.iter().any(|r| r.row_id.ends_with("@G3")), "G3 の行");
    assert!(rows.iter().any(|r| r.row_id.ends_with("@A4")), "A4 の行");
    assert_eq!(
        manifest.item_count as usize,
        rows.len(),
        "項目数は音高を掛けたもの"
    );
}

/// 知らないプリセットは黙って単独音にしない。
#[test]
fn 知らないプリセットは断る() {
    let (mut s, _root) = studio("unknown");
    let e = s.create_project_with("なに", "なにこれ").expect_err("断る");
    assert_eq!(e.kind, "preset.unknown");
}

/// 多音階の配布物は、音高ごとに区画と oto.ini を分ける（`TR-ALN-22`, `TR-PKG-04`）。
///
/// **1区画にまとめると、フォルダ間でエイリアスが衝突する。** 3音高の「か」が
/// 同じ名前で3つ並び、`TR-PKG-19` の一意性を割る。
#[test]
fn 多音階は音高ごとに区画を分ける() {
    use koeru_package::bank::{Character, Readme, Subbank, VoiceBank};
    use koeru_package::profile::Profile;
    use koeru_package::{character, tree};

    let tones = [55, 62, 69];
    let subbanks: Vec<Subbank> = tones
        .iter()
        .map(|t| {
            let name = koeru_core::tone::name(*t);
            Subbank {
                folder: Some(name.clone()),
                color: name.clone(),
                prefix: String::new(),
                suffix: name,
                tone: Some(*t),
                samples: Vec::new(),
            }
        })
        .collect();
    let bank = VoiceBank {
        distribution_name: "koeru".to_owned(),
        character: Character {
            name: "こえる".to_owned(),
            ..Character::default()
        },
        readme: Readme::default(),
        method: koeru_core::project::Method::MultiPitchSequential,
        subbanks,
        rules: koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
    };

    // サフィックスは音階名。 一括で付く（`TR-ALN-22`）。
    assert_eq!(
        bank.subbanks
            .iter()
            .map(|s| s.suffix.as_str())
            .collect::<Vec<_>>(),
        ["G3", "D4", "A4"]
    );
    // フォルダ名は ASCII の英語音名（`TR-REC-36`）。
    for s in &bank.subbanks {
        let f = s.folder.as_deref().expect("多音階はフォルダを持つ");
        assert!(f.is_ascii(), "{f}");
    }

    // prefix.map は 84 行で、収録音高から floor 割り当てで作る（`TR-RCL-06`）。
    let map = character::prefix_map(&bank).expect("多音階なら出る");
    assert_eq!(map.lines().count(), 84);
    assert!(map.lines().next().is_some_and(|l| l.ends_with("G3")));

    // 素材が無いので oto.ini は出ないが、組み立ては通る。
    let files = tree::build(&bank, Profile::Both).expect("組み立てられる");
    assert!(
        files.iter().any(|f| f.path == "presamp.ini"),
        "presamp.ini を同梱する"
    );
}
