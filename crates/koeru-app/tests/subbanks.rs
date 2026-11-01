//! サブバンクの枠（`DEC-RCL-018`、`TR-RCL-28`〜`31`）を `Studio` の口から扱う。
//!
//! 枠を足しても消しても収録セッションを始め直さないこと（`TR-REC-30`）を、貸与と
//! セッションの識別子で見る。 録音の途中は `test-hooks` の入口で作る
//! （`tests/project_lifecycle.rs` と同じ手順）。
//!
//! `Studio::open` は MFA のモデルを読むので、書いていない OS 向けの組み立て
//! （`koeru_force_unsupported_backend`）では設計どおり失敗する（`DEC-ALN-016`）。

#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
#![cfg(feature = "test-hooks")]

use std::path::PathBuf;

use koeru_app_lib::Studio;
use koeru_app_lib::studio::{AddSubbank, ReviseSubbank, SubbankDenied, SubbankSummary, Violation};
use koeru_audio::DeviceId;
use koeru_core::alias::Method as AliasMethod;
use koeru_core::db::Ledger;
use koeru_core::db::subbank::SubbankRefusal;
use koeru_model::id::SubbankId;

fn studio(tag: &str) -> (Studio, PathBuf) {
    let root = std::env::temp_dir().join(format!("koeru-subbanks-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let s = Studio::open(root.clone()).expect("ライブラリを開ける");
    (s, root)
}

/// 開いた音源の、貸与と収録セッション。 枠の操作で動いてはいけない（`TR-REC-30`）。
fn session_of(s: &Studio) -> (koeru_runtime::ProjectLease, i32) {
    (
        *s.runtime().expect("開いている").lease(),
        s.test_session_id().expect("開いている"),
    )
}

fn add(s: &mut Studio, color: Option<&str>, preset: &str, tone: i32) -> SubbankSummary {
    s.add_subbank(&AddSubbank {
        voice_color: color,
        preset,
        tone,
        after: None,
    })
    .expect("台帳を書ける")
    .expect("足せる")
}

/// 枠のまだ録っていない行の1つ目。
fn first_row_of(s: &Studio, id: SubbankId) -> String {
    let mut l = Ledger::open(s.project_dir().expect("開いている").db_path()).expect("開ける");
    l.unrecorded_rows_of(id, koeru_core::inventory::UnitSet::Core)
        .expect("引ける")
        .into_iter()
        .next()
        .expect("行がある")
        .id
}

/// 作った音源は収録音階ごとに声色なしの枠を持つ。 担当音域は直すまで floor 割り当て
/// （`TR-RCL-31`）で、低い端と高い端は録れる範囲（C-1..G9）の端まで伸びる。
#[test]
fn 作った音源は収録音階ごとに枠を持ち_担当音域は_floor_で分ける() {
    let (mut s, root) = studio("create");
    let id = s
        .create_project_with("多音階", "sequential", &[69, 55, 62])
        .expect("作れる");
    s.open_project(id).expect("開ける");

    let all = s.subbanks().expect("読める");
    let tones: Vec<i32> = all.iter().map(|b| b.tone).collect();
    assert_eq!(tones, [55, 62, 69], "並びは音階の昇順");
    for b in &all {
        assert_eq!(b.voice_color, None);
        assert_eq!(b.method, AliasMethod::Sequential);
        assert_eq!(b.preset_id.as_deref(), Some("sequential"));
        assert!(!b.range_edited);
        assert!(b.rows > 0);
        assert_eq!(b.recorded_rows, 0);
        assert!(b.remaining_seconds > 0.0);
    }
    let ranges: Vec<(i32, i32)> = all.iter().map(|b| (b.range.low, b.range.high)).collect();
    assert_eq!(ranges, [(0, 61), (62, 68), (69, 127)]);

    let _ = std::fs::remove_dir_all(&root);
}

/// 足す・付け替える・並べ替える・担当音域を直す・消す。 どれも収録セッションを始め直さない
/// （`TR-REC-30`）。 音階が2つになったら、音高ごとのサブディレクトリを揃える（`TR-REC-36`）。
#[test]
fn 枠の操作は収録セッションも貸与も変えない() {
    let (mut s, root) = studio("session");
    let id = s.create_project("単独音").expect("作れる");
    s.open_project(id).expect("開ける");
    let before = session_of(&s);
    let plain = s.subbanks().expect("読める");
    assert_eq!(plain.len(), 1, "単音階の音源は枠を1つ持つ");
    let plain = plain[0].clone();

    let strong = add(&mut s, Some("強"), "single", plain.tone);
    assert_eq!(strong.voice_color.as_ref().map(|c| c.as_str()), Some("強"));
    assert!(strong.rows > 0, "足した枠も録音リストを持つ");
    assert_eq!(session_of(&s), before);

    // 同じ声色・方式・収録音階の枠は2つ持たない（`TR-RCL-28`）。
    let again = s
        .add_subbank(&AddSubbank {
            voice_color: Some("強"),
            preset: "single",
            tone: plain.tone,
            after: None,
        })
        .expect("台帳を書ける");
    assert_eq!(
        again,
        Err(SubbankDenied::Refused(SubbankRefusal::Conflict {
            existing: strong.id
        }))
    );

    let high = add(&mut s, None, "single", 64);
    let audio = s.project_dir().expect("開いている").audio_dir();
    for t in [plain.tone, 64] {
        assert!(
            audio.join(koeru_core::tone::name(t)).is_dir(),
            "音階が2つになったら音高ごとのディレクトリがある"
        );
    }

    let revised = s
        .revise_subbank(
            strong.id,
            &ReviseSubbank {
                voice_color: Some("優"),
                ..ReviseSubbank::default()
            },
        )
        .expect("台帳を書ける")
        .expect("付け替えられる");
    assert_eq!(revised.voice_color.as_ref().map(|c| c.as_str()), Some("優"));
    assert_eq!(revised.id, strong.id, "付け替えても識別子は変わらない");

    let order = [high.id, strong.id, plain.id];
    let reordered = s
        .reorder_subbanks(&order)
        .expect("台帳を書ける")
        .expect("並べ替えられる");
    assert_eq!(reordered.iter().map(|b| b.id).collect::<Vec<_>>(), order);

    let changed = s
        .set_subbank_tone_range(plain.id, Some((40, 50)))
        .expect("台帳を書ける")
        .expect("直せる");
    assert_eq!(changed.len(), 1);
    assert_eq!((changed[0].range.low, changed[0].range.high), (40, 50));
    assert!(changed[0].range_edited);
    let reset = s
        .set_subbank_tone_range(plain.id, None)
        .expect("台帳を書ける")
        .expect("戻せる");
    assert!(!reset[0].range_edited, "省くと既定へ戻る");

    let removed = s
        .remove_subbank(strong.id, 0)
        .expect("台帳を書ける")
        .expect("消せる");
    assert_eq!(removed.removed_takes, 0);
    let left: Vec<SubbankId> = s.subbanks().expect("読める").iter().map(|b| b.id).collect();
    assert_eq!(left, [high.id, plain.id]);

    assert_eq!(
        session_of(&s),
        before,
        "枠の操作で収録セッションを始め直さない"
    );
    let _ = std::fs::remove_dir_all(&root);
}

/// 録ってある枠は方式を変えられない（`TR-RCL-30`）。 消すと、その枠のテイクと音声も消え、
/// ほかの枠は変わらない（`TR-RCL-29`、`INV-RCL-102`）。 消えるテイクの数が本人に示した数と
/// 違えば断る。
#[test]
fn 録ってある枠は方式を変えられず_消すとテイクと音声も消える() {
    let (mut s, root) = studio("remove");
    let id = s.create_project("単独音").expect("作れる");
    s.open_project(id).expect("開ける");
    let plain = s.subbanks().expect("読める")[0].clone();
    let other = add(&mut s, None, "single", 64);

    let row = first_row_of(&s, other.id);
    s.seed_material_for_test(&row).expect("素材を置ける");
    let wav = s
        .project_dir()
        .expect("開いている")
        .root()
        .join(format!("audio/{row}_1.wav"));
    assert!(wav.exists(), "前提: 音声がある");
    let recorded = s
        .subbanks()
        .expect("読める")
        .into_iter()
        .find(|b| b.id == other.id)
        .expect("ある");
    assert_eq!((recorded.takes, recorded.recorded_rows), (1, 1));

    let locked = s
        .revise_subbank(
            other.id,
            &ReviseSubbank {
                preset: Some("sequential"),
                ..ReviseSubbank::default()
            },
        )
        .expect("台帳を書ける");
    assert_eq!(
        locked,
        Err(SubbankDenied::Refused(SubbankRefusal::MethodLocked))
    );

    let stale = s.remove_subbank(other.id, 0).expect("台帳を書ける");
    assert_eq!(
        stale,
        Err(SubbankDenied::Refused(SubbankRefusal::TakesChanged {
            actual: 1
        }))
    );
    assert!(wav.exists(), "断ったなら音声は残る");

    let removed = s
        .remove_subbank(other.id, 1)
        .expect("台帳を書ける")
        .expect("消せる");
    assert_eq!(removed.removed_takes, 1);
    assert!(!wav.exists(), "消した枠の音声は残らない");

    let left = s.subbanks().expect("読める");
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, plain.id);
    assert_eq!(left[0].rows, plain.rows, "ほかの枠の行は変わらない");

    let _ = std::fs::remove_dir_all(&root);
}

/// 録っている途中は枠を消さず、収録音階と方式も付け替えない。 声色は付け替えられる。
#[test]
fn 録っている途中は枠を消せず_音階も変えられない() {
    let (mut s, root) = studio("recording");
    let id = s.create_project("単独音").expect("作れる");
    s.open_project(id).expect("開ける");
    let plain = s.subbanks().expect("読める")[0].clone();

    let missing = DeviceId::new("koeru-test-subbanks-存在しない識別子");
    s.test_select_device(&missing).expect("選べる");
    let row = first_row_of(&s, plain.id);
    s.test_begin_capture(&row).expect("収録を始められる");
    let before = session_of(&s);

    assert_eq!(
        s.remove_subbank(plain.id, 0).expect("台帳を書ける"),
        Err(SubbankDenied::Refused(SubbankRefusal::Recording))
    );
    assert_eq!(
        s.revise_subbank(
            plain.id,
            &ReviseSubbank {
                tone: Some(60),
                ..ReviseSubbank::default()
            },
        )
        .expect("台帳を書ける"),
        Err(SubbankDenied::Refused(SubbankRefusal::Recording))
    );
    s.revise_subbank(
        plain.id,
        &ReviseSubbank {
            voice_color: Some("強"),
            ..ReviseSubbank::default()
        },
    )
    .expect("台帳を書ける")
    .expect("声色は付け替えられる");
    // 足すのは録っている行に触らない。
    add(&mut s, None, "single", 64);
    assert_eq!(session_of(&s), before, "録っている途中のセッションは続く");

    s.test_set_device_alive(false);
    s.check_device().expect("見失いを扱える");
    let _ = std::fs::remove_dir_all(&root);
}

/// 入力の誤りは値で返す（`DEC-PLT-038`）。 音階は C-1..G9、範囲は low ≤ high だけを見る。
#[test]
fn 入力の誤りはどの欄かを添えて断る() {
    let (mut s, root) = studio("invalid");
    let id = s.create_project("単独音").expect("作れる");
    s.open_project(id).expect("開ける");
    let plain = s.subbanks().expect("読める")[0].clone();

    let invalid = |field, code| Err::<(), _>(SubbankDenied::Invalid(Violation { field, code }));
    for (input, field, code) in [
        (
            AddSubbank {
                voice_color: None,
                preset: "single",
                tone: 128,
                after: None,
            },
            "tone",
            "subbank.tone_out_of_range",
        ),
        (
            AddSubbank {
                voice_color: None,
                preset: "no-such-preset",
                tone: 60,
                after: None,
            },
            "preset",
            "preset.unknown",
        ),
        (
            AddSubbank {
                voice_color: Some(""),
                preset: "single",
                tone: 60,
                after: None,
            },
            "voiceColor",
            "subbank.empty_voice_color",
        ),
    ] {
        assert_eq!(
            s.add_subbank(&input).expect("台帳を書ける").map(|_| ()),
            invalid(field, code)
        );
    }
    assert_eq!(
        s.revise_subbank(
            plain.id,
            &ReviseSubbank {
                voice_color: Some("強"),
                clear_voice_color: true,
                ..ReviseSubbank::default()
            },
        )
        .expect("台帳を書ける")
        .map(|_| ()),
        invalid("clearVoiceColor", "subbank.voice_color_and_clear")
    );
    assert_eq!(
        s.set_subbank_tone_range(plain.id, Some((60, 50)))
            .expect("台帳を書ける")
            .map(|_| ()),
        invalid("range", "subbank.tone_range_inverted")
    );
    assert_eq!(
        s.set_subbank_tone_range(plain.id, Some((-1, 50)))
            .expect("台帳を書ける")
            .map(|_| ()),
        invalid("range", "subbank.tone_range_out_of_range")
    );
    // 欠けも重複も同じ断り。
    let other = add(&mut s, None, "single", 64);
    for order in [vec![plain.id], vec![plain.id, plain.id, other.id]] {
        assert_eq!(
            s.reorder_subbanks(&order)
                .expect("台帳を書ける")
                .map(|_| ()),
            Err(SubbankDenied::Refused(SubbankRefusal::BadOrder))
        );
    }
    assert_eq!(
        s.subbanks().expect("読める").len(),
        2,
        "断ったなら枠は変わらない"
    );

    let _ = std::fs::remove_dir_all(&root);
}
