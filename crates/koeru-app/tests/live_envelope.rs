//! **波形の目盛りが、実時間どおりに進む**（`TR-REC-43`）。
//!
//! # 何が壊れたか
//!
//! 最初の実装は、排出のたびにリング（1.5 秒ぶん、265 KB）を丸ごと写していた。
//! **1回 461 µs、排出は 2ms ごとなので、1秒あたり 230 ms を写すことに使う。**
//! 排出が実時間から遅れ、**画面の波形が速くなったり遅くなったりした。**
//!
//! 画面側も `setInterval` で問い合わせていた。1回が間隔より長くかかると
//! **問い合わせが重なり、遅れて届いた古い包絡で波形が巻き戻る**（「ループする」）。
//!
//! **実機は要らない。** リングへ直接流し込めば、pump の経路をそのまま通せる。

#![allow(clippy::print_stdout)]

use koeru_app_lib::pump::Pump;
use koeru_audio::{ring, wav};

/// 倍音のある音。
fn tone(hz: f64, rate: u32, ms: u64) -> Vec<f32> {
    let n = (u64::from(rate) * ms / 1000) as usize;
    (0..n)
        .map(|i| {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f64 / f64::from(rate);
            #[allow(clippy::cast_possible_truncation)]
            let v = (0.5 * (std::f64::consts::TAU * hz * t).sin()) as f32;
            v
        })
        .collect()
}

