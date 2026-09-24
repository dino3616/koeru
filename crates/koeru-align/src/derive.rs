//! oto の5値を境界から導く（`TR-ALN-13`〜`18`）と、その規約（`TR-ALN-23`）。
//!
//! データ型 [`koeru_core::oto::Oto`] は `koeru-core` にある（`DEC-ALN-009`）。
//! 5値はプロジェクトのデータで DB を正とし（`TR-PKG-40`）、制約（`TR-EDT-43`）は
//! 原音設定エディタも使う。ここが持つのは導出と規約だけ。
//!
//! # 導出は三分法で分ける（`TR-ALN-13`）
//!
//! - 機械導出群 — オフセット / 先行発声 / 右ブランク。境界から直接
//! - 派生規約群 — オーバーラップ。機械導出群からの比率
//! - 混合群 — 子音部。単独音・CV では母音定常区間の推定を含む
//!
//! アライメントを変えずに規約だけ変えられる形にしてある。 混ぜると、
//! マージンを 20ms から 25ms にするだけで推論をやり直すことになる。

use koeru_core::inventory::Unit;
use koeru_core::oto::{Boundary, Oto};
use koeru_core::reclist::Slot;

use crate::preset::{ConsonantClass, Preset};

/// 単独音・CV の5値を、境界から導く。
///
/// 三分法で分ける（`TR-ALN-13`）。
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
    // 【機械導出】オフセット = 発声開始 − 前余白マージン。0 未満はクリップ（`TR-ALN-14`）。
    let offset_ms = (voice_start_ms - preset.leading_margin_ms).max(0.0);

    // 【機械導出】先行発声 = 母音開始のオフセットからの相対。常に 0 以上（`TR-ALN-15`）。
    let preutterance_ms = (vowel_start_ms - offset_ms).max(0.0);

    // 【混合群】子音部 = 先行発声 + 母音定常マージン（`TR-ALN-17`）。
    // 常に 0 以上、かつ先行発声より右。
    let consonant_ms = preutterance_ms + c.vowel_steady_margin_ms;

    // 【派生規約】オーバーラップ = オフセットから先行発声までの区間に比を掛ける（`TR-ALN-16`）。
    // 比は子音クラスごと（`TR-ALN-17` の子音クラス別係数）。無声破裂音は 0——
    // 前の音と重ねると破裂が濁る。
    let overlap_ms = preutterance_ms * c.overlap_ratio;

    // 【機械導出】右ブランク = 母音定常区間終端から。負値表現が既定（`TR-ALN-18`）。
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

/// 保存した境界から5値を作り直す（`TR-ALN-34`）。
///
/// 入力は境界と規約プリセットだけ。 アライナを呼ばない。
///
/// 使う場面は2つ。
///
/// - 規約プリセットを編集したとき（`TR-ALN-23` の「再アライメントを要求しない」）
/// - 下位方式へ書き出すとき（`TR-PKG-24`）。対象方式のプリセットで作り直す
///
/// **値を流用しない。** 連続音の `- CV` を素の `CV` として複製すると、
/// 語頭の子音区間を持ったままの oto が単独音として配られる（`TR-RCL-21`）。
#[must_use]
pub fn rederive(b: &Boundary, file_len_ms: f64, preset: &Preset, class: ConsonantClass) -> Oto {
    derive_cv(
        b.voice_start_ms,
        b.vowel_start_ms,
        b.vowel_end_ms,
        file_len_ms,
        preset,
        class,
    )
}

