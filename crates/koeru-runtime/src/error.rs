//! `ProjectRuntime` / `ProjectReadSession` の操作が失敗した理由（`DEC-PLT-038`）。
//!
//! ドメイン層（`koeru-core` の `LedgerError` / `ProjectError`）はそのまま包む。
//! この crate 自身が持つ失敗は、貸与の失効と mutex の毒だけ。

use koeru_core::db::LedgerError;
use koeru_core::project::ProjectError;
use koeru_failure::{Class, Failure};

/// `ProjectRuntime` / `ProjectReadSession` の操作が失敗した理由。
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// 貸与がこの実行時のものと違う、または閉じたあとに使われた
    /// （`specs/application/schema/project.graphql` の `ProjectLeaseExpired`）。
    #[error("project への貸与がもう生きていない")]
    LeaseExpired,

    /// この実行時が持つ mutex が毒されている（別のスレッドがロックを持ったまま panic した）。
    ///
    /// 単一の書き手・読み手プールのどちらでも起きうる。 場所を区別しても
    /// 呼び出し側が取れる手は変わらないので、1つにまとめてある。
    #[error("実行時の内部状態が壊れている")]
    Poisoned,

    #[error(transparent)]
    Ledger(#[from] LedgerError),

    #[error(transparent)]
    Project(#[from] ProjectError),
}

impl Failure for RuntimeError {
    fn code(&self) -> &'static str {
        match self {
            Self::LeaseExpired => "runtime.lease_expired",
            Self::Poisoned => "runtime.poisoned",
            Self::Ledger(e) => e.code(),
            Self::Project(e) => e.code(),
        }
    }

    fn class(&self) -> Class {
        match self {
            // `Class::Conflict` の doc は「読んだものが古い、または対象がもう無い」。
            // 貸与は閉じて開き直すと別の値になるので、古い貸与での要求は
            // 「（この実行時という）対象がもう無い」に当たる——貸与の値そのものが
            // 「開いている project」の存在を表す（`lease.rs` のドキュメント）。
            Self::LeaseExpired => Class::Conflict,
            // mutex の毒は内部の不変条件が壊れたということ。呼び出し側が
            // 直せる入力の誤りではない。
            Self::Poisoned => Class::Internal,
            Self::Ledger(e) => e.class(),
            Self::Project(e) => e.class(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 貸与の失効は_conflict() {
        assert_eq!(RuntimeError::LeaseExpired.class(), Class::Conflict);
        assert_eq!(RuntimeError::LeaseExpired.code(), "runtime.lease_expired");
    }

    #[test]
    fn 毒された_mutex_は_internal() {
        assert_eq!(RuntimeError::Poisoned.class(), Class::Internal);
        assert_eq!(RuntimeError::Poisoned.code(), "runtime.poisoned");
    }

    #[test]
    fn 下の層の失敗は畳んでも_code_と分類を保つ() {
        let e = RuntimeError::from(LedgerError::UnknownTake);
        assert_eq!(e.code(), "ledger.unknown_take");
        assert_eq!(e.class(), Class::Conflict);
    }
}
