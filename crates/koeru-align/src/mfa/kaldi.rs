//! MFA 日本語音響モデルを Kaldi 経由で叩く（`TR-ALN-05`, `DEC-ALN-008`）。
//!
//! **一次経路。** 退避経路は [`crate::segment`]（`DEC-ALN-006`）。
//!
//! # Python は要らない
//!
//! MFA 3.0 は Kaldi のバイナリを呼ぶ方式をやめ、共有ライブラリを直接呼ぶ形になった。
//! **KOERU が使うのはモデル（CC BY 4.0）と Kaldi（Apache-2.0）だけで、
//! MFA というアプリケーションは同梱しない**（`TR-ALN-05`）。
//! `TR-ALN-01` の実行時ランタイム境界（Python / conda / 外部 DB サーバ /
//! 常駐プロセスを含めない）はそのまま守れている。
//!
//! # 16kHz へ落とす必要がある
//!
//! **モデルは 16kHz を前提にしている**（`EVID-ALN-001`。`meta.json` の
//! `sample_frequency: 16000`）。KOERU のマスターは 44100 Hz（`TR-REC-02`）なので、
//! アライメントの入口でダウンサンプルが要る。**黙って変換しない**——
//! [`MfaAligner::features`] は 16kHz を受け取る前提で、合わなければ拒む
//! （`TR-SYN-31` と同じ規律）。
//!
//! # 特徴パイプライン
//!
//! ```text
//! MFCC(13) → CMVN → splice(±3)=91 → LDA+MLLT(40x91) → 40 → fMLLR
//! ```
//!
//! **`meta.json` の `uses_splices` / `uses_deltas` は当てにならない。**
//! `final.mdl` の `<DIMENSION>` が 40、`lda.mat` が 40×91 で、Δ+ΔΔ の 39 ではない
//! （`EVID-ALN-001`）。

use std::ffi::{CString, c_char, c_float, c_int};
use std::path::Path;

/// モデルが前提とするサンプリング周波数（`EVID-ALN-001`）。
pub const MODEL_SAMPLE_RATE_HZ: u32 = 16_000;

/// フレーム進み幅（ミリ秒）。**`TR-ALN-06` の 2ms はサブフレーム補間で作る。**
pub const FRAME_SHIFT_MS: f64 = 10.0;

#[repr(C)]
struct KoeruKaldi {
    _private: [u8; 0],
}

// **C 境界を跨ぐのは PCM バッファとパラメータだけ**（`TR-PLT-06`）。
unsafe extern "C" {
    fn koeru_kaldi_open(model_dir: *const c_char) -> *mut KoeruKaldi;
    fn koeru_kaldi_close(h: *mut KoeruKaldi);
    fn koeru_kaldi_feature_dim(h: *const KoeruKaldi) -> c_int;
    fn koeru_kaldi_num_phones(h: *const KoeruKaldi) -> c_int;
    fn koeru_kaldi_min_length(h: *const KoeruKaldi, phone: c_int) -> c_int;
    fn koeru_kaldi_features(
        h: *mut KoeruKaldi,
        samples: *const c_float,
        n_samples: c_int,
        out: *mut c_float,
        out_capacity_frames: c_int,
    ) -> c_int;
    fn koeru_kaldi_align(
        h: *mut KoeruKaldi,
        samples: *const c_float,
        n_samples: c_int,
        phone_ids: *const c_int,
        n_phones: c_int,
        boundaries_ms: *mut c_float,
        log_likelihood: *mut c_float,
        posteriors: *mut c_float,
        posteriors_capacity: c_int,
        n_frames: *mut c_int,
    ) -> c_int;
}

/// 1テイクのアライメント結果（C 境界から返るそのまま）。
#[derive(Debug, Clone, PartialEq)]
pub struct RawAlignment {
    /// 音素の境目（ミリ秒）。**長さは `音素数 + 3`**（前後の `sil` を含む）。
    pub boundaries_ms: Vec<f32>,
    /// 音素列全体の対数尤度（`TR-ALN-09` (c)）。
    pub log_likelihood: f64,
    /// フレーム数。
    pub frames: usize,
    /// フレーム × (音素数 + 2) の事後確率、行優先（`TR-ALN-03`）。
    pub posteriors: Vec<f32>,
}

