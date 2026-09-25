//! 入力レベルの校正（`TR-REC-14`, `TR-REC-15`）。
//!
//! 目的は「破綻の防止」ではなく「初期値の妥当化」に限る。
//! 校正しても収録中の破綻は防げない。ここが担うのは、最初のひと声を録る前に、
//! 明らかに小さすぎる／大きすぎる状態を外すことだけ。
//!
//! 関門にしない（`TR-REC-14`）。収束しなくても収録に進める。
//! 3時間の収録の前に、レベル合わせで止められる方がよほど困る。
//!
//! ここは純粋な計算だけを持つ。OS のゲイン API は `koeru-audio` が持つ。

/// 目標範囲の下限（dBFS）。16 bit のヘッドルームより 32 bit float の余裕を優先する。
pub const TARGET_MIN_DBFS: f64 = -12.0;

/// 目標範囲の上限（dBFS）。
pub const TARGET_MAX_DBFS: f64 = -6.0;

/// 校正に使う発声の長さ（秒）。そのプロジェクトで最も高い音高の全力発声。
pub const UTTERANCE_SECONDS: std::ops::RangeInclusive<f64> = 3.0..=5.0;

/// 測定の上限回数（`TR-REC-14`）。2回で収束させ、収束しなくても進む。
pub const MAX_ATTEMPTS: u32 = 2;

/// 校正の結果。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {
    /// 範囲に入った。
    Settled,
    /// 範囲外なので、ゲインをこの値へ動かしてもう一度測る。
    Adjust {
        /// 次に設定するゲイン（0.0〜1.0）。
        next_gain: f32,
    },
    /// 範囲外だが、これ以上は動かせない。
    ///
    /// ここでも収録には進める（`TR-REC-14`）。関門にしない。
    GaveUp {
        /// なぜ動かせないか。
        reason: GaveUp,
    },
}

/// 動かせない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GaveUp {
    /// 測定の上限に達した。
    OutOfAttempts,
    /// ゲインが端に張り付いていて、これ以上動かせない。
    AtLimit,
    /// ゲインを読み書きできない。OS 設定での調整を1回だけ案内する。
    NoControl,
    /// 発声が無かった（無音）。
    Silent,
}

impl GaveUp {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::OutOfAttempts => "out_of_attempts",
            Self::AtLimit => "at_limit",
            Self::NoControl => "no_control",
            Self::Silent => "silent",
        }
    }
}

/// 1回の測定から、次にどうするかを決める。
///
/// `peak_dbfs` は測った区間のサンプルピーク。`current_gain` は 0.0〜1.0、
/// 読み書きできなければ `None`。`attempt` は1から数えた回数。
///
/// ゲインは dB ではなくスカラで持つ（CoreAudio も PipeWire もそう）。
/// 目標との差を dB で出し、それを線形の倍率に直して掛ける。
#[must_use]
pub fn step(peak_dbfs: f64, current_gain: Option<f32>, attempt: u32) -> Outcome {
    if !peak_dbfs.is_finite() {
        return Outcome::GaveUp {
            reason: GaveUp::Silent,
        };
    }
    if (TARGET_MIN_DBFS..=TARGET_MAX_DBFS).contains(&peak_dbfs) {
        return Outcome::Settled;
    }
    let Some(gain) = current_gain else {
        return Outcome::GaveUp {
            reason: GaveUp::NoControl,
        };
    };
    if attempt >= MAX_ATTEMPTS {
        return Outcome::GaveUp {
            reason: GaveUp::OutOfAttempts,
        };
    }

    // 範囲の中央を狙う。端を狙うと、次の一声で簡単に外れる。
    let target = f64::midpoint(TARGET_MIN_DBFS, TARGET_MAX_DBFS);
    let delta_db = target - peak_dbfs;
    #[allow(
        clippy::cast_possible_truncation,
        reason = "ゲインは 0.0..=1.0 の f32。clamp してから丸める"
    )]
    let next = (f64::from(gain) * 10.0_f64.powf(delta_db / 20.0)).clamp(0.0, 1.0) as f32;

    // 端に張り付いていて、動かしたい向きへ動けない。
    let stuck = (next - gain).abs() < 1e-4
        || (gain >= 1.0 - 1e-4 && delta_db > 0.0)
        || (gain <= 1e-4 && delta_db < 0.0);
    if stuck {
        return Outcome::GaveUp {
            reason: GaveUp::AtLimit,
        };
    }

    Outcome::Adjust { next_gain: next }
}

