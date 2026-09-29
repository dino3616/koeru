// JobId — ジョブの一意な識別子（shared.graphql の `JobId` scalar）。
//
// `koeru-model::id::RowId` / `TakeId` と同じパターンで、外部生成の UUID 文字列。
// serde::Serialize / Deserialize / Copy を実装して、シリアライズとコピー可能。

use serde::{Deserialize, Serialize};

/// ジョブの一意な識別子。
///
/// GraphQL スカラ `JobId` の Rust 実装。
/// 外部（画面側）で `uuid::Uuid::new_v4().to_string()` 等によって生成し、
/// Rust コードからは ID として使用のみ。
///
/// UUID v4 の数値を u128 に格納する。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct JobId(pub u128);

impl JobId {
    /// 生成した UUID v4 の数値を返す。
    pub fn as_u128(&self) -> u128 {
        self.0
    }
}

impl std::fmt::Display for JobId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Format as UUID v4 string: 8-4-4-4-12
        let h = format!("{:032x}", self.0);
        write!(
            f,
            "{}-{}-{}-{}-{}",
            &h[0..8],
            &h[8..12],
            &h[12..16],
            &h[16..20],
            &h[20..32]
        )
    }
}

/// 有効な UUID v4 文字列から `JobId` を生成する。
///
/// 無効な形式の場合は `None` を返す（失敗時は呼び出し側が失敗処理へ）。
pub fn from_uuid_v4(uuid: uuid::Uuid) -> JobId {
    JobId(uuid.as_u128())
}

/// JobId 生成のエラー。
#[derive(Debug)]
pub enum JobIdError {
    InvalidFormat,
}

impl std::str::FromStr for JobId {
    type Err = JobIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        // Parse as UUID string (8-4-4-4-12 format)
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 5 {
            return Err(JobIdError::InvalidFormat);
        }
        let hex: String = parts.join("");
        if hex.len() != 32 {
            return Err(JobIdError::InvalidFormat);
        }
        u128::from_str_radix(&hex, 16)
            .map(JobId)
            .map_err(|_| JobIdError::InvalidFormat)
    }
}

impl std::convert::From<uuid::Uuid> for JobId {
    fn from(uuid: uuid::Uuid) -> Self {
        Self(uuid.as_u128())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_from_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let id = from_uuid_v4(uuid);
        assert_ne!(id.0, 0);
    }

    #[test]
    fn display_formats_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let id = from_uuid_v4(uuid);
        let s = id.to_string();
        // Check UUID format: 8-4-4-4-12
        assert_eq!(s.len(), 36);
        let chars: Vec<char> = s.chars().collect();
        assert_eq!(chars[8], '-');
        assert_eq!(chars[13], '-');
        assert_eq!(chars[18], '-');
        assert_eq!(chars[23], '-');
    }

    #[test]
    fn from_str_valid_uuid() {
        let uuid = uuid::Uuid::new_v4();
        let s = uuid.to_string();
        let id = s.parse::<JobId>().expect("parse");
        assert_eq!(id.0, uuid.as_u128());
    }

    #[test]
    fn from_str_invalid_uuid() {
        let result = "invalid".parse::<JobId>();
        assert!(result.is_err());
    }
}
