//! デバイス消失時の破棄と、同一デバイス復帰の検知（`TR-REC-04`, `REQ-REC-109`, `DEC-REC-011`）。
//!
//! 実機のマイクは使わない。 `test-hooks` の入口で選択済みデバイスと収録中の
//! 予定を直接作り、`Studio::check_device` を呼ぶ——生死の答えだけを差し込み、
//! そこから先の判断（`device_transition` 以降）は本番と共通の経路を通す。
//!
//! `Studio::open` は MFA のモデルを読むので、書いていない OS 向けの組み立て
//! （`koeru_force_unsupported_backend`）では設計どおり失敗する（`DEC-ALN-016`）。
//! ほかの実機ハーネス（`tests/vertical_slice.rs`）と同じくモジュールごと外す。

// 実機ハーネス（抜き差しの1本）が人向けの出力を使う。 手順を画面に出す必要が
// あり、`tracing` の既定フィルタでは見えない（`tests/vertical_slice.rs` と同じ理由）。
#![allow(clippy::print_stdout)]
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
#![cfg(feature = "test-hooks")]

use koeru_app_lib::Studio;
use koeru_audio::DeviceId;

/// 実在しない識別子。 「同じ識別子で再び開く」ときに `mac::open` が
/// `DeviceNotFound` で断ることを、実機無しでそのまま再現できる
/// ——`on_device_returned` が呼ぶ `open_armed_stream` の失敗をここで観測する。
/// USB の抜き差しで、一覧に出てから実際に開けるようになるまで間がある
/// ケースの写し（1回目の再オープンが失敗する）。
fn missing_device() -> DeviceId {
    DeviceId::new("koeru-test-device-loss-存在しない識別子")
}