/// CVVC の VC エントリの5値を、境界から導く（`TR-ALN-19`）。
///
/// VC は音符に対応しない。 前モーラの母音区間から次モーラの子音区間にまたがる
/// 範囲を1エントリとして切り出す。だから入力も CV とは別で、
/// 「前の母音がどこで終わるか」と「次の母音がどこで始まるか」を取る。
///
/// 規約は4つ（`TR-ALN-19`）。
/// - 先行発声 = 前モーラの母音の終わり
/// - 右ブランク = 次モーラの子音と母音の間
/// - 子音部 = 先行発声と右ブランクの中点
/// - 左ブランクとオーバーラップは既定では調整しない（規約値のまま）
///
/// # 無声破裂音は閉鎖の中で閉じる
///
/// `TR-ALN-17` が「無声破裂音の VC では固定範囲と右ブランクをともに閉鎖の
/// 無音部に置く」と定めている。母音の終わりから次の母音の始まりまでが閉鎖で、
/// その中点で切る。**破裂まで含めると、次の CV の子音と二重に鳴る。**
///
/// `prev_vowel_start_ms` は左ブランクの下限。 これより手前へ出すと、
/// 前モーラの子音が VC の頭に入る。
#[must_use]
pub fn derive_vc(
    prev_vowel_start_ms: f64,
    prev_vowel_end_ms: f64,
    next_vowel_start_ms: f64,
    file_len_ms: f64,
    preset: &Preset,
    class: ConsonantClass,
) -> Oto {
    let c = preset.coefficients(class);

    // 【規約】左ブランク。 前の母音を規約ぶんだけ残す。前モーラの子音へは食い込まない。
    let offset_ms = (prev_vowel_end_ms - preset.vc_vowel_context_ms).max(prev_vowel_start_ms);

    // 【機械導出】先行発声 = 前モーラの母音の終わり。
    let preutterance_ms = (prev_vowel_end_ms - offset_ms).max(0.0);

    // 【機械導出】右ブランク = 次モーラの子音と母音の間。
    // 無声破裂音だけは閉鎖の中で閉じる（`TR-ALN-17`）。
    let end_ms = if class == ConsonantClass::UnvoicedPlosive {
        prev_vowel_end_ms.midpoint(next_vowel_start_ms)
    } else {
        next_vowel_start_ms
    };
    let usable = (end_ms - offset_ms).max(0.0).min(file_len_ms - offset_ms);
    let cutoff_ms = -usable;

    // 【派生規約】子音部 = 先行発声と右ブランクの中点。
    // 無声破裂音では、右ブランクが閉鎖の中にあるので子音部も自動的にそこへ入る。
    let consonant_ms = preutterance_ms.midpoint(usable);

    // 【派生規約】オーバーラップ。VC は「先行発声の 1/3」が既定（`TR-ALN-16`）。
    let overlap_ms = preutterance_ms * c.overlap_ratio;

    Oto {
        offset_ms,
        consonant_ms,
        cutoff_ms,
        preutterance_ms,
        overlap_ms,
    }
}

/// 語尾エントリの5値（`TR-RCL-05` の V-）。
///
/// 母音が消えていく区間。 子音が無いので先行発声は 0 で、
/// 右ブランクはファイル末尾まで取る。
#[must_use]
pub fn derive_ending(vowel_start_ms: f64, file_len_ms: f64, preset: &Preset) -> Oto {
    let offset_ms = (vowel_start_ms - preset.leading_margin_ms).max(0.0);
    let usable = (file_len_ms - offset_ms).max(0.0);
    Oto {
        offset_ms,
        // 伸ばす区間が無い。 語尾は減衰そのものなので、固定範囲を置かない。
        consonant_ms: 0.0,
        cutoff_ms: -usable,
        preutterance_ms: 0.0,
        overlap_ms: 0.0,
    }
}

/// 行のエイリアスごとの5値（`TR-ALN-19`, `TR-RCL-05`, `TR-ALN-34`）。
///
/// 入力はモーラごとの境界と、行が生むエイリアスの表
/// （`koeru_core::reclist::row_entries`）。 綴りは呼び出し側が決めた表を
/// そのまま使い、ここは「どの区間から導くか」だけを見る。
///
/// **CV しか作っていなかった。** CVVC を選んでも `oto.ini` に渡りも語尾も
/// 入らず、受け取った側は CV だけで繋ぐことになる。連続音では綴りが仮名の
/// ままになり、`a か` を1つも引けない音源が出ていた。**踏んだ。**
///
/// 境界の足りないモーラを指す枠は落とす。 部分的な5値を作るより、
/// そのエイリアスを出さないほうが分かりやすい——被覆の判定が拾う。
#[must_use]
pub fn derive_row(
    entries: &[(String, Slot)],
    boundaries: &[Boundary],
    units: &[Unit],
    file_len_ms: f64,
    preset: &Preset,
) -> Vec<(String, Oto)> {
    let class = |i: usize| {
        units
            .get(i)
            .map_or(ConsonantClass::None, |u| ConsonantClass::of(u.consonant))
    };
    entries
        .iter()
        .filter_map(|(alias, slot)| {
            let oto = match *slot {
                Slot::Cv { mora } => {
                    rederive(boundaries.get(mora)?, file_len_ms, preset, class(mora))
                }
                // 渡りは前後2モーラのあいだ。 子音は入っていく側のもの。
                Slot::Vc { prev, next } => {
                    let (a, b) = (boundaries.get(prev)?, boundaries.get(next)?);
                    derive_vc(
                        a.vowel_start_ms,
                        a.vowel_end_ms,
                        b.vowel_start_ms,
                        file_len_ms,
                        preset,
                        class(next),
                    )
                }
                Slot::Ending { mora } => {
                    derive_ending(boundaries.get(mora)?.vowel_start_ms, file_len_ms, preset)
                }
            };
            Some((alias.clone(), oto))
        })
        .collect()
}

