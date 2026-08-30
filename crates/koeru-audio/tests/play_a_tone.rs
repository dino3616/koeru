//! **再生の実機ハーネス。** レンダーコールバックが実際に呼ばれることを確かめる。
//!
//! 出力デバイスが無い環境では途中で戻る。**これは回帰テストではない。**
//! 何が起きたかを読むために標準出力を使う。

#![allow(clippy::print_stdout)]
// **macOS 専用。** 他 OS のバックエンドはまだ無い。
#![cfg(target_os = "macos")]

use koeru_audio::backend::macos as mac;

#[test]
fn 短い音を鳴らしてコールバックが進むことを確かめる() {
    const RATE: u32 = 44_100;
    const MS: usize = 250;

    // **小さい音にする。** テストが人の耳を殴らない。
    let frames = RATE as usize * MS / 1000;
    let samples: Vec<f32> = (0..frames)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            // 端をなだらかにして、ぶつっと切れないようにする。
            let env = (std::f32::consts::PI * i as f32 / frames as f32).sin();
            (2.0 * std::f32::consts::PI * 440.0 * t).sin() * 0.02 * env
        })
        .collect();

    let Ok(p) = mac::play(samples, RATE) else {
        println!("出力デバイスが無い。ここで戻る");
        return;
    };

    // 鳴り終わるまで待つ。**余裕を持たせる**（バッファのぶん遅れる）。
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
