//! 仕事の識別子・試行の番号・公開の受領証。

use std::fmt;
use std::num::NonZeroU32;
use std::str::FromStr;

use uuid::Uuid;

/// 仕事1件の識別子（`specs/application/schema/shared.graphql` の `JobId` scalar）。
///
/// ランダムな UUID v4。 同じ仕事をやり直しても（新しい [`Attempt`] を振っても）
/// 変わらない。 変わるのは試行の番号だけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JobId(Uuid);

impl JobId {
    /// 新しい仕事に振る。 `JobRuntime` の投入だけが呼ぶ。
    pub(crate) fn generate() -> Self {
        Self(Uuid::new_v4())
    }
}

impl fmt::Display for JobId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// wire から読み戻した文字列が UUID の形でない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[error("仕事の識別子の形が読めない")]
pub struct ParseJobIdError(());

impl koeru_failure::Failure for ParseJobIdError {
    fn code(&self) -> &'static str {
        "jobs.job_id_parse_failed"
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

impl FromStr for JobId {
    type Err = ParseJobIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Uuid::parse_str(s)
            .map(Self)
            .map_err(|_| ParseJobIdError(()))
    }
}

/// 何回目の試行か。 1 始まり。
///
/// 同じ仕事をやり直すと1つ進む。 古い試行の完了は、新しい試行が走り始めたあとでは
/// 公開されない（`PublishRefused::Superseded`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Attempt(NonZeroU32);

impl Attempt {
    /// 最初の試行。
    pub const FIRST: Self = Self(NonZeroU32::MIN);

    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }

    /// 次の試行。 `u32` を使い切ることは無いが、溢れても戻らないよう飽和させる。
    pub(crate) fn next(self) -> Self {
        NonZeroU32::new(self.0.get().saturating_add(1)).map_or(self, Self)
    }
}

impl fmt::Display for Attempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, f)
    }
}

/// 公開が確定したことの受領証。
///
/// 中身は呼び出し側が決める（台帳へ書いたときの版の番号など）。 `JobRuntime` は
/// 解釈せず、同じ仕事に対して同じ値を返すだけ——`cancel` が公開のあとに
/// `CancelOutcome::AlreadyCommitted` で返す値は、公開が返した値と一致する。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Receipt(u64);

impl Receipt {
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// 同じ種類の待ちを1つにまとめる鍵（`OverflowPolicy::Coalesce`）。
///
/// 中身は呼び出し側のもの（対象のテイクの ID など）で、`JobRuntime` は
/// 等しいかだけを見る。 **トレースには載せない。**
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct CoalesceKey(Box<str>);

impl CoalesceKey {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self(key.into().into_boxed_str())
    }
}

impl fmt::Debug for CoalesceKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // 中身は対象を指しうるので、`Debug` でも出さない。
        f.write_str("CoalesceKey(..)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 仕事の識別子は表示した文字列を読み戻せる() {
        let id = JobId::generate();
        let back: JobId = id.to_string().parse().expect("読み戻せること");
        assert_eq!(back, id);
    }

    #[test]
    fn 仕事の識別子は毎回違う() {
        assert_ne!(JobId::generate(), JobId::generate());
    }

    #[test]
    fn uuid_の形でない文字列は読まない() {
        assert!("".parse::<JobId>().is_err());
        assert!("not-a-uuid".parse::<JobId>().is_err());
    }

    #[test]
    fn 試行は1から始まり1つずつ進む() {
        assert_eq!(Attempt::FIRST.get(), 1);
        assert_eq!(Attempt::FIRST.next().get(), 2);
        assert!(Attempt::FIRST < Attempt::FIRST.next());
    }

    #[test]
    fn 試行の番号は溢れても戻らない() {
        let last = Attempt(NonZeroU32::MAX);
        assert_eq!(last.next(), last);
    }

    #[test]
    fn 受領証は渡した値をそのまま返す() {
        assert_eq!(Receipt::new(7).get(), 7);
        assert_eq!(Receipt::new(7), Receipt::new(7));
        assert_ne!(Receipt::new(7), Receipt::new(8));
    }

    #[test]
    fn まとめる鍵は_debug_で中身を出さない() {
        let key = CoalesceKey::new("秘密の音源の名前");
        assert!(!format!("{key:?}").contains("秘密"));
    }
}
