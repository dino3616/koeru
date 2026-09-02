//! アライナの呼び出し口（`TR-ALN-03`）。
//!
//! **共通の形にするのは要件がそう指定しているから。**
//!
//! > いずれの実装も emission 行列（フレーム×音素の事後確率）を呼び出し側に返し、
//! > `TR-ALN-24` の確信度計算に使えること
//!
//! `DEC-REC-001`（音声 I/O に抽象レイヤを挟まない）とは事情が違う。あちらは各 OS 固有の
//! 制御を包むと出せなくなるものがあったが、**こちらは出すものが要件で揃えられている。**
//!
//! # 実装は2つ
//!
//! 一次経路は MFA の日本語音響モデル（`DEC-ALN-008`）。退避経路は
//! [`crate::segment`] の音響モデルを使わない実装（`DEC-ALN-006`）。
//!
//! **退避経路は emission 行列を持たない。** 短時間パワーとゼロ交差率で境界を出すので、
//! フレーム×音素の事後確率という概念が無い。[`Alignment::posteriors`] が `None` を返し、
//! 確信度の成分が欠けた状態として扱う（`TR-ALN-24` の成分 (1) 経路確信度、
//! `TR-ALN-26` (4) の次善候補が出せない）。**黙って 0 を入れない。**

use crate::phoneme::Phoneme;

/// アライナへ渡す1テイク。
///
/// **音素列は呼び出し側が確定させてから渡す**（`TR-ALN-07`）。ここで g2p はしない。
/// 録音リストが「表示テキスト」「エイリアス列」「音素列」「モーラ境界」を
/// 明示的に持つデータとして同梱されており、そこから直接構成する。
#[derive(Debug, Clone)]
pub struct AlignRequest<'a> {
    /// 44100 Hz / モノラルのサンプル列。
    pub samples: &'a [f64],
    /// サンプリング周波数（Hz）。
    pub sample_rate_hz: u32,
    /// 発話内容として固定する音素列（`TR-ALN-09` の線形鎖）。
    ///
    /// **前後の無音は含めない。** 実装側が `pau` を足す。
    pub phonemes: &'a [Phoneme],
    /// 収録グリッド由来の事前分布（`TR-ALN-08`）。無ければ `None`。
    pub grid: Option<Grid>,
}

/// 収録グリッド（`TR-ALN-08`）。
///
/// **これはソフトな事前分布で、探索空間を制限しない。**
/// グリッドを無視した解が音響的に十分に優勢なら、そちらが選ばれる。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// 想定する発声開始位置（ミリ秒）。
    pub expected_onset_ms: f64,
    /// 想定モーラ数。
    pub expected_moras: u32,
    /// 収録テンポ（BPM）。ガイドを使わない方式では `None`。
    pub tempo_bpm: Option<f64>,
}

/// フレーム×音素の事後確率（`TR-ALN-03`）。
///
/// **行がフレーム、列が音素。** `frames * phonemes` の長さを持つ行優先の平坦な配列で、
/// `posterior(f, p)` で引く。行列型を持ち込まないのは、ここを跨ぐのが
/// 数値の並びだけだから（`TR-PLT-06` の FFI 境界と同じ考え方）。
#[derive(Debug, Clone, PartialEq)]
pub struct Posteriors {
    /// フレーム数。
    pub frames: usize,
    /// 音素数（`AlignRequest::phonemes` に前後の `pau` を足した数）。
    pub phonemes: usize,
    /// フレーム進み幅（ミリ秒）。
    pub hop_ms: f64,
    /// 行優先の事後確率。長さは `frames * phonemes`。
    pub values: Vec<f32>,
}

impl Posteriors {
    /// フレーム `f` で音素 `p` である事後確率。範囲外は `0.0`。
    #[must_use]
    pub fn get(&self, f: usize, p: usize) -> f32 {
        if f >= self.frames || p >= self.phonemes {
            return 0.0;
        }
        self.values[f * self.phonemes + p]
    }
}

/// アライメントの結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Alignment {
    /// 音素ごとの区間。`AlignRequest::phonemes` の前後に `pau` を足した並び。
    pub segments: Vec<Segment>,
    /// フレーム×音素の事後確率（`TR-ALN-03`）。
    ///
    /// **退避経路は持たない**ので `None`。確信度の成分が欠けた状態として扱う。
    pub posteriors: Option<Posteriors>,
    /// 音素列全体の対数尤度。**テキスト逸脱の判定に使う**（`TR-ALN-09` (c)）。
    ///
    /// 退避経路は `None`。
    pub log_likelihood: Option<f64>,
    /// グリッド解と非制限解のスコア差（`TR-ALN-08`）。
    ///
    /// **閾値を超えたらグリッド逸脱として確信度を下げ、非制限解を採用する。**
    /// グリッドを渡していなければ `None`。
    pub grid_divergence: Option<f64>,
}