/// **流し込んだぶんだけ進む。** 取りこぼしも作り足しもしない。
#[test]
fn 通算フレーム数が流した量と一致する() {
    let rate = 48_000_u32;
    let (producer, consumer) = ring::channel(rate as usize * 4);
    let pump = Pump::start(consumer, rate);

    let fed = tone(440.0, rate, 1000);
    for c in fed.chunks(1024) {
        producer.push_or_drop(c);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // 排出が追いつくのを待つ。
    std::thread::sleep(std::time::Duration::from_millis(300));

    let (_, position) = pump.envelope();
    // 44100 へ落としたぶん。**端は目盛りの区切りで丸まる。**
    let want = u64::from(wav::MASTER_RATE_HZ);
    println!("  流した 1000ms / 通算 {position} フレーム（44100 なら {want}）");
    let diff = position.abs_diff(want);
    assert!(
        diff < want / 20,
        "実時間と合っていない: {position} フレーム（{want} のはず）"
    );
}

/// **通算フレーム数は単調に増える。** 巻き戻ると波形がループして見える。
#[test]
fn 通算フレーム数は巻き戻らない() {
    let rate = 48_000_u32;
    let (producer, consumer) = ring::channel(rate as usize * 4);
    let pump = Pump::start(consumer, rate);

    let feed = std::thread::spawn(move || {
        let x = tone(440.0, rate, 2000);
        for c in x.chunks(512) {
            producer.push_or_drop(c);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    let mut last = 0_u64;
    for _ in 0..60 {
        let (_, p) = pump.envelope();
        assert!(p >= last, "巻き戻った: {last} → {p}");
        last = p;
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    feed.join().expect("流し終える");
    assert!(last > 0, "1つも進んでいない");
    println!("  {last} フレームまで進んだ");
}

/// **包絡を引いても排出は遅れない**（`TR-REC-43`）。
///
/// 画面は 50ms ごとに引く。**引くたびに 1.5 秒ぶんを写していては追いつかない。**
#[test]
fn 引き続けても実時間に追いつく() {
    let rate = 48_000_u32;
    let (producer, consumer) = ring::channel(rate as usize * 4);
    let pump = Pump::start(consumer, rate);

    let feed = std::thread::spawn(move || {
        let x = tone(440.0, rate, 2000);
        for c in x.chunks(480) {
            producer.push_or_drop(c);
            // 480 サンプル = 10ms ぶん。**実時間で流す。**
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    });

    // 画面と同じ間隔で引き続ける。
    let started = std::time::Instant::now();
    while started.elapsed() < std::time::Duration::from_millis(2000) {
        let _ = pump.envelope();
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    feed.join().expect("流し終える");
    std::thread::sleep(std::time::Duration::from_millis(300));

    let (_, position) = pump.envelope();
    let want = u64::from(wav::MASTER_RATE_HZ) * 2;
    #[allow(clippy::cast_precision_loss)]
    let ratio = position as f64 / want as f64;
    println!(
        "  実時間 2000ms に対して {position} フレーム（{:.0}%）",
        ratio * 100.0
    );
    assert!(
        ratio > 0.9,
        "引きながらだと排出が追いつかない: {position} フレーム（{want} のはず）"
    );
}

/// **細かく流しても、通算が実時間からずれない**（`TR-REC-43`）。
///
/// 最初の実装は、**目盛りが揃った回にだけ**その回のフレーム数を足していた。
/// 揃わなかった回のぶんは数えられず、通算が少しずつ足りなくなる。
/// **目盛り（5ms）より細かく流すと、ずれが目に見える。**
#[test]
fn 目盛りより細かく流しても通算がずれない() {
    let rate = 48_000_u32;
    let (producer, consumer) = ring::channel(rate as usize * 4);
    let pump = Pump::start(consumer, rate);

    // 1回 64 サンプル ＝ 1.3ms。**目盛り（5ms）より細かい。**
    let x = tone(440.0, rate, 1000);
    for c in x.chunks(64) {
        producer.push_or_drop(c);
        std::thread::sleep(std::time::Duration::from_micros(1333));
    }
    std::thread::sleep(std::time::Duration::from_millis(300));

    let (_, position) = pump.envelope();
    let want = u64::from(wav::MASTER_RATE_HZ);
    #[allow(clippy::cast_precision_loss)]
    let off = (position as f64 - want as f64) / want as f64;
    println!(
        "  細かく流した 1000ms / 通算 {position}（ずれ {:+.2}%）",
        off * 100.0
    );
    assert!(
        off.abs() < 0.01,
        "通算が実時間からずれた: {position}（{want} のはず）"
    );
}

/// **環をまたいでも、流した量より多く排出しない**（`DEC-REC-007`）。
///
/// リングの `head` / `tail` を剰余で持っていたころ、**環をまたいだ瞬間から
/// 消費側が「余分に読める」と誤認し、読み終えた古い音を読み直していた。**
/// 実測で **127%**——流した量の 1.27 倍を排出していた。
///
/// 波形では「前に流れたものがまた流れる」、
/// 収録では**実時間より長いテイク**と**7 秒の先頭余白**として出た。
#[test]
fn 環をまたいでも余分に排出しない() {
    let rate = 48_000_u32;
    // **1秒で環をまたぐ容量。** 3秒流して3回またぐ。
    let (producer, consumer) = ring::channel(rate as usize);
    let pump = Pump::start(consumer, rate);

    let feed = std::thread::spawn(move || {
        let block = vec![0.2_f32; 512];
        let t = std::time::Instant::now();
        let mut fed = 0_u64;
        while t.elapsed() < std::time::Duration::from_secs(3) {
            producer.push_or_drop(&block);
            fed += 512;
            // 512 サンプル = 10.7ms ぶん。**実時間で流す。**
            std::thread::sleep(std::time::Duration::from_micros(10_666));
        }
        fed
    });
    let fed = feed.join().expect("流し終える");
    std::thread::sleep(std::time::Duration::from_millis(400));

    let (_, drained) = pump.envelope();
    #[allow(clippy::cast_precision_loss)]
    let want = fed as f64 * 44_100.0 / 48_000.0;
    #[allow(clippy::cast_precision_loss)]
    let ratio = drained as f64 / want;
    println!("  流した {fed} / 排出 {drained}（{:.0}%）", ratio * 100.0);
    assert!(
        ratio < 1.05,
        "流した量より多く排出した: {drained}（{want:.0} のはず）。古い音を読み直している"
    );
    assert!(ratio > 0.9, "排出が追いついていない: {drained}");
}
