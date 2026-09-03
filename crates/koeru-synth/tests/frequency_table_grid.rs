//! **周波数表は素材の切り出し位置と格子に合わせて渡す**（`TR-SYN-08`, `TR-PKG-05`）。
//!
//! `.frq` は **ファイル全体**を **hop=256 サンプルの格子**で持つ（`TR-PKG-05`）。
//! 合成器が見るのは **oto で切り出した区間**を **5ms の格子**で並べたもの。
//! **どちらも合わないので、そのまま渡すと別のフレームの F0 を当てることになる。**
//!
//! 44100 Hz では hop=256 は 5.805ms。**1フレームあたり 16% ずれる**うえ、
//! offset の手前にある無音（F0=0）が発声の先頭に当たる。
//! F0=0 のフレームは WORLD が無声として合成するので、
//! **声が雑音になり、音高も乗らない。**

#![allow(clippy::print_stdout)]

use koeru_core::oto::Oto;
use koeru_synth::resampler::{FrequencyTable, RenderRequest, midi_to_hz, render};

const FS: u32 = 44_100;
const SOURCE_HZ: f64 = 220.0;
const LEAD_MS: f64 = 1000.0;
const TONE_MS: f64 = 1500.0;

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn n_samples(ms: f64) -> usize {
    (ms / 1000.0 * f64::from(FS)) as usize
}

/// 前に無音を置いた、倍音のある音。**実際のテイクと同じ形にする。**
fn source() -> Vec<f64> {
    let mut s = vec![0.0; n_samples(LEAD_MS)];
    for i in 0..n_samples(TONE_MS) {
        #[allow(clippy::cast_precision_loss)]
        let t = i as f64 / f64::from(FS);
        // 倍音を積む。**正弦波1本だと包絡が立たない。**
        let v: f64 = (1..=12)
            .map(|h| {
                let a = 1.0 / f64::from(h);
                a * (2.0 * std::f64::consts::PI * SOURCE_HZ * f64::from(h) * t).sin()
            })
            .sum();
        s.push(v * 0.1);
    }
    s.extend(std::iter::repeat_n(0.0, n_samples(500.0)));
    s
}

/// `.frq` と同じ格子（hop=256、ファイル全体）で周波数表を作る。
fn frq_table(samples: &[f64]) -> Vec<f64> {
    let hop = 256_usize;
    let lead = n_samples(LEAD_MS);
    let tone_end = lead + n_samples(TONE_MS);
    (0..samples.len().div_ceil(hop))
        .map(|i| {
            let at = i * hop;
            if at >= lead && at < tone_end {
                SOURCE_HZ
            } else {
                0.0
            }
        })
        .collect()
}

/// 自己相関で基本周波数を測る。
fn measure_hz(y: &[f64]) -> f64 {
    // 定常部だけ見る。
    let x = &y[y.len() / 4..y.len() * 3 / 4];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (lo, hi) = (
        (f64::from(FS) / 900.0) as usize,
        (f64::from(FS) / 80.0) as usize,
    );
    let mut best = (0.0_f64, lo);
    for lag in lo..hi.min(x.len() / 2) {
        let c: f64 = x[..x.len() - lag]
            .iter()
            .zip(&x[lag..])
            .map(|(p, q)| p * q)
            .sum();
        if c > best.0 {
            best = (c, lag);
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let lag = best.1 as f64;
    f64::from(FS) / lag
}

fn oto() -> Oto {
    Oto {
        offset_ms: LEAD_MS,
        consonant_ms: 50.0,
        // 負の cutoff は「offset からの長さ」。
        cutoff_ms: -TONE_MS,
        preutterance_ms: 30.0,
        overlap_ms: 10.0,
    }
}

fn request<'a>(s: &'a [f64], table: Option<&'a [f64]>, midi: i32) -> RenderRequest<'a> {
    RenderRequest {
        samples: s,
        sample_rate_hz: FS,
        tone: midi,
        oto: oto(),
        required_length_ms: 800.0,
        consonant_velocity: 100.0,
        volume: 100.0,
        modulation: 0.0,
        tempo: 120.0,
        pitch_bend_cents: &[],
        frequency_table: table.map(|f0| FrequencyTable {
            f0,
            hop_samples: 256,
        }),
    }
}

/// **周波数表を渡しても、指定した音高で鳴る**（`TR-SYN-08`）。
///
/// 表を渡さない経路（合成器が推定する）と同じ結果になること。
/// **表の格子と切り出し位置を合わせないと、ここが落ちる。**
#[test]
fn 周波数表を渡しても指定した音高で鳴る() {
    let s = source();
    let table = frq_table(&s);
    for midi in [60, 67, 72] {
        let want = midi_to_hz(midi);

        let with = render(&request(&s, Some(&table), midi)).expect("表ありで合成できる");
        let without = render(&request(&s, None, midi)).expect("表なしで合成できる");

        let (a, b) = (measure_hz(&with), measure_hz(&without));
        let cents = |got: f64| 1200.0 * (got / want).log2();
        println!(
            "  MIDI {midi} 目標 {want:.1}Hz / 表あり {a:.1}Hz ({:+.0}c) / 表なし {b:.1}Hz ({:+.0}c)",
            cents(a),
            cents(b)
        );

        assert!(
            cents(b).abs() < 50.0,
            "表なしで音高が合っていない: {b:.1}Hz（目標 {want:.1}Hz）"
        );
        assert!(
            cents(a).abs() < 50.0,
            "**周波数表を渡すと音高が合わない**: {a:.1}Hz（目標 {want:.1}Hz）。\
             表はファイル全体・hop=256 の格子で、合成器が見るのは \
             oto で切り出した区間の 5ms 格子（TR-SYN-08, TR-PKG-05）"
        );
    }
}

/// **周波数表を渡しても声が有声のまま合成される。**
///
/// F0=0 のフレームを当ててしまうと WORLD は無声として合成する。
/// **音高の試験だけでは、雑音の中に周期が見えて通ることがある。**
/// 周期性そのものを見る。
#[test]
fn 周波数表を渡しても有声のまま合成される() {
    let s = source();
    let table = frq_table(&s);
    let y = render(&request(&s, Some(&table), 60)).expect("合成できる");

    // 定常部の自己相関のピーク比。**有声なら 1 に近い。**
    let x = &y[y.len() / 4..y.len() * 3 / 4];
    let energy: f64 = x.iter().map(|v| v * v).sum();
    assert!(energy > 0.0, "何も鳴っていない");
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let lag = (f64::from(FS) / measure_hz(&y)).round() as usize;
    let c: f64 = x[..x.len() - lag]
        .iter()
        .zip(&x[lag..])
        .map(|(p, q)| p * q)
        .sum();
    let r = c / energy;
    println!("  周期性 {r:.2}");
    assert!(
        r > 0.5,
        "**雑音になっている**（周期性 {r:.2}）。周波数表の当て方を確認すること"
    );
}
