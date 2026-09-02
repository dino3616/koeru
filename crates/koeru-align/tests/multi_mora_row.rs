//! **録音リストが実際に生成する行**でアライメントが通ることを確かめる
//! （`TR-RCL-03`, `TR-ALN-11`, `DEC-ALN-013`）。
//!
//! # なぜこの試験があるか
//!
//! `koeru-align` の試験はすべて1〜2音素の音素列で書いていて、
//! **録音リストの実際の形（1行に最大8モーラ）を一度も食わせていなかった。**
//! 縦切りの実機ハーネスはマイクが無い環境で途中で戻るので、そこも通らない。
//!
//! 結果、`Boundaries::from_alignment` が `[sil, C, V, sil]` の4区間しか受けないまま
//! 出荷され、**画面に「発声を見つけられませんでした」と出て初めて分かった。**
//! アライメントは成功していて、境界への変換が落ちていた。**メッセージが嘘をついていた。**
//!
//! **ここが見ているのは「生成された行がそのまま通ること」。**

use koeru_align::phoneme;
use koeru_align::segment::Boundaries;
use koeru_core::inventory::UnitSet;
use koeru_core::reclist;

/// 生成した行の読みから、区間の数を数える。
fn slots_for(readings: &[&str]) -> usize {
    phoneme::phonemes_for_all(readings).expect("引ける").len() + 2
}

/// 区間だけを持つアライメントを組む（境界の取り出しを見るので中身は問わない）。
fn alignment(slots: usize) -> koeru_align::aligner::Alignment {
    let sil = phoneme::Phoneme::new(phoneme::SILENCE).expect("ある");
    koeru_align::aligner::Alignment {
        segments: (0..slots)
            .map(|i| koeru_align::aligner::Segment {
                phoneme: sil,
                start_ms: i as f64 * 100.0,
                end_ms: (i + 1) as f64 * 100.0,
            })
            .collect(),
        posteriors: None,
        log_likelihood: None,
        grid_divergence: None,
    }
}

/// **生成された行の読みが、すべて辞書で引ける**（`TR-ALN-07`）。
#[test]
fn 生成した行の読みが全て辞書にある() {
    for set in [UnitSet::Core, UnitSet::Extended] {
        for per_row in [1, 5, reclist::MAX_UNITS_PER_ROW] {
            let rows = reclist::generate_single(set, per_row).expect("生成できる");
            assert!(!rows.is_empty());
            for row in &rows {
                let readings: Vec<&str> = row.units.iter().map(|u| u.kana).collect();
                phoneme::phonemes_for_all(&readings)
                    .unwrap_or_else(|e| panic!("{readings:?} が引けない: {}", e.kind()));
            }
        }
    }
}

/// **生成された行すべてで、モーラごとの境界が取れる**（`DEC-ALN-013`）。
///
/// **既定の 5 と上限の 8 を両方通す。** 1行1モーラのときも同じ口で通ること。
#[test]
fn 生成した行すべてでモーラごとの境界が取れる() {
    for set in [UnitSet::Core, UnitSet::Extended] {
        for per_row in [1, 5, reclist::MAX_UNITS_PER_ROW] {
            let rows = reclist::generate_single(set, per_row).expect("生成できる");
            for row in &rows {
                let readings: Vec<&str> = row.units.iter().map(|u| u.kana).collect();
                let a = alignment(slots_for(&readings));
                let per = Boundaries::per_mora(&a, &readings).unwrap_or_else(|| {
                    panic!("{readings:?}（{} 区間）で境界が取れない", a.segments.len())
                });
                assert_eq!(
                    per.len(),
                    readings.len(),
                    "{readings:?} でモーラ数と境界の数が合わない"
                );
                // **境界が単調で、区間が重ならない。**
                for w in per.windows(2) {
                    assert!(w[1].voice_start_ms >= w[0].vowel_end_ms);
                }
                for b in &per {
                    assert!(b.vowel_start_ms >= b.voice_start_ms);
                    assert!(b.vowel_end_ms >= b.vowel_start_ms);
                }
            }
        }
    }
}

/// **1行に複数モーラが入る。** ここが 1 なら `DEC-ALN-013` の前提が崩れている。
#[test]
fn 既定の行は複数モーラを含む() {
    let rows = reclist::generate_single(UnitSet::Core, 5).expect("生成できる");
    let multi = rows.iter().filter(|r| r.units.len() > 1).count();
    assert!(
        multi > rows.len() / 2,
        "複数モーラの行が {multi} / {} しかない",
        rows.len()
    );
    // 中核 102 単位が 5 ずつなら約 21 行。
    assert!(rows.len() < 30, "行が {} と多すぎる", rows.len());
}

/// **1行あたりのモーラ数が増えても、区間の数が読みから決まる。**
///
/// ここが崩れると、行の長さによって境界の取り出しが黙って失敗する。
#[test]
fn 行の長さによらず区間の数が読みから決まる() {
    for per_row in 1..=reclist::MAX_UNITS_PER_ROW {
        let rows = reclist::generate_single(UnitSet::Core, per_row).expect("生成できる");
        let row = rows.first().expect("1行はある");
        let readings: Vec<&str> = row.units.iter().map(|u| u.kana).collect();
        let want: usize = readings
            .iter()
            .map(|r| phoneme::phonemes_for(r).expect("引ける").len())
            .sum::<usize>()
            + 2;
        assert_eq!(slots_for(&readings), want);
        assert!(Boundaries::per_mora(&alignment(want), &readings).is_some());
        // **1つずれたら受けない。** 黙って先頭から詰めない。
        assert!(Boundaries::per_mora(&alignment(want + 1), &readings).is_none());
        assert!(Boundaries::per_mora(&alignment(want - 1), &readings).is_none());
    }
}
