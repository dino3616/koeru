//! プロジェクトの開閉と貸与（`DEC-PLT-034`、`DEC-PLT-044`、T05b-2）。
//!
//! `Studio` の台帳の書き手は `ProjectRuntime` が持つ。 開く・移る・閉じるたびに
//! 貸与（世代）が切り替わり、プロジェクトの寿命を持つ写しが持ち越されないことを、
//! 実音声なしで確かめる。 録音の途中は `test-hooks` の入口で作る
//! （`tests/device_loss.rs` と同じ手順）。
//!
//! `Studio::open` は MFA のモデルを読むので、書いていない OS 向けの組み立て
//! （`koeru_force_unsupported_backend`）では設計どおり失敗する（`DEC-ALN-016`）。
//! `tests/methods.rs` と同じくモジュールごと外す。

#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
#![cfg(feature = "test-hooks")]

use std::path::PathBuf;
use std::sync::Arc;

use koeru_app_lib::Studio;
use koeru_audio::DeviceId;
use koeru_audio::wav::{MASTER_RATE_HZ, PartialTake};
use koeru_failure::Failure as _;
use koeru_runtime::RuntimeError;
use uuid::Uuid;

fn studio(tag: &str) -> (Studio, PathBuf) {
    let root = std::env::temp_dir().join(format!("koeru-lifecycle-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let s = Studio::open(root.clone()).expect("ライブラリを開ける");
    (s, root)
}

fn next_row(studio: &mut Studio) -> String {
    studio
        .progress()
        .expect("進み具合を引ける")
        .next_row
        .expect("次に録る行がある")
        .0
}

/// 一定の振幅の WAV を、すでにあるファイルの上へ置き直す。
fn overwrite_wav(path: &std::path::Path, amplitude: f32) {
    std::fs::remove_file(path).expect("置いてあった WAV を消せる");
    let mut part = PartialTake::create(path, MASTER_RATE_HZ).expect("作れる");
    part.write(&vec![amplitude; MASTER_RATE_HZ as usize])
        .expect("書ける");
    part.finalize().expect("確定できる");
}

/// 収録中に閉じられない。 止まっていれば閉じられ、閉じたあとは開き直せる。
#[test]
fn 収録中は閉じられず_止めれば閉じて開き直せる() {
    let (mut s, root) = studio("close");
    let a = s.create_project("A").expect("作れる");
    s.open_project(a).expect("開ける");
    let first = *s.runtime().expect("開いている").lease();

    // 収録中の予定を作る（実機のマイクは使わない）。
    let missing = DeviceId::new("koeru-test-project-lifecycle-存在しない識別子");
    s.test_select_device(&missing).expect("選べる");
    let row = next_row(&mut s);
    s.test_begin_capture(&row).expect("収録を始められる");

    let err = s.close_project().expect_err("収録中は閉じられない");
    assert_eq!(err.code, "app.already_recording");
    assert_eq!(
        *s.runtime().expect("まだ開いている").lease(),
        first,
        "断ったなら貸与は動かない"
    );
    // 別の音源へ移るのも同じ理由で断る（既存の規則）。
    let b = s.create_project("B").expect("作れる");
    let err = s.open_project(b).expect_err("収録中は移れない");
    assert_eq!(err.code, "app.already_recording");

    // デバイスを見失うと、進行中の予定が破棄されて収録中でなくなる。
    s.test_set_device_alive(false);
    s.check_device().expect("見失いを扱える");

    s.close_project().expect("止まっていれば閉じられる");
    assert!(s.runtime().is_none(), "閉じたら実行時は無い");
    for err in [
        s.progress().expect_err("開いていない"),
        s.close_project().expect_err("もう閉じている"),
    ] {
        assert_eq!(err.code, "app.no_project");
    }

    s.open_project(a).expect("開き直せる");
    assert_ne!(
        *s.runtime().expect("開いている").lease(),
        first,
        "開き直した貸与は、前の貸与と違う値になる"
    );
    s.progress().expect("開き直せば操作できる");

    let _ = std::fs::remove_dir_all(&root);
}

/// A → B → A。 貸与は開くたびに新しくなり、前の貸与を握った持ち手は断られる。
#[test]
fn 移るたびに貸与が変わり_前の持ち手は失効する() {
    let (mut s, root) = studio("switch");
    let a = s.create_project("A").expect("作れる");
    let b = s.create_project("B").expect("作れる");

    s.open_project(a).expect("開ける");
    let held = s.runtime().expect("開いている");
    let lease_a1 = *held.lease();
    // いま握っている貸与は使える。
    drop(held.read(&lease_a1).expect("読める"));
    drop(held.writer(&lease_a1).expect("書き手を取れる"));

    s.open_project(b).expect("移れる");
    let lease_b = *s.runtime().expect("開いている").lease();
    assert_ne!(lease_b, lease_a1);
    assert_eq!(
        s.runtime().expect("開いている").project_id(),
        b,
        "いま開いているのは B"
    );

    // A の持ち手は、A の貸与でも断られる。 閉じたので。
    let read = held.read(&lease_a1).expect_err("閉じた貸与では読めない");
    assert!(matches!(read, RuntimeError::LeaseExpired));
    assert_eq!(read.code(), "runtime.lease_expired");
    let write = held.writer(&lease_a1).expect_err("閉じた貸与では書けない");
    assert_eq!(write.code(), "runtime.lease_expired");

    s.open_project(a).expect("A へ戻れる");
    let held_again = s.runtime().expect("開いている");
    let lease_a2 = *held_again.lease();
    assert_ne!(lease_a2, lease_a1, "同じ音源でも、開き直せば別の貸与");
    assert_ne!(lease_a2, lease_b);
    assert!(!Arc::ptr_eq(&held, &held_again));
    // 前の A の貸与は、新しい A の実行時にも通らない。
    assert!(matches!(
        held_again.read(&lease_a1),
        Err(RuntimeError::LeaseExpired)
    ));
    drop(held_again.read(&lease_a2).expect("新しい貸与は読める"));

    let _ = std::fs::remove_dir_all(&root);
}

/// 同じ音源を開き直しても、実行時も収録セッションも作り直さない。
#[test]
fn 同じ音源を開き直しても貸与とセッションは変わらない() {
    let (mut s, root) = studio("reopen");
    let a = s.create_project("A").expect("作れる");
    s.open_project(a).expect("開ける");
    let row = next_row(&mut s);
    s.mark_recorded_for_test(&row).expect("印を付けられる");

    let runtime = s.runtime().expect("開いている");
    let session = s.test_session_id().expect("開いている");
    assert_ne!(session, 0, "セッションが始まっていること");

    s.open_project(a).expect("開き直せる");
    assert!(
        Arc::ptr_eq(&runtime, &s.runtime().expect("開いている")),
        "同じ音源なら同じ実行時"
    );
    assert_eq!(s.test_session_id().expect("開いている"), session);
    drop(runtime.read(runtime.lease()).expect("貸与はまだ生きている"));

    let _ = std::fs::remove_dir_all(&root);
}

/// プロジェクトの写しは、移ると捨てられる。
///
/// テイクの ID は台帳ごとに 1 から振るので、A のテイク 1 の波形の段が、B の
/// テイク 1 の波形として返っていた（`Studio` が段を持ち越していた）。
#[test]
fn プロジェクトの写しは移ると持ち越されない() {
    let (mut s, root) = studio("caches");
    let a = s.create_project("A").expect("作れる");
    let b = s.create_project("B").expect("作れる");

    s.open_project(a).expect("開ける");
    let row = next_row(&mut s);
    let take_a = s.seed_material_for_test(&row).expect("素材を置ける");
    let peak = |s: &mut Studio, take| -> f32 {
        s.waveform_window(take, 0.0, 1000.0, 8)
            .expect("波形を引ける")
            .iter()
            .map(|(lo, hi)| lo.abs().max(hi.abs()))
            .fold(0.0, f32::max)
    };
    let in_a = peak(&mut s, take_a);
    assert!((in_a - 0.2).abs() < 0.01, "A の素材は 0.2 のはず: {in_a}");
    s.test_observe_f0(vec![220.0; 4], 80.0).expect("埋められる");
    let (mipmaps, observed, floor) = s.test_project_caches().expect("開いている");
    assert_eq!((mipmaps, observed, floor), (1, 1, true), "A で溜まった");

    s.open_project(b).expect("移れる");
    assert_eq!(
        s.test_project_caches().expect("開いている"),
        (0, 0, false),
        "移ったら、前の音源の写しは空"
    );
    // 同じ行 ID・同じテイク ID（台帳ごとに 1 から振る）で、中身だけ違う素材を置く。
    let row_b = next_row(&mut s);
    assert_eq!(row_b, row, "どの音源も同じ録音リストから始まる");
    let take_b = s.seed_material_for_test(&row_b).expect("素材を置ける");
    assert_eq!(take_b, take_a, "テイク ID は音源をまたいで衝突する");
    let path = s
        .project_dir()
        .expect("開いている")
        .audio_dir()
        .join(format!("{row_b}_1.wav"));
    overwrite_wav(&path, 0.9);

    let in_b = peak(&mut s, take_b);
    assert!(
        (in_b - 0.9).abs() < 0.01,
        "B のテイクの波形は B の素材から組む（A の段が残っていると 0.2 になる）: {in_b}"
    );

    // 閉じても空になる。
    s.close_project().expect("閉じられる");
    s.open_project(a).expect("開き直せる");
    assert_eq!(s.test_project_caches().expect("開いている"), (0, 0, false));

    let _ = std::fs::remove_dir_all(&root);
}

/// 読みのセッションは、開いた時点の版に固定される。 `Studio` を通した書き込みは、
/// 新しく開いたセッションにだけ見える。
#[test]
fn 読みは新しいセッションを開いて初めて書き込みを見る() {
    let (mut s, root) = studio("read");
    let a = s.create_project("A").expect("作れる");
    s.open_project(a).expect("開ける");
    let runtime = s.runtime().expect("開いている");
    let lease = *runtime.lease();
    let row = next_row(&mut s);

    let before = runtime.read(&lease).expect("読める");
    let adopted = |session: &koeru_runtime::ProjectReadSession| {
        session
            .recording()
            .expect("収録 facet を組める")
            .row(&row)
            .expect("その行がある")
            .adopted
            .is_some()
    };

    s.mark_recorded_for_test(&row).expect("印を付けられる");

    assert!(
        !adopted(&before),
        "前に開いたセッションは、書き込み前の版のまま"
    );
    let after = runtime.read(&lease).expect("読める");
    assert!(adopted(&after), "新しく開けば書き込みが見える");
    assert_ne!(
        before.revision(),
        after.revision(),
        "書き込みを挟んだので版が違う"
    );

    let _ = std::fs::remove_dir_all(&root);
}

/// 移り先が開けなかったら、前の音源は何も変わらない（貸与も、開いたストリームも）。
#[test]
fn 移り先が開けなければ前の音源はそのまま() {
    let (mut s, root) = studio("failed-switch");
    let a = s.create_project("A").expect("作れる");
    let b = s.create_project("B").expect("作れる");
    s.open_project(a).expect("開ける");
    let held = s.runtime().expect("開いている");
    let lease = *held.lease();
    // ストリームを開いた状態（選択済み）にして、落とされないことを見る。
    let missing = DeviceId::new("koeru-test-project-lifecycle-存在しない識別子");
    s.test_select_device(&missing).expect("選べる");
    assert_eq!(s.test_device_state(), "selected");

    // B の manifest を壊す。 台帳は開けるが、確認キューの組み直しの前に読めず落ちる。
    let manifest = root.join(b.to_string()).join("manifest.toml");
    std::fs::write(&manifest, "これは manifest ではない = = =").expect("壊せる");

    s.open_project(b).expect_err("壊れた音源は開けない");

    let now = s.runtime().expect("A が開いたまま");
    assert!(Arc::ptr_eq(&held, &now), "同じ実行時のまま");
    assert_eq!(*now.lease(), lease, "貸与は動かない");
    assert_eq!(s.test_device_state(), "selected", "A の選択は残る");
    drop(held.read(&lease).expect("A の貸与はまだ読める"));
    drop(held.writer(&lease).expect("A の貸与はまだ書ける"));
    s.progress().expect("A はまだ操作できる");

    let _ = std::fs::remove_dir_all(&root);
}

/// 一覧に無い識別子を断るのは、前の音源を閉じる前（移る途中で何も開いていない
/// 状態を作らない）。
#[test]
fn 一覧に無い音源は開けず_開いている音源も閉じない() {
    let (mut s, root) = studio("unknown");
    let a = s.create_project("A").expect("作れる");
    s.open_project(a).expect("開ける");
    let lease = *s.runtime().expect("開いている").lease();

    s.open_project(Uuid::new_v4()).expect_err("一覧に無い");
    assert_eq!(
        *s.runtime().expect("開いたまま").lease(),
        lease,
        "断った場合は、いま開いているものを閉じない"
    );
    s.progress().expect("まだ操作できる");

    let _ = std::fs::remove_dir_all(&root);
}
