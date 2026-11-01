//! 利用状況とクラッシュ報告の同意を、アプリの設定として持つ（`TR-PKG-54`, `TR-PKG-57`）。
//!
//! 同意は製品全体で1つなので、ライブラリの外に1つのファイルで置く。 project の
//! 台帳には入れない——project が2つになった瞬間に同意が複製される
//! （`specs/requirements/telemetry-consent.fsl`）。
//!
//! 送る経路はまだ無い（`Q-TEL-001`）。 ここは同意と送った記録を読み書きするだけで、
//! 何も送らない。

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use koeru_failure::{Class, Failure, Outcome};
use koeru_model::consent::{Consent, ConsentSubject, Consents};
use serde::{Deserialize, Serialize};

/// 同意と送った記録。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PrivacySettings {
    pub consents: Consents,
    /// 最後に送った時刻（RFC 3339）。 送る経路が無いので、今は誰も書かない。
    pub last_sent_at: Option<String>,
}

/// 同意を読み書きできなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum PrivacyError {
    #[error("プライバシーの設定を読めない")]
    Read(#[source] io::Error),

    #[error("プライバシーの設定を保存できない")]
    Write(#[source] io::Error),

    /// 読めたが中身が形になっていない。 呼び出し側へは返さず、その場で記録する
    /// （[`PrivacyStore::load`]）。
    #[error("プライバシーの設定が壊れている")]
    Corrupt(#[source] serde_json::Error),
}

impl Failure for PrivacyError {
    fn code(&self) -> &'static str {
        match self {
            Self::Read(_) => "privacy.unreadable",
            Self::Write(_) => "privacy.unwritable",
            Self::Corrupt(_) => "privacy.corrupt",
        }
    }

    fn class(&self) -> Class {
        match self {
            Self::Read(e) | Self::Write(e) => koeru_failure::io_class(e),
            Self::Corrupt(_) => Class::Corrupt,
        }
    }
}

/// ファイルの形。 同意は固定の語で持つ。
///
/// 知らない語は「訊いていない」として読む。 同意の無いものを送らないほうへ倒す。
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(default)]
struct Stored {
    usage: String,
    crash_reports: String,
    last_sent_at: Option<String>,
}

const NOT_ASKED: &str = "not_asked";
const GRANTED: &str = "granted";
const REVOKED: &str = "revoked";

const fn word(c: Consent) -> &'static str {
    match c {
        Consent::NotAsked => NOT_ASKED,
        Consent::Granted => GRANTED,
        Consent::Revoked => REVOKED,
    }
}

fn consent(word: &str) -> Consent {
    match word {
        GRANTED => Consent::Granted,
        REVOKED => Consent::Revoked,
        _ => Consent::NotAsked,
    }
}

impl From<Stored> for PrivacySettings {
    fn from(s: Stored) -> Self {
        Self {
            consents: Consents {
                usage: consent(&s.usage),
                crash_reports: consent(&s.crash_reports),
            },
            last_sent_at: s.last_sent_at,
        }
    }
}

impl From<&PrivacySettings> for Stored {
    fn from(p: &PrivacySettings) -> Self {
        Self {
            usage: word(p.consents.usage).to_owned(),
            crash_reports: word(p.consents.crash_reports).to_owned(),
            last_sent_at: p.last_sent_at.clone(),
        }
    }
}

/// 同意を置くファイル。
#[derive(Debug)]
pub struct PrivacyStore {
    path: PathBuf,
    /// 読んで書き戻すあいだ、別の変更を待たせる。 2つのトグルを続けて押すと、
    /// 後の書き込みが先の変更を古い値で上書きする。
    ///
    /// 中身を持たないので、毒されても壊れた状態が残らない。 毒は無視する。
    writing: Mutex<()>,
}