/// MFA の経路で起きる失敗。
#[derive(Debug, thiserror::Error)]
pub enum MfaError {
    /// モデルを読めない。
    #[error("音響モデルを読めない")]
    Model,

    /// 引数が不正。
    #[error("引数が不正")]
    Args,

    /// 音声が短すぎて、フレームが1つも作れない。
    #[error("音声が短すぎる")]
    TooShort,

    /// Kaldi 側で例外が起きた。
    #[error("音響モデルの処理に失敗した")]
    Internal,

    /// **サンプリング周波数が合わない。黙って変換しない**（`TR-SYN-31` と同じ規律）。
    #[error("サンプリング周波数がモデルの前提と違う")]
    RateMismatch,

    /// モデルのパスに `\0` が入っている。
    #[error("モデルのパスを扱えない")]
    BadPath,
}

impl MfaError {
    /// 送信してよい種別文字列。**パスも歌詞も載せない**（AGENTS.md #3）。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Model => "mfa.model",
            Self::Args => "mfa.args",
            Self::TooShort => "mfa.too_short",
            Self::Internal => "mfa.internal",
            Self::RateMismatch => "mfa.rate_mismatch",
            Self::BadPath => "mfa.bad_path",
        }
    }

    const fn from_code(c: c_int) -> Self {
        match c {
            -1 => Self::Model,
            -3 => Self::TooShort,
            -4 => Self::Internal,
            _ => Self::Args,
        }
    }
}

type Result<T> = std::result::Result<T, MfaError>;

/// 読み込んだ MFA モデル。
///
/// **`final.alimdl` と `final.mdl` を両方常駐させる**（`TGT-ALN-004` の 160MB）。
/// 逐次ロードならピークは半分で済むが、`TR-ALN-10` の逐次推定でテイクごとに
/// 48MiB を読み直すことになり、`TGT-ALN-001` と `TGT-ALN-005` を割る（`DEC-ALN-008`）。
#[derive(Debug)]
pub struct MfaAligner {
    handle: *mut KoeruKaldi,
    identity: String,
}

// **ハンドルの中身は Kaldi の読み取り専用のモデル。** 送っても壊れない。
//
// **`Sync` は付けない。** Kaldi の `Mfcc` が内部に作業バッファを持っていて、
// 同時に呼ぶと壊れる。`!Sync` なので `&MfaAligner` は他スレッドへ渡せず、
// **`&self` を取るメソッドでも同時呼び出しは起きない。**
// （Rust 側の状態は何も変えないので `&mut self` にする理由が無い——
// `&self` から `&mut self` を作るのは未定義動作。**踏んだ。**）
unsafe impl Send for MfaAligner {}

impl MfaAligner {
    /// モデルのディレクトリを開く。
    ///
    /// `final.mdl` / `final.alimdl` / `tree` / `lda.mat` があること。
    /// `identity` は決定性の鍵に混ぜる文字列で、**モデルの版を含めること**（`TR-ALN-29`）。
    ///
    /// # Errors
    ///
    /// モデルを読めない、パスに `\0` が入っている。
    pub fn open(dir: &Path, identity: impl Into<String>) -> Result<Self> {
        let c = CString::new(dir.as_os_str().as_encoded_bytes()).map_err(|_| MfaError::BadPath)?;
        // SAFETY: `c` は有効な NUL 終端の文字列で、呼び出しの間だけ生きていればよい。
        // 返るのは所有権付きのハンドルか NULL。
        let handle = unsafe { koeru_kaldi_open(c.as_ptr()) };
        if handle.is_null() {
            return Err(MfaError::Model);
        }
        Ok(Self {
            handle,
            identity: identity.into(),
        })
    }

    /// 決定性の鍵に混ぜる文字列（`TR-ALN-29`）。
    #[must_use]
    pub fn identity(&self) -> &str {
        &self.identity
    }

    /// 特徴の次元。**LDA の出力次元で、40 のはず。**
    #[must_use]
    pub fn feature_dim(&self) -> usize {
        // SAFETY: `self.handle` は `open` が返した非 NULL のハンドル。
        let d = unsafe { koeru_kaldi_feature_dim(self.handle) };
        usize::try_from(d).unwrap_or(0)
    }

