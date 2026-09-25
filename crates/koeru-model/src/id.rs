//! 取り違えると別のものを指す ID（`DEC-PLT-034`）。
//!
//! 数や文字列のまま持ち回すと、テイクの番号をセッションの番号の欄へ渡しても組み立ては
//! 通る。 ここに置くのは、取り違えが本当に起きる ID だけ。 Hz や件数まで型にしない。
//!
//! 移行中。 台帳（`koeru_core::db`）はまだ素の `String` / `i32` で持っていて、
//! 規則を呼ぶところで写す。 台帳の鍵を差し替えるのは移行の段（`DEC-RCL-017`）。

use std::fmt;

/// 録音リストの行（`TR-RCL-18`）。
///
/// 表示文でも並びの位置でもない。 詰め直した行の ID は本文から導くので
/// （`reclist::packed_id`）、同じ ID が別の本文を指すことはない。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RowId(String);

impl RowId {
    /// 台帳が持つ文字列から。
    #[must_use]
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    /// 台帳へ渡す文字列。
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 台帳へ渡す文字列（持ち主ごと）。
    #[must_use]
    pub fn into_string(self) -> String {
        self.0
    }
}

impl fmt::Display for RowId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// 確定したテイク（`TR-REC-28`, `TR-RCL-25`）。
///
/// **確定した順に増える。** 台帳が自動採番で振るので、小さいほど先に確定した。
/// 綴りの持ち主（`DEC-RCL-016`）はこの順で決まる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TakeId(i32);

impl TakeId {
    /// 台帳が振った番号から。
    #[must_use]
    pub const fn new(id: i32) -> Self {
        Self(id)
    }

    /// 台帳へ渡す番号。
    #[must_use]
    pub const fn get(self) -> i32 {
        self.0
    }
}

impl fmt::Display for TakeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