/// 無声破裂音の分岐が閉鎖の無音の上に乗っていない綴り（`TR-ALN-16`, `TR-ALN-17`, `DEC-ALN-018`）。
///
/// 分岐（オーバーラップ 0、VC の終わりを閉鎖の中に置く）は「そこが無音だ」という
/// 前提の上に立つ。境界の推定がずれていたり、息が漏れていたりすると、素材の
/// 終わりが声の途中に来る。ここが返したものに分岐不一致の印を付け、
/// 確認キューの先頭へ回す。
///
/// 見るのは直前の母音を同じ WAV の中に持つエントリだけ。 単独音の CV と、
/// 行頭の語頭形は比べる相手が無いので見ない——**見ないものは印も付かないが、
/// 確信度の対象から外しもしない**（`TR-ALN-16` の「確信度対象外にしてよいのは、
/// この事後検証に通った分岐だけとする」）。単独音の行の中の次のモーラは、
/// 1音ずつ間をあけて読む（`TR-RCL-03`）ので直前の母音ではない。
#[must_use]
pub fn closure_mismatches(
    entries: &[(String, Slot)],
    boundaries: &[Boundary],
    units: &[Unit],
    samples: &[f64],
    rate_hz: u32,
    method: koeru_core::alias::Method,
    preset: &Preset,
) -> std::collections::BTreeSet<String> {
    if method == koeru_core::alias::Method::Single {
        return std::collections::BTreeSet::new();
    }
    let plosive = |i: usize| {
        units
            .get(i)
            .is_some_and(|u| ConsonantClass::of(u.consonant) == ConsonantClass::UnvoicedPlosive)
    };
    entries
        .iter()
        .filter_map(|(alias, slot)| {
            // 閉鎖を挟む2つのモーラ。 入っていく側が無声破裂音のときだけ分岐する。
            let (prev, next) = match *slot {
                Slot::Cv { mora } if mora > 0 => (mora - 1, mora),
                Slot::Vc { prev, next } => (prev, next),
                Slot::Cv { .. } | Slot::Ending { .. } => return None,
            };
            if !plosive(next) {
                return None;
            }
            let holds = match (boundaries.get(prev), boundaries.get(next)) {
                (Some(a), Some(b)) => {
                    closure_holds(samples, rate_hz, a, b, preset.plosive_closure_drop_db)
                }
                // 境界が無ければ閉鎖も確かめられない。
                _ => false,
            };
            (!holds).then(|| alias.clone())
        })
        .collect()
}

/// 前のモーラの母音と次のモーラの母音のあいだに、閉鎖の無音があるか（`DEC-ALN-018`）。
///
/// 閉鎖の区間は、前の母音の終わりから次の母音の始まりまでの**前半**。 後半には
/// 破裂と気息が入る。VC の終わりもこの中点に置いている（[`derive_vc`]）。
/// 閉鎖の RMS が前の母音の RMS より `drop_db` 以上低ければ通る。
///
/// 区間が1標本も無ければ通さない。 閉鎖が見つからないということなので。
#[must_use]
pub fn closure_holds(
    samples: &[f64],
    rate_hz: u32,
    prev: &Boundary,
    next: &Boundary,
    drop_db: f64,
) -> bool {
    let closure_end = prev.vowel_end_ms.midpoint(next.vowel_start_ms);
    let (Some(vowel), Some(closure)) = (
        rms(samples, rate_hz, prev.vowel_start_ms, prev.vowel_end_ms),
        rms(samples, rate_hz, prev.vowel_end_ms, closure_end),
    ) else {
        return false;
    };
    // 対数を取らずに比で比べる。 無音（0）の閉鎖で `log10(0)` を踏まない。
    vowel > 0.0 && closure <= vowel * 10_f64.powf(-drop_db / 20.0)
}