impl PrivacyStore {
    /// `path` のファイルを使う。 ここでは読まず、作らない。
    #[must_use]
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            writing: Mutex::new(()),
        }
    }

    /// 今の同意。 ファイルが無ければ既定（どちらも訊いていない）。
    ///
    /// 壊れていても既定として読む。 同意を確かめられないものは、同意が無いものと
    /// して扱う（`TR-PKG-54`）。 ファイルはそのまま残し、本人が次に選んだときに
    /// 書き直す。
    ///
    /// # Errors
    ///
    /// ファイルはあるが読めない（権限が無いなど）。
    pub fn load(&self) -> Result<PrivacySettings, PrivacyError> {
        let bytes = match fs::read(&self.path) {
            Ok(b) => b,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return Ok(PrivacySettings::default());
            }
            Err(e) => return Err(PrivacyError::Read(e)),
        };
        match serde_json::from_slice::<Stored>(&bytes) {
            Ok(s) => Ok(s.into()),
            Err(e) => {
                koeru_failure::record_failure(
                    &PrivacyError::Corrupt(e),
                    Outcome::NotCommitted,
                    "privacy.load",
                );
                Ok(PrivacySettings::default())
            }
        }
    }

    /// 1つの同意を本人の選んだ向きへ動かし、保存したあとの値を返す。
    ///
    /// # Errors
    ///
    /// 読めない、または書けない。 書けなかったときファイルは前のまま。
    #[tracing::instrument(skip_all)]
    pub fn choose(
        &self,
        subject: ConsentSubject,
        granted: bool,
    ) -> Result<PrivacySettings, PrivacyError> {
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut settings = self.load()?;
        settings.consents = settings.consents.choose(subject, granted);
        self.save(&settings)?;
        Ok(settings)
    }

    /// 一時ファイルへ書いてから置き換える。 途中で落ちても、前の同意か新しい同意の
    /// どちらかが残る。
    fn save(&self, settings: &PrivacySettings) -> Result<(), PrivacyError> {
        let text = serde_json::to_vec_pretty(&Stored::from(settings))
            .map_err(|e| PrivacyError::Write(io::Error::other(e)))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir).map_err(PrivacyError::Write)?;
        }
        let tmp = self.path.with_extension("json.tmp");
        if let Err(e) = replace(&tmp, &self.path, &text) {
            let _ = fs::remove_file(&tmp);
            return Err(PrivacyError::Write(e));
        }
        Ok(())
    }
}

fn replace(tmp: &Path, path: &Path, bytes: &[u8]) -> io::Result<()> {
    use io::Write as _;
    let mut f = fs::File::create(tmp)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "koeru-privacy-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&dir).expect("一時ディレクトリを作れること");
        dir.join("privacy.json")
    }

    #[test]
    fn ファイルが無ければどちらもオフで送った記録も無い() {
        let path = tmp("default");
        let store = PrivacyStore::at(&path);
        let p = store.load().expect("読めること");
        assert_eq!(p, PrivacySettings::default());
        assert!(!p.consents.usage.allows_sending());
        assert!(!p.consents.crash_reports.allows_sending());
        assert_eq!(p.last_sent_at, None);
        assert!(!path.exists(), "読むだけでファイルを作らない");
    }

    #[test]
    fn 選んだ同意は開き直しても残る() {
        let path = tmp("roundtrip");
        let p = PrivacyStore::at(&path)
            .choose(ConsentSubject::CrashReports, true)
            .expect("保存できること");
        assert_eq!(p.consents.crash_reports, Consent::Granted);
        assert_eq!(p.consents.usage, Consent::NotAsked);

        let again = PrivacyStore::at(&path).load().expect("読めること");
        assert_eq!(again, p);
        assert!(
            !path.with_extension("json.tmp").exists(),
            "一時ファイルを残さない"
        );
    }

    #[test]
    fn 撤回したものは撤回として残る() {
        let path = tmp("revoke");
        let store = PrivacyStore::at(&path);
        store
            .choose(ConsentSubject::Usage, true)
            .expect("保存できること");
        let p = store
            .choose(ConsentSubject::Usage, false)
            .expect("保存できること");
        assert_eq!(p.consents.usage, Consent::Revoked);
        assert_eq!(
            store.load().expect("読めること").consents.usage,
            Consent::Revoked
        );
    }

    #[test]
    fn 壊れたファイルは同意が無いものとして読み_消さない() {
        let path = tmp("corrupt");
        fs::write(&path, b"{ not json").expect("書けること");
        let p = PrivacyStore::at(&path).load().expect("読めること");
        assert_eq!(p, PrivacySettings::default());
        assert_eq!(fs::read(&path).expect("残っていること"), b"{ not json");
    }

    #[test]
    fn 知らない語は訊いていないとして読む() {
        let path = tmp("unknown");
        fs::write(
            &path,
            br#"{"usage":"maybe","crash_reports":"granted","last_sent_at":null}"#,
        )
        .expect("書けること");
        let p = PrivacyStore::at(&path).load().expect("読めること");
        assert_eq!(p.consents.usage, Consent::NotAsked);
        assert_eq!(p.consents.crash_reports, Consent::Granted);
    }

    #[test]
    fn 失敗の_code_と分類() {
        let e = PrivacyError::Write(io::Error::from(io::ErrorKind::PermissionDenied));
        assert_eq!(e.code(), "privacy.unwritable");
        assert_eq!(
            e.class(),
            koeru_failure::io_class(&io::Error::from(io::ErrorKind::PermissionDenied))
        );
        let e = PrivacyError::Corrupt(
            serde_json::from_slice::<Stored>(b"x").expect_err("壊れていること"),
        );
        assert_eq!(e.code(), "privacy.corrupt");
        assert_eq!(e.class(), Class::Corrupt);
    }
}
