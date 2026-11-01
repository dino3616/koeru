//! 利用状況とクラッシュ報告の同意（`TR-PKG-54`, `TR-PKG-57`）。
//!
//! 状態と遷移は `specs/requirements/telemetry-consent.fsl` が持つ。 同意は製品全体で
//! 1つで、project には属さない。
//!
//! 「訊いたかどうか」（FSL の `asked`）はまだ持たない。 同意を求める画面が無く、
//! ここへ来るのは本人が設定を開いて選んだときだけ。

/// 1つの送るものへの同意。 既定は [`Consent::NotAsked`] で、送らない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Consent {
    #[default]
    NotAsked,
    Granted,
    Revoked,
}

impl Consent {
    /// 送ってよいか。 訊いていないのも、撤回したのも、送らない。
    #[must_use]
    pub const fn allows_sending(self) -> bool {
        matches!(self, Self::Granted)
    }

    /// 本人が選んだ向きへ動かす。
    ///
    /// 訊く前にオフを選んでも撤回にはしない。 FSL の撤回は同意からだけ起きる。
    #[must_use]
    pub const fn choose(self, granted: bool) -> Self {
        match (self, granted) {
            (_, true) => Self::Granted,
            (Self::Granted, false) => Self::Revoked,
            (other, false) => other,
        }
    }
}

/// 同意の単位。 クラッシュ報告は利用状況と別に選ぶ（`TR-PKG-57`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConsentSubject {
    Usage,
    CrashReports,
}

/// 2つの同意。 片方を選んでも、もう片方は動かない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Consents {
    pub usage: Consent,
    pub crash_reports: Consent,
}

impl Consents {
    #[must_use]
    pub const fn of(self, subject: ConsentSubject) -> Consent {
        match subject {
            ConsentSubject::Usage => self.usage,
            ConsentSubject::CrashReports => self.crash_reports,
        }
    }

    #[must_use]
    pub const fn choose(mut self, subject: ConsentSubject, granted: bool) -> Self {
        match subject {
            ConsentSubject::Usage => self.usage = self.usage.choose(granted),
            ConsentSubject::CrashReports => {
                self.crash_reports = self.crash_reports.choose(granted);
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 既定はどちらも訊いておらず送らない() {
        let c = Consents::default();
        assert_eq!(c.usage, Consent::NotAsked);
        assert_eq!(c.crash_reports, Consent::NotAsked);
        assert!(!c.usage.allows_sending());
        assert!(!c.crash_reports.allows_sending());
    }

    #[test]
    fn 同意したものだけが送れる() {
        assert!(Consent::Granted.allows_sending());
        assert!(!Consent::Revoked.allows_sending());
    }

    #[test]
    fn 撤回は同意からだけ起きる() {
        assert_eq!(Consent::Granted.choose(false), Consent::Revoked);
        assert_eq!(Consent::NotAsked.choose(false), Consent::NotAsked);
        assert_eq!(Consent::Revoked.choose(false), Consent::Revoked);
        assert_eq!(Consent::Revoked.choose(true), Consent::Granted);
    }

    #[test]
    fn 利用状況に同意してもクラッシュ報告は動かない() {
        let c = Consents::default().choose(ConsentSubject::Usage, true);
        assert_eq!(c.of(ConsentSubject::Usage), Consent::Granted);
        assert_eq!(c.of(ConsentSubject::CrashReports), Consent::NotAsked);

        let c = c.choose(ConsentSubject::CrashReports, true);
        let c = c.choose(ConsentSubject::Usage, false);
        assert_eq!(c.usage, Consent::Revoked);
        assert_eq!(c.crash_reports, Consent::Granted);
    }
}