/// 校正の記録（`TR-REC-15`）。プロジェクトに保存して、次の収録で突き合わせる。
#[derive(Debug, Clone, PartialEq)]
pub struct Calibration {
    /// 校正で決めたゲイン（0.0〜1.0）。読み書きできなければ `None`。
    pub gain: Option<f32>,
    /// ゲインをどう扱えたか（`hardware` / `software` / `unavailable`）。
    pub control: String,
    /// 最後に測ったピーク（dBFS）。
    pub peak_dbfs: f64,
    /// 範囲に入ったか。入らなくても収録には進める。
    pub settled: bool,
    /// どのデバイスで校正したか（永続識別子）。
    pub device_id: String,
    /// モノラルの元にするチャンネル（`TR-REC-06`）。-1 は混ぜる。
    pub source_channel: i32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 範囲に入っていれば終わり() {
        assert_eq!(step(-9.0, Some(0.5), 1), Outcome::Settled);
        assert_eq!(step(TARGET_MIN_DBFS, Some(0.5), 1), Outcome::Settled);
        assert_eq!(step(TARGET_MAX_DBFS, Some(0.5), 1), Outcome::Settled);
    }

    #[test]
    fn 小さすぎればゲインを上げる() {
        let Outcome::Adjust { next_gain } = step(-24.0, Some(0.3), 1) else {
            panic!("上げること");
        };
        assert!(next_gain > 0.3, "{next_gain}");
    }

    #[test]
    fn 大きすぎればゲインを下げる() {
        let Outcome::Adjust { next_gain } = step(-1.0, Some(0.8), 1) else {
            panic!("下げること");
        };
        assert!(next_gain < 0.8, "{next_gain}");
    }

    /// 範囲の中央を狙う。 端を狙うと次の一声で簡単に外れる。
    #[test]
    fn 中央を狙って一度で入る() {
        // -18 dBFS のとき、-9 へ持っていきたい → 倍率は約 2.82。
        let Outcome::Adjust { next_gain } = step(-18.0, Some(0.2), 1) else {
            panic!("調整すること");
        };
        // 掛けた倍率で測り直したら中央になるはず。
        let new_peak = -18.0 + 20.0 * f64::from(next_gain / 0.2).log10();
        assert!((new_peak + 9.0).abs() < 0.5, "{new_peak}");
    }

    /// 収束しなくても進める（`TR-REC-14`）。関門にしない。
    #[test]
    fn 二回で諦める() {
        assert_eq!(
            step(-30.0, Some(0.5), MAX_ATTEMPTS),
            Outcome::GaveUp {
                reason: GaveUp::OutOfAttempts
            }
        );
    }

    #[test]
    fn 端に張り付いていたら諦める() {
        assert_eq!(
            step(-30.0, Some(1.0), 1),
            Outcome::GaveUp {
                reason: GaveUp::AtLimit
            },
            "これ以上上げられない"
        );
        assert_eq!(
            step(-1.0, Some(0.0), 1),
            Outcome::GaveUp {
                reason: GaveUp::AtLimit
            },
            "これ以上下げられない"
        );
    }

    /// 読み書きできないデバイスでは自動調整しない（`TR-REC-14`）。
    #[test]
    fn ゲインを触れなければ諦める() {
        assert_eq!(
            step(-30.0, None, 1),
            Outcome::GaveUp {
                reason: GaveUp::NoControl
            }
        );
        // ただし範囲に入っていれば、触れなくても終わり。
        assert_eq!(step(-9.0, None, 1), Outcome::Settled);
    }

    #[test]
    fn 無音は校正できない() {
        assert_eq!(
            step(f64::NEG_INFINITY, Some(0.5), 1),
            Outcome::GaveUp {
                reason: GaveUp::Silent
            }
        );
    }

    #[test]
    fn 目標範囲は要件どおり() {
        assert!((TARGET_MIN_DBFS - -12.0).abs() < f64::EPSILON);
        assert!((TARGET_MAX_DBFS - -6.0).abs() < f64::EPSILON);
        assert_eq!(MAX_ATTEMPTS, 2);
    }
}
