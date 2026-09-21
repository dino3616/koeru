//! 同梱の Kaldi から組んだ C++ が、モデルを読んで特徴を出せるかを見る。
//!
//! # なぜ要るのか
//!
//! 他の試験は FFI を越えない。 `ledger` と `phoneme` と `model_dir` しか触らず、
//! 実音声ハーネスは環境変数が無いと静かに戻る。だから C++ の組み方が変わっても
//! 全部緑のまま通る。 開発ツールを Nix へ移したとき（`DEC-PLT-033`）、cc ラッパが
//! triple の食い違いを警告したが、実害かどうかを他の試験では判定できなかった。
//!
//! # 何を見ていないか
//!
//! 精度は見ていない。 境界が全部ずれていても、ここは通る（`EVID-ALN-001`）。
//! 位置が合っているかは実音声で見る（`alignment_on_real_audio`）。

// Kaldi を組んでいる OS だけで走らせる。 書いていない OS では
// `mfa::MfaAligner::open` が常に `Unsupported` を返すので、閉じないと
// ubuntu と windows の試験が落ちる。 `mfa/mod.rs` と同じ cfg を使う。
#![cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]

use koeru_align::mfa::{self, FRAME_SHIFT_MS, MODEL_SAMPLE_RATE_HZ, MfaAligner};

#[test]
fn 組んだ_kaldi_がモデルを読んで特徴を出す() {
    // モデルは submodule ＋ LFS で入る（`DEC-ALN-012`）。無い環境では静かに戻る。
    let Some(dir) = mfa::model_dir() else {
        return;
    };
    let aligner = MfaAligner::open(&dir, "kaldi_build").expect("モデルを開ける");

    // 変換行列を読み違えると、ここが 13 や 39 になる
    // （`EVID-ALN-001` の「LDA+MLLT の 40 次」）。
    assert_eq!(
        aligner.feature_dim(),
        40,
        "特徴の次数が LDA+MLLT のものでない"
    );
    assert!(aligner.num_phones() > 0, "音素数が 0");

    // 1秒の正弦波。 中身は問わない——出る形と、有限であることだけを見る。
    let rate = MODEL_SAMPLE_RATE_HZ;
    let signal: Vec<f32> = (0..rate as usize)
        .map(|i| {
            let t = i as f32 / rate as f32;
            0.3 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
        })
        .collect();
    let (rows, features) = aligner.features(&signal, rate).expect("特徴が出る");

    let want_rows = (1000.0 / FRAME_SHIFT_MS) as usize;
    assert_eq!(rows, want_rows, "1秒から出るフレーム数が進み幅と合わない");
    assert_eq!(
        features.len(),
        rows * aligner.feature_dim(),
        "特徴の要素数が行数×次数でない"
    );
    // NaN が混ざると尤度が全部 NaN になり、境界が先頭へ寄る。
    assert!(
        features.iter().all(|x| x.is_finite()),
        "特徴に NaN か Inf がある"
    );
}
