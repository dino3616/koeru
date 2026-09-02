//! 本人が指した resampler を呼ぶ（`TR-SYN-35`）。
//!
//! **これが唯一、外部プロセスを起動する場所。**
//! `TR-SYN-01` は「外部プロセス（`.exe`、Wine、Python インタプリタ）の起動と
//! ネットワーク通信を一切行わない」と定めているが、
//! **本人が明示的に指した resampler だけは別**（`TR-SYN-35`）。
//!
//! # 探さない・入れない・更新しない
//!
//! **KOERU は候補を探索せず、自動導入もせず、更新もしない**（`TR-SYN-35`）。
//! 指すのは常に本人の明示的な操作。勝手に見つけて使うと、
//! **どのエンジンで鳴っているか分からないまま声を評価させることになる。**
//!
//! # プロジェクトに焼き付けない
//!
//! **実行可能ファイルはアプリ設定として持ち、プロジェクトに絶対パスで記録しない**
//! （`TR-SYN-35`）。プロジェクトを別のマシンや別の OS で開いたとき、
//! 解決できなければ**既定のコアへ戻し、そのことを提示する。黙って別の音で鳴らさない。**

use std::path::{Path, PathBuf};

use koeru_core::oto::Oto;

/// 外部 resampler の呼び出しが失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum ExternalError {
    /// 指された実行可能ファイルが無い。
    ///
    /// **既定のコアへ戻し、そのことを提示する。** 黙って別の音で鳴らさない。
    #[error("指された resampler が見つからない")]
    NotFound,

    /// 起動に失敗した。
    #[error("resampler を起動できなかった")]
    Spawn(#[source] std::io::Error),

    /// 実行はできたが、期待した出力が無い。
    #[error("resampler が出力を返さなかった")]
    NoOutput,
}

impl ExternalError {
    /// 送信してよい種別文字列。**パスは送らない**（利用者名を含む）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::NotFound => "external.not_found",
            Self::Spawn(_) => "external.spawn",
            Self::NoOutput => "external.no_output",
        }
    }
}

/// UTAU classic resampler の引数（`TR-SYN-08`, `TR-SYN-35`）。
///
/// **KOERU 独自の引数を足さない。** 足すと、既存の resampler が受け取れない。
#[derive(Debug, Clone, PartialEq)]
pub struct ClassicArgs {
    /// 素材の WAV。
    pub input: PathBuf,
    /// 出力先の WAV。
    pub output: PathBuf,
    /// 音名（`C4` のような）。
    pub tone: String,
    /// 子音速度。
    pub velocity: f64,
    /// フラグ文字列。
    pub flags: String,
    /// oto の5値。
    pub oto: Oto,
    /// 鳴らしたい長さ（ミリ秒）。
    pub length_ms: f64,
    /// モジュレーション（%）。
    pub modulation: f64,
    /// ピッチベンド（Base64 の実行長圧縮）。
    pub pitch: String,
}

impl ClassicArgs {
    /// UTAU classic resampler の順序で並べる（`TR-SYN-35`）。
    ///
    /// `resampler in.wav out.wav tone velocity flags offset length consonant cutoff volume modulation tempo pitch`
    #[must_use]
    pub fn to_argv(&self) -> Vec<String> {
        vec![
            self.input.to_string_lossy().into_owned(),
            self.output.to_string_lossy().into_owned(),
            self.tone.clone(),
            format!("{:.0}", self.velocity),
            self.flags.clone(),
            format!("{:.3}", self.oto.offset_ms),
            format!("{:.3}", self.length_ms),
            format!("{:.3}", self.oto.consonant_ms),
            format!("{:.3}", self.oto.cutoff_ms),
            // 音量は 100 固定。**試唱のフラグは既定に固定する**（TR-SYN-09）。
            "100".to_owned(),
            format!("{:.0}", self.modulation),
            // テンポ。UTAU は `!120.0` の形で渡す。
            "!120.0".to_owned(),
            self.pitch.clone(),
        ]
    }
}

