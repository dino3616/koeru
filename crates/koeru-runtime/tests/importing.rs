//! 曲の取り込み（`importing`）を、本物のプロジェクトと SQLite（WAL）で確かめる
//! （T08d、`DEC-PLT-054`、`TR-RCL-12`）。
//!
//! 音を鳴らさず、ウィンドウも要らないので、どの OS でも走る。 画面側の経路
//! （`Studio::import_songs`）は `koeru-app` の `tests/import_song.rs` が見る。

use std::path::PathBuf;
use std::sync::Arc;

use koeru_core::db::OperationId;
use koeru_core::project::{Library, Manifest, Method, ProjectDir};
use koeru_failure::{Class, Failure};
use koeru_formats::text::{self, TextEncoding};
use koeru_formats::ust;
use koeru_runtime::importing::{
    self, Diagnostic, ImportError, Outcome, Problem, Rejection, prepare,
};
use koeru_runtime::{ProjectRuntime, RuntimeError};

fn tmp(tag: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    let d = std::env::temp_dir().join(format!(
        "koeru-runtime-importing-{}-{tag}-{n}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).expect("一時ディレクトリを作れること");
    d
}

fn project(tag: &str) -> Arc<ProjectRuntime> {
    let lib = Library::open(tmp(tag)).expect("開けること");
    let dir: ProjectDir = lib
        .create(&Manifest {
            display_name: "取り込み".to_owned(),
            method: Method::Single,
            item_count: 5,
            derived_from: None,
            preset_id: None,
            inventory_version: None,
        })
        .expect("作れること");
    // `Library::create` は `project.db` を作らない（`read_session.rs` と同じ手順）。
    koeru_core::db::Ledger::open(dir.db_path()).expect("台帳を作れること");
    ProjectRuntime::open(dir).expect("開けること")
}

/// OpenUtau が書く USTX。 2トラック。
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
  - position: 960
    duration: 480
    tone: 64
    lyric: く
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

fn titles(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| (*s).to_owned()).collect()
}

/// 台帳にある曲の ID を並べ替えて返す。 同梱曲も入る。
fn song_ids(rt: &ProjectRuntime) -> Vec<String> {
    let mut v: Vec<String> = rt
        .writer(rt.lease())
        .expect("書ける")
        .all_songs()
        .expect("引ける")
        .into_iter()
        .map(|(id, ..)| id)
        .collect();
    v.sort();
    v
}

/// 版。 等しいかだけを比べる（`DEC-PLT-043`）。
fn revision(rt: &ProjectRuntime) -> koeru_core::db::Revision {
    rt.writer(rt.lease())
        .expect("書ける")
        .revision()
        .expect("読める")
}

fn imported(o: Outcome) -> Vec<(String, koeru_core::song::Song)> {
    match o {
        Outcome::Imported(s) => s,
        other => panic!("取り込めること: {other:?}"),
    }
}

/// 読むだけでは何も書かない（`prepare` は計画を返すだけ）。
#[test]
fn 下見は何も書かない() {
    let rt = project("prepare");
    let before = revision(&rt);

    let plan = prepare(USTX.as_bytes(), "/どこか/New Project.ustx").expect("読める");
    assert_eq!(plan.songs().len(), 2, "1トラックが1曲");
    assert_eq!(plan.songs()[0].title, "New Project — 主旋律");
    assert_eq!(
        plan.title_hint(),
        "New Project",
        "ディレクトリと拡張子を落とす"
    );
    assert!(plan.is_importable());

    assert_eq!(revision(&rt), before, "下見で版が進まない");
    assert!(song_ids(&rt).is_empty(), "曲が入らない");
}

/// 取り込んだ曲は台帳に入り、原文がそのまま残る。
#[test]
fn 取り込むと曲と原文が入る() {
    let rt = project("commit");
    let plan = prepare(USTX.as_bytes(), "/どこか/デモ.ustx").expect("読める");
    let op = OperationId::generate();
    let songs = imported(
        importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["  主  ", "ハモ"]))
            .expect("取り込める"),
    );

    assert_eq!(songs.len(), 2);
    assert_eq!(songs[0].1.title, "主", "前後の空白を落とす");
    assert_eq!(songs[1].1.title, "ハモ");
    assert_eq!(songs[0].1.notes.len(), 2);
    assert!(
        (songs[0].1.tempo_bpm - 96.0).abs() < f64::EPSILON,
        "テンポを落とさない"
    );
    assert_eq!(
        songs[0].1.notes[1].rest_ticks, 480,
        "ノートのあいだの空きを休みとして持つ"
    );
    let stored: Vec<String> = song_ids(&rt);
    for (id, _) in &songs {
        assert!(stored.contains(id), "台帳にある: {id}");
    }

    // 原文はバイト列のまま。 ファイル名はディレクトリを落とした名前だけ。
    let mut w = rt.writer(rt.lease()).expect("書ける");
    for (i, (id, _)) in songs.iter().enumerate() {
        let source = w.song_source(id).expect("引ける").expect("原文がある");
        assert_eq!(source.bytes, USTX.as_bytes());
        assert_eq!(source.file_name, "デモ.ustx");
        assert_eq!(source.ordinal, i);
        assert_eq!(source.operation, op);
    }
}

