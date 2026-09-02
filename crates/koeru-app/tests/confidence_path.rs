//! **MFA が動いたときに、経路確信度が確信度へ届く**ことを確かめる（`TR-ALN-24`）。
//!
//! # なぜこの試験があるか
//!
//! 一度、`align_take` が `Alignment` を境界だけに畳んで返していた。
//! **事後確率が捨てられ、MFA が動いていても退避経路用の確信度が使われていた。**
//! 結果、成分 (1) 経路確信度が常に `None` になり、
//! `TR-ALN-26` (3) の主因ラベルで `Path` が出ることがなく、
//! 確認キューの並び（`TR-ALN-25`）が曖昧なアライメントを区別できなかった。
//!
//! **型は通っていた。** 4成分の器はあり、退避経路が `None` を入れるのも正しく、
//! それでも一次経路のデータが一度も流れていなかった。
//! **ここが見ているのは「器が埋まること」。**

use koeru_align::aligner::{Alignment, Posteriors, Segment};
use koeru_align::confidence::{Cause, Confidence};
use koeru_align::phoneme::{self, Phoneme};

/// 音素3つ分の事後確率を持つアライメントを組む。
fn alignment(rows: &[[f32; 3]], edges: [f64; 4]) -> Alignment {
    let sil = Phoneme::new(phoneme::SILENCE).expect("音素セットにある");
    Alignment {
        segments: (0..3)
            .map(|i| Segment {
                phoneme: sil,
                start_ms: edges[i],
                end_ms: edges[i + 1],
            })
            .collect(),
        posteriors: Some(Posteriors {
            frames: rows.len(),
            phonemes: 3,
            hop_ms: 10.0,
            values: rows.iter().flatten().copied().collect(),
        }),
        log_likelihood: Some(-1.0),
        grid_divergence: None,
    }
}

/// **事後確率を持つアライメントからは、成分が4つ揃う。**
#[test]
fn 経路確信度が確信度へ届く() {
    let rows = [
        [1.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [0.0, 0.0, 1.0],
    ];
    let a = alignment(&rows, [0.0, 20.0, 40.0, 60.0]);
    let c = Confidence::from_alignment(&a, &[0.5; 1000]).expect("組み立てられる");

    assert!(c.is_complete(), "経路確信度が欠けている: {c:?}");
    assert!(c.path.expect("ある") > 0.9);
}

/// **解が競っていれば、主因が経路になりうる**（`TR-ALN-26` (3)）。
///
/// 退避経路用の確信度しか使っていないと、`Cause::Path` は永久に出ない。
#[test]
fn 競っているときは主因が経路になる() {
    // どのフレームでも3つが競っていて、境界もはっきりしない。
    let rows = [[0.4, 0.35, 0.25]; 6];
    let a = alignment(&rows, [0.0, 20.0, 40.0, 60.0]);
    let c = Confidence::from_alignment(&a, &[0.5; 1000]).expect("組み立てられる");

    assert!(c.path.expect("ある") < 0.5, "path {:?}", c.path);
    // 音響は問題ないので、主因は経路か境界のどちらか。
    assert!(
        matches!(c.cause(0.6), Some(Cause::Path | Cause::Sharpness)),
        "主因が {:?}",
        c.cause(0.6)
    );
}

/// **確信度の高い解と低い解で、並びが変わる**（`TR-ALN-25` の確認キュー）。
///
/// ここが同じ値になると、確認の順序が意味を失う。
#[test]
fn 確信度の差が確認キューの並びに出る() {
    use koeru_align::review::{Entry, ReviewQueue};
    use koeru_core::oto::Oto;
    use std::time::Duration;

    let clear = alignment(
        &[
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 1.0],
        ],
        [0.0, 20.0, 40.0, 60.0],
    );
    let murky = alignment(&[[0.4, 0.35, 0.25]; 6], [0.0, 20.0, 40.0, 60.0]);

    let c_clear = Confidence::from_alignment(&clear, &[0.5; 1000]).expect("組み立てられる");
    let c_murky = Confidence::from_alignment(&murky, &[0.5; 1000]).expect("組み立てられる");
    assert!(
        c_clear.score() > c_murky.score(),
        "はっきりした解 {} が曖昧な解 {} より低い",
        c_clear.score(),
        c_murky.score()
    );

    let oto = Oto {
        offset_ms: 0.0,
        consonant_ms: 10.0,
        cutoff_ms: -100.0,
        preutterance_ms: 5.0,
        overlap_ms: 1.0,
    };
    let mut q = ReviewQueue::new(Duration::from_secs(30));
    q.insert("clear", Entry::new(oto));
    q.insert("murky", Entry::new(oto));
    q.estimate_low_confidence("clear", c_clear).expect("入る");
    q.estimate_low_confidence("murky", c_murky).expect("入る");

    // **確信度の低い順。** 曖昧なほうが先に人へ届く。
    let ids: Vec<&str> = q.queued().iter().map(|(k, _)| *k).collect();
    assert_eq!(ids, ["murky", "clear"]);
}

/// **退避経路は成分が欠けたまま。** 0 で埋めない。
#[test]
fn 退避経路では経路確信度が欠ける() {
    let mut a = alignment(&[[1.0, 0.0, 0.0]; 3], [0.0, 10.0, 20.0, 30.0]);
    a.posteriors = None;
    assert!(
        Confidence::from_alignment(&a, &[0.5; 100]).is_none(),
        "事後確率が無いのに組み立てている"
    );
}
