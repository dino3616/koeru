//! UST / USTX を取り込み、そこから録音リストを組む（`TR-RCL-12`, `TR-RCL-16`）。
//!
//! 音を鳴らさないので、どの OS でも通る。

use koeru_app_lib::Studio;

/// OpenUtau が書く USTX。2トラック。
const USTX: &str = r#"name: New Project
resolution: 480
tempos:
- position: 0
  bpm: 96
tracks:
- track_name: 主旋律
- track_name: ハモリ
voice_parts:
- name: Part1
  track_no: 0
  position: 0
  notes:
  - position: 0
    duration: 480
    tone: 62
    lyric: さ
    vibrato: {length: 0, period: 175, depth: 25, in: 10, out: 10, shift: 0, drift: 0}
  - position: 480
    duration: 480
    tone: 64
    lyric: く
  - position: 960
    duration: 480
    tone: 62
    lyric: ら
- name: Part2
  track_no: 1
  position: 0
  notes:
  - position: 0
    duration: 960
    tone: 57
    lyric: ほ
wave_parts: []
"#;

const UST: &str = "[#VERSION]\nUST Version1.2\n[#SETTING]\nTempo=150.00\n[#0000]\nLength=480\nLyric=な\nNoteNum=62\n[#0001]\nLength=480\nLyric=R\nNoteNum=62\n[#0002]\nLength=480\nLyric=に\nNoteNum=64\n[#TRACKEND]\n";

fn opened(tag: &str) -> (Studio, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!("koeru-import-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut s = Studio::open(root.clone()).expect("開ける");
    let id = s.create_project("取り込み").expect("作れる");
    s.open_project(id).expect("開ける");
    (s, root)
}

/// UST は1ファイル1曲。題はファイル名から採る。
#[test]
fn ust_を取り込める() {
    let (mut s, _root) = opened("ust");
    let got = s
        .import_songs(UST.as_bytes(), "/どこか/ふるさと.ust")
        .expect("取り込める");

    assert_eq!(got.len(), 1);
    assert_eq!(got[0].1.title, "ふるさと");
    assert_eq!(got[0].1.notes.len(), 2, "休符は落とす");
    assert!(
        (got[0].1.tempo_bpm - 150.0).abs() < f64::EPSILON,
        "テンポを落とさない"
    );

    // 曲バンクへ入り、「あと何音で歌えるか」が出る（`TR-RCL-19`）。
    let songs = s.song_status().expect("引ける");
    assert!(songs.iter().any(|x| x.title == "ふるさと"));
}

/// USTX は1トラックが1曲。ハモリを主旋律へ混ぜない。
#[test]
fn ustx_はトラックごとに曲になる() {
    let (mut s, _root) = opened("ustx");
    let got = s
        .import_songs(USTX.as_bytes(), "デモ.ustx")
        .expect("取り込める");

    let titles: Vec<&str> = got.iter().map(|(_, song)| song.title.as_str()).collect();
    assert_eq!(titles, ["デモ — 主旋律", "デモ — ハモリ"]);
    assert_eq!(got[0].1.notes.len(), 3);
    assert_eq!(got[1].1.notes.len(), 1);
    assert_eq!(
        s.song_status().expect("引ける").len(),
        3,
        "同梱の1曲と合わせて3"
    );
}

/// 歌詞を読めない曲を台帳へ入れない（`TR-RCL-12` の「必要単位集合は読み込み時に算出する」）。
///
/// 入れると要求が空の曲になり、「いま歌えます」と並ぶ。押すまで嘘だと分からない。
#[test]
fn 歌詞を読めない曲は取り込まない() {
    let (mut s, _root) = opened("unreadable");
    let romaji = USTX.replace("lyric: さ", "lyric: sa");
    let e = s
        .import_songs(romaji.as_bytes(), "ローマ字.ustx")
        .expect_err("拒むこと");
    assert_eq!(e.kind, "app.unreadable_lyrics");

    // 途中まで入れて止めない。 1曲目で拒むので、2曲目も台帳に無い。
    let titles: Vec<String> = s
        .all_songs()
        .expect("引ける")
        .into_iter()
        .map(|(_, song, _)| song.title)
        .collect();
    assert!(
        !titles.iter().any(|t| t.starts_with("ローマ字")),
        "{titles:?}"
    );
}

/// 題はファイル名から採るので、そのままでは並べられないことがある。
#[test]
fn 題を後から変えられる() {
    let (mut s, _root) = opened("rename");
    let got = s
        .import_songs(USTX.as_bytes(), "New Project.ustx")
        .expect("取り込める");
    let id = got[0].0.clone();

    s.rename_song(&id, "  はじまりの歌  ").expect("変えられる");
    let titles: Vec<String> = s
        .all_songs()
        .expect("引ける")
        .into_iter()
        .map(|(_, song, _)| song.title)
        .collect();
    assert!(
        titles.contains(&"はじまりの歌".to_owned()),
        "前後の空白を落とす"
    );

    // 空の題にはしない。 一覧に押す的が見えない行ができる。
    assert_eq!(
        s.rename_song(&id, "   ").expect_err("拒むこと").kind,
        "app.empty_title"
    );
}

/// 外した曲も一覧には残る。残さないと、戻す道が無くなる。
#[test]
fn バンクから外して戻せる() {
    let (mut s, _root) = opened("bank");
    let got = s
        .import_songs(UST.as_bytes(), "ふるさと.ust")
        .expect("取り込める");
    let id = got[0].0.clone();

    s.set_song_in_bank(&id, false).expect("外せる");
    assert!(
        !s.song_status()
            .expect("引ける")
            .iter()
            .any(|x| x.title == "ふるさと"),
        "歌える曲の一覧からは消える"
    );
    let all = s.all_songs().expect("引ける");
    assert!(
        all.iter().any(|(got, _, in_bank)| *got == id && !*in_bank),
        "組み替えの一覧には残る"
    );

    s.set_song_in_bank(&id, true).expect("戻せる");
    assert!(
        s.song_status()
            .expect("引ける")
            .iter()
            .any(|x| x.title == "ふるさと")
    );
}

/// 取り込んだ曲の一部を選んで、そこを歌えるようにする（`TR-RCL-16`, `DEC-RCL-011`）。
#[test]
fn 取り込んだ曲の一部から録音リストを詰め直せる() {
    let (mut s, _root) = opened("repack");
    let got = s
        .import_songs(USTX.as_bytes(), "デモ.ustx")
        .expect("取り込める");
    let id = got[0].0.clone();

    // 範囲を選ぶ画面が読むノート列。
    let notes = s.song_notes(&id).expect("引ける");
    assert_eq!(notes.len(), 3);
    assert_eq!(notes[0].lyric, "さ");

    // 「く ら」だけを歌えるようにする。
    let added = s
        .repack_for_selection(&[(id, vec![(1, 3)])])
        .expect("詰め直せる");
    assert!(added > 0, "行が足りていない");
}