/// 同じ OperationId の送り直しは二重に入れず、同じ ID を返す。
#[test]
fn 同じ操作の送り直しは二重に入らず同じ結果を返す() {
    let rt = project("replay");
    let plan = prepare(USTX.as_bytes(), "デモ.ustx").expect("読める");
    let op = OperationId::generate();
    let t = titles(&["主", "ハモ"]);

    let first = imported(importing::commit(&rt, rt.lease(), &op, &plan, &t).expect("取り込める"));
    let before = revision(&rt);
    let count = song_ids(&rt).len();

    // 応答が失われた consumer が、同じ要求をもう一度送る。
    let again = importing::commit(&rt, rt.lease(), &op, &plan, &t).expect("答える");
    let Outcome::Replayed(replayed) = again else {
        panic!("送り直しとして答えること: {again:?}");
    };
    assert_eq!(replayed, first, "同じ ID・同じ曲を返す");
    assert_eq!(song_ids(&rt).len(), count, "二重に入らない");
    assert_eq!(revision(&rt), before, "何も書かない");

    // 取り込み直後に題を変えても、送り直しには要求どおりの結果を返す。
    rt.writer(rt.lease())
        .expect("書ける")
        .rename_song(&first[0].0, "後から変えた題")
        .expect("変えられる");
    let Outcome::Replayed(replayed) =
        importing::commit(&rt, rt.lease(), &op, &plan, &t).expect("答える")
    else {
        panic!("送り直しとして答えること");
    };
    assert_eq!(
        replayed.iter().map(|(id, _)| id).collect::<Vec<_>>(),
        first.iter().map(|(id, _)| id).collect::<Vec<_>>()
    );
}

/// 受領証は台帳にあるので、開き直したあとの送り直しにも答えられる。
#[test]
fn 開き直したあとの送り直しにも答える() {
    let rt = project("reopen");
    let plan = prepare(USTX.as_bytes(), "デモ.ustx").expect("読める");
    let op = OperationId::generate();
    let t = titles(&["主", "ハモ"]);
    let first = imported(importing::commit(&rt, rt.lease(), &op, &plan, &t).expect("取り込める"));

    let dir = rt.dir().clone();
    let lease = *rt.lease();
    rt.close(&lease).expect("閉じられる");
    drop(rt);
    let rt = ProjectRuntime::open(dir).expect("開き直せる");

    let Outcome::Replayed(replayed) =
        importing::commit(&rt, rt.lease(), &op, &plan, &t).expect("答える")
    else {
        panic!("送り直しとして答えること");
    };
    assert_eq!(replayed, first);
    assert_eq!(song_ids(&rt).len(), 2);
}

