//! oto の5値。
//!
//! **データ型はここに置く**（`DEC-ALN-009`）。導出（`TR-ALN-13`〜`18`）と
//! 規約プリセット（`TR-ALN-23`）は `koeru-align` が持つ。5値は DB を正とする
//! プロジェクトのデータで（`TR-PKG-40`）、制約（`TR-EDT-43`）は
//! 原音設定エディタも使うため、両方の下に置く。
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

#[cfg(test)]
mod tests {
    use super::*;

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
