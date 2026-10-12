//! `JobRuntime` の設定。 lane の分け方と、上限・保持の値。
//!
//! **上限・期限・保持の既定値はすべて仮の値で、根拠が無い**（`Q-PLT-006`）。
//! 実測が無い値を、実測したような顔で置かない。 決め直す手順は `Q-PLT-006` に
//! 残してある。 値を変えるときは、その論点を見る。

use std::time::Duration;

use super::model::Priority;

/// 前景の lane（録音直後の前処理）が待たせてよい仕事の数。
///
/// **仮の値**（`Q-PLT-006`）。 確定の応答が解析を待つので、待ちが溜まるのは
/// 録音が解析より速く進んだときだけ。 そうなったら増やすのではなく、
/// 保存済みのテイクは `OverflowPolicy::Defer` で失敗にしない。
pub const FOREGROUND_MAX_PENDING: usize = 8;

/// 前景の lane の、待ち＋走っている仕事の申告バイト数の上限。
///
/// **仮の値**（`Q-PLT-006`）。 数テイク分のマスターを同時に抱えられる程度。
pub const FOREGROUND_MAX_INFLIGHT_BYTES: usize = 256 * 1024 * 1024;

/// 試唱の lane が待たせてよい仕事の数。
///
/// **仮の値**（`Q-PLT-006`）。 新しい試唱は古い試唱を取り消してから積むので、
/// 待つのは古い試唱の作業スレッドが戻るまでの新しい1本だけ。 それに余裕を1本足してある。
pub const PREVIEW_MAX_PENDING: usize = 2;

/// 試唱の lane の、待ち＋走っている仕事の申告バイト数の上限。
///
/// **仮の値**（`Q-PLT-006`）。 試唱の仕事が申告するのは、先頭フレーズの合成結果
/// （仕事が抱えて流す最初の塊）の大きさ。 戻りかけの古い試唱と新しい試唱が
/// 並んでも、数十秒の先頭フレーズなら収まる程度。
pub const PREVIEW_MAX_INFLIGHT_BYTES: usize = 64 * 1024 * 1024;

/// 背景の lane（全件再推定とチャート事前計算）が待たせてよい仕事の数。
///
/// **仮の値**（`Q-PLT-006`）。 誰も待っていないので、小さく倒してある。
pub const BACKGROUND_MAX_PENDING: usize = 4;

/// 背景の lane の申告バイト数の上限。 **仮の値**（`Q-PLT-006`）。
pub const BACKGROUND_MAX_INFLIGHT_BYTES: usize = 64 * 1024 * 1024;

/// 1つの lane が、固まったと見なした作業スレッドを何本まで許すか。
///
/// 期限を過ぎた走行中の仕事は、止められない外部エンジンの呼び出しの中にいると
/// 見なし、代わりのスレッドを立てる。 立て続けに固まる環境で、スレッドを
/// 際限なく増やさないための上限。 超えたら、その lane は仕事を断る
/// （`JobError::LaneUnavailable`）。 **仮の値**（`Q-PLT-006`）。
pub const MAX_HUNG_PER_LANE: usize = 2;

/// 終わった仕事を何件まで引けるようにするか。 **仮の値**（`Q-PLT-006`）。
pub const RETAIN_FINISHED_MAX: usize = 256;

/// 終わった仕事をどれだけの間引けるようにするか（`JobQueue.job` の
/// 「保持の期間を過ぎていれば null」）。 **仮の値**（`Q-PLT-006`）。
pub const RETAIN_FINISHED_FOR: Duration = Duration::from_secs(10 * 60);

/// 閉じた貸与を何個まで覚えておくか。 覚えている間は、その貸与での投入を断る。
///
/// 忘れたあとの遅い投入は断れないが、台帳への書き込みは
/// `ProjectRuntime::writer` が貸与を見て断るので、書き戻しは起きない。
/// 貸与は 16 バイトの値で、**仮の値**（`Q-PLT-006`）。
pub const CLOSED_LEASES_REMEMBERED: usize = 64;

/// 公開の途中の仕事を取り消そうとしたとき、公開が終わるのを待つ上限。
///
/// 公開は短い（writer の短い書き込み）ので普通は待ち切れる。 待ち切れないのは
/// writer を長く握っている側がいるときで、そこで取り消す側を止め続けない。
/// **仮の値**（`Q-PLT-006`）。
pub const PUBLISH_WAIT_FOR_CANCEL: Duration = Duration::from_secs(2);

/// lane 1本の設定。
///
/// lane は作業スレッドの束で、どの優先度の仕事を回すかを持つ。 同じ lane の中は
/// 厳密な優先度で、別の lane は互いを待たない（長い背景の仕事が前景の解析を塞がない）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaneConfig {
    pub(crate) name: &'static str,
    pub(crate) priorities: Vec<Priority>,
    pub(crate) threads: usize,
    pub(crate) max_pending: usize,
    pub(crate) max_inflight_bytes: usize,
}

