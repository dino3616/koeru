//! **マスターは常に 44100 Hz で書かれる**（`TR-REC-01`, `TR-REC-02`）。
//!
//! > キャプチャは 32bit float・デバイスのネイティブレートで受ける。
//! > **44100 Hz でない場合はアプリ内の固定リサンプラで1回だけ変換し、
//! > 44100 Hz / 32bit float の WAV（マスター）として保存する**
//!
//! # なぜここで見るのか
//!
//! **変換そのものが抜けていた**（`DEC-REC-006`）。48000 Hz のデバイスで録ると
//! 48000 Hz のマスターが書かれ、試唱が「素材のサンプルレートが合わない」で止まる。
//! `write_distribution` はヘッダに 44100 と書くだけなので、**そのまま配れば
//! 44100 と名乗る 48000 の音**になる。
//!
//! **実機は要らない。** リングへ直接流し込めば、pump の経路をそのまま通せる。

#![allow(clippy::print_stdout)]

use koeru_app_lib::pump::Pump;
use koeru_audio::{ring, wav};

/// 倍音のある音を作る。**無音だと、変換が効いているか見えない。**
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

/// 自己相関で基本周波数を測る。
fn measure_hz(y: &[f32], rate: u32) -> f64 {
    let a = y.len() / 4;
    let x = &y[a..y.len() - a];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (lo, hi) = (
        (f64::from(rate) / 2000.0) as usize,
        (f64::from(rate) / 100.0) as usize,
    );
    let mut best = (0.0_f64, lo);
    for lag in lo..hi.min(x.len() / 2) {
        let c: f64 = x[..x.len() - lag]
            .iter()
            .zip(&x[lag..])
            .map(|(p, q)| f64::from(*p) * f64::from(*q))
            .sum();
        if c > best.0 {
            best = (c, lag);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let lag = best.1 as f64;
    f64::from(rate) / lag
}

/// リングへ流しながら1テイク録って、書かれた WAV を返す。
fn record_at(device_rate_hz: u32) -> wav::Wav {
    let dir = std::env::temp_dir().join(format!("koeru-master-{device_rate_hz}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("作れる");
    let path = dir.join("take.wav");

    let (producer, consumer) = ring::channel(device_rate_hz as usize * 4);
    let pump = Pump::start(consumer, device_rate_hz);

    // **押す前の音も流しておく**（プリロールが要る）。
    let lead = tone(440.0, device_rate_hz, 600);
    for c in lead.chunks(1024) {
        producer.push_or_drop(c);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    pump.start_take(path.clone()).expect("始められる");

    let body = tone(440.0, device_rate_hz, 1000);
    for c in body.chunks(1024) {
        producer.push_or_drop(c);
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // 末尾ぶんも流す（**フレームで数えているので、足りないと確定しない**）。
    let tail = tone(440.0, device_rate_hz, 700);
    std::thread::spawn(move || {
        for c in tail.chunks(1024) {
            producer.push_or_drop(c);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
        // 確定を待つ側が止まらないよう、しばらく流し続ける。
        let pad = vec![0.0_f32; 1024];
        for _ in 0..2000 {
            producer.push_or_drop(&pad);
            std::thread::sleep(std::time::Duration::from_millis(1));
        }
    });

    let f = pump.finish_take().expect("確定できる");
    wav::read(&f.path).expect("読める")
}

/// **48000 Hz のデバイスでも、マスターは 44100 Hz。**
#[test]
fn ネイティブが48kでもマスターは44100() {
    let w = record_at(48_000);
    println!("  レート {} / {} フレーム", w.rate_hz, w.samples.len());
    assert_eq!(
        w.rate_hz,
        wav::MASTER_RATE_HZ,
        "マスターが 44100 Hz で書かれていない（TR-REC-02）"
    );

    // **音の高さが変わっていない。** 変換を飛ばしてヘッダだけ書き換えると、
    // 440Hz が 404Hz（8.8% 低い）になる。
    let hz = measure_hz(&w.samples, w.rate_hz);
    println!("  基本周波数 {hz:.1} Hz");
    assert!(
        (hz - 440.0).abs() < 10.0,
        "音の高さが変わっている: {hz:.1} Hz（440 Hz のはず）"
    );
}

/// **44100 Hz のデバイスなら素通しする。** 要らない変換で鈍らせない。
#[test]
fn ネイティブが44100なら素通し() {
    let w = record_at(44_100);
    assert_eq!(w.rate_hz, wav::MASTER_RATE_HZ);
    let hz = measure_hz(&w.samples, w.rate_hz);
    assert!((hz - 440.0).abs() < 10.0, "{hz:.1} Hz");
}
