// JobTarget — ジョブのターゲット（`BackgroundJob.take` / `song`）。

use serde::{Deserialize, Serialize};

/// ジョブのターゲット。
///
/// SDL の `BackgroundJob.take: TakeId` および `song: SongId` を表す。
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum JobTarget {
    /// テイク（单个の録音テイク）
    Take(RecordedTake),
    /// 楽曲（SongId を持つ楽曲全体）
    Song(String),
}

/// テイク ID。
///
/// `koeru-model::id::TakeId` のエイリアス。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordedTake(pub i32);

impl RecordedTake {
    /// 新規作成。
    pub fn new(id: i32) -> Self {
        Self(id)
    }

    /// テイク ID の値を返す。
    pub fn value(&self) -> i32 {
        self.0
    }
}