/// MIDI ノート番号を UTAU の音名にする。
///
/// **UTAU は `C4` = 60。** MIDI の慣例（C4 = 60）と同じ。
#[must_use]
pub fn tone_name(midi: i32) -> String {
    const NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];
    let octave = midi.div_euclid(12) - 1;
    let name = NAMES
        .get(usize::try_from(midi.rem_euclid(12)).unwrap_or(0))
        .copied()
        .unwrap_or("C");
    format!("{name}{octave}")
}

/// 指された resampler が使えるか（`TR-SYN-35`）。
///
/// **解決できなければ既定のコアへ戻す。** 黙って別の音で鳴らさない。
#[must_use]
pub fn is_usable(path: Option<&Path>) -> bool {
    path.is_some_and(|p| p.is_file())
}

/// 外部 resampler を1音ぶん呼ぶ（`TR-SYN-35`）。
///
/// **ここだけが外部プロセスを起動する。** 呼ぶのは本人が指したときだけ。
///
/// # Errors
///
/// 実行可能ファイルが無い、起動できない、出力が無いとき。
#[tracing::instrument(skip(exe, args), fields(kind = "external_resampler"), err)]
pub fn run(exe: &Path, args: &ClassicArgs) -> Result<PathBuf, ExternalError> {
    if !exe.is_file() {
        return Err(ExternalError::NotFound);
    }
    let status = std::process::Command::new(exe)
        .args(args.to_argv())
        .status()
        .map_err(ExternalError::Spawn)?;

    if !status.success() || !args.output.is_file() {
        return Err(ExternalError::NoOutput);
    }
    Ok(args.output.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_align::derive::derive_cv;
    use koeru_align::preset::{ConsonantClass, Preset};
    use koeru_core::alias::Method;

    fn args() -> ClassicArgs {
        ClassicArgs {
            input: PathBuf::from("/x/a.wav"),
            output: PathBuf::from("/x/out.wav"),
            tone: tone_name(60),
            velocity: 100.0,
            flags: String::new(),
            oto: derive_cv(
                0.0,
                20.0,
                480.0,
                500.0,
                &Preset::default_for(Method::Single).expect("既定がある"),
                ConsonantClass::None,
            ),
            length_ms: 500.0,
            modulation: 0.0,
            pitch: String::new(),
        }
    }

    #[test]
    fn 音名に直せる() {
        assert_eq!(tone_name(60), "C4");
        assert_eq!(tone_name(69), "A4");
        assert_eq!(tone_name(57), "A3");
        assert_eq!(tone_name(72), "C5");
        assert_eq!(tone_name(61), "C#4");
    }

    /// **KOERU 独自の引数を足さない**（TR-SYN-35）。
    /// classic resampler の順序どおり13個。
    #[test]
    fn 引数の並びが_classic_と同じ() {
        let v = args().to_argv();
        assert_eq!(v.len(), 13, "{v:?}");
        assert_eq!(v[0], "/x/a.wav");
        assert_eq!(v[1], "/x/out.wav");
        assert_eq!(v[2], "C4");
        assert_eq!(v[3], "100");
        assert_eq!(v[9], "100", "音量は既定に固定");
        assert_eq!(v[11], "!120.0");
    }

    /// **解決できなければ使わない**（TR-SYN-35）。
    #[test]
    fn 見つからなければ使えない() {
        assert!(!is_usable(None));
        assert!(!is_usable(Some(Path::new("/存在しない/resampler"))));
    }

    #[test]
    fn 見つからないパスを呼ぶと拒む() {
        let mut a = args();
        a.input = PathBuf::from("/x/a.wav");
        let e = run(Path::new("/存在しない/resampler"), &a).expect_err("拒むこと");
        assert_eq!(e.kind(), "external.not_found");
    }

    /// **本人が指すまで使わない。** 既定は「指されていない」。
    #[test]
    fn 既定では指されていない() {
        let nothing: Option<&Path> = None;
        assert!(!is_usable(nothing), "探索も自動導入もしない");
    }
}
