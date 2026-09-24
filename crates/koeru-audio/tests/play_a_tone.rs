//! 再生の実機ハーネス。 レンダーコールバックが実際に呼ばれることを確かめる。
//!
//! これは回帰テストではない。 `#[ignore]` なので既定では走らず、実機で `--ignored` を
//! 付けて走らせる。 出力デバイスが無ければ落ちる（黙って戻ると「通過」と数えられる。
//! `DEC-PLT-039`）。 何が起きたかを読むために標準出力を使う。

// 実機ハーネスなので `println!` を通す。 ここは人が読む出力で、
// 走らせた本人が数値を見て判断する。`tracing` へ出すと、
// 既定のフィルタでは見えず、走らせた意味が無くなる。
#![allow(clippy::print_stdout)]
// macOS 専用。 他 OS のバックエンドはまだ無い。
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

use koeru_audio::backend::macos as mac;

#[test]
#[ignore = "出力デバイスが要る実機ハーネス。--ignored を付けて走らせる"]
fn 短い音を鳴らしてコールバックが進むことを確かめる() {
    const RATE: u32 = 44_100;
    const MS: usize = 250;

    // 小さい音にする。 テストが人の耳を殴らない。
    let frames = RATE as usize * MS / 1000;
    let samples: Vec<f32> = (0..frames)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // 端をなだらかにして、ぶつっと切れないようにする。
            let env = (std::f32::consts::PI * i as f32 / frames as f32).sin();
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.02 * env
        })
        .collect();

    let p = mac::play(samples, RATE).expect("出力デバイスを開けない。つないで走らせる");

    // 鳴り終わるまで待つ。余裕を持たせる（バッファのぶん遅れる）。
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(2000);
    while !p.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let pos = p.position();
    println!(
        "再生位置 {pos} / {frames} フレーム、終了={}",
        p.is_finished()
    );
    assert!(pos > 0, "レンダーコールバックが1度も呼ばれていない");
    assert!(p.is_finished(), "末尾まで流し終えていない");
    assert_eq!(pos, frames, "全フレームを流し切ること");
}

/// 鳴らしながら継ぎ足せる（`TR-SYN-03`）。
///
/// 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る。
#[test]
#[ignore = "出力デバイスが要る実機ハーネス。--ignored を付けて走らせる"]
fn 鳴らしながら継ぎ足せる() {
    const RATE: u32 = 44_100;
    const CHUNK_MS: usize = 150;

    let chunk = |hz: f32| -> Vec<f32> {
        let n = RATE as usize * CHUNK_MS / 1000;
        (0..n)
            .map(|i| {
                let t = i as f32 / RATE as f32;
                let env = (std::f32::consts::PI * i as f32 / n as f32).sin();
                (2.0 * std::f32::consts::PI * hz * t).sin() * 0.02 * env
            })
            .collect()
    };

    let p =
        mac::play_streaming(chunk(440.0), RATE).expect("出力デバイスを開けない。つないで走らせる");

    // 先行を保ちながら足す。 足し終わるまで終わらない。
    for hz in [523.0, 659.0, 784.0] {
        std::thread::sleep(std::time::Duration::from_millis(80));
        p.push(&chunk(hz));
        println!("  継ぎ足した。残り {} サンプル", p.buffered());
    }
    assert!(!p.is_finished(), "seal する前は終わらないこと");

    p.seal();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(3000);
    while !p.is_finished() && std::time::Instant::now() < deadline {
        std::thread::sleep(std::time::Duration::from_millis(20));
    }

    let want = RATE as usize * CHUNK_MS / 1000 * 4;
    println!(
        "再生位置 {} / {want} フレーム、枯渇 {} 回",
        p.position(),
        p.starved()
    );
    assert!(p.is_finished(), "末尾まで流し終えること");
    assert_eq!(p.position(), want, "継ぎ足したぶんも全部鳴ること");
}
