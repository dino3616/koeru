//! 推定の決定性と再現性（`TR-ALN-29`）。
//!
//! > 同一の WAV・同一の録音リスト・同一の規約プリセット・同一のモデルに対して、
//! > アライメントと oto 推定の出力がビット単位で同一になることを保証する。
//! > 推定結果は、入力のハッシュ・モデルバージョン・プリセットバージョンとともに
//! > プロジェクトに保存し、いずれかが変わったときだけ再計算する。
//!
//! # 4つの入力を1つの指紋に畳む
//!
//! [`Fingerprint`] が持つのは WAV の内容ハッシュ・読み・プリセットの版・アライナの版。
//! どれが変わっても指紋が変わるので、再計算するかどうかは指紋の比較だけで決まる。
//!
//! # モデルが変わったときだけ扱いが違う
//!
//! `TR-ALN-29`:
//!
//! > モデル更新時は、既存プロジェクトの推定結果を自動で上書きせず、
//! > 再推定するかをユーザーが選べるようにする。
//!
//! プリセットを自分で変えたなら再計算されて当然だが、アプリを更新したら
//! 昨日まで確認し終えた oto が全部作り直されている、は事故。
//! [`Change::of`] がこの2つを分けて返す。
//!
//! # 浮動小数点の決定性
//!
//! `TR-ALN-29` notes:
//!
//! > 浮動小数点演算の決定性は、SIMD やスレッド数、プラットフォーム間で崩れる。
//! > ビット単位の同一性は実装制約として重い可能性がある
//!
//! ここが保証するのは「同じ入力なら再計算しない」まで。 同じ入力から
//! 同じビット列が出ることは、アライナの実装側の責任として残る（`DEC-ALN-008`）。

use sha2::{Digest as _, Sha256};

use crate::preset::Preset;

/// 推定結果を作った入力の指紋（`TR-ALN-29`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fingerprint {
    pub audio: String,
    /// 読み（録音リストがその行に持っている音素列の元）。
    pub reading: String,
    /// 規約プリセットの識別（`Preset::identity`）。
    pub preset: String,
    /// アライナの識別（`crate::aligner::Aligner::identity`）。モデルの版を含む。
    pub aligner: String,
}

impl Fingerprint {
    /// 入力から指紋を作る。
    #[must_use]
    pub fn new(samples: &[f64], reading: &str, preset: &Preset, aligner: &str) -> Self {
        Self {
            audio: hash_samples(samples),
            reading: reading.to_owned(),
            preset: preset.identity(),
            aligner: aligner.to_owned(),
        }
    }

    /// 保存用の1行表現。並びは常に同じ。
    #[must_use]
    pub fn to_key(&self) -> String {
        format!(
            "{}|{}|{}|{}",
            self.audio, self.reading, self.preset, self.aligner
        )
    }
}

/// 何が変わったか（`TR-ALN-29`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    /// 何も変わっていない。再計算しない。
    None,
    /// 入力が変わった（録り直し・読みの修正・プリセットの編集）。
    ///
    /// 自動で再計算してよい。 本人が動かしたものだから。
    Input,
    /// モデルが変わった。 アプリの更新でこうなる。
    ///
    /// 自動で上書きしない。 再推定するかを本人に選ばせる。
    Model,
}

impl Change {
    /// 送信してよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::None => "determinism.none",
            Self::Input => "determinism.input",
            Self::Model => "determinism.model",
        }
    }

    /// 黙って再計算してよいか。
    #[must_use]
    pub const fn may_recompute_silently(self) -> bool {
        matches!(self, Self::Input)
    }

    /// 保存した指紋といまの指紋を比べる。
    ///
    /// モデルの変化を優先して返す。 両方変わっていたら、
    /// 本人に選ばせるほうへ倒す——勝手に上書きするより、余分に訊くほうが安い。
    #[must_use]
    pub fn of(saved: &Fingerprint, current: &Fingerprint) -> Self {
        if saved.aligner != current.aligner {
            return Self::Model;
        }
        if saved == current {
            Self::None
        } else {
            Self::Input
        }
    }
}

