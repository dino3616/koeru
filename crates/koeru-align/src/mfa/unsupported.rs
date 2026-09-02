//! まだ Kaldi を組んでいない OS の MFA 経路。
//!
//! **黙って何もしないのではなく、書いていないことを言う。**
//! ここが返すのは常に [`MfaError::Unsupported`] で、呼び出し側は
//! 「この OS ではまだ自動原音設定ができない」と表示できる。
//!
//! Windows と Linux は BLAS の引き方（`HAVE_OPENBLAS` など）を書けば通る見込み。
//! 必要な Kaldi のモジュールは OS に依らない（`EVID-ALN-001`）。**ここはその席。**

use std::path::Path;

/// モデルが前提とするサンプリング周波数（`EVID-ALN-001`）。
pub const MODEL_SAMPLE_RATE_HZ: u32 = 16_000;

/// フレーム進み幅（ミリ秒）。
pub const FRAME_SHIFT_MS: f64 = 10.0;

/// この OS では Kaldi をまだ組んでいない。
#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    /// この OS ではまだ書いていない。
    #[error("この OS の自動原音設定はまだ書いていない")]
    Unsupported,
}

impl MfaError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        "mfa.unsupported_platform"
    }
}

/// 読み込んだ MFA モデル。**この OS では作れない。**
#[derive(Debug)]
pub struct MfaAligner {
    _never: std::convert::Infallible,
}

impl MfaAligner {
    /// モデルを開く。**この OS では常に失敗する。**
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

    /// **書いていないことを言う。** 黙って成功しない。
    #[test]
    fn この_os_では開けない() {
        let e = MfaAligner::open(Path::new("/any"), "t").unwrap_err();
        assert_eq!(e.kind(), "mfa.unsupported_platform");
    }
}