impl LaneConfig {
    /// 名前（記録に出る固定の語）と、この lane が回す優先度。
    #[must_use]
    pub fn new(name: &'static str, priorities: impl IntoIterator<Item = Priority>) -> Self {
        Self {
            name,
            priorities: priorities.into_iter().collect(),
            threads: 1,
            max_pending: BACKGROUND_MAX_PENDING,
            max_inflight_bytes: BACKGROUND_MAX_INFLIGHT_BYTES,
        }
    }

    /// 作業スレッドの数。 既定は1本——重いエンジンの状態を複製しない。
    /// 増やす根拠は、試験で示せたときだけ。
    #[must_use]
    pub const fn threads(mut self, threads: usize) -> Self {
        self.threads = threads;
        self
    }

    /// 待たせてよい仕事の数。 走っている仕事は数えない。
    #[must_use]
    pub const fn max_pending(mut self, max_pending: usize) -> Self {
        self.max_pending = max_pending;
        self
    }

    /// 待ち＋走っている仕事の申告バイト数の上限。
    ///
    /// **何も走っていない lane は、これを超える仕事も1つだけ受ける。** 上限より
    /// 大きい仕事が永久に入れなくなるのを避けるため（呼び出し側は、その入力を
    /// もう抱えている）。
    #[must_use]
    pub const fn max_inflight_bytes(mut self, max_inflight_bytes: usize) -> Self {
        self.max_inflight_bytes = max_inflight_bytes;
        self
    }
}

/// 設定が成り立たない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JobConfigError {
    /// lane が1本も無い。
    #[error("lane が1本も無い")]
    NoLanes,
    /// どの lane も回さない優先度がある。 その優先度の仕事は永久に走らない。
    #[error("どの lane にも属さない優先度がある")]
    PriorityUnassigned,
    /// 同じ優先度を2本の lane が回す。 どちらで回すかが決まらない。
    #[error("同じ優先度が複数の lane に属している")]
    PriorityAssignedTwice,
    /// 作業スレッドの数・待ちの件数・申告バイト数の上限のどれかが 0。
    #[error("lane の数や上限が 0")]
    ZeroLimit,
}

impl koeru_failure::Failure for JobConfigError {
    fn code(&self) -> &'static str {
        match self {
            Self::NoLanes => "jobs.config_no_lanes",
            Self::PriorityUnassigned => "jobs.config_priority_unassigned",
            Self::PriorityAssignedTwice => "jobs.config_priority_assigned_twice",
            Self::ZeroLimit => "jobs.config_zero_limit",
        }
    }

    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

/// `JobRuntime` の設定。 作れた値は必ず成り立つ（[`Self::new`] が検査する）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobConfig {
    pub(crate) lanes: Vec<LaneConfig>,
    pub(crate) max_hung_per_lane: usize,
    pub(crate) retain_finished_max: usize,
    pub(crate) retain_finished_for: Duration,
}

impl JobConfig {
    /// lane の分け方を決めて作る。 保持などは既定の値。
    ///
    /// # Errors
    ///
    /// lane が無い、優先度が漏れている・重なっている、数や上限に 0 がある。
    pub fn new(lanes: Vec<LaneConfig>) -> Result<Self, JobConfigError> {
        if lanes.is_empty() {
            return Err(JobConfigError::NoLanes);
        }
        if lanes
            .iter()
            .any(|l| l.threads == 0 || l.max_pending == 0 || l.max_inflight_bytes == 0)
        {
            return Err(JobConfigError::ZeroLimit);
        }
        for p in Priority::ALL {
            match lanes.iter().filter(|l| l.priorities.contains(&p)).count() {
                0 => return Err(JobConfigError::PriorityUnassigned),
                1 => {}
                _ => return Err(JobConfigError::PriorityAssignedTwice),
            }
        }
        // 1本の lane の中で同じ優先度を2度書いても、重ねて数えない。
        Ok(Self {
            lanes,
            max_hung_per_lane: MAX_HUNG_PER_LANE,
            retain_finished_max: RETAIN_FINISHED_MAX,
            retain_finished_for: RETAIN_FINISHED_FOR,
        })
    }

    /// 固まったと見なした作業スレッドを、lane ごとに何本まで許すか。
    #[must_use]
    pub const fn max_hung_per_lane(mut self, max: usize) -> Self {
        self.max_hung_per_lane = max;
        self
    }

    /// 終わった仕事の保持。 件数か期間のどちらかを超えたら引けなくなる。
    #[must_use]
    pub const fn retention(mut self, max_finished: usize, keep_for: Duration) -> Self {
        self.retain_finished_max = max_finished;
        self.retain_finished_for = keep_for;
        self
    }
}

