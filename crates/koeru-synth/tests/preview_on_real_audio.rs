//! **実際に録った声を、指定した音高で歌わせてみる実機ハーネス。**
//!
//! **これは回帰テストではない。** 音声が無い環境では静かに戻る
//! （`koeru-align` の `alignment_on_real_audio.rs` と同じ形）。
//!
//! ```bash
//! KOERU_SYNTH_SAMPLE_WAV=/path/to/take.wav \
//! KOERU_SYNTH_SAMPLE_OFFSET_MS=1235 \
//! KOERU_SYNTH_SAMPLE_LENGTH_MS=550 \
//!   cargo test --package koeru-synth --test preview_on_real_audio -- --nocapture
//! ```
//!
//! # 何を見ているか
//!
//! **音色の良し悪しは測らない。** 見るのは2つだけ——
//!
//! 1. **指定した音高で鳴るか**（`TR-SYN-02`）
//! 2. **有声のまま合成されるか**——雑音になっていないか
//!
//! どちらも「本人の声が歌になる」の最低線で、正解データが要らない。
//!
//! **合成音の試験では両方すり抜けた。** 周波数表をファイル全体・hop=256 のまま
//! 渡していたとき、単体試験は表を区間ぶん・5ms 格子で作っていたので通り、
//! **アプリだけが雑音を出していた**（`tests/frequency_table_grid.rs` で閉じた）。

#![allow(clippy::print_stdout)]

use koeru_core::analysis::TakeAnalysis;
use koeru_core::oto::Oto;
use koeru_synth::resampler::{FrequencyTable, RenderRequest, midi_to_hz, render};
use koeru_synth::world;

fn env_f64(key: &str) -> Option<f64> {
    std::env::var(key).ok()?.parse().ok()
}

/// パワーで発声区間を粗く出す。**ピークに対する比で切る。**
fn voiced_span_ms(samples: &[f64], rate_hz: u32) -> Option<(f64, f64)> {
    let win = (rate_hz as usize / 100).max(1); // 10ms
    let peak = samples.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if peak <= 0.0 {
        return None;
    }
    let (mut first, mut last) = (None, 0_usize);
    for (i, c) in samples.chunks(win).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let rms = (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt();
        if rms > peak * 0.05 {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    first.map(|f| (f as f64 * 10.0, last as f64 * 10.0))
}

/// 自己相関で基本周波数と周期性を測る。
fn measure(y: &[f64], rate_hz: u32) -> (f64, f64) {
    let x = &y[y.len() / 4..y.len() * 3 / 4];
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (lo, hi) = (
        (f64::from(rate_hz) / 900.0) as usize,
        (f64::from(rate_hz) / 70.0) as usize,
    );
    let energy: f64 = x.iter().map(|v| v * v).sum();
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
    let r = if energy > 0.0 { best.0 / energy } else { 0.0 };
    (f64::from(rate_hz) / lag, r)
}

/// **実際の声が、指定した音高で、雑音にならずに鳴る。**
#[test]
fn 実音声を指定した音高で歌わせられる() {
    let Ok(path) = std::env::var("KOERU_SYNTH_SAMPLE_WAV") else {
        return;
    };
    let w = koeru_audio::wav::read(std::path::Path::new(&path)).expect("wav を読める");
    let s: Vec<f64> = w.samples.iter().map(|v| f64::from(*v)).collect();

    // 切り出し。**指定が無ければパワーで見た発声区間を使う。**
    let (lo, hi) = voiced_span_ms(&s, w.rate_hz).expect("発声がある");
    let offset_ms = env_f64("KOERU_SYNTH_SAMPLE_OFFSET_MS").unwrap_or(lo);
    let length_ms = env_f64("KOERU_SYNTH_SAMPLE_LENGTH_MS").unwrap_or(hi - lo);
    println!("  切り出し {offset_ms:.0}ms から {length_ms:.0}ms");

    // **アプリと同じ手順で `.frq` を作る**——ファイル全体を hop=256 の格子で。
    let frame_ms = world::DEFAULT_FRAME_PERIOD_MS;
    let (f0, _) = world::estimate_f0(
        &s,
        w.rate_hz,
        world::F0Method::DioStoneMask,
        55.0,
        1100.0,
        frame_ms,
    );
    let analysis = TakeAnalysis::compute(&w.samples, w.rate_hz, &f0, frame_ms / 1000.0);
    let table = analysis.frq.f0;

    let oto = Oto {
        offset_ms,
        consonant_ms: 50.0,
        cutoff_ms: -length_ms,
        preutterance_ms: 30.0,
        overlap_ms: 10.0,
    };

    for midi in [55, 60, 67, 72] {
        let want = midi_to_hz(midi);
        let y = render(&RenderRequest {
            samples: &s,
            sample_rate_hz: w.rate_hz,
            tone: midi,
            oto,
            required_length_ms: 800.0,
            consonant_velocity: 100.0,
            volume: 100.0,
            modulation: 0.0,
            tempo: 120.0,
            pitch_bend_cents: &[],
            frequency_table: Some(FrequencyTable {
                f0: &table,
                hop_samples: koeru_core::frq::HOP_SIZE,
            }),
        })
        .expect("合成できる");

        let (hz, r) = measure(&y, w.rate_hz);
        let cents = 1200.0 * (hz / want).log2();
        println!("  MIDI {midi} 目標 {want:6.1}Hz / 実測 {hz:6.1}Hz ({cents:+5.0}c) 周期性 {r:.2}");

        // **オクターブを外していないこと。** 音色は測らない。
        assert!(
            cents.abs() < 100.0,
            "指定した音高で鳴っていない: {hz:.1}Hz（目標 {want:.1}Hz、{cents:+.0}セント）"
        );
        // **雑音になっていないこと。** 周波数表の当て方を間違えるとここが落ちる。
        assert!(r > 0.4, "雑音になっている（周期性 {r:.2}）");
    }
}
