//! 方式プリセットを選んでプロジェクトを作る（`TR-RCL-01`, `TR-REC-36`, `TR-RCL-26`）。
//!
//! 音を鳴らさない。 ただし MFA を組んでいない OS では走らない（下の `cfg`）。

// MFA を組んでいない OS では KOERU が起動しない（`DEC-ALN-016`）。
// `Studio::open` が設計どおり失敗するので、ここは走らせない——
// その契約そのものは `align.rs` の単体試験が見ている。
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

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
        .create_project_with("連続音", "sequential", &[57])
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
///
/// **音高は方式とは別に選ぶ**（`TR-RCL-01`）。 ここで渡しているのは
/// `TR-RCL-06` の推奨値だが、本数も音高も本人が決められる。
#[test]
fn 多音階は音高ごとに分かれる() {
    let (mut s, _root) = studio("multi");
    let id = s
        .create_project_with("多音階", "sequential", &[55, 62, 69])
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

/// 多音階の提示順は台帳の行 ID で、低い音高から並ぶ（`TR-SYN-19`, `TR-RCL-26`）。
///
/// 素の行 ID で返していた。 台帳の `s001@G3` と一致せず、録る順が常に
/// 正準順へ落ちていた。
#[test]
fn 多音階の提示順は音高ごとに並ぶ() {
    let (mut s, _root) = studio("multi-order");
    let id = s
        .create_project_with("多音階の順", "sequential", &[55, 62])
        .expect("作れる");
    s.open_project(id).expect("開ける");
    let (_, order) = s.recording_order().expect("引ける");
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    let rows = l.rows_with_takes().expect("引ける");
    assert_eq!(order.len(), rows.len(), "未収録の行は全部並ぶ");
    assert!(
        order.iter().all(|o| rows.iter().any(|r| &r.row_id == o)),
        "台帳に無い ID を返さない"
    );
    let first_d4 = order
        .iter()
        .position(|o| o.ends_with("@D4"))
        .expect("D4 がある");
    assert!(
        order[..first_d4].iter().all(|o| o.ends_with("@G3")),
        "G3 を先に並べる"
    );
    assert!(
        order[first_d4..].iter().all(|o| o.ends_with("@D4")),
        "音高を行き来しない"
    );
}

/// 知らないプリセットは黙って単独音にしない。
#[test]
fn 知らないプリセットは断る() {
    let (mut s, _root) = studio("unknown");
    let e = s
        .create_project_with("なに", "なにこれ", &[57])
        .expect_err("断る");
    assert_eq!(e.kind, "preset.unknown");
}

/// 本数も音高も本人が決める（`TR-RCL-01`）。間隔で咎めない。
#[test]
fn 収録音高は本人が決める() {
    let (mut s, _root) = studio("tones");
    // 2 半音差の 2 本。以前は「狭すぎる」と警告していた組み合わせ。
    let id = s
        .create_project_with("せまい", "single", &[60, 62])
        .expect("作れる");
    s.open_project(id).expect("開ける");
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    assert_eq!(l.recording_tones().expect("引ける"), [60, 62]);
}

/// 鳴らせない音高は断る。`prefix.map` が覆うのは C1〜B7。
#[test]
fn 鳴らせない音高は断る() {
    let (mut s, _root) = studio("badtones");
    assert_eq!(
        s.create_project_with("から", "single", &[])
            .expect_err("断る")
            .kind,
        "tone.empty"
    );
    assert_eq!(
        s.create_project_with("そと", "single", &[0])
            .expect_err("断る")
            .kind,
        "tone.out_of_range"
    );
    assert_eq!(
        s.create_project_with("だぶり", "single", &[60, 60])
            .expect_err("断る")
            .kind,
        "tone.duplicate"
    );
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
        method: koeru_core::alias::Method::Sequential,
        tones: vec![55, 62, 69],
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

/// 書き出す `oto.ini` の綴りは、その方式のもの（`TR-SYN-12`, `TR-RCL-18`）。
///
/// **仮名をそのまま並べていた。** 連続音を選んでも配られる `oto.ini` は
/// 単独音のもので、受け取った側は `a か` を1つも引けなかった。
#[test]
fn 連続音の書き出しは文脈つきの綴りになる() {
    let (mut s, _root) = studio("seq-oto");
    let id = s
        .create_project_with("連続音", "sequential", &[57])
        .expect("作れる");
    s.open_project(id).expect("開ける");

    // 1行だけ録る。 全部録らなくても、綴りは1行で分かる。
    let row = first_row(&mut s);
    s.seed_material_for_test(&row).expect("置ける");

    let aliases = exported_aliases(&mut s);
    assert!(
        aliases.iter().any(|a| a.starts_with("- ")),
        "語頭 CV がある: {aliases:?}"
    );
    assert!(
        aliases
            .iter()
            .any(|a| a.contains(' ') && !a.starts_with("- ")),
        "文脈つきの CV がある: {aliases:?}"
    );
}

/// CVVC は CV・VC・語尾の3種を書き出す（`TR-RCL-05`）。
///
/// **CV しか作っていなかった。** 渡りも語尾も `oto.ini` に入らないので、
/// CVVC を選んだ意味が無くなる。
#[test]
fn cvvc_の書き出しは渡りと語尾を持つ() {
    let (mut s, _root) = studio("cvvc-oto");
    let id = s
        .create_project_with("CVVC", "cvvc", &[57])
        .expect("作れる");
    s.open_project(id).expect("開ける");

    let row = first_row(&mut s);
    s.seed_material_for_test(&row).expect("置ける");

    let aliases = exported_aliases(&mut s);
    assert!(
        aliases.iter().any(|a| a.ends_with(" -")),
        "語尾がある: {aliases:?}"
    );
    // 渡りは「母音 子音」。 語頭形とも語尾とも違う形。
    assert!(
        aliases
            .iter()
            .any(|a| a.contains(' ') && !a.starts_with("- ") && !a.ends_with(" -")),
        "渡りがある: {aliases:?}"
    );
}

/// 作るときに選んだ `presamp.ini` が綴りを決める（`TR-SYN-36`, `DEC-SYN-013`）。
///
/// **作ったあとで置かせていた。** 台帳は作った瞬間に既定の表で綴りを書くので、
/// あとから置いた表の綴りは台帳と噛み合わず、一度も効かなかった。
#[test]
fn 選んだ_presamp_が綴りを決める() {
    let (mut s, _root) = studio("presamp");
    // 語頭形の印を `-` から `^` へ替えるだけの表。
    let id = s
        .create_project_with_presamp(
            "差し替え",
            "sequential",
            &[57],
            Some(b"[VERSION]\n1.0\n[BEGINING_CV]\n^ %CV%\n"),
        )
        .expect("作れる");
    s.open_project(id).expect("開ける");
    let dir = s.project_dir().expect("開いている").clone();

    // 録音リストが最初から選んだ表の綴りで入っている。
    let mut l = Ledger::open(dir.db_path()).expect("開ける");
    let aliases = l.all_aliases().expect("引ける");
    assert!(
        aliases.iter().any(|a| a.starts_with("^ ")),
        "選んだ表の語頭形を使う: {:?}",
        aliases.iter().take(8).collect::<Vec<_>>()
    );
    assert!(
        !aliases.iter().any(|a| a.starts_with("- ")),
        "既定の語頭形が混ざらない"
    );
    // 書いていない表は既定で埋まる（`TR-SYN-36`）。 所属表が空のままだと
    // `%v%` が空文字になり、先頭が空白の綴りができる。
    assert!(
        aliases.iter().all(|a| !a.starts_with(' ')),
        "先頭が空白の綴りを作らない: {:?}",
        aliases
            .iter()
            .filter(|a| a.starts_with(' '))
            .collect::<Vec<_>>()
    );
    assert!(
        aliases.iter().any(|a| a.starts_with("a ")),
        "文脈つきの綴りは既定の所属表で解ける: {:?}",
        aliases.iter().take(8).collect::<Vec<_>>()
    );
    // 本人が中身を見られるよう、フォルダにも置く。
    let placed = std::fs::read_to_string(dir.presamp_path()).expect("置いてある");
    assert!(placed.contains("^ %CV%"), "{placed}");
    assert_eq!(
        s.presamp_notice().expect("開いている"),
        None,
        "戻していない"
    );
}

/// あとから書き換えた `presamp.ini` は戻し、中身を別名で残して知らせる（`DEC-SYN-013`）。
#[test]
fn 書き換えた_presamp_は戻して知らせる() {
    let (mut s, root) = studio("presamp-restore");
    let id = s
        .create_project_with("戻す", "sequential", &[57])
        .expect("作れる");
    s.open_project(id).expect("開ける");
    let dir = s.project_dir().expect("開いている").clone();
    let original = std::fs::read_to_string(dir.presamp_path()).expect("置いてある");

    std::fs::write(
        dir.presamp_path(),
        "[VERSION]\n1.0\n[BEGINING_CV]\n^ %CV%\n",
    )
    .expect("置ける");
    // 同じ Studio を開き直しても、同じ音源なら `Open` を作り直さない
    // （`TR-REC-30`）。読ませるには開き直しが要るので、別の Studio で開く。
    let mut s = Studio::open(root).expect("開ける");
    s.open_project(id).expect("開ける");

    let kept = s
        .presamp_notice()
        .expect("開いている")
        .expect("戻したことを知らせる");
    assert!(
        std::fs::read_to_string(dir.root().join(&kept))
            .expect("残してある")
            .contains("^ %CV%"),
        "書き換えた中身を消さない"
    );
    assert_eq!(
        std::fs::read_to_string(dir.presamp_path()).expect("ある"),
        original,
        "作ったときの表へ戻す"
    );

    // 綴りは作ったときの表のまま。
    let song = bundled_song_id(&mut s);
    s.repack_for_selection(&[(song, Vec::new())])
        .expect("詰め直せる");
    let mut l = Ledger::open(dir.db_path()).expect("開ける");
    assert!(
        !l.all_aliases()
            .expect("引ける")
            .iter()
            .any(|a| a.starts_with("^ ")),
        "書き換えた表の綴りを使わない"
    );

    s.dismiss_presamp_notice().expect("下ろせる");
    assert_eq!(s.presamp_notice().expect("開いている"), None);
}

/// 最初の行の ID。
fn first_row(s: &mut Studio) -> String {
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    l.rows_with_takes().expect("引ける")[0].row_id.clone()
}

/// 同梱曲の ID。 詰め直しの入力に要る。
fn bundled_song_id(s: &mut Studio) -> String {
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    l.songs_in_bank().expect("引ける")[0].0.clone()
}

/// 書き出す `oto.ini` に並ぶ綴り。
fn exported_aliases(s: &mut Studio) -> Vec<String> {
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    l.distribution_samples()
        .expect("引ける")
        .into_iter()
        .flat_map(|x| x.otos.into_iter().map(|(a, _)| a))
        .collect()
}

/// 録る順は「次に何を録るか」に効く（`TR-SYN-19`, `TR-REC-18`）。
///
/// **一覧の並び替えだけに使っていた。** 次のフレーズの札も録音の開始も
/// 台帳の正準順で引いていたので、モードを切り替えても録る順は動かなかった。
/// `TR-SYN-19` は「変わるのは『次に何を録るか』の並びだけ」と定めている。
#[test]
fn 録る順が次のフレーズに効く() {
    use koeru_core::order::Mode;

    let (mut s, _root) = studio("order");
    let id = s.create_project("順番").expect("作れる");
    s.open_project(id).expect("開ける");

    // 被覆効率の先頭。 正準順の先頭とは一致しないはず。
    s.set_recording_order(Mode::CoverageEfficiency)
        .expect("切り替えられる");
    let (_, order) = s.recording_order().expect("引ける");
    let head = order.first().cloned().expect("提示順がある");
    assert_eq!(
        s.progress().expect("引ける").next_row.map(|(id, _)| id),
        Some(head.clone()),
        "次のフレーズは提示順の先頭"
    );

    // 曲バンク優先へ戻すと、先頭も変わる。 切り替えは可逆（`TR-SYN-19`）。
    s.set_recording_order(Mode::SongBankFirst).expect("戻せる");
    let (mode, bank_order) = s.recording_order().expect("引ける");
    assert_eq!(mode, Mode::SongBankFirst);
    assert_ne!(
        bank_order.first(),
        Some(&head),
        "同梱曲を優先すると先頭が変わる"
    );
    assert_eq!(
        s.progress().expect("引ける").next_row.map(|(id, _)| id),
        bank_order.first().cloned(),
        "戻したモードの先頭になる"
    );
}
