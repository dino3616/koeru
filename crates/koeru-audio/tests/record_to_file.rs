//! **縦切りの1枚目: 実際に録音して、テイクのファイルを確定させる。**
//!
//! recording-input.fsl の状態機械を通し、macOS のキャプチャから
//! `.wav.part` → fsync → rename までを実機で確かめる。
//!
//! **これは回帰テストではなく、実機ハーネス。** マイクが無い環境では途中で戻る。
//! 何が起きたかを読むために標準出力を使う。ここだけ `print_stdout` を許す
//! （出力は tracing に統一する規律は、アプリのコードに掛かるもの）。

#![allow(clippy::print_stdout)]
// **macOS 専用。** 他 OS のバックエンドはまだ無い（DEC-REC-001 で後回しと決めた）。
// これを付け忘れて Windows / Linux の CI を落とした。
#![cfg(target_os = "macos")]

use koeru_audio::{Session, backend::macos as mac, wav};

#[test]
fn 録音してテイクのファイルを作る() {
    let devices = mac::enumerate_input_devices().expect("列挙");
    // **信号が届いているデバイスを選ぶ。** 既定が無音のことがある（実機で踏んだ）。
    let mut chosen = None;
    for d in &devices {
        let Ok((c, cons)) = mac::open(&d.id, 48_000) else {
            continue;
        };
        c.arm();
        std::thread::sleep(std::time::Duration::from_millis(200));
        c.disarm();
        let mut b = vec![0.0_f32; 32768];
        let mut pk = 0.0_f32;
        loop {
            let n = cons.pop(&mut b);
            if n == 0 {
                break;
            }
            for v in &b[..n] {
                pk = pk.max(v.abs());
            }
        }
        println!("  候補 {:?}: ピーク {pk:.6}", d.id);
        if pk > 1e-6 {
            chosen = Some(d);
            break;
        }
    }
    let Some(dev) = chosen.or(devices.first()) else {
        println!("入力デバイスが無い。飛ばす");
        return;
    };

    // ── 状態機械の手順どおりに進める（AC-REC-101）──
    let mut s = Session::new(3);
    s.select_device(dev.id.clone()).expect("デバイスを選ぶ");

    let (cap, consumer) = mac::open(&dev.id, 48_000 * 4).expect("ストリームを開く");
    s.open_stream().expect("開いた状態にする");

    // マイクモードが standard なら「効果を無効化できた」
    let mode = mac::active_microphone_mode();
    if mode.is_clean() {
        s.effects_all_disabled().expect("効果なし");
    } else {
        s.effects_some_remain().expect("効果が残る");
        s.show_prompt_once().expect("一度だけ提示");
        println!("**OS 側の加工が残っている: {mode:?}**");
    }

    s.calibrate_gain().expect("校正");

    // ── 入力が届いているか（TR-REC-17）──
    // **権限が無いと macOS は無音を返すので、成否ではなく中身を見る。**
    cap.arm();
    std::thread::sleep(std::time::Duration::from_millis(300));
    let mut probe = vec![0.0_f32; 48_000];
    let mut peak = 0.0_f32;
    loop {
        let n = consumer.pop(&mut probe);
        if n == 0 {
            break;
        }
        for v in &probe[..n] {
            peak = peak.max(v.abs());
        }
    }
    let alive = peak > 1e-6;
    println!(
        "入力の生死判定: ピーク {peak:.6} → {}",
        if alive {
            "届いている"
        } else {
            "**届いていない**"
        }
    );
    if alive {
        s.input_is_alive().expect("生きている");
    } else {
        s.input_is_dead().expect("死んでいる");
        cap.disarm();
        println!("入力が届いていないので収録しない（TR-REC-17）");
        return;
    }

    s.estimate_space(1_000_000, u64::MAX).expect("残量");

    // ── 収録（REQ-REC-108）──
    let mut path = std::env::temp_dir();
    path.push(format!("koeru-take-{}.wav", std::process::id()));
    let mut take =
        wav::PartialTake::create(&path, cap.format().sample_rate_hz).expect("part を開く");

    s.start_take().expect("収録開始");
    let t0 = std::time::Instant::now();
    let mut buf = vec![0.0_f32; 8192];
    while t0.elapsed() < std::time::Duration::from_millis(800) {
        let n = consumer.pop(&mut buf);
        if n > 0 {
            take.write(&buf[..n]).expect("書ける");
        } else {
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    }
    // 残りを吸い出す
    loop {
        let n = consumer.pop(&mut buf);
        if n == 0 {
            break;
        }
        take.write(&buf[..n]).expect("書ける");
    }
    cap.disarm();

    // ── 取りこぼしの判定（TR-REC-07）──
    let dropped = consumer.dropped();
    let jumps = cap.discontinuities();
    println!(
        "捨て {dropped} / 飛び {jumps} / レンダ失敗 {}",
        cap.render_errors()
    );

    if dropped > 0 || jumps > 0 {
        println!("**取りこぼしがあるのでテイクを無効にする**（TR-REC-07）");
        take.discard().expect("捨てられる");
        s.finish_take().expect("状態は進める");
        return;
    }

    let frames = take.frames();
    let final_path = take.finalize().expect("確定できる");
    s.finish_take().expect("テイク確定");

    // ── 読み戻して確かめる ──
    let w = wav::read(&final_path).expect("読み戻せる");
    println!("確定: {} フレーム / {} Hz", frames, w.rate_hz);
    println!("読み戻し: {} サンプル", w.samples.len());
    assert_eq!(w.samples.len() as u64, frames, "書いた数と読めた数が一致");
    assert_eq!(s.takes(), 1);
    assert!(s.is_stream_open(), "テイク確定後もストリームは開いたまま");

    let peak = w.samples.iter().fold(0.0_f32, |a, b| a.max(b.abs()));
    println!("ピーク振幅: {peak:.6}");
    println!(
        "ファイル: {} バイト",
        std::fs::metadata(&final_path).unwrap().len()
    );
    std::fs::remove_file(&final_path).ok();
}
