//! **回り込み検査の実機ハーネス**（`TR-REC-24`）。
//!
//! 出力の種別を判定し、ガイドを鳴らしながら録って相関を見る。
//! **スピーカで走らせれば漏れると出るのが正しい。** ヘッドホンなら漏れないと出る。

#![allow(clippy::print_stdout)]
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

use koeru_app_lib::Studio;

#[test]
fn 出力の種別を判定できる() {
    let kind = Studio::output_kind();
    println!("既定の出力: {kind:?}");
    // **どれであっても正しい。** 判定できないことも正規の結果（TR-REC-24 の [Fact]）。
    println!("  スピーカと断定できるか: {}", kind.definitely_speakers());
}

#[test]
fn ガイドを鳴らして回り込みを測る() {
    let root = std::env::temp_dir().join(format!("koeru-leak-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut studio = Studio::open(root.clone()).expect("開ける");
    let id = studio.create_project("回り込み検査").expect("作れる");
    studio.open_project(id).expect("開ける");

    // **信号が届いているデバイスを選ぶ。**
    // 届いていないと、状態機械がストリームを閉じてデバイス選択へ戻す
    //（REQ-REC-106）。そこから先の検査はできない。
    let devices = Studio::devices().expect("列挙できる");
    let mut chosen = None;
    let mut best = 1e-6_f32;
    for d in &devices {
        if studio.arm_device(&d.id).is_err() {
            continue;
        }
        let peak = studio.probe_input(250).unwrap_or(0.0);
        if peak > best {
            best = peak;
            chosen = Some(d.id.clone());
        }
    }
    let Some(device) = chosen else {
        println!("入力が届くデバイスが無い。ここで戻る");
        return;
    };
    studio.arm_device(&device).expect("開き直せる");
    studio.probe_input(250).expect("生死を判定できる");

    let got = studio.check_guide_leak(60).expect("測れること");
    println!(
        "回り込み: 相関 {:.3} / 遅れ {:.1}ms / 漏れている={}",
        got.correlation, got.lag_ms, got.leaking
    );

    // **漏れているなら音高提示を鳴らさない**（TR-REC-24）。
    let played = studio.play_pitch(60);
    if got.leaking {
        let e = played.expect_err("鳴らさないこと");
        assert_eq!(e.kind, "recording.guide_leaks");
        println!("  漏れているので音高提示を鳴らさない");
    } else {
        played.expect("鳴らせること");
        std::thread::sleep(std::time::Duration::from_millis(600));
        studio.stop_preview();
        println!("  漏れていないので音高提示を鳴らせる");
    }

    drop(studio);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn 確かめる前は音高提示を鳴らさない() {
    let root = std::env::temp_dir().join(format!("koeru-leak-guard-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let mut studio = Studio::open(root.clone()).expect("開ける");
    let id = studio.create_project("未検査").expect("作れる");
    studio.open_project(id).expect("開ける");

    let devices = Studio::devices().expect("列挙できる");
    let Some(d) = devices.first() else {
        return;
    };
    if studio.arm_device(&d.id).is_err() {
        return;
    }

    // **確かめる前に鳴らさない。** 鳴らしたものが全テイクに混じる。
    let e = studio.play_pitch(60).expect_err("鳴らさないこと");
    assert_eq!(e.kind, "recording.leak_unchecked");

    drop(studio);
    let _ = std::fs::remove_dir_all(&root);
}