/// 区間の RMS。 区間が1標本も無ければ `None`。
fn rms(samples: &[f64], rate_hz: u32, from_ms: f64, to_ms: f64) -> Option<f64> {
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "位置はミリ秒。標本数へ落として範囲に丸める"
    )]
    let cut = |ms: f64| ((ms / 1000.0 * f64::from(rate_hz)).max(0.0) as usize).min(samples.len());
    let (a, b) = (cut(from_ms), cut(to_ms));
    let part = samples.get(a..b).filter(|p| !p.is_empty())?;
    #[allow(clippy::cast_precision_loss, reason = "区間の標本数は 2^53 に届かない")]
    let n = part.len() as f64;
    Some((part.iter().map(|x| x * x).sum::<f64>() / n).sqrt())
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

    /// 無声破裂音ではオーバーラップを 0 にする（`TR-ALN-16`）。
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

    /// ファイル先頭で余白が取れない場合は 0 にクリップする（`TR-ALN-14`）。
    #[test]
    fn 先頭では余白を取らずにゼロへ倒す() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        let o = derive_cv(5.0, 30.0, 400.0, 1000.0, &p, ConsonantClass::None);
        assert_eq!(o.offset_ms, 0.0, "5 − 20 は負なので 0");
        assert_eq!(o.preutterance_ms, 30.0);
        assert!(o.violations(1000.0).is_empty());
    }

    /// 母音始まりでは先行発声が発声開始と同じ位置になる。
    #[test]
    fn 母音始まりでも導ける() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        let o = derive_cv(100.0, 100.0, 500.0, 1000.0, &p, ConsonantClass::None);
        assert_eq!(o.offset_ms, 80.0);
        assert_eq!(o.preutterance_ms, 20.0, "前余白ぶんだけ右");
    }
}

#[cfg(test)]
mod vc_tests {
    use super::*;
    use koeru_core::alias::Method;

    fn cvvc() -> Preset {
        Preset::default_for(Method::Cvvc).expect("既定がある")
    }

    /// 先行発声は前モーラの母音の終わり（`TR-ALN-19` (a)）。
    #[test]
    fn 先行発声は前の母音の終わり() {
        let p = cvvc();
        // 前の母音 100〜400ms、次の母音 500ms から。
        let o = derive_vc(100.0, 400.0, 500.0, 1000.0, &p, ConsonantClass::Nasal);
        assert!(
            (o.offset_ms - (400.0 - p.vc_vowel_context_ms)).abs() < 1e-9,
            "左ブランクは規約ぶん手前: {}",
            o.offset_ms
        );
        assert!(
            (o.preutterance_ms - p.vc_vowel_context_ms).abs() < 1e-9,
            "先行発声が母音の終わりを指す: {}",
            o.preutterance_ms
        );
    }

    /// 左ブランクは前モーラの子音へ食い込まない（`TR-ALN-19`）。
    #[test]
    fn 左ブランクは前の母音より手前へ出ない() {
        let p = cvvc();
        // 母音が規約より短い（380〜400ms の 20ms しかない）。
        let o = derive_vc(380.0, 400.0, 500.0, 1000.0, &p, ConsonantClass::Nasal);
        assert!((o.offset_ms - 380.0).abs() < 1e-9, "{}", o.offset_ms);
        assert!((o.preutterance_ms - 20.0).abs() < 1e-9);
    }

    /// 右ブランクは次の子音と母音の間（`TR-ALN-19` (b)）。
    #[test]
    fn 右ブランクは次の母音の始まり() {
        let o = derive_vc(100.0, 400.0, 500.0, 1000.0, &cvvc(), ConsonantClass::Nasal);
        let end = o.offset_ms - o.cutoff_ms;
        assert!((end - 500.0).abs() < 1e-9, "{end}");
        assert!(o.cutoff_ms < 0.0, "負値表現が既定（`TR-ALN-18`）");
    }

    /// 子音部は先行発声と右ブランクの中点（`TR-ALN-19` (c)）。
    #[test]
    fn 子音部は中点() {
        let o = derive_vc(100.0, 400.0, 500.0, 1000.0, &cvvc(), ConsonantClass::Nasal);
        let usable = -o.cutoff_ms;
        assert!(
            (o.consonant_ms - (o.preutterance_ms + usable) / 2.0).abs() < 1e-9,
            "{}",
            o.consonant_ms
        );
        assert!(o.consonant_ms > o.preutterance_ms, "先行発声より右");
    }

