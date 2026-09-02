//! 同梱するモデルの台帳（`TR-ALN-31`）。
//!
//! > 出所 URL、ライセンス、学習データの出所とそのライセンス、必要な帰属表示、
//! > 商用配布の可否を台帳として持ち**ライセンス表記に反映する**。
//!
//! # 空欄を作らない
//!
//! `TR-ALN-31` が「**未確認の項目を空欄にせず未確認と書く**」と定めている。
//! [`CorpusStatus::Unverified`] と [`Commercial::Unverified`] がその「未確認」で、
//! **黙って通さないための型。**
//!
//! # 判断で通したものには判断記録の ID が要る
//!
//! ライセンスまたは学習データの出所が確認できないモデルは、原則として同梱しない。
//! **例外は判断記録に書いて通したものに限り、台帳にその ID を併記する。**
//! [`check`] がこれを検査していて、**ID の無い `restricted` / `unverified` は落ちる。**
//!
//! # CC BY 系はアプリ内に出す
//!
//! 帰属表示・ライセンスへのリンク・変更の明示が義務なので、台帳に留めない。
//! [`notice`] がライセンス表記の本文を組み立てる。

/// 台帳の原文。
const MODELS_TOML: &str = include_str!("../resources/models.toml");

/// 学習コーパスの状態（`TR-ALN-31`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorpusStatus {
    /// 条件に問題がない。
    Ok,
    /// 再配布禁止・非商用限定などが含まれる。**判断記録が要る。**
    Restricted,
    /// 出所またはライセンスが未確認。**判断記録が要る。**
    Unverified,
}

/// 商用配布の可否。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Commercial {
    /// 可。
    Yes,
    /// 不可。
    No,
    /// **未確認。空欄にしない**（`TR-ALN-31`）。
    Unverified,
}

/// 台帳の1件。
#[derive(Debug, Clone, PartialEq)]
pub struct ModelEntry {
    /// 識別子。
    pub id: String,
    /// 表示名。
    pub name: String,
    /// 何に使うか。
    pub purpose: String,
    /// 出所 URL。
    pub source_url: String,
    /// ライセンス（SPDX 識別子または原文）。
    pub license: String,
    /// 必要な帰属表示。**CC BY 系では必須。**
    pub attribution: String,
    /// 学習コーパスの状態。
    pub corpus_status: CorpusStatus,
    /// 学習コーパスの出所とライセンス。
    pub corpus: String,
    /// 商用配布の可否。
    pub commercial_ok: Commercial,
    /// 判断で通した場合の判断記録 ID。**通していなければ空。**
    pub judgement: String,
    /// 補足。
    pub note: String,
}

impl ModelEntry {
    /// 判断記録が要るか（`TR-ALN-31`）。
    #[must_use]
    pub const fn needs_judgement(&self) -> bool {
        matches!(
            self.corpus_status,
            CorpusStatus::Restricted | CorpusStatus::Unverified
        )
    }

    /// 帰属表示が要るか。**CC BY 系は義務**（`DEC-ALN-008`）。
    #[must_use]
    pub fn needs_attribution(&self) -> bool {
        self.license.starts_with("CC-BY")
    }
}

/// 台帳の不備（`TR-ALN-31`）。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LedgerError {
    /// TOML として読めない。
    #[error("台帳を読めない")]
    Malformed,

    /// 必要な欄が無い、または知らない値が入っている。
    #[error("台帳に必要な欄が無い")]
    MissingField,

    /// **判断記録の ID が無いまま、確認できていないモデルを載せている。**
    #[error("判断記録の無いモデルが台帳にある")]
    UnjudgedModel,

    /// **CC BY 系なのに帰属表示が空。**
    #[error("帰属表示が要るのに空になっている")]
    MissingAttribution,
}

impl LedgerError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Malformed => "ledger.malformed",
            Self::MissingField => "ledger.missing_field",
            Self::UnjudgedModel => "ledger.unjudged_model",
            Self::MissingAttribution => "ledger.missing_attribution",
        }
    }
}

type Result<T> = std::result::Result<T, LedgerError>;

/// 同梱するモデルの一覧（`TR-ALN-31`）。
///
/// # Errors
///
/// 台帳が TOML として読めない、必要な欄が無い。
pub fn models() -> Result<Vec<ModelEntry>> {
    let doc: toml_edit::DocumentMut = MODELS_TOML.parse().map_err(|_| LedgerError::Malformed)?;
    let arr = doc
        .get("model")
        .and_then(toml_edit::Item::as_array_of_tables)
        .ok_or(LedgerError::MissingField)?;
    arr.iter().map(entry).collect()
}

fn entry(t: &toml_edit::Table) -> Result<ModelEntry> {
    let s = |k: &str| {
        t.get(k)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or(LedgerError::MissingField)
    };
    Ok(ModelEntry {
        id: s("id")?,
        name: s("name")?,
        purpose: s("purpose")?,
        source_url: s("source_url")?,
        license: s("license")?,
        attribution: s("attribution")?,
        corpus_status: match s("corpus_status")?.as_str() {
            "ok" => CorpusStatus::Ok,
            "restricted" => CorpusStatus::Restricted,
            "unverified" => CorpusStatus::Unverified,
            _ => return Err(LedgerError::MissingField),
        },
        corpus: s("corpus")?,
        commercial_ok: match s("commercial_ok")?.as_str() {
            "yes" => Commercial::Yes,
            "no" => Commercial::No,
            "unverified" => Commercial::Unverified,
            _ => return Err(LedgerError::MissingField),
        },
        judgement: s("judgement")?,
        note: s("note")?,
    })
}

