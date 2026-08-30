//! oto の5値。
//!
//! **絶対サンプル位置ではなくミリ秒で持つ**（oto.ini の表現に合わせる）。
//! 内部の単一データモデルは絶対サンプル位置（`TR-EDT-01`）だが、
//! ここは resampler と oto.ini の境界なので、外の表現に揃える。
//!
//! ## 5値の意味
//!
//! ```text
//!  ファイル先頭
//!  |<- offset ->|<- overlap ->|
//!  |            |<--- preutterance --->|
//!  |            |<------ consonant ------>|
//!  |                                              |<- cutoff ->| ファイル末尾
//! ```
//!
//! - **オフセット（左ブランク）** — 使い始める位置（`TR-ALN-14`）
//! - **先行発声** — オフセットからの相対で、子音から母音への境界（`TR-ALN-15`）
//! - **オーバーラップ** — 前の音と重ねる長さ。**負値を許す**（`TR-ALN-16`）
//! - **子音部（固定範囲）** — 伸縮させない範囲（`TR-ALN-17`）
//! - **右ブランク** — 使い終わる位置。**負値表現を既定にする**（`TR-ALN-18`）

/// oto.ini の1エントリ。単位はすべてミリ秒。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oto {
    /// 左ブランク。ファイル先頭からの絶対位置。**常に 0 以上**（`TR-ALN-14`）。
    pub offset_ms: f64,
    /// 子音部（固定範囲）。オフセットからの相対。**常に 0 以上**（`TR-ALN-17`）。
    pub consonant_ms: f64,
    /// 右ブランク。**負値ならオフセットからの相対、正値ならファイル末尾から**
    /// （`TR-ALN-18` は負値表現を既定とする）。
    pub cutoff_ms: f64,
    /// 先行発声。オフセットからの相対。**常に 0 以上**（`TR-ALN-15`）。
    pub preutterance_ms: f64,
    /// オーバーラップ。オフセットからの相対。**負値を許す**（`TR-ALN-16`）。
    pub overlap_ms: f64,
}

impl Oto {
    /// 使う区間の長さ（ミリ秒）。
    ///
    /// 右ブランクが負なら「オフセットからの相対」、正なら「ファイル末尾から」。
    #[must_use]
    pub fn usable_ms(&self, file_len_ms: f64) -> f64 {
        if self.cutoff_ms <= 0.0 {
            -self.cutoff_ms
        } else {
            (file_len_ms - self.offset_ms - self.cutoff_ms).max(0.0)
        }
    }

    /// 制約を満たしているか（`TR-EDT-43` の11条件のうち、値そのものに関わるもの）。
    ///
    /// **オーバーラップだけが負を許される。**
    #[must_use]
    pub fn violations(&self, file_len_ms: f64) -> Vec<Violation> {
        let mut v = Vec::new();
        if self.offset_ms < 0.0 {
            v.push(Violation::NegativeOffset);
        }
        if self.offset_ms > file_len_ms {
            v.push(Violation::OffsetBeyondFile);
        }
        if self.preutterance_ms < 0.0 {
            v.push(Violation::NegativePreutterance);
        }
        if self.offset_ms + self.preutterance_ms > file_len_ms {
            v.push(Violation::PreutteranceBeyondFile);
        }
        if self.consonant_ms < 0.0 {
            v.push(Violation::NegativeConsonant);
        }
        if self.offset_ms + self.consonant_ms > file_len_ms {
            v.push(Violation::ConsonantBeyondFile);
        }
        if self.offset_ms + self.overlap_ms > file_len_ms {
            v.push(Violation::OverlapBeyondFile);
        }
        if self.usable_ms(file_len_ms) <= 0.0 {
            v.push(Violation::EmptyRegion);
        }
        v
    }
}

/// 制約違反の種類（`TR-EDT-43`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Violation {
    NegativeOffset,
    OffsetBeyondFile,
    NegativePreutterance,
    PreutteranceBeyondFile,
    NegativeConsonant,
    ConsonantBeyondFile,
    OverlapBeyondFile,
    /// 使える区間が無い。
    EmptyRegion,
}

impl Violation {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::NegativeOffset => "oto.negative_offset",
            Self::OffsetBeyondFile => "oto.offset_beyond_file",
            Self::NegativePreutterance => "oto.negative_preutterance",
            Self::PreutteranceBeyondFile => "oto.preutterance_beyond_file",
            Self::NegativeConsonant => "oto.negative_consonant",
            Self::ConsonantBeyondFile => "oto.consonant_beyond_file",
            Self::OverlapBeyondFile => "oto.overlap_beyond_file",
            Self::EmptyRegion => "oto.empty_region",
        }
    }
}

/// 導出の規約（`TR-ALN-23` の規約プリセット）。
///
/// **外部化して上級モードで編集できるようにする**（`TR-ALN-16`）ので、
/// 定数を直に書かず、この型に集める。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OtoPreset {
    /// オフセットの前に残す余白（`TR-ALN-14`）。
    pub leading_margin_ms: f64,
    /// 母音定常マージン。子音部の下限に効く（`TR-ALN-17`）。
    pub vowel_steady_margin_ms: f64,
    /// CV のオーバーラップ比。**オフセットから先行発声までの区間に掛ける**（`TR-ALN-16`）。
    pub cv_overlap_ratio: f64,
}