    /// モデルが知っている音素の数。
    #[must_use]
    pub fn num_phones(&self) -> usize {
        // SAFETY: 同上。
        let n = unsafe { koeru_kaldi_num_phones(self.handle) };
        usize::try_from(n).unwrap_or(0)
    }

    /// その音素を通過するのに要る最短フレーム数。
    ///
    /// **HMM の状態数とは限らない。** topology が飛び越しを許していれば短くなる。
    #[must_use]
    pub fn min_length(&self, phone: u32) -> usize {
        let p = c_int::try_from(phone).unwrap_or(0);
        // SAFETY: `self.handle` は `open` が返した非 NULL のハンドル。
        let n = unsafe { koeru_kaldi_min_length(self.handle, p) };
        usize::try_from(n).unwrap_or(0)
    }

    /// 特徴量を作る（MFCC → CMVN → splice → LDA）。
    ///
    /// `samples` は **16kHz モノラルの [-1, 1]**。返るのは `(フレーム数, 平坦な行優先の行列)`。
    ///
    /// # Errors
    ///
    /// サンプリング周波数が合わない、音声が短すぎる、Kaldi 側の失敗。
    pub fn features(&self, samples: &[f32], sample_rate_hz: u32) -> Result<(usize, Vec<f32>)> {
        if sample_rate_hz != MODEL_SAMPLE_RATE_HZ {
            return Err(MfaError::RateMismatch);
        }
        let n = c_int::try_from(samples.len()).map_err(|_| MfaError::Args)?;

        // まずフレーム数だけ測る。**出力の大きさを推測しない。**
        // SAFETY: `samples` は `n` 要素あり、`out` に NULL を渡すのは
        //         「測るだけ」を意味する取り決め（`koeru_kaldi.h`）。
        let frames = unsafe {
            koeru_kaldi_features(self.handle, samples.as_ptr(), n, std::ptr::null_mut(), 0)
        };
        if frames < 0 {
            return Err(MfaError::from_code(frames));
        }
        let frames = usize::try_from(frames).map_err(|_| MfaError::Internal)?;
        let dim = self.feature_dim();
        let mut out = vec![0.0_f32; frames * dim];

        // SAFETY: `out` は `frames * dim` 要素あり、容量をフレーム数で渡している。
        let got = unsafe {
            koeru_kaldi_features(
                self.handle,
                samples.as_ptr(),
                n,
                out.as_mut_ptr(),
                c_int::try_from(frames).map_err(|_| MfaError::Internal)?,
            )
        };
        if got < 0 {
            return Err(MfaError::from_code(got));
        }
        Ok((frames, out))
    }
}

impl MfaAligner {
    /// 1テイクを強制アライメントして、C 境界の生の結果を返す。
    ///
    /// **`Aligner::align` の下回り。** リサンプルもサブフレーム補間もしない——
    /// 16kHz を受け取り、フレームの刻みで境界を返す。
    /// **`Aligner` を通す側を使うこと**（trait 側が入口の変換と `TR-ALN-06` を担う）。
    ///
    /// `phones` はモデル内の音素番号。**前後の無音は入れない**——
    /// シム側が `sil` を足す（`TR-ALN-09` の (a)(b)「前後の無音区間の長さを自由にする」）。
    ///
    /// # Errors
    ///
    /// サンプリング周波数が合わない、音素列が空、音声が短すぎる、Kaldi 側の失敗。
    pub fn align_raw(
        &self,
        samples: &[f32],
        sample_rate_hz: u32,
        phones: &[u32],
    ) -> Result<RawAlignment> {
        if sample_rate_hz != MODEL_SAMPLE_RATE_HZ {
            return Err(MfaError::RateMismatch);
        }
        if phones.is_empty() {
            return Err(MfaError::Args);
        }
        let ids: Vec<c_int> = phones
            .iter()
            .map(|p| c_int::try_from(*p).map_err(|_| MfaError::Args))
            .collect::<Result<_>>()?;

        // **出力の大きさを推測しない。** 先にフレーム数を測る。
        let frames = {
            let n = c_int::try_from(samples.len()).map_err(|_| MfaError::Args)?;
            // SAFETY: `samples` は `n` 要素あり、`out` の NULL は「測るだけ」の取り決め。
            let f = unsafe {
                koeru_kaldi_features(self.handle, samples.as_ptr(), n, std::ptr::null_mut(), 0)
            };
            if f < 0 {
                return Err(MfaError::from_code(f));
            }
            usize::try_from(f).map_err(|_| MfaError::Internal)?
        };

        let slots = phones.len() + 2; // 前後の `sil`
        let mut boundaries = vec![0.0_f32; slots + 1];
        let mut ll = 0.0_f32;
        let mut post = vec![0.0_f32; frames * slots];
        let mut got_frames: c_int = 0;

        // SAFETY: 出力の各バッファは上で確保した長さぶんあり、容量も同じ値で渡している。
        //         `ids` は `phones.len()` 要素。
        let rc = unsafe {
            koeru_kaldi_align(
                self.handle,
                samples.as_ptr(),
                c_int::try_from(samples.len()).map_err(|_| MfaError::Args)?,
                ids.as_ptr(),
                c_int::try_from(ids.len()).map_err(|_| MfaError::Args)?,
                boundaries.as_mut_ptr(),
                &raw mut ll,
                post.as_mut_ptr(),
                c_int::try_from(post.len()).map_err(|_| MfaError::Args)?,
                &raw mut got_frames,
            )
        };
        if rc != 0 {
            return Err(MfaError::from_code(rc));
        }
        Ok(RawAlignment {
            boundaries_ms: boundaries,
            log_likelihood: f64::from(ll),
            frames: usize::try_from(got_frames).map_err(|_| MfaError::Internal)?,
            posteriors: post,
        })
    }
}

