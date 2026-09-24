//! 失敗の語彙。分類・確定の状態・次の手の決め方は `DEC-PLT-038`。
//!
//! 個々のエラー型はこの crate に置かない。 各 crate が自分の `thiserror` の列挙体に
//! [`Failure`] を実装し、変種ごとに code と分類を返す。境界の対応表で code から
//! 分類を引く形にすると、変種を足しても型が分類の書き忘れを教えなくなる。
//!
//! どの crate からも引けるよう葉に置く（`DEC-PLT-034`）。 `koeru-audio` は
//! 他の KOERU crate に依存しないので、共通の型はその下に要る。

use std::io;

/// 呼び出し側が次に何をするかで分けた失敗の種類。
///
/// crate の名前や、どの層で起きたかでは分けない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Class {
    /// 構文・型・大きさの違反。直さずに再試行しても同じ結果になる。
    InvalidInput,
    /// 要求は正しいが、今の状態では受けられない。
    Rejected,
    /// 読んだものが古い、または対象がもう無い。
    Conflict,
    /// 取り消した。正常な終わりの1つ。
    Cancelled,
    /// 取り消しが遅く、もう確定していた。
    AlreadyCommitted,
    /// 混んでいる。
    Busy,
    /// マイクや出力が使えない。無音を録音として扱わない。
    DeviceUnavailable,
    /// 一時的な入出力の失敗。確定したかどうかを照合してから再試行する。
    TransientIo,
    /// 容量が足りない、または権限が無い。
    Storage,
    /// 保存されたものが壊れている。自動で正常化しない。
    Corrupt,
    /// この環境では使えない。
    Unsupported,
    /// 外部エンジン（アライナ・合成）の失敗。元のデータは残る。
    EngineFailed,
    /// 内部の不変条件が破れた。欠陥として報告する。
    Internal,
}

impl Class {
    /// 記録と画面に渡す固定の名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::Rejected => "rejected",
            Self::Conflict => "conflict",
            Self::Cancelled => "cancelled",
            Self::AlreadyCommitted => "already_committed",
            Self::Busy => "busy",
            Self::DeviceUnavailable => "device_unavailable",
            Self::TransientIo => "transient_io",
            Self::Storage => "storage",
            Self::Corrupt => "corrupt",
            Self::Unsupported => "unsupported",
            Self::EngineFailed => "engine_failed",
            Self::Internal => "internal",
        }
    }

    /// 分類だけから言える次の手。
    ///
    /// 同じ分類でも code によって手が違うことがある（デバイスは選び直すのか、
    /// 許可するのか）。そこは [`Failure::action`] を上書きする。
    #[must_use]
    pub const fn action(self) -> Action {
        match self {
            Self::InvalidInput => Action::FixInput,
            Self::Rejected => Action::MeetCondition,
            Self::Conflict => Action::Refresh,
            Self::Cancelled => Action::Nothing,
            Self::AlreadyCommitted => Action::ShowReceipt,
            Self::Busy => Action::Wait,
            Self::DeviceUnavailable => Action::Reconnect,
            Self::TransientIo => Action::RetrySame,
            Self::Storage => Action::FreeSpace,
            Self::Corrupt => Action::Recover,
            Self::Unsupported => Action::UseAvailable,
            Self::EngineFailed => Action::Rebuild,
            Self::Internal => Action::Report,
        }
    }

    /// 分類だけから言える確定の状態。
    ///
    /// **持ち主が知っているなら、そちらで上書きする。** 確定したあとの工程で
    /// 起きた失敗は、分類が何であれ確定済みになる。分類から決めてよいのは、
    /// 文脈が無いときの控えめな既定だけで、分からないものは [`Outcome::Unknown`] に倒す。
    #[must_use]
    pub const fn outcome(self) -> Outcome {
        match self {
            Self::InvalidInput | Self::Unsupported => Outcome::NotStarted,
            Self::Rejected
            | Self::Conflict
            | Self::Cancelled
            | Self::Busy
            | Self::DeviceUnavailable => Outcome::NotCommitted,
            Self::AlreadyCommitted => Outcome::Committed,
            Self::TransientIo
            | Self::Storage
            | Self::Corrupt
            | Self::EngineFailed
            | Self::Internal => Outcome::Unknown,
        }
    }
}

/// 操作が確定したかどうか。
///
/// 再試行してよいかの真偽値1つでは足りない。 確定していないなら同じ操作をやり直せるが、
/// 不明なら先に受領証を照会する。新しい収録を作って解かない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Outcome {
    NotStarted,
    NotCommitted,
    Committed,
    Unknown,
}

impl Outcome {
    /// 記録と画面に渡す固定の名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NotStarted => "not_started",
            Self::NotCommitted => "not_committed",
            Self::Committed => "committed",
            Self::Unknown => "unknown",
        }
    }
}