impl Default for JobConfig {
    /// 前景（録音直後の前処理）・試唱（試唱の要求）・背景（全件再推定・チャート事前計算）の
    /// 3本（`DEC-PLT-051`）。 確定の応答が解析を待つので、長い背景の仕事の後ろにも、
    /// 曲の長さの間ずっと走る試唱の後ろにも、解析を並ばせない
    /// （`00-architecture-report.md` §7 の「重要 control は別 lane」）。
    ///
    /// 優先度の順（`TR-SYN-34`）が意味を持つのは同じ lane の中だけで、lane どうしは並んで走る。
    fn default() -> Self {
        let foreground = LaneConfig::new("foreground", [Priority::PostRecording])
            .max_pending(FOREGROUND_MAX_PENDING)
            .max_inflight_bytes(FOREGROUND_MAX_INFLIGHT_BYTES);
        let preview = LaneConfig::new("preview", [Priority::PreviewRequest])
            .max_pending(PREVIEW_MAX_PENDING)
            .max_inflight_bytes(PREVIEW_MAX_INFLIGHT_BYTES);
        let background = LaneConfig::new(
            "background",
            [Priority::Reestimate, Priority::ChartPrecompute],
        )
        .max_pending(BACKGROUND_MAX_PENDING)
        .max_inflight_bytes(BACKGROUND_MAX_INFLIGHT_BYTES);
        // 既定の分け方は全部の優先度を1度ずつ配っているので、必ず成り立つ。
        Self {
            lanes: vec![foreground, preview, background],
            max_hung_per_lane: MAX_HUNG_PER_LANE,
            retain_finished_max: RETAIN_FINISHED_MAX,
            retain_finished_for: RETAIN_FINISHED_FOR,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_in_one() -> LaneConfig {
        LaneConfig::new("all", Priority::ALL)
    }

    #[test]
    fn 既定の設定は成り立つ分け方を3本の_lane_で配る() {
        let d = JobConfig::default();
        let again = JobConfig::new(d.lanes.clone()).expect("既定は成り立つ");
        assert_eq!(again.lanes, d.lanes);
        let shape: Vec<(&str, Vec<Priority>)> = d
            .lanes
            .iter()
            .map(|l| (l.name, l.priorities.clone()))
            .collect();
        assert_eq!(
            shape,
            [
                ("foreground", vec![Priority::PostRecording]),
                ("preview", vec![Priority::PreviewRequest]),
                (
                    "background",
                    vec![Priority::Reestimate, Priority::ChartPrecompute]
                ),
            ]
        );
    }

    #[test]
    fn 既定の試唱の_lane_は仮の上限を持ち作業スレッドは1本() {
        let d = JobConfig::default();
        let preview = &d.lanes[1];
        assert_eq!(preview.name, "preview");
        assert_eq!(preview.max_pending, PREVIEW_MAX_PENDING);
        assert_eq!(preview.max_inflight_bytes, PREVIEW_MAX_INFLIGHT_BYTES);
        assert_eq!(preview.threads, 1, "重いエンジンの状態を複製しない");
    }

    #[test]
    fn lane_が無い設定は作れない() {
        assert_eq!(JobConfig::new(vec![]), Err(JobConfigError::NoLanes));
    }

    #[test]
    fn 優先度が漏れている設定は作れない() {
        let lanes = vec![LaneConfig::new(
            "front",
            [Priority::PostRecording, Priority::PreviewRequest],
        )];
        assert_eq!(
            JobConfig::new(lanes),
            Err(JobConfigError::PriorityUnassigned)
        );
    }

    #[test]
    fn 優先度が重なる設定は作れない() {
        let lanes = vec![
            all_in_one(),
            LaneConfig::new("again", [Priority::Reestimate]),
        ];
        assert_eq!(
            JobConfig::new(lanes),
            Err(JobConfigError::PriorityAssignedTwice)
        );
    }

    #[test]
    fn 数や上限が_0_の設定は作れない() {
        for lane in [
            all_in_one().threads(0),
            all_in_one().max_pending(0),
            all_in_one().max_inflight_bytes(0),
        ] {
            assert_eq!(JobConfig::new(vec![lane]), Err(JobConfigError::ZeroLimit));
        }
    }

    #[test]
    fn 上限が_1_なら作れる() {
        let lane = all_in_one().threads(1).max_pending(1).max_inflight_bytes(1);
        assert!(JobConfig::new(vec![lane]).is_ok());
    }

    #[test]
    fn 一本の_lane_が全部の優先度を回す設定は作れる() {
        assert!(JobConfig::new(vec![all_in_one()]).is_ok());
    }
}