fn project(name: &str) -> Studio {
    let root =
        std::env::temp_dir().join(format!("koeru-device-loss-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut studio = Studio::open(root).expect("ライブラリを開ける");
    let id = studio.create_project("デバイス消失の試験").expect("作れる");
    studio.open_project(id).expect("開ける");
    studio
}

/// 次に録る行の ID。 台帳のカバレッジが動いていないことも、この値の
/// 変化の有無で確かめられる。
fn next_row(studio: &mut Studio) -> String {
    studio
        .progress()
        .expect("進み具合を引ける")
        .next_row
        .expect("次に録る行がある")
        .0
}

#[test]
fn 収録中に見失うと予定を破棄しファイルも消える() {
    let mut studio = project("recording");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");
    assert_eq!(studio.test_device_state(), "selected");

    let row = next_row(&mut studio);
    let (capture, wav_path) = studio.test_begin_capture(&row).expect("収録を始められる");
    assert!(wav_path.exists(), "置いた WAV がまだあること");

    studio.test_set_device_alive(false);
    studio.check_device().expect("見張りは失敗しない");

    assert_eq!(studio.test_device_state(), "lost", "デバイスを見失った状態");
    assert_eq!(
        studio.test_intent_state(&capture).expect("予定を読める"),
        Some("discarded"),
        "予定は Discarded で閉じる（孤児の自動削除とは別）"
    );
    assert!(!wav_path.exists(), "破棄した予定の WAV は消える");
    assert_eq!(
        next_row(&mut studio),
        row,
        "テイクは1件も増えていない（次に録る行が変わらない）"
    );
}

#[test]
fn 収録していない間に見失っても構わない() {
    let mut studio = project("idle");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");

    studio.test_set_device_alive(false);
    studio.check_device().expect("見張りは失敗しない");

    assert_eq!(studio.test_device_state(), "lost");
}

#[test]
fn 見失っている間はストリームが要る操作を専用の符号で断る() {
    let mut studio = project("rejects");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");
    studio.test_set_device_alive(false);
    studio.check_device().expect("見張りは失敗しない");

    let err = studio.start_take().expect_err("見失っている間は録れない");
    assert_eq!(err.code, "recording.device_lost");

    let err = studio
        .probe_input(10)
        .expect_err("見失っている間は測れない");
    assert_eq!(err.code, "recording.device_lost");
}

#[test]
fn 同じ識別子が戻ると再オープンを試みるが失敗しても選択を保つ() {
    let mut studio = project("returns");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");
    studio.test_set_device_alive(false);
    studio.check_device().expect("見張りは失敗しない");
    assert_eq!(studio.test_device_state(), "lost");

    // 同じ識別子が「生きている」と答えさせる。 実機を積んでいないので、
    // `open_armed_stream` の `mac::open` が `DeviceNotFound` で失敗する
    // ——USB を挿し直した直後、一覧には出たがまだ開けない、という
    // 現実にある窓を実機無しで再現している。
    studio.test_set_device_alive(true);
    let result = studio.check_device();
    assert!(
        result.is_err(),
        "実機が無いので再オープンは失敗する。ここではそれ自体を確かめる"
    );

    // **再オープンに失敗しても選択と `Lost` を手放さない**（`REQ-REC-109`）。
    // `same_device_returned` は `mac::open` が成功するまで呼ばない
    // （`Studio::open_armed_stream` の doc）ので、ここで巻き戻る先が無い。
    assert_eq!(
        studio.test_device_state(),
        "lost",
        "失敗しても Lost のまま。次の一覧の変化でまた試せる"
    );
    let (chosen_id, armed, recording) = studio.chosen_device().expect("読める");
    assert_eq!(
        chosen_id.as_deref(),
        Some(id.as_str()),
        "選択した識別子を覚えたまま"
    );
    assert!(!armed, "実機が無いのでストリームは開いていない");
    assert!(!recording, "自動では録音を始めない");
}

#[test]
fn 再オープンに繰り返し失敗しても選択と_lost_を保ち続ける() {
    // USB が挿さり直った直後、何回か一覧の変化が起きても実際に開けるように
    // なるまで待たされる経路の写し。`check_device` を何度呼んでも、
    // 選択（`self.device`）と `Lost` は崩れない——崩れると、実際に開けるように
    // なったときの `check_device` がもう `Returned` を作れなくなる。
    let mut studio = project("retries");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");
    studio.test_set_device_alive(false);
    studio.check_device().expect("見張りは失敗しない");
    assert_eq!(studio.test_device_state(), "lost");

    studio.test_set_device_alive(true);
    for attempt in 1..=3 {
        assert!(
            studio.check_device().is_err(),
            "{attempt} 回目も実機が無いので失敗する"
        );
        assert_eq!(
            studio.test_device_state(),
            "lost",
            "{attempt} 回目のあとも Lost"
        );
        let (chosen_id, _, _) = studio.chosen_device().expect("読める");
        assert_eq!(
            chosen_id.as_deref(),
            Some(id.as_str()),
            "{attempt} 回目のあとも選択を覚えている"
        );
    }
}

#[test]
fn 別のデバイスの着脱では何もしない() {
    let mut studio = project("other-device");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");

    // 選択中のものはまだ生きている。 一覧の変化が別のデバイスの着脱でも、
    // 見張りは選択を動かさない。
    studio.test_set_device_alive(true);
    studio.check_device().expect("見張りは失敗しない");
    assert_eq!(studio.test_device_state(), "selected");
}

#[test]
fn 見失う前の一覧の変化では何も起こさない() {
    let mut studio = project("never-lost");
    let id = missing_device();
    studio.test_select_device(&id).expect("選べる");

    // 一度も見失っていない状態で `check_device` を繰り返し呼んでも、
    // `arm_device` を試みたりしない（`device_transition` が `Selected, true` を
    // `None` にしか写さない）。
    studio.test_set_device_alive(true);
    studio.check_device().expect("見張りは失敗しない");
    studio.check_device().expect("見張りは失敗しない");
    assert_eq!(studio.test_device_state(), "selected");
}

/// 実機で、同一デバイスを本当に抜き差しして戻ってくることを確かめる。
///
/// マイクを一度取り外し、`check_device` が破棄を検知したあと、同じデバイスを
/// 挿し直して自動で録音を再開できる状態に戻ることを見る。 再オープンが
/// 「失敗してから成功する」経路そのものは
/// `再オープンに繰り返し失敗しても選択と_lost_を保ち続ける` が実機無しで確かめる
/// ——ここで見るのは、実機で最終的に本当に開き直ることだけ。
/// 自動化した手順が無いので、手元でしか通せない（`tests/vertical_slice.rs` と同じ扱い）。
#[test]
#[ignore = "マイクの抜き差しが要る実機ハーネス。--ignored を付けて走らせる"]
fn 実機での抜き差しから復帰できる() {
    let mut studio = project("real-hardware");
    let devices = Studio::devices().expect("デバイスを挙げられる");
    let Some(device) = devices.first() else {
        println!("入力デバイスが無い。抜き差しの実機ハーネスは省略する");
        return;
    };
    studio.arm_device(&device.id).expect("開ける");
    println!("この device を今すぐ取り外し、10 秒待ってから挿し直してください: {device:?}");
    std::thread::sleep(std::time::Duration::from_secs(15));
    studio.check_device().expect("見張りが最後まで通ること");
    let (chosen, armed, _) = studio.chosen_device().expect("読める");
    assert_eq!(chosen.as_deref(), Some(device.id.as_str()));
    assert!(armed, "抜き差し後にストリームが開き直っていること");
}
