//! project への貸与（`specs/application/schema/shared.graphql` の `ProjectLease` scalar）。
//!
//! **貸与そのものが世代（epoch）。** 閉じて開き直すと別の値になり、同じ値を
//! 二度と振らない（`docs/reports/architecture/00-architecture-report.md` §14）。
//! view の mount / unmount や、同じ project へ2つ目の consumer が繋いだだけでは
//! 作り直さない——作り直すのは [`crate::runtime::ProjectRuntime::open`] だけ。

use std::fmt;
use std::str::FromStr;

use uuid::Uuid;

/// 開いた project への貸与。
///
/// ランダムな UUID v4。 連番にしないのは、復元・移行のあとに同じ値が出て
/// 「同じ貸与」に見えることを防ぐため——`DEC-PLT-043` が版のスコープを
/// 「同じ project の同じ開いている貸与」に閉じているのと同じ理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ProjectLease(Uuid);

impl ProjectLease {
    /// 新しい貸与を振る。 [`crate::runtime::ProjectRuntime::open`] だけが呼ぶ——
    /// それ以外の場所で作ると、貸与が「開いた」以外の契機でも増えてしまう。
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for ProjectLease {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// wire から読み戻した文字列が UUID の形でない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("project への貸与の形が読めない")]
pub struct ParseLeaseError(());

impl koeru_failure::Failure for ParseLeaseError {
    fn code(&self) -> &'static str {
        "runtime.lease_parse_failed"
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

impl FromStr for ProjectLease {
    type Err = ParseLeaseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| ParseLeaseError(()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 開くたびに違う値を振る() {
        let a = ProjectLease::generate();
        let b = ProjectLease::generate();
        assert_ne!(a, b, "同じ値を二度と振らないこと");
    }

    #[test]
    fn 表示した文字列を読み戻せる() {
        let lease = ProjectLease::generate();
        let text = lease.to_string();
        let back: ProjectLease = text.parse().expect("読み戻せること");
        assert_eq!(back, lease);
    }

    #[test]
    fn uuid_の形でない文字列は読まない() {
        assert!("not-a-uuid".parse::<ProjectLease>().is_err());
        assert!("".parse::<ProjectLease>().is_err());
    }
}