/// サンプル列の内容ハッシュ。
///
/// バイト表現を固定する。 `f64` をそのまま読むと環境で並びが変わりうるので、
/// リトルエンディアンに揃えてから食わせる。
fn hash_samples(samples: &[f64]) -> String {
    let mut h = Sha256::new();
    h.update(samples.len().to_le_bytes());
    for s in samples {
        h.update(s.to_le_bytes());
    }
    format!("{:x}", h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_core::alias::Method;

    fn preset() -> Preset {
        Preset::default_for(Method::Single).expect("既定がある")
    }

    fn fp(samples: &[f64], reading: &str, aligner: &str) -> Fingerprint {
        Fingerprint::new(samples, reading, &preset(), aligner)
    }

    #[test]
    fn 同じ入力からは同じ指紋になる() {
        let a = fp(&[0.1, 0.2, 0.3], "か", "mfa@3.0.0");
        let b = fp(&[0.1, 0.2, 0.3], "か", "mfa@3.0.0");
        assert_eq!(a, b);
        assert_eq!(a.to_key(), b.to_key());
    }

    /// 長さが違えば違う指紋。 長さを混ぜないと、
    /// `[0.0]` と `[0.0, 0.0]` の前半が同じ扱いになる。
    #[test]
    fn 長さが違えば指紋が変わる() {
        assert_ne!(
            fp(&[0.0], "か", "m").audio,
            fp(&[0.0, 0.0], "か", "m").audio
        );
    }

    #[test]
    fn 音が変われば指紋が変わる() {
        assert_ne!(fp(&[0.1], "か", "m").audio, fp(&[0.2], "か", "m").audio);
    }

    #[test]
    fn 何も変わらなければ再計算しない() {
        let a = fp(&[0.1], "か", "mfa@3.0.0");
        assert_eq!(Change::of(&a, &a), Change::None);
        assert!(!Change::None.may_recompute_silently());
    }

    /// **録り直しは黙って再計算してよい。** 本人が動かしたものだから。
    #[test]
    fn 録り直しは黙って再計算してよい() {
        let a = fp(&[0.1], "か", "mfa@3.0.0");
        let b = fp(&[0.9], "か", "mfa@3.0.0");
        assert_eq!(Change::of(&a, &b), Change::Input);
        assert!(Change::Input.may_recompute_silently());
    }

    /// プリセットの編集も入力の変化。
    #[test]
    fn プリセットが変われば再計算する() {
        let mut p = preset();
        let a = Fingerprint::new(&[0.1], "か", &p, "mfa@3.0.0");
        p.version += 1;
        let b = Fingerprint::new(&[0.1], "か", &p, "mfa@3.0.0");
        assert_eq!(Change::of(&a, &b), Change::Input);
    }

    /// モデルの更新は黙って上書きしない（`TR-ALN-29`）。
    /// ここが破れると、アプリを更新した翌日に確認済みの oto が全部作り直される。
    #[test]
    fn モデルが変わったら本人に選ばせる() {
        let a = fp(&[0.1], "か", "mfa@3.0.0");
        let b = fp(&[0.1], "か", "mfa@3.1.0");
        assert_eq!(Change::of(&a, &b), Change::Model);
        assert!(!Change::Model.may_recompute_silently());
    }

    /// 両方変わっていたら、本人に選ばせるほうへ倒す。
    #[test]
    fn 両方変わればモデル側を優先する() {
        let a = fp(&[0.1], "か", "mfa@3.0.0");
        let b = fp(&[0.9], "き", "mfa@3.1.0");
        assert_eq!(Change::of(&a, &b), Change::Model);
    }

    #[test]
    fn 種別は固定文字列() {
        for c in [Change::None, Change::Input, Change::Model] {
            assert!(c.kind().starts_with("determinism."));
        }
    }
}