/// 音素1つぶんの区間。
///
/// **境界はミリ秒の連続値で持つ**（`TR-ALN-06`）。フレーム分解能より細かい値は
/// 境界近傍の事後確率からのサブフレーム補間で決める。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    /// この区間の音素。
    pub phoneme: Phoneme,
    /// 開始位置（ミリ秒）。
    pub start_ms: f64,
    /// 終了位置（ミリ秒）。
    pub end_ms: f64,
}

/// アライメントの失敗。
#[derive(Debug, thiserror::Error)]
pub enum AlignError {
    /// 音素列が空。
    #[error("音素列が空")]
    EmptyPhonemes,

    /// サンプルが短すぎて、音素の数だけ区間を作れない。
    #[error("音声が音素列に対して短すぎる")]
    TooShort,

    /// サンプリング周波数が想定と違う。**黙って変換しない**（`TR-SYN-31` と同じ規律）。
    #[error("サンプリング周波数が合わない")]
    RateMismatch,

    /// **テキスト逸脱**（`TR-ALN-09` (c)）。
    ///
    /// 音素列全体の尤度が閾値を下回った。**強制アライメントの結果を採用しない。**
    /// 呼び出し側は oto を自動確定させず、確認キューへ回す。
    #[error("録音の内容が想定した読みから外れている")]
    TextDeviation,

    /// モデルを読めなかった。
    #[error("音響モデルを読めない")]
    ModelUnavailable,
}

impl AlignError {
    /// 送信してよい種別文字列。
    ///
    /// **`Display` は送らない。** トレースに載せてよいのはここに列挙した固定文字列だけ
    /// （AGENTS.md の破ってはいけないもの #3）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::EmptyPhonemes => "align.empty_phonemes",
            Self::TooShort => "align.too_short",
            Self::RateMismatch => "align.rate_mismatch",
            Self::TextDeviation => "align.text_deviation",
            Self::ModelUnavailable => "align.model_unavailable",
        }
    }
}

/// 強制アライメントを行うもの（`TR-ALN-03`）。
pub trait Aligner {
    /// この実装を識別する文字列。**決定性の鍵に混ぜる**（`TR-ALN-29`）。
    ///
    /// モデルの版まで含めること。同じ入力でも実装が変われば結果が変わるので、
    /// これが変わったら再計算の対象になる。
    fn identity(&self) -> &str;

    /// 1テイクをアライメントする。
    ///
    /// **発話内容は既知として線形鎖に固定する**（`TR-ALN-09`）。前後の無音区間の長さは
    /// 自由にし、想定音素列の前後に任意長の非音声を許す。
    ///
    /// # Errors
    ///
    /// 音素列が空、音声が短すぎる、サンプリング周波数が合わない、
    /// テキスト逸脱、モデルが読めない。
    fn align(&self, req: &AlignRequest<'_>) -> Result<Alignment, AlignError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn posteriors() -> Posteriors {
        Posteriors {
            frames: 3,
            phonemes: 2,
            hop_ms: 10.0,
            values: vec![0.9, 0.1, 0.4, 0.6, 0.2, 0.8],
        }
    }

    #[test]
    fn 事後確率を行優先で引ける() {
        let p = posteriors();
        assert!((p.get(0, 0) - 0.9).abs() < 1e-6);
        assert!((p.get(1, 1) - 0.6).abs() < 1e-6);
        assert!((p.get(2, 1) - 0.8).abs() < 1e-6);
    }

    /// **範囲外は 0 を返す。** パニックさせない——境界の探索は端をまたぐ。
    #[test]
    fn 範囲外はゼロ() {
        let p = posteriors();
        assert_eq!(p.get(3, 0), 0.0);
        assert_eq!(p.get(0, 2), 0.0);
    }

    /// **種別文字列に音源名・パス・歌詞を混ぜない**（AGENTS.md #3）。
    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            AlignError::EmptyPhonemes,
            AlignError::TooShort,
            AlignError::RateMismatch,
            AlignError::TextDeviation,
            AlignError::ModelUnavailable,
        ] {
            assert!(e.kind().starts_with("align."), "{}", e.kind());
        }
    }
}