/// 別の識別子なら、同じバイト列でも別の取り込み。 現行の挙動（毎回新しい ID）のまま。
#[test]
fn 別の識別子なら同じ原文でも新しい取り込み() {
    let rt = project("new-operation");
    let plan = prepare(UST.as_bytes(), "ふるさと.ust").expect("読める");
    let t = titles(&["ふるさと"]);

    let first = imported(
        importing::commit(&rt, rt.lease(), &OperationId::generate(), &plan, &t)
            .expect("取り込める"),
    );
    let second = imported(
        importing::commit(&rt, rt.lease(), &OperationId::generate(), &plan, &t)
            .expect("取り込める"),
    );
    assert_ne!(first[0].0, second[0].0, "別の曲として並ぶ");
    assert_eq!(song_ids(&rt).len(), 2);
}

/// 同じ識別子で別の要求は、取り込まずに先の曲の ID を返す。
#[test]
fn 同じ識別子で別の要求は取り込まない() {
    let rt = project("reused");
    let op = OperationId::generate();
    let plan = prepare(UST.as_bytes(), "ふるさと.ust").expect("読める");
    let first = imported(
        importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["ふるさと"])).expect("取り込める"),
    );

    // 題が違う。
    let o = importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["別の題"])).expect("答える");
    assert_eq!(o, Outcome::OperationIdReused(vec![first[0].0.clone()]));

    // バイト列が違う。
    let other = prepare(USTX.as_bytes(), "別.ustx").expect("読める");
    let o =
        importing::commit(&rt, rt.lease(), &op, &other, &titles(&["主", "ハモ"])).expect("答える");
    assert!(matches!(o, Outcome::OperationIdReused(_)), "{o:?}");
    assert_eq!(song_ids(&rt).len(), 1, "曲は増えない");
}

/// 残した原文から読み直すと、取り込んだ曲と同じ曲になる。
///
/// 符号化は CP932 のまま。 UTF-8 へ直して残していたら、ここで読み直した曲は
/// 原文が宣言どおりに読めるかどうかに依存してしまう。
#[test]
fn 残した原文から同じ曲を作り直せる() {
    let rt = project("reproduce");
    let cp932 = text::encode(UST, TextEncoding::Cp932).expect("書ける");
    assert_ne!(cp932, UST.as_bytes(), "前提: 符号化が違う");

    let plan = prepare(&cp932, "C:\\songs\\ふるさと.ust").expect("読める");
    let songs = imported(
        importing::commit(
            &rt,
            rt.lease(),
            &OperationId::generate(),
            &plan,
            &titles(&["ふるさと"]),
        )
        .expect("取り込める"),
    );

    let source = rt
        .writer(rt.lease())
        .expect("書ける")
        .song_source(&songs[0].0)
        .expect("引ける")
        .expect("原文がある");
    assert_eq!(source.bytes, cp932, "バイト列のまま");
    assert_eq!(source.file_name, "ふるさと.ust", "名前だけ");

    let mut again = ust::parse_file(&source.bytes, &source.file_name).expect("読める");
    assert_eq!(again.len(), 1);
    again[0].title = songs[0].1.title.clone();
    assert_eq!(again[0], songs[0].1, "同じ曲");

    // 台帳から読み戻した曲とも同じ（休符もテンポも落ちていない）。
    let stored = rt
        .writer(rt.lease())
        .expect("書ける")
        .songs_in_bank()
        .expect("引ける")
        .into_iter()
        .find(|(id, _)| *id == songs[0].0)
        .expect("ある")
        .1;
    assert_eq!(stored.notes, again[0].notes);
    assert!((stored.tempo_bpm - again[0].tempo_bpm).abs() < f64::EPSILON);
}