impl Default for OtoPreset {
    fn default() -> Self {
        Self {
            leading_margin_ms: 20.0,
            vowel_steady_margin_ms: 30.0,
            // **1/3 が既定**（TR-ALN-16）。
            cv_overlap_ratio: 1.0 / 3.0,
        }
    }
}

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
    preset: &OtoPreset,
    unvoiced_plosive: bool,
) -> Oto {
    // 【機械導出】オフセット = 発声開始 − 前余白マージン。**0 未満はクリップ**（TR-ALN-14）。
    let offset_ms = (voice_start_ms - preset.leading_margin_ms).max(0.0);

    // 【機械導出】先行発声 = 母音開始のオフセットからの相対。**常に 0 以上**（TR-ALN-15）。
    let preutterance_ms = (vowel_start_ms - offset_ms).max(0.0);

    // 【混合群】子音部 = 先行発声 + 母音定常マージン（TR-ALN-17）。
    // **常に 0 以上、かつ先行発声より右。**
    let consonant_ms = preutterance_ms + preset.vowel_steady_margin_ms;

    // 【派生規約】オーバーラップ = オフセットから先行発声までの 1/3（TR-ALN-16）。
    // **無声破裂音では 0。** 前の音と重ねると破裂が濁る。
    let overlap_ms = if unvoiced_plosive {
        0.0
    } else {
        preutterance_ms * preset.cv_overlap_ratio
    };

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

    #[test]
    fn 単独音の五値を導ける() {
        let p = OtoPreset::default();
        // 発声 100ms、母音 150ms、母音終端 600ms、ファイル長 1000ms
        let o = derive_cv(100.0, 150.0, 600.0, 1000.0, &p, false);
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
        let p = OtoPreset::default();
        let o = derive_cv(100.0, 150.0, 600.0, 1000.0, &p, true);
        assert_eq!(o.overlap_ms, 0.0);
    }

    /// **ファイル先頭で余白が取れない場合は 0 にクリップする**（TR-ALN-14）。
    #[test]
    fn 先頭では余白を取らずにゼロへ倒す() {
        let p = OtoPreset::default();
        let o = derive_cv(5.0, 30.0, 400.0, 1000.0, &p, false);
        assert_eq!(o.offset_ms, 0.0, "5 − 20 は負なので 0");
        assert_eq!(o.preutterance_ms, 30.0);
        assert!(o.violations(1000.0).is_empty());
    }

    /// **母音始まりでは先行発声が発声開始と同じ位置になる。**
    #[test]
    fn 母音始まりでも導ける() {
        let p = OtoPreset::default();
        let o = derive_cv(100.0, 100.0, 500.0, 1000.0, &p, false);
        assert_eq!(o.offset_ms, 80.0);
        assert_eq!(o.preutterance_ms, 20.0, "前余白ぶんだけ右");
    }

    #[test]
    fn 使える区間の長さを負値表現から求められる() {
        let o = Oto {
            offset_ms: 80.0,
            consonant_ms: 100.0,
            cutoff_ms: -520.0,
            preutterance_ms: 70.0,
            overlap_ms: 23.0,
        };
        assert_eq!(o.usable_ms(1000.0), 520.0, "負値はオフセットからの相対");
    }

    #[test]
    fn 正値表現の右ブランクも読める() {
        let o = Oto {
            offset_ms: 100.0,
            consonant_ms: 50.0,
            cutoff_ms: 200.0,
            preutterance_ms: 40.0,
            overlap_ms: 13.0,
        };
        assert_eq!(o.usable_ms(1000.0), 700.0, "1000 − 100 − 200");
    }

    /// **オーバーラップだけが負を許される**（TR-ALN-16）。
    #[test]
    fn 負のオーバーラップは違反ではない() {
        let o = Oto {
            offset_ms: 100.0,
            consonant_ms: 50.0,
            cutoff_ms: -500.0,
            preutterance_ms: 40.0,
            overlap_ms: -10.0,
        };
        assert!(o.violations(1000.0).is_empty());
    }

    #[test]
    fn 制約違反を検出できる() {
        let o = Oto {
            offset_ms: -1.0,
            consonant_ms: -1.0,
            cutoff_ms: -500.0,
            preutterance_ms: -1.0,
            overlap_ms: 0.0,
        };
        let v = o.violations(1000.0);
        assert!(v.contains(&Violation::NegativeOffset));
        assert!(v.contains(&Violation::NegativeConsonant));
        assert!(v.contains(&Violation::NegativePreutterance));
    }

    #[test]
    fn 使える区間が無いのは違反() {
        let o = Oto {
            offset_ms: 100.0,
            consonant_ms: 10.0,
            cutoff_ms: 0.0,
            preutterance_ms: 10.0,
            overlap_ms: 0.0,
        };
        assert!(o.violations(100.0).contains(&Violation::EmptyRegion));
    }
}
