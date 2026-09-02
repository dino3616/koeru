//! oto の5値を境界から導く（`TR-ALN-13`〜`18`）と、その規約（`TR-ALN-23`）。
//!
//! **データ型 [`koeru_core::oto::Oto`] は `koeru-core` にある**（`DEC-ALN-009`）。
//! 5値はプロジェクトのデータで DB を正とし（`TR-PKG-40`）、制約（`TR-EDT-43`）は
//! 原音設定エディタも使う。**ここが持つのは導出と規約だけ。**
//!
//! # 導出は三分法で分ける（`TR-ALN-13`）
//!
//! - **機械導出群** — オフセット / 先行発声 / 右ブランク。境界から直接
//! - **派生規約群** — オーバーラップ。機械導出群からの比率
//! - **混合群** — 子音部。単独音・CV では母音定常区間の推定を含む
//!
//! **アライメントを変えずに規約だけ変えられる形にしてある。** 混ぜると、
//! マージンを 20ms から 25ms にするだけで推論をやり直すことになる。

use koeru_core::oto::Oto;

use crate::preset::{ConsonantClass, Preset};

/// 単独音・CV の5値を、境界から導く。
///
/// **三分法で分ける**（`TR-ALN-13`）。
/// - 機械導出群: オフセット / 先行発声 / 右ブランク — 境界から導く
/// - 派生規約群: オーバーラップ — 機械導出群からの比率
/// - 混合群: 子音部 — 単独音・CV では母音定常区間の推定を含むので機械導出群と同じ扱い
///
/// `voice_start_ms` は発声開始、`vowel_start_ms` は子音から母音への境界、
/// `vowel_end_ms` は母音の定常区間終端。母音始まりなら `voice_start` と `vowel_start` は同じ。
#[must_use]
pub fn derive_cv(
    voice_start_ms: f64,
    vowel_start_ms: f64,
    vowel_end_ms: f64,
    file_len_ms: f64,
    preset: &Preset,
    class: ConsonantClass,
) -> Oto {
    let c = preset.coefficients(class);
    // 【機械導出】オフセット = 発声開始 − 前余白マージン。**0 未満はクリップ**（TR-ALN-14）。
    let offset_ms = (voice_start_ms - preset.leading_margin_ms).max(0.0);

    // 【機械導出】先行発声 = 母音開始のオフセットからの相対。**常に 0 以上**（TR-ALN-15）。
    let preutterance_ms = (vowel_start_ms - offset_ms).max(0.0);

    // 【混合群】子音部 = 先行発声 + 母音定常マージン（TR-ALN-17）。
    // **常に 0 以上、かつ先行発声より右。**
    let consonant_ms = preutterance_ms + c.vowel_steady_margin_ms;

    // 【派生規約】オーバーラップ = オフセットから先行発声までの区間に比を掛ける（TR-ALN-16）。
    // **比は子音クラスごと**（TR-ALN-17 の子音クラス別係数）。無声破裂音は 0——
    // 前の音と重ねると破裂が濁る。
    let overlap_ms = preutterance_ms * c.overlap_ratio;

    // 【機械導出】右ブランク = 母音定常区間終端から。**負値表現が既定**（TR-ALN-18）。
    let usable = (vowel_end_ms - offset_ms)
        .max(0.0)
        .min(file_len_ms - offset_ms);
    let cutoff_ms = -usable;

    Oto {
        offset_ms,
        consonant_ms,
        cutoff_ms,
        preutterance_ms,
        overlap_ms,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_core::alias::Method;

    #[test]
    fn 単独音の五値を導ける() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        // 発声 100ms、母音 150ms、母音終端 600ms、ファイル長 1000ms
        let o = derive_cv(100.0, 150.0, 600.0, 1000.0, &p, ConsonantClass::None);
        assert_eq!(o.offset_ms, 80.0, "発声開始 − 前余白 20ms");
        assert_eq!(o.preutterance_ms, 70.0, "母音開始 150 − オフセット 80");
        assert_eq!(o.consonant_ms, 100.0, "先行発声 70 + 母音定常マージン 30");
        assert!((o.overlap_ms - 70.0 / 3.0).abs() < 1e-9, "先行発声の 1/3");
        assert_eq!(
            o.cutoff_ms, -520.0,
            "母音終端 600 − オフセット 80 の負値表現"
        );
        assert!(o.violations(1000.0).is_empty(), "違反なし");
    }

    /// **無声破裂音ではオーバーラップを 0 にする**（TR-ALN-16）。
    #[test]
    fn 無声破裂音はオーバーラップを取らない() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        let o = derive_cv(
            100.0,
            150.0,
            600.0,
            1000.0,
            &p,
            ConsonantClass::UnvoicedPlosive,
        );
        assert_eq!(o.overlap_ms, 0.0);
    }

    /// **ファイル先頭で余白が取れない場合は 0 にクリップする**（TR-ALN-14）。
    #[test]
    fn 先頭では余白を取らずにゼロへ倒す() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        let o = derive_cv(5.0, 30.0, 400.0, 1000.0, &p, ConsonantClass::None);
        assert_eq!(o.offset_ms, 0.0, "5 − 20 は負なので 0");
        assert_eq!(o.preutterance_ms, 30.0);
        assert!(o.violations(1000.0).is_empty());
    }

    /// **母音始まりでは先行発声が発声開始と同じ位置になる。**
    #[test]
    fn 母音始まりでも導ける() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        let o = derive_cv(100.0, 100.0, 500.0, 1000.0, &p, ConsonantClass::None);
        assert_eq!(o.offset_ms, 80.0);
        assert_eq!(o.preutterance_ms, 20.0, "前余白ぶんだけ右");
    }
}