/// 台帳の規律を検査する（`TR-ALN-31`）。
///
/// **判断記録の無い未確認モデルと、帰属表示の抜けを落とす。**
/// 試験から呼ぶことで、台帳へ行を足すときに規律が効く。
///
/// # Errors
///
/// 判断記録の無い未確認モデルがある、CC BY 系なのに帰属表示が空。
pub fn check(models: &[ModelEntry]) -> Result<()> {
    for m in models {
        if m.needs_judgement() && m.judgement.is_empty() {
            return Err(LedgerError::UnjudgedModel);
        }
        if m.needs_attribution() && m.attribution.is_empty() {
            return Err(LedgerError::MissingAttribution);
        }
    }
    Ok(())
}

/// アプリ内のライセンス表記の本文（`TR-ALN-31`, `DEC-ALN-008`）。
///
/// **CC BY 系は帰属表示・ライセンス・変更の明示を必ず含める。**
#[must_use]
pub fn notice(models: &[ModelEntry]) -> String {
    let mut s = String::from("# 同梱しているモデルと辞書\n");
    for m in models {
        s.push_str(&format!(
            "\n## {}\n\n- 用途: {}\n- 出所: {}\n- ライセンス: {}\n",
            m.name, m.purpose, m.source_url, m.license
        ));
        if !m.attribution.is_empty() {
            s.push_str(&format!("- 帰属表示: {}\n", m.attribution));
        }
        s.push_str(&format!("- 学習データ: {}\n", m.corpus));
        if m.corpus_status != CorpusStatus::Ok {
            s.push_str(&format!(
                "- 学習データの条件: {}（判断記録 {}）\n",
                match m.corpus_status {
                    CorpusStatus::Restricted => "再配布禁止・非商用限定が含まれる",
                    CorpusStatus::Unverified => "未確認",
                    CorpusStatus::Ok => unreachable!(),
                },
                m.judgement
            ));
        }
        if !m.note.is_empty() {
            s.push_str(&format!("- 補足: {}\n", m.note));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 台帳を読める() {
        let m = models().expect("読める");
        assert!(m.len() >= 3);
        assert!(m.iter().any(|e| e.id == "mfa-japanese-acoustic"));
    }

    /// **判断記録の無い未確認モデルを載せられない**（`TR-ALN-31`）。
    /// 台帳へ行を足すときに、この試験が規律になる。
    #[test]
    fn 台帳の規律を満たしている() {
        check(&models().expect("読める")).expect("規律を満たす");
    }

    /// **MFA は判断で通したもの。** 判断記録の ID が併記されている。
    #[test]
    fn 判断で通したモデルには判断記録がある() {
        let m = models().expect("読める");
        let mfa = m
            .iter()
            .find(|e| e.id == "mfa-japanese-acoustic")
            .expect("ある");
        assert_eq!(mfa.corpus_status, CorpusStatus::Restricted);
        assert!(mfa.needs_judgement());
        assert_eq!(mfa.judgement, "DEC-ALN-008");
    }

    /// **未確認を空欄にしない**（`TR-ALN-31`）。
    #[test]
    fn 未確認は未確認と書いてある() {
        let m = models().expect("読める");
        let julius = m
            .iter()
            .find(|e| e.id == "julius-segmentation-kit")
            .expect("ある");
        assert_eq!(julius.corpus_status, CorpusStatus::Unverified);
        assert!(julius.corpus.contains("未確認"));
        assert_eq!(julius.commercial_ok, Commercial::Unverified);
    }

    /// **CC BY 系は帰属表示が要る**（`DEC-ALN-008`）。
    #[test]
    fn cc_by_のモデルは帰属表示を持つ() {
        for m in models().expect("読める") {
            if m.needs_attribution() {
                assert!(!m.attribution.is_empty(), "{} の帰属表示が空", m.id);
            }
        }
    }

    /// **判断記録が無ければ検査が落ちる。**
    #[test]
    fn 判断記録の無い未確認モデルは落ちる() {
        let mut m = models().expect("読める");
        m[0].judgement.clear();
        assert_eq!(check(&m), Err(LedgerError::UnjudgedModel));
    }

    #[test]
    fn 帰属表示の抜けも落ちる() {
        let mut m = models().expect("読める");
        for e in &mut m {
            e.judgement = "DEC-X".to_owned();
            if e.needs_attribution() {
                e.attribution.clear();
            }
        }
        assert_eq!(check(&m), Err(LedgerError::MissingAttribution));
    }

    /// **ライセンス表記に帰属表示と学習データの条件が出る**（`TR-ALN-31`）。
    #[test]
    fn ライセンス表記に必要なものが出る() {
        let n = notice(&models().expect("読める"));
        assert!(n.contains("CC-BY-4.0"));
        assert!(n.contains("Montreal Forced Aligner"));
        assert!(n.contains("DEC-ALN-008"));
        assert!(n.contains("再配布禁止・非商用限定が含まれる"));
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            LedgerError::Malformed,
            LedgerError::MissingField,
            LedgerError::UnjudgedModel,
            LedgerError::MissingAttribution,
        ] {
            assert!(e.kind().starts_with("ledger."));
        }
    }
}