/// 断った取り込みは何も書かない。
#[test]
fn 断った要求は台帳を変えない() {
    let rt = project("rejected");
    let before = revision(&rt);
    let op = OperationId::generate();
    let plan = prepare(USTX.as_bytes(), "x.ustx").expect("読める");

    let one = importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["主"])).expect("答える");
    assert_eq!(one, Outcome::Rejected(Rejection::TitleCount));
    let empty =
        importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["主", "  "])).expect("答える");
    assert_eq!(empty, Outcome::Rejected(Rejection::EmptyTitle));

    // 歌詞を読めない曲。 下見では診断として見え、取り込みでは断られる。
    let romaji = USTX.replace("lyric: さ", "lyric: sa");
    let plan = prepare(romaji.as_bytes(), "x.ustx").expect("構文としては読める");
    assert_eq!(
        plan.diagnostics(),
        [Diagnostic {
            song: 0,
            problem: Problem::UnreadableLyrics
        }]
    );
    assert!(!plan.is_importable());
    let o =
        importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["主", "ハモ"])).expect("答える");
    assert_eq!(
        o,
        Outcome::Rejected(Rejection::Unimportable(plan.diagnostics()[0]))
    );

    assert_eq!(revision(&rt), before, "版が進まない");
    assert!(song_ids(&rt).is_empty());
    // 断られた識別子は使われていない。 直した要求で同じ識別子を使える。
    let plan = prepare(USTX.as_bytes(), "x.ustx").expect("読める");
    imported(
        importing::commit(&rt, rt.lease(), &op, &plan, &titles(&["主", "ハモ"]))
            .expect("取り込める"),
    );
}

/// 取り込めない理由は、何番目の曲の何番目のノートかだけを持つ。
#[test]
fn 取り込めない理由を診断として返す() {
    let two = USTX.replace("lyric: く", "lyric: くら");
    let plan = prepare(two.as_bytes(), "x.ustx").expect("読める");
    assert_eq!(
        plan.diagnostics(),
        [Diagnostic {
            song: 0,
            problem: Problem::NoteNotOneMora { note: 1 }
        }]
    );

    let wild = USTX.replace("tone: 64", "tone: -2147483648");
    let plan = prepare(wild.as_bytes(), "x.ustx").expect("読める");
    assert_eq!(
        plan.diagnostics(),
        [Diagnostic {
            song: 0,
            problem: Problem::NoteOutOfRange { note: 1 }
        }]
    );

    // 拗音は1ノート1音。
    let ok = USTX.replace("lyric: く", "lyric: きゃ");
    assert!(
        prepare(ok.as_bytes(), "x.ustx")
            .expect("読める")
            .is_importable()
    );
}

/// 壊れた USTX は、落ちずに分類された失敗で断る。
#[test]
fn 壊れた_ustx_は分類された失敗で断る() {
    let hostile: Vec<(&str, Vec<u8>)> = vec![
        ("構文が壊れている", b"\tname: x\n  - broken".to_vec()),
        (
            "深い入れ子",
            format!("{}{}", "[".repeat(50_000), "]".repeat(50_000)).into_bytes(),
        ),
        ("空", Vec::new()),
        (
            "位置が桁あふれする",
            USTX.replace(
                "  track_no: 0\n  position: 0\n",
                "  track_no: 0\n  position: 9223372036854775807\n",
            )
            .into_bytes(),
        ),
    ];
    for (what, bytes) in hostile {
        let e = prepare(&bytes, "x.ustx").expect_err(what);
        assert!(matches!(e, ImportError::Parse(_)), "{what}: {e:?}");
        assert_eq!(e.class(), Class::InvalidInput, "{what}");
        assert!(e.code().starts_with("ust."), "{what}: {}", e.code());
    }
}

/// 閉じた貸与では書けない。 原文だけが入る、ということも無い。
#[test]
fn 失効した貸与では取り込めない() {
    let rt = project("lease");
    let plan = prepare(UST.as_bytes(), "ふるさと.ust").expect("読める");
    let lease = *rt.lease();
    rt.close(&lease).expect("閉じられる");

    let e = importing::commit(
        &rt,
        &lease,
        &OperationId::generate(),
        &plan,
        &titles(&["ふるさと"]),
    )
    .expect_err("断る");
    assert!(
        matches!(e, ImportError::Runtime(RuntimeError::LeaseExpired)),
        "{e:?}"
    );
    assert_eq!(e.code(), "runtime.lease_expired");
}