/// テキスト逸脱と判定する、1フレームあたりの対数尤度の下限（`TR-ALN-09` (c)）。
///
/// **[Unknown] この値に根拠はない。** モデルの `average_log_likelihood` は -0.103 だが
/// （`EVID-ALN-001`）、それは学習コーパス上の値で、KOERU の収録条件のものではない。
/// **到達水準の判定を M6 へ送った以上、ここも実測で決められない**（`DEC-ALN-007`）。
/// 明らかに読みが違うテイクだけを弾く、緩い線として置いてある。
pub(crate) const TEXT_DEVIATION_FLOOR: f64 = -200.0;

/// サブフレーム補間で交点を探す幅（フレーム）。**境界の前後 3 フレーム = ±30ms。**
const REFINE_WINDOW: usize = 3;

impl crate::aligner::Aligner for MfaAligner {
    fn identity(&self) -> &str {
        &self.identity
    }

    fn align(
        &self,
        req: &crate::aligner::AlignRequest<'_>,
    ) -> std::result::Result<crate::aligner::Alignment, crate::aligner::AlignError> {
        use crate::aligner::{AlignError, Alignment, Posteriors, Segment};

        if req.phonemes.is_empty() {
            return Err(AlignError::EmptyPhonemes);
        }

        // **16kHz へ落としてから渡す**（`EVID-ALN-001`）。マスターには触らない。
        let mono: Vec<f32> = req.samples.iter().map(|v| *v as f32).collect();
        let wave = if req.sample_rate_hz == MODEL_SAMPLE_RATE_HZ {
            mono
        } else {
            crate::resample::resample(&mono, req.sample_rate_hz, MODEL_SAMPLE_RATE_HZ)
                .map_err(|_| AlignError::RateMismatch)?
        };

        let ids: Vec<u32> = req
            .phonemes
            .iter()
            .map(crate::phoneme::Phoneme::id)
            .collect();
        let raw = self
            .align_raw(&wave, MODEL_SAMPLE_RATE_HZ, &ids)
            .map_err(|e| match e {
                MfaError::TooShort => AlignError::TooShort,
                MfaError::RateMismatch => AlignError::RateMismatch,
                MfaError::Model | MfaError::BadPath => AlignError::ModelUnavailable,
                MfaError::Args | MfaError::Internal => AlignError::TooShort,
            })?;

        // **テキスト逸脱**（`TR-ALN-09` (c)）。1フレームあたりで見る——
        // 長いテイクほど尤度が下がるので、総和で切ると長さで判定が変わる。
        #[allow(clippy::cast_precision_loss)]
        let per_frame = if raw.frames == 0 {
            f64::NEG_INFINITY
        } else {
            raw.log_likelihood / raw.frames as f64
        };
        if per_frame < TEXT_DEVIATION_FLOOR {
            return Err(AlignError::TextDeviation);
        }

        // **前後の `sil` を含めた並び**（`TR-ALN-09` (a)(b)）。
        let sil = crate::phoneme::Phoneme::new(crate::phoneme::SILENCE)
            .ok_or(AlignError::ModelUnavailable)?;
        let mut phones = Vec::with_capacity(req.phonemes.len() + 2);
        phones.push(sil);
        phones.extend_from_slice(req.phonemes);
        phones.push(sil);

        // **境界をサブフレーム補間で連続値にする**（`TR-ALN-06`）。
        let slots = phones.len();
        let mut edges = Vec::with_capacity(slots + 1);
        edges.push(0.0_f64);
        for s in 1..slots {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let frame = (f64::from(raw.boundaries_ms[s]) / FRAME_SHIFT_MS).round() as usize;
            let r = crate::subframe::refine(&crate::subframe::Request {
                posteriors: &raw.posteriors,
                phones: slots,
                frames: raw.frames,
                frame,
                before: s - 1,
                after: s,
                hop_ms: FRAME_SHIFT_MS,
                window: REFINE_WINDOW,
            });
            // **単調にする。** 補間が前の境界より手前を指したら、前に合わせる。
            let prev = edges[s - 1];
            edges.push(r.ms.max(prev));
        }
        edges.push(f64::from(raw.boundaries_ms[slots]));

        let segments = (0..slots)
            .map(|s| Segment {
                phoneme: phones[s],
                start_ms: edges[s],
                end_ms: edges[s + 1],
            })
            .collect();

        Ok(Alignment {
            segments,
            posteriors: Some(Posteriors {
                frames: raw.frames,
                phonemes: slots,
                hop_ms: FRAME_SHIFT_MS,
                values: raw.posteriors,
            }),
            log_likelihood: Some(raw.log_likelihood),
            // グリッドは未実装（`TR-ALN-08`）。**渡していないので `None`。**
            grid_divergence: None,
        })
    }
}

