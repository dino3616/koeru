//! **ゲイン校正の実機ハーネス**（`TR-REC-14`, `TR-REC-15`）。
//!
//! CoreAudio の入力ボリュームを実際に読み書きできるかを確かめる。
//! **触ったら必ず元へ戻す。** 利用者のマイク設定を変えたままにしない。

#![allow(clippy::print_stdout)]
#![cfg(target_os = "macos")]

use koeru_audio::backend::macos as mac;

#[test]
fn 入力ゲインを読み書きして元へ戻す() {
    let devices = mac::enumerate_input_devices().expect("列挙できる");
    if devices.is_empty() {
        println!("入力デバイスが無い。ここで戻る");
        return;
    }

    let mut touched = 0;
    for d in &devices {
        let control = mac::gain_control(&d.id);
        let now = mac::read_gain(&d.id);
        println!("{:?}: {:?} / ゲイン {now:?}", d.id, control);

        if !control.is_usable() {
            // **ソフトウェアのボリュームは校正に使えない**（TR-REC-14）。
            // 読めても書かない。
            assert!(
                mac::write_gain(&d.id, 0.5).is_err(),
                "校正に使えないデバイスへは書けないこと"
            );
            continue;
        }

        let Some(before) = now else {
            continue;
        };

        // **必ず戻せる値で試す。**
        let probe = if before > 0.5 {
            before - 0.1
        } else {
            before + 0.1
        };
        mac::write_gain(&d.id, probe).expect("書けること");
        let after = mac::read_gain(&d.id).expect("読めること");
        println!("  {before:.3} → 書き込み {probe:.3} → 読み戻し {after:.3}");
        assert!(
            (after - probe).abs() < 0.05,
            "書いた値の近くが読めること（OS 側の丸めは許す）"
        );

        // 元へ戻す。**ここが本体。**
        mac::write_gain(&d.id, before).expect("戻せること");
        let restored = mac::read_gain(&d.id).expect("読めること");
        assert!((restored - before).abs() < 0.05, "元の値へ戻ること");
        touched += 1;
    }

    println!("ハードウェアゲインを触れたデバイス: {touched} 台");
}

#[test]
fn 校正の判定は要件どおりの範囲() {
    use koeru_core::calibration::{Outcome, TARGET_MAX_DBFS, TARGET_MIN_DBFS, step};

    // 範囲の内と外。**-12 〜 -6 dBFS**（TR-REC-14）。
    assert_eq!(step(-9.0, Some(0.5), 1), Outcome::Settled);
    assert!(matches!(
        step(TARGET_MIN_DBFS - 0.1, Some(0.5), 1),
        Outcome::Adjust { .. }
    ));
    assert!(matches!(
        step(TARGET_MAX_DBFS + 0.1, Some(0.5), 1),
        Outcome::Adjust { .. }
    ));
}