    /// 無声破裂音は閉鎖の中で閉じる（`TR-ALN-17`）。
    ///
    /// 破裂まで含めると、次の CV の子音と二重に鳴る。
    #[test]
    fn 無声破裂音の_vc_は閉鎖で切る() {
        let p = cvvc();
        let plosive = derive_vc(
            100.0,
            400.0,
            500.0,
            1000.0,
            &p,
            ConsonantClass::UnvoicedPlosive,
        );
        let voiced = derive_vc(100.0, 400.0, 500.0, 1000.0, &p, ConsonantClass::Nasal);
        let plosive_end = plosive.offset_ms - plosive.cutoff_ms;
        assert!(
            (plosive_end - 450.0).abs() < 1e-9,
            "閉鎖の中点: {plosive_end}"
        );
        assert!(-plosive.cutoff_ms < -voiced.cutoff_ms, "有声より手前で切る");
        // 固定範囲も閉鎖の中に入る。
        assert!(plosive.offset_ms + plosive.consonant_ms <= plosive_end + 1e-9);
    }

    /// 無声破裂音はオーバーラップを取らない（`TR-ALN-16`）。
    #[test]
    fn 無声破裂音の_vc_は重ねない() {
        let o = derive_vc(
            100.0,
            400.0,
            500.0,
            1000.0,
            &cvvc(),
            ConsonantClass::UnvoicedPlosive,
        );
        assert!((o.overlap_ms - 0.0).abs() < 1e-9);
    }

    /// オーバーラップは先行発声の 1/3（`TR-ALN-16`）。
    #[test]
    fn オーバーラップは先行発声の三分の一() {
        let o = derive_vc(100.0, 400.0, 500.0, 1000.0, &cvvc(), ConsonantClass::Nasal);
        assert!((o.overlap_ms - o.preutterance_ms / 3.0).abs() < 1e-9);
    }

    /// 語尾は減衰そのもの。固定範囲を置かない（`TR-RCL-05`）。
    #[test]
    fn 語尾は末尾まで取る() {
        let p = cvvc();
        let o = derive_ending(800.0, 1000.0, &p);
        assert!((o.offset_ms - (800.0 - p.leading_margin_ms)).abs() < 1e-9);
        assert!((o.preutterance_ms - 0.0).abs() < 1e-9);
        assert!((o.consonant_ms - 0.0).abs() < 1e-9);
        assert!(
            (o.offset_ms - o.cutoff_ms - 1000.0).abs() < 1e-9,
            "末尾まで"
        );
    }

    /// ファイルの外へは出ない。
    #[test]
    fn ファイル末尾を越えない() {
        let o = derive_vc(100.0, 400.0, 900.0, 600.0, &cvvc(), ConsonantClass::Nasal);
        assert!(o.offset_ms - o.cutoff_ms <= 600.0 + 1e-9);
    }
}

#[cfg(test)]
mod rederive_tests {
    use super::*;
    use koeru_core::alias::Method;

    /// 規約プリセットを変えると値が変わる。境界は変わらない（`TR-ALN-23`）。
    #[test]
    fn プリセットを変えても再アライメントは要らない() {
        let b = Boundary {
            voice_start_ms: 100.0,
            vowel_start_ms: 150.0,
            vowel_end_ms: 600.0,
        };
        let mut p = Preset::default_for(Method::Single).expect("既定がある");
        let before = rederive(&b, 1000.0, &p, ConsonantClass::Nasal);
        p.leading_margin_ms = 40.0;
        let after = rederive(&b, 1000.0, &p, ConsonantClass::Nasal);
        assert!(
            (before.offset_ms - after.offset_ms).abs() > 1e-9,
            "余白を変えたら左ブランクが動く"
        );
        // 境界そのものは入力のまま。アライナを呼んでいない。
        assert!((after.offset_ms - (100.0 - 40.0)).abs() < 1e-9);
    }

    /// 下位方式のプリセットで作り直すと、その方式の値になる（`TR-PKG-24`）。
    #[test]
    fn 下位方式のプリセットで作り直せる() {
        let b = Boundary {
            voice_start_ms: 100.0,
            vowel_start_ms: 150.0,
            vowel_end_ms: 600.0,
        };
        let seq = Preset::default_for(Method::Sequential).expect("既定がある");
        let single = Preset::default_for(Method::Single).expect("既定がある");
        let a = rederive(&b, 1000.0, &seq, ConsonantClass::UnvoicedPlosive);
        let c = rederive(&b, 1000.0, &single, ConsonantClass::UnvoicedPlosive);
        // どちらも同じ境界から出るので、規約が同じ値なら一致する。
        // 違うのは規約であって、境界ではない。
        assert!((a.preutterance_ms - c.preutterance_ms).abs() < 1e-9);
        assert_eq!(a.overlap_ms, 0.0, "無声破裂音は重ねない（`TR-ALN-16`）");
    }

