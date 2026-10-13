//! まだ Kaldi を組んでいない OS の MFA 経路。
//!
//! 黙って何もしないのではなく、書いていないことを言う。
//! ここが返すのは常に [`MfaError::Unsupported`] で、呼び出し側は
//! 「この OS ではまだ自動原音設定ができない」と表示できる。
//!
//! Windows と Linux は BLAS の引き方（`HAVE_OPENBLAS` など）を書けば通る見込み。
//! 必要な Kaldi のモジュールは OS に依らない（`EVID-ALN-001`）。ここはその席。

use std::path::Path;

/// モデルが前提とするサンプリング周波数（`EVID-ALN-001`）。
pub const MODEL_SAMPLE_RATE_HZ: u32 = 16_000;

/// フレーム進み幅（ミリ秒）。
pub const FRAME_SHIFT_MS: f64 = 10.0;

/// この OS で MFA を動かせるか（`DEC-PLT-038`）。 組んでいない OS では使えない。
///
/// 理由の code と分類は [`MfaError::Unsupported`] と同じ。 失敗として返す口（[`MfaAligner::open`]）と
/// 値として問う口（これ）で、2通りに言わない。
///
/// `kaldi.rs` にも同じ名前で置いてある。 **片方だけに足さない。**
#[must_use]
pub fn availability() -> crate::aligner::Availability {
    use koeru_failure::Failure as _;
    crate::aligner::Availability::Unavailable(crate::aligner::Unavailable {
        code: MfaError::Unsupported.code(),
        class: MfaError::Unsupported.class(),
    })
}

/// この OS では Kaldi をまだ組んでいない。
#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    /// この OS ではまだ書いていない。
    #[error("この OS の自動原音設定はまだ書いていない")]
    Unsupported,
}

impl koeru_failure::Failure for MfaError {
    fn code(&self) -> &'static str {
        "mfa.unsupported_platform"
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::Unsupported
    }
}

/// 読み込んだ MFA モデル。この OS では作れない。
#[derive(Debug)]
pub struct MfaAligner {
    _never: std::convert::Infallible,
}

impl MfaAligner {
    /// モデルを開く。この OS では常に失敗する。
    ///
    /// # Errors
    ///
    /// 常に [`MfaError::Unsupported`]。
    pub fn open(_dir: &Path, _identity: impl Into<String>) -> Result<Self, MfaError> {
        Err(MfaError::Unsupported)
    }

    /// 決定性の鍵に混ぜる文字列。
    #[must_use]
    pub fn identity(&self) -> &str {
        match self._never {}
    }

    /// 特徴の次元。
    #[must_use]
    pub fn feature_dim(&self) -> usize {
        match self._never {}
    }

    /// モデルが知っている音素の数。
    #[must_use]
    pub fn num_phones(&self) -> usize {
        match self._never {}
    }

    /// 特徴量を作る。
    ///
    /// # Errors
    ///
    /// この OS では到達しない（[`Self::open`] が先に失敗する）。
    pub fn features(&self, _samples: &[f32], _rate: u32) -> Result<(usize, Vec<f32>), MfaError> {
        match self._never {}
    }
}

impl crate::aligner::Aligner for MfaAligner {
    fn identity(&self) -> &str {
        match self._never {}
    }

    fn align(
        &self,
        _req: &crate::aligner::AlignRequest<'_>,
    ) -> Result<crate::aligner::Alignment, crate::aligner::AlignError> {
        match self._never {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 使えるかを値で問える。 理由は開けないときの失敗と同じ code と分類。
    #[test]
    fn この_os_では使えないと値で言う() {
        use crate::aligner::{Availability, Unavailable};
        assert_eq!(
            availability(),
            Availability::Unavailable(Unavailable {
                code: "mfa.unsupported_platform",
                class: koeru_failure::Class::Unsupported,
            })
        );
        assert_eq!(
            availability().reason().map(|r| r.code),
            Some("mfa.unsupported_platform")
        );
    }

    /// **書いていないことを言う。** 黙って成功しない。
    #[test]
    fn この_os_では開けない() {
        let e = MfaAligner::open(Path::new("/any"), "t").unwrap_err();
        assert_eq!(koeru_failure::Failure::code(&e), "mfa.unsupported_platform");
        assert_eq!(
            koeru_failure::Failure::class(&e),
            koeru_failure::Class::Unsupported
        );
    }
}
