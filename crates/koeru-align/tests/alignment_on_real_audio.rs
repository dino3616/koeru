//! 実際に録った音声でアライメントが正気かを見る実機ハーネス。
//!
//! これは回帰テストではない。 音声が無い環境では静かに戻る
//! （`koeru-audio` の `record_to_file.rs` と同じ形）。
//!
//! ```bash
//! KOERU_ALIGN_SAMPLE_WAV=/path/to/s004_1.wav \
//! KOERU_ALIGN_SAMPLE_READING='ぎ ぎゃ ぎゅ ぎょ' \
//!   cargo test --package koeru-align --test alignment_on_real_audio -- --nocapture
//! ```
//!
//! # 何を見ているか
//!
//! 正解の境界は持っていない（`DEC-ALN-007` で評価を M6 へ送った）。
//! だから精度は測らず、**「アライナが置いた発声区間が、
//! パワーで見た発声区間と重なるか」**だけを見る。正解データが要らない性質。
//!
//! これは実際に壊れた形を捕まえる。 CMVN の分散まで正規化していたとき、
//! 8音素が発声の外の 80ms に潰れ、重なりがほぼ 0 になった。
//! 合成音の試験は全部通っていたので、ここでしか気づけない。

// 実機ハーネスなので `println!` を通す。 ここは人が読む出力で、
// 走らせた本人が数値を見て判断する。`tracing` へ出すと、
// 既定のフィルタでは見えず、走らせた意味が無くなる。
#![allow(clippy::print_stdout)]
// バックエンドが書いてある OS だけ。 `koeru_force_unsupported_backend` では
// `MfaAligner::open` が `ModelUnavailable` を返すので、組み立てから外す。
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

use koeru_align::aligner::{AlignRequest, Aligner as _};
use koeru_align::{mfa::MfaAligner, phoneme, segment::Boundaries};

/// パワーで発声区間を粗く出す。ピークに対する比で切る。
fn voiced_span_ms(samples: &[f64], rate_hz: u32) -> Option<(f64, f64)> {
    let win = (rate_hz as usize / 100).max(1); // 10ms
    let peak = samples.iter().fold(0.0_f64, |m, v| m.max(v.abs()));
    if peak <= 0.0 {
        return None;
    }
    let mut first = None;
    let mut last = 0_usize;
    for (i, c) in samples.chunks(win).enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let rms = (c.iter().map(|v| v * v).sum::<f64>() / c.len() as f64).sqrt();
        if rms > peak * 0.02 {
            if first.is_none() {
                first = Some(i);
            }
            last = i;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    first.map(|f| (f as f64 * 10.0, last as f64 * 10.0))
}

fn model_dir() -> Option<std::path::PathBuf> {
    let p =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("models/japanese_mfa/acoustic");
    p.join("final.mdl").is_file().then_some(p)
}

/// アライナが置いた発声区間が、パワーで見た発声区間と重なる。
#[test]
fn 実音声で発声の位置がパワーと合う() {
    let (Ok(wav), Ok(reading)) = (
        std::env::var("KOERU_ALIGN_SAMPLE_WAV"),
        std::env::var("KOERU_ALIGN_SAMPLE_READING"),
    ) else {
        return;
    };
    let Some(dir) = model_dir() else {
        println!("  モデルの submodule が初期化されていない");
        return;
    };

    let w = koeru_audio::wav::read(std::path::Path::new(&wav)).expect("wav を読める");
    let s: Vec<f64> = w.samples.iter().map(|v| f64::from(*v)).collect();
    let (lo, hi) = voiced_span_ms(&s, w.rate_hz).expect("発声がある");
    println!("  パワーで見た発声: {lo:.0} 〜 {hi:.0} ms");

    let readings: Vec<&str> = reading.split_whitespace().collect();
    let ph = phoneme::phonemes_for_all(&readings).expect("読みを引ける");
    let a = MfaAligner::open(&dir, "harness").expect("モデルを読める");
    let r = a
        .align(&AlignRequest {
            samples: &s,
            sample_rate_hz: w.rate_hz,
            phonemes: &ph,
            grid: None,
        })
        .expect("アライメントできる");

    let per = Boundaries::per_mora(&r, &readings).expect("モーラごとに取れる");
    for (b, k) in per.iter().zip(&readings) {
        println!(
            "  {k:<4} {:>8.1} 〜 {:>8.1} ms",
            b.voice_start_ms, b.vowel_end_ms
        );
    }
    let (a_lo, a_hi) = (
        per.first().expect("1つはある").voice_start_ms,
        per.last().expect("1つはある").vowel_end_ms,
    );
    println!("  アライナが置いた発声: {a_lo:.0} 〜 {a_hi:.0} ms");

    // 重なりの割合で見る。 正解は持っていないので、
    // 「まったく別の場所を指していない」ことだけを確かめる。
    let overlap = (a_hi.min(hi) - a_lo.max(lo)).max(0.0);
    let union = a_hi.max(hi) - a_lo.min(lo);
    let ratio = if union > 0.0 { overlap / union } else { 0.0 };
    println!("  重なり {:.0}%", ratio * 100.0);

    // CMVN の分散を正規化していたときは、ここが 0 近くまで落ちた。
    assert!(
        ratio > 0.5,
        "発声の位置がパワーと合っていない（重なり {:.0}%）。\
         アライナ {a_lo:.0}〜{a_hi:.0}ms / パワー {lo:.0}〜{hi:.0}ms",
        ratio * 100.0
    );
}
