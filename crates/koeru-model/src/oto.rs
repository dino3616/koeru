//! oto の5値。
//!
//! データ型はここに置く（`DEC-ALN-009`）。導出（`TR-ALN-13`〜`18`）と
//! 規約プリセット（`TR-ALN-23`）は `koeru-align` が持つ。5値は DB を正とする
//! プロジェクトのデータで（`TR-PKG-40`）、制約（`TR-EDT-43`）は
//! 原音設定エディタも使うため、両方の下に置く。
//!
//! 絶対サンプル位置ではなくミリ秒で持つ（oto.ini の表現に合わせる）。
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
//! - オフセット（左ブランク） — 使い始める位置（`TR-ALN-14`）
//! - 先行発声 — オフセットからの相対で、子音から母音への境界（`TR-ALN-15`）
//! - オーバーラップ — 前の音と重ねる長さ。負値を許す（`TR-ALN-16`）
//! - 子音部（固定範囲） — 伸縮させない範囲（`TR-ALN-17`）
//! - 右ブランク — 使い終わる位置。負値表現を既定にする（`TR-ALN-18`）

/// アライメントが出した境界（ミリ秒、`TR-ALN-34`）。
///
/// **プロジェクトのデータなので、導く側（`koeru-align`）より下に置く**（`DEC-ALN-009` が
/// [`Oto`] で同じことをしている）。5値は境界と規約プリセットから導く派生物で、
/// 境界のほうが上流にある（`TR-ALN-13` の三分法）。
///
/// 取り出す側は `koeru-align` の `segment`。 ここが持つのは形だけ。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Boundary {
    /// 発声開始。無音の終わり。
    pub voice_start_ms: f64,
    /// 子音から母音への境界。母音始まりなら `voice_start_ms` と同じ。
    pub vowel_start_ms: f64,
    /// 母音の定常区間終端。
    pub vowel_end_ms: f64,
}

/// oto.ini の1エントリ。単位はすべてミリ秒。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oto {
    /// 左ブランク。ファイル先頭からの絶対位置。常に 0 以上（`TR-ALN-14`）。
    pub offset_ms: f64,
    /// 子音部（固定範囲）。オフセットからの相対。常に 0 以上（`TR-ALN-17`）。
    pub consonant_ms: f64,
    /// 右ブランク。負値ならオフセットからの相対、正値ならファイル末尾から
    /// （`TR-ALN-18` は負値表現を既定とする）。
    pub cutoff_ms: f64,
    /// 先行発声。オフセットからの相対。常に 0 以上（`TR-ALN-15`）。
    pub preutterance_ms: f64,
    /// オーバーラップ。オフセットからの相対。負値を許す（`TR-ALN-16`）。
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

    /// 右ブランクの位置（ファイル先頭からの絶対ミリ秒）。
    ///
    /// 符号の約束をここ1箇所に閉じる。 負値なら「オフセットからの相対」、
    /// 正値なら「ファイル末尾から」で、書き出し前検証（`TR-PKG-49`）は
    /// この位置と先行発声・オーバーラップ・子音部を比べる。
    #[must_use]
    pub fn cutoff_position_ms(&self, file_len_ms: f64) -> f64 {
        self.offset_ms + self.usable_ms(file_len_ms)
    }

    /// 制約を満たしているか（`TR-EDT-43` の11条件のうち、値そのものに関わるもの）。
    ///
    /// オーバーラップだけが負を許される。
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

/// 渡り（CVVC の VC）の最小の長さ（ティック、`TR-SYN-12`）。
///
/// OpenUtau の `JapanesePresampPhonemizer` が `Math.Max(30, ...)` で切る値。
/// **ミリ秒ではなくティック**なので、テンポで長さが変わる。
const MIN_TRANSITION_TICKS: f64 = 30.0;

/// 次の CV の oto が引けないときの渡りの長さ（ティック）。
///
/// OpenUtau の `int vcLength = 120;`。
const DEFAULT_TRANSITION_TICKS: f64 = 120.0;

/// 4分音符のティック数。
const TICKS_PER_QUARTER: f64 = 480.0;