impl Drop for MfaAligner {
    fn drop(&mut self) {
        // SAFETY: `self.handle` は `open` が返したもので、ここでしか解放しない。
        unsafe { koeru_kaldi_close(self.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aligner::Aligner;

    /// 実モデルの置き場所。**環境変数で指す。**
    ///
    /// **リポジトリにモデルを入れていない**ので、無ければ試験は静かに戻る
    /// （`koeru-audio` の実機ハーネスと同じ形）。
    fn model_dir() -> Option<std::path::PathBuf> {
        let p = std::path::PathBuf::from(std::env::var("KOERU_MFA_MODEL_DIR").ok()?);
        p.join("final.mdl").is_file().then_some(p)
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            MfaError::Model,
            MfaError::Args,
            MfaError::TooShort,
            MfaError::Internal,
            MfaError::RateMismatch,
            MfaError::BadPath,
        ] {
            assert!(e.kind().starts_with("mfa."), "{}", e.kind());
        }
    }

    /// **無いディレクトリは素直に失敗する。** 落ちない。
    #[test]
    fn 無いモデルは開けない() {
        let e = MfaAligner::open(Path::new("/nonexistent/koeru/model"), "test").unwrap_err();
        assert_eq!(e.kind(), "mfa.model");
    }

    /// 実モデルを読む。**`KOERU_MFA_MODEL_DIR` が無ければ戻る。**
    #[test]
    fn 実モデルを読める() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "mfa-japanese@3.0.0").expect("読める");
        // **LDA の出力次元は 40**（EVID-ALN-001）。
        assert_eq!(a.feature_dim(), 40);
        // **音素は 83 + sil + spn**（phones.txt の 86 から `<eps>` を除いた数）。
        assert_eq!(a.num_phones(), 85);
    }