/// 呼び出し側の次の手。 画面はこれで分岐し、表示文を解析しない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Action {
    FixInput,
    /// 足りない条件を満たしてから、もう一度操作する。
    MeetCondition,
    /// 最新を読み直す。古い要求を自動で当て直さない。
    Refresh,
    Nothing,
    ShowReceipt,
    /// 待つ。再試行は利用者が明示する。
    Wait,
    /// デバイスを選び直す、つなぎ直す。
    Reconnect,
    /// OS の許可を与える。
    Grant,
    /// 確定したかを照合してから、同じ操作をやり直す。
    RetrySame,
    /// 容量や権限を直す。
    FreeSpace,
    Recover,
    UseAvailable,
    /// 派生物を作り直す。元のデータには触らない。
    Rebuild,
    Report,
}

impl Action {
    /// 記録と画面に渡す固定の名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FixInput => "fix_input",
            Self::MeetCondition => "meet_condition",
            Self::Refresh => "refresh",
            Self::Nothing => "nothing",
            Self::ShowReceipt => "show_receipt",
            Self::Wait => "wait",
            Self::Reconnect => "reconnect",
            Self::Grant => "grant",
            Self::RetrySame => "retry_same",
            Self::FreeSpace => "free_space",
            Self::Recover => "recover",
            Self::UseAvailable => "use_available",
            Self::Rebuild => "rebuild",
            Self::Report => "report",
        }
    }
}

/// エラー型が実装する。
///
/// code は `recording.not_enough_space` のような点区切りの固定文字列で、型をまたいで
/// 一意にする。code を変えるのは契約の変更。 `Display` は利用者に見せる固定の文で、
/// 利用者のデータを差し込まない。どちらも `koeru-app` の試験が source を読んで確かめる。
pub trait Failure: std::error::Error {
    fn code(&self) -> &'static str;
    fn class(&self) -> Class;

    fn action(&self) -> Action {
        self.class().action()
    }
}

/// OS の入出力の失敗を分類する。
///
/// 見つからないものを [`Class::Corrupt`] に倒すのは、KOERU が読みに行くファイルが
/// 自分で書いたものだけだから。 プロジェクトの中で要るものが無いのは、
/// 呼び出し側の誤りではなく、保存されたものが欠けている。
#[must_use]
pub fn io_class(e: &io::Error) -> Class {
    use io::ErrorKind as K;
    match e.kind() {
        K::StorageFull | K::QuotaExceeded | K::PermissionDenied | K::ReadOnlyFilesystem => {
            Class::Storage
        }
        K::NotFound => Class::Corrupt,
        K::Unsupported => Class::Unsupported,
        _ => Class::TransientIo,
    }
}

/// 失敗を記録する。 結果を決める持ち主が、1回だけ呼ぶ（`DEC-PLT-038`）。
///
/// 載せるのは code・分類・確定の状態・段だけ。 `Display` も source も載せない。
/// `phase` はどの工程で起きたかを示す固定の名前で、画面から来た値を入れない。
pub fn record(code: &'static str, class: Class, outcome: Outcome, phase: &'static str) {
    let (name, outcome) = (class.as_str(), outcome.as_str());
    match class {
        Class::Internal => tracing::error!(code, class = name, outcome, phase, "失敗した"),
        Class::Cancelled => tracing::debug!(code, class = name, outcome, phase, "取り消した"),
        _ => tracing::warn!(code, class = name, outcome, phase, "失敗した"),
    }
}

/// [`Failure`] を記録する。 呼び出し側が失敗を畳まずにその場で結果を決める
/// （縮退して続ける、仕事をやめる）ときに使う。
pub fn record_failure<E: Failure + ?Sized>(e: &E, outcome: Outcome, phase: &'static str) {
    record(e.code(), e.class(), outcome, phase);
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL: [Class; 13] = [
        Class::InvalidInput,
        Class::Rejected,
        Class::Conflict,
        Class::Cancelled,
        Class::AlreadyCommitted,
        Class::Busy,
        Class::DeviceUnavailable,
        Class::TransientIo,
        Class::Storage,
        Class::Corrupt,
        Class::Unsupported,
        Class::EngineFailed,
        Class::Internal,
    ];

    #[test]
    fn 分類の名前は重ならない() {
        let mut seen = std::collections::BTreeSet::new();
        for c in ALL {
            assert!(seen.insert(c.as_str()), "{} が重なる", c.as_str());
        }
    }

    #[test]
    fn 確定したと言える分類は取り消しの遅れだけ() {
        // 分類だけで確定済みと言い切ると、確定前の失敗を「保存済み」と見せる。
        let committed: Vec<_> = ALL
            .into_iter()
            .filter(|c| c.outcome() == Outcome::Committed)
            .collect();
        assert_eq!(committed, [Class::AlreadyCommitted]);
    }

    #[test]
    fn 容量と権限の失敗は容量の分類になる() {
        for kind in [
            io::ErrorKind::StorageFull,
            io::ErrorKind::PermissionDenied,
            io::ErrorKind::ReadOnlyFilesystem,
        ] {
            assert_eq!(io_class(&io::Error::from(kind)), Class::Storage);
        }
        assert_eq!(
            io_class(&io::Error::from(io::ErrorKind::Interrupted)),
            Class::TransientIo
        );
    }
}