/// 渡り（CVVC の VC）の長さ（ミリ秒、`TR-SYN-12`, `TR-SYN-11`）。
///
/// **長さは次の CV から取る。** 渡り自身の oto ではない。
/// 先行発声は「その素材が音符の頭よりどれだけ前から鳴り始めるか」で、
/// 渡りはちょうどその助走ぶんを埋めるためにある。
///
/// これは presamp の `[VCLENGTH] 0`（既定）の振る舞いで、UTAU（presamp.exe）でも
/// OpenUtau でも同じ。`[VCLENGTH] 1`（渡り自身から取る）は既定ではない。
/// `TR-SYN-11` が「OpenUtau の公開された振る舞いを仕様として参照する」と
/// 定めているので、既定のほうを採る。
///
/// ```text
/// OpenUtau.Plugin.Builtin/JapanesePresampPhonemizer.cs
///   if (nextOto.Overlap < 0) vcLength = MsToTick(nextOto.Preutter - nextOto.Overlap);
///   else                     vcLength = MsToTick(nextOto.Preutter);
///   vcLength = Min(totalDuration / 2, Max(30, vcLength));
/// ```
///
/// **オーバーラップが負なら足す。** 負のオーバーラップは「前の音と重ねずに離す」
/// 指定で、そのぶん助走が長くなる。
///
/// `owner_duration_ms` は渡りが乗る音符（直前の音符）の長さ。 その半分で頭打ちに
/// する——短い音符を渡りが食い潰さないため。
#[must_use]
pub fn transition_ms(next: Option<&Oto>, owner_duration_ms: f64, beat_ms: f64) -> f64 {
    let tick_ms = beat_ms / TICKS_PER_QUARTER;
    let want = next.map_or(DEFAULT_TRANSITION_TICKS * tick_ms, |o| {
        if o.overlap_ms < 0.0 {
            o.preutterance_ms - o.overlap_ms
        } else {
            o.preutterance_ms
        }
    });
    want.max(MIN_TRANSITION_TICKS * tick_ms)
        .min(owner_duration_ms / 2.0)
        .max(0.0)
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

    /// オーバーラップだけが負を許される（`TR-ALN-16`）。
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

    /// 渡りの長さは次の CV の先行発声から取る（`TR-SYN-12`）。
    #[test]
    fn 渡りは次の先行発声から決まる() {
        let beat = 500.0; // 120 BPM
        let next = Oto {
            offset_ms: 0.0,
            consonant_ms: 50.0,
            cutoff_ms: -300.0,
            preutterance_ms: 80.0,
            overlap_ms: 20.0,
        };
        assert!((transition_ms(Some(&next), 400.0, beat) - 80.0).abs() < 1e-9);
    }

    /// オーバーラップが負なら、そのぶん助走が長くなる。
    #[test]
    fn 負のオーバーラップは足す() {
        let beat = 500.0;
        let next = Oto {
            offset_ms: 0.0,
            consonant_ms: 50.0,
            cutoff_ms: -300.0,
            preutterance_ms: 80.0,
            overlap_ms: -30.0,
        };
        assert!((transition_ms(Some(&next), 400.0, beat) - 110.0).abs() < 1e-9);
    }

    /// 短い音符を渡りが食い潰さない。半分で頭打ち。
    #[test]
    fn 渡りは直前の音符の半分を超えない() {
        let beat = 500.0;
        let next = Oto {
            offset_ms: 0.0,
            consonant_ms: 50.0,
            cutoff_ms: -300.0,
            preutterance_ms: 300.0,
            overlap_ms: 20.0,
        };
        assert!((transition_ms(Some(&next), 100.0, beat) - 50.0).abs() < 1e-9);
    }

    /// 下限は 30 ティック。**ミリ秒ではない**ので、テンポで変わる。
    #[test]
    fn 渡りの下限はティックで効く() {
        let next = Oto {
            offset_ms: 0.0,
            consonant_ms: 5.0,
            cutoff_ms: -300.0,
            preutterance_ms: 1.0,
            overlap_ms: 5.0,
        };
        // 120 BPM: 1 ティック = 500/480 ms → 下限 31.25 ms
        let slow = transition_ms(Some(&next), 400.0, 500.0);
        assert!((slow - 30.0 * 500.0 / 480.0).abs() < 1e-9, "{slow}");
        // 240 BPM では半分になる。
        let fast = transition_ms(Some(&next), 400.0, 250.0);
        assert!((fast - slow / 2.0).abs() < 1e-9, "{fast}");
    }

    /// 次の oto を引けなければ 120 ティック（OpenUtau の既定）。
    #[test]
    fn 次を引けなければ既定の長さ() {
        let got = transition_ms(None, 400.0, 500.0);
        assert!((got - 120.0 * 500.0 / 480.0).abs() < 1e-9, "{got}");
    }
}