    /// **16kHz でないものは黙って変換しない**（`TR-SYN-31` と同じ規律）。
    #[test]
    fn サンプリング周波数が合わなければ拒む() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let e = a.features(&[0.0; 16000], 44_100).unwrap_err();
        assert_eq!(e.kind(), "mfa.rate_mismatch");
    }

    /// 実モデルで特徴を作る。**次元とフレーム数が理屈に合うこと。**
    #[test]
    fn 実モデルで特徴を作れる() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");

        // 1秒ぶんの正弦波。**無音だと CMVN の分散が 0 になる。**
        let n = MODEL_SAMPLE_RATE_HZ as usize;
        let wave: Vec<f32> = (0..n)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / MODEL_SAMPLE_RATE_HZ as f32;
                0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
            })
            .collect();

        let (frames, feats) = a.features(&wave, MODEL_SAMPLE_RATE_HZ).expect("作れる");
        // **1秒 / 10ms = 100 フレーム前後。** `snip_edges: false` なので端が少し増える。
        assert!((95..=105).contains(&frames), "フレーム数 {frames}");
        assert_eq!(feats.len(), frames * 40);
        assert!(feats.iter().all(|v| v.is_finite()), "有限でない値がある");
        // **全部 0 ではない。** 0 なら特徴が作れていない。
        assert!(feats.iter().any(|v| v.abs() > 1e-6));
    }

    /// 「か」を模した合成音を、実モデルでアライメントする。
    ///
    /// **無音 → 無声子音（雑音）→ 母音（倍音）→ 無音** という形を作って、
    /// 境界がその並びに沿って出るかを見る。**精度の検証ではない**——
    /// 経路が通り、境界が単調で、区間が音の構造とだいたい合うことの確認
    /// （到達水準の判定は M6。`DEC-ALN-007`）。
    #[test]
    fn 実モデルで単独音をアライメントできる() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");

        let wave = syllable(200.0, 80.0, 500.0, 200.0);
        // `k` と `a`。**辞書と同じ音素番号を引く。**
        let k = crate::phoneme::Phoneme::new("k").expect("ある").id();
        let vowel = crate::phoneme::Phoneme::new("a").expect("ある").id();

        let r = a
            .align_raw(&wave, MODEL_SAMPLE_RATE_HZ, &[k, vowel])
            .expect("アライメントできる");

        // 境界は「先頭 sil + k + a + 末尾 sil」の4区間なので5点。
        assert_eq!(r.boundaries_ms.len(), 5);
        // **単調非減少。** 境界が戻ったら区間が負になる。
        for w in r.boundaries_ms.windows(2) {
            assert!(w[1] >= w[0], "境界が戻っている: {:?}", r.boundaries_ms);
        }
        assert_eq!(r.boundaries_ms[0], 0.0);
        // 全長は 980ms。フレーム進み 10ms なので末尾は 980 前後。
        assert!(
            (900.0..=1050.0).contains(&r.boundaries_ms[4]),
            "末尾 {}",
            r.boundaries_ms[4]
        );
        assert!(r.log_likelihood.is_finite());

        // **事後確率が返っている**（`TR-ALN-03`）。
        assert_eq!(r.posteriors.len(), r.frames * 4);
        // 各フレームで、4つの音素にかかる確率の和が 1 に近い。
        for t in 0..r.frames {
            let sum: f32 = r.posteriors[t * 4..(t + 1) * 4].iter().sum();
            assert!((sum - 1.0).abs() < 0.05, "フレーム {t} の和が {sum}");
        }
    }

    /// **各音素の区間が、topology の最短長を下回らない。**
    ///
    /// グラフの組み立てが壊れると、ここが真っ先に破れる（**実際に一度破れた**——
    /// 最後の音素の出口アークが範囲外の添字を指していて、DP 配列の外へ書いていた）。
    ///
    /// **`MinLength` は状態数とは限らない。** このモデルは3状態の音素でも
    /// 飛び越しを許していて最短 1 フレーム（`EVID-ALN-001`）。
    #[test]
    fn 各音素の区間が最短長を下回らない() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let wave = syllable(200.0, 80.0, 500.0, 200.0);
        let k = crate::phoneme::Phoneme::new("k").expect("ある").id();
        let vowel = crate::phoneme::Phoneme::new("a").expect("ある").id();
        let r = a
            .align_raw(&wave, MODEL_SAMPLE_RATE_HZ, &[k, vowel])
            .expect("できる");

        // boundaries = [sil開始, k開始, a開始, 末尾sil開始, 終端]
        let sil = crate::phoneme::Phoneme::new(crate::phoneme::SILENCE)
            .expect("ある")
            .id();
        for (slot, phone) in [(0, sil), (1, k), (2, vowel), (3, sil)] {
            let span = f64::from(r.boundaries_ms[slot + 1] - r.boundaries_ms[slot]);
            #[allow(clippy::cast_precision_loss)]
            let least = a.min_length(phone) as f64 * FRAME_SHIFT_MS;
            assert!(
                span >= least - 1e-6,
                "{slot} 番目の区間が {span}ms で、最短 {least}ms を下回っている"
            );
        }
    }

    /// **同じ入力からは同じ境界が出る**（`TR-ALN-29`）。
    #[test]
    fn 同じ入力からは同じ境界が出る() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let wave = syllable(150.0, 60.0, 400.0, 150.0);
        let k = crate::phoneme::Phoneme::new("k").expect("ある").id();
        let vowel = crate::phoneme::Phoneme::new("a").expect("ある").id();
        let r1 = a
            .align_raw(&wave, MODEL_SAMPLE_RATE_HZ, &[k, vowel])
            .expect("できる");
        let r2 = a
            .align_raw(&wave, MODEL_SAMPLE_RATE_HZ, &[k, vowel])
            .expect("できる");
        assert_eq!(r1.boundaries_ms, r2.boundaries_ms);
        assert_eq!(r1.posteriors, r2.posteriors);
        assert!((r1.log_likelihood - r2.log_likelihood).abs() < 1e-9);
    }

    /// **音声が長くなれば、最後の境界も後ろへ動く。**
    ///
    /// 当たり前に見えるが、**境界が入力と無関係な定数になっていないこと**の確認。
    #[test]
    fn 音声が長くなれば末尾の境界も動く() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let k = crate::phoneme::Phoneme::new("k").expect("ある").id();
        let vowel = crate::phoneme::Phoneme::new("a").expect("ある").id();

        let short = a
            .align_raw(
                &syllable(100.0, 60.0, 300.0, 100.0),
                MODEL_SAMPLE_RATE_HZ,
                &[k, vowel],
            )
            .expect("できる");
        let long = a
            .align_raw(
                &syllable(100.0, 60.0, 900.0, 100.0),
                MODEL_SAMPLE_RATE_HZ,
                &[k, vowel],
            )
            .expect("できる");
        assert!(long.frames > short.frames);
        assert!(long.boundaries_ms[4] > short.boundaries_ms[4]);
    }

    /// **音素列が空なら拒む。**
    #[test]
    fn 空の音素列は拒む() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let e = a
            .align_raw(&[0.0; 16000], MODEL_SAMPLE_RATE_HZ, &[])
            .unwrap_err();
        assert_eq!(e.kind(), "mfa.args");
    }

    /// **短すぎる音声は拒む。** 状態の数だけフレームが要る。
    #[test]
    fn 短すぎる音声は拒む() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let k = crate::phoneme::Phoneme::new("k").expect("ある").id();
        // 10ms しかない。**フレームが1〜2個。**
        let e = a
            .align_raw(&[0.1; 160], MODEL_SAMPLE_RATE_HZ, &[k])
            .unwrap_err();
        assert_eq!(e.kind(), "mfa.too_short");
    }

    /// 無音 → 無声子音（雑音）→ 母音（倍音）→ 減衰 という形を作る。
    ///
    /// `segment.rs` の試験と同じ作り方。**16kHz で作る。**
    fn syllable(silence_ms: f64, consonant_ms: f64, vowel_ms: f64, tail_ms: f64) -> Vec<f32> {
        let per_ms = f64::from(MODEL_SAMPLE_RATE_HZ) / 1000.0;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = |ms: f64| (ms * per_ms) as usize;
        let mut out = vec![0.0_f32; n(silence_ms)];

        // 無声子音: 高いゼロ交差率の雑音。**振幅は母音より小さい。**
        let mut state = 0x1234_5678_9abc_def0_u64;
        for _ in 0..n(consonant_ms) {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            #[allow(clippy::cast_precision_loss)]
            let r = (state >> 40) as f32 / 16_777_216.0 - 0.5;
            out.push(r * 0.08);
        }

        // 母音: 基本波 ＋ 倍音。
        let nv = n(vowel_ms);
        for i in 0..nv {
            #[allow(clippy::cast_precision_loss)]
            let t = i as f32 / MODEL_SAMPLE_RATE_HZ as f32;
            let tau = 2.0 * std::f32::consts::PI;
            let v = 0.45 * (tau * 220.0 * t).sin()
                + 0.20 * (tau * 440.0 * t).sin()
                + 0.10 * (tau * 880.0 * t).sin();
            out.push(v);
        }

        out.extend(std::iter::repeat_n(0.0, n(tail_ms)));
        out
    }

    /// **trait を通したアライメントが、44100 Hz の入力で通る**（`TR-ALN-03`, `TR-ALN-06`）。
    ///
    /// 入口でリサンプルし、境界をサブフレーム補間で連続値にして返す。
    #[test]
    fn trait_経由で四四一〇〇の音声を扱える() {
        use crate::aligner::AlignRequest;

        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "mfa-japanese@3.0.0").expect("読める");

        // **44100 Hz で作る。** マスターと同じ。
        let rate = 44_100_u32;
        let n = rate as usize;
        let wave: Vec<f64> = (0..n)
            .map(|i| {
                let t = f64::from(u32::try_from(i).unwrap_or(0)) / f64::from(rate);
                0.3 * (std::f64::consts::TAU * 220.0 * t).sin()
            })
            .collect();

        let k = crate::phoneme::Phoneme::new("k").expect("ある");
        let vowel = crate::phoneme::Phoneme::new("a").expect("ある");
        let r = Aligner::align(
            &a,
            &AlignRequest {
                samples: &wave,
                sample_rate_hz: rate,
                phonemes: &[k, vowel],
                grid: None,
            },
        )
        .expect("アライメントできる");

        // 前後の `sil` を含めて4区間。
        assert_eq!(r.segments.len(), 4);
        assert_eq!(r.segments[0].phoneme.as_str(), crate::phoneme::SILENCE);
        assert_eq!(r.segments[1].phoneme, k);
        assert_eq!(r.segments[2].phoneme, vowel);
        assert_eq!(r.segments[3].phoneme.as_str(), crate::phoneme::SILENCE);

        // **区間が繋がっていて、単調。**
        for w in r.segments.windows(2) {
            assert!((w[0].end_ms - w[1].start_ms).abs() < 1e-9);
        }
        assert!(r.segments[0].start_ms >= 0.0);
        assert!(r.segments[3].end_ms > r.segments[0].start_ms);

        // **emission 行列を返している**（`TR-ALN-03`）。
        let p = r.posteriors.expect("事後確率がある");
        assert_eq!(p.phonemes, 4);
        assert_eq!(p.values.len(), p.frames * 4);
        assert!((p.hop_ms - FRAME_SHIFT_MS).abs() < 1e-9);
        assert!(r.log_likelihood.is_some());

        // **フレーム数は 16kHz 換算。** 1秒なら 100 前後。
        assert!((95..=105).contains(&p.frames), "frames {}", p.frames);
    }

    /// **退避経路と違い、MFA は経路確信度を出せる**（`TR-ALN-24` の成分 (1)）。
    #[test]
    fn trait_の識別子にモデルの版が入る() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "mfa-japanese@3.0.0").expect("読める");
        assert_eq!(Aligner::identity(&a), "mfa-japanese@3.0.0");
    }

    /// **同じ入力からは同じ特徴が出る**（`TR-ALN-29` の決定性）。
    ///
    /// **[Risk] `dither: 1` は乱数を使う。** Kaldi の dither は固定シードなので
    /// 同一プロセス内では再現するが、ここが崩れたら `TR-ALN-29` が成り立たない。
    #[test]
    fn 同じ入力からは同じ特徴が出る() {
        let Some(dir) = model_dir() else {
            return;
        };
        let a = MfaAligner::open(&dir, "t").expect("読める");
        let wave: Vec<f32> = (0..8000)
            .map(|i| {
                #[allow(clippy::cast_precision_loss)]
                let t = i as f32 / 16000.0;
                0.2 * (2.0 * std::f32::consts::PI * 330.0 * t).sin()
            })
            .collect();
        let (f1, v1) = a.features(&wave, 16_000).expect("作れる");
        let (f2, v2) = a.features(&wave, 16_000).expect("作れる");
        assert_eq!(f1, f2);
        assert_eq!(v1, v2, "同じ入力から違う特徴が出ている");
    }
}