    fn cvvc_preset() -> Preset {
        Preset::default_for(Method::Cvvc).expect("既定がある")
    }

    /// 1 kHz で「あか」。母音 0〜100ms、閉鎖 100〜150ms、破裂 150〜200ms、母音 200〜400ms。
    /// 閉鎖の振幅だけを変える。
    fn aka(closure: f64) -> (Vec<f64>, Vec<Boundary>, Vec<Unit>) {
        let wave = |amp: f64, n: usize| (0..n).map(move |i| if i % 2 == 0 { amp } else { -amp });
        let samples: Vec<f64> = wave(0.5, 100)
            .chain(wave(closure, 50))
            .chain(wave(0.5, 250))
            .collect();
        let bounds = vec![
            Boundary {
                voice_start_ms: 0.0,
                vowel_start_ms: 0.0,
                vowel_end_ms: 100.0,
            },
            Boundary {
                voice_start_ms: 150.0,
                vowel_start_ms: 200.0,
                vowel_end_ms: 400.0,
            },
        ];
        let table = koeru_core::inventory::units(koeru_core::inventory::UnitSet::Core);
        let line = ["あ", "か"]
            .iter()
            .filter_map(|k| table.iter().find(|u| u.kana == *k).cloned())
            .collect();
        (samples, bounds, line)
    }

    /// 閉鎖が直前の母音より十分低ければ印を付けない（`DEC-ALN-018`）。
    #[test]
    fn 閉鎖が無音なら分岐は通る() {
        let (samples, bounds, line) = aka(0.001);
        let entries = vec![
            ("a k".to_owned(), Slot::Vc { prev: 0, next: 1 }),
            ("か".to_owned(), Slot::Cv { mora: 1 }),
        ];
        let got = closure_mismatches(
            &entries,
            &bounds,
            &line,
            &samples,
            1000,
            Method::Cvvc,
            &cvvc_preset(),
        );
        assert!(got.is_empty(), "{got:?}");
    }

    /// 息が漏れて閉鎖が無音でなければ、VC にも CV にも印を付ける（`TR-ALN-16`, `TR-ALN-17`）。
    #[test]
    fn 閉鎖が無音でなければ分岐不一致() {
        // 母音より約 8 dB 低いだけ。
        let (samples, bounds, line) = aka(0.2);
        let entries = vec![
            ("- あ".to_owned(), Slot::Cv { mora: 0 }),
            ("a k".to_owned(), Slot::Vc { prev: 0, next: 1 }),
            ("か".to_owned(), Slot::Cv { mora: 1 }),
        ];
        let got = closure_mismatches(
            &entries,
            &bounds,
            &line,
            &samples,
            1000,
            Method::Cvvc,
            &cvvc_preset(),
        );
        let want: std::collections::BTreeSet<String> =
            ["a k".to_owned(), "か".to_owned()].into_iter().collect();
        assert_eq!(got, want, "母音始まりの語頭は分岐しない");
    }

    /// 単独音は直前の母音を持たないので見ない（`DEC-ALN-018`）。
    #[test]
    fn 単独音は閉鎖を見ない() {
        let (samples, bounds, line) = aka(0.2);
        let entries = vec![("か".to_owned(), Slot::Cv { mora: 1 })];
        let single = Preset::default_for(Method::Single).expect("既定がある");
        assert!(
            closure_mismatches(
                &entries,
                &bounds,
                &line,
                &samples,
                1000,
                Method::Single,
                &single
            )
            .is_empty()
        );
    }

    /// 閾値は規約プリセットが持つ（`TR-ALN-23`）。 仮置きの 20 dB（`Q-ALN-004`）。
    #[test]
    fn 閉鎖の閾値はプリセットにある() {
        for m in [Method::Sequential, Method::Cvvc] {
            let p = Preset::default_for(m).expect("既定がある");
            assert!((p.plosive_closure_drop_db - 20.0).abs() < 1e-9, "{m:?}");
        }
    }
}
