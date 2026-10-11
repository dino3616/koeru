//! 仕事の語彙。 種類・優先度・入力の印・状態・取消の結果。

use std::collections::BTreeMap;
use std::fmt;

use koeru_failure::{Action, Class, Failure, Outcome};

use super::ids::{Attempt, CoalesceKey, JobId, Receipt};
use crate::lease::ProjectLease;

/// 仕事の種類（`specs/application/schema/jobs.graphql` の `JobKind`）。
///
/// `JobRuntime` は種類によって振る舞いを変えない。 優先度と lane を決めるのは
/// [`Priority`] で、種類は観測と画面の見せ方のためにある。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JobKind {
    Analysis,
    PreviewRender,
    Chart,
}

impl JobKind {
    /// 記録に渡す固定の名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Analysis => "analysis",
            Self::PreviewRender => "preview_render",
            Self::Chart => "chart",
        }
    }
}

/// 仕事の優先度（`TR-SYN-34`）。
///
/// 録音直後の前処理 > 試唱の要求 > 全件再推定 > チャート事前計算。
/// 数が大きいほど先に回る。 **並びは `crates/koeru-app/src/workers.rs` の
/// `Priority` から変えていない**——変えるなら根拠を `TR-SYN-34` の側に残してから。
///
/// この順にする理由は、待っている人がいるかどうか。
/// 録音直後の前処理は「次に試唱を押す人」を待たせる。試唱の要求は「いま押した人」。
/// 全件再推定とチャートは誰も待っていない。
///
/// **厳密な優先度で、同じ段は先に積んだものから回す。** 高い段が途切れずに来ると、
/// 低い段は待ち続ける（飢餓）。 これは今の意味の帰結として受け入れている：
/// 低い段は誰も待っていないので、待たされても困る人がいない。 待たせたくない
/// 仕事を低い段に置かない。 動かさない試験は `tests/jobs.rs` の
/// 「低い段は高い段が途切れるまで待つ」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Priority {
    /// チャートの事前計算。誰も待っていない。
    ChartPrecompute = 0,
    /// 全件再推定。誰も待っていない。
    Reestimate = 1,
    /// 試唱の要求。いま押した人が待っている。
    PreviewRequest = 2,
    /// 録音直後の前処理。次に試唱を押す人を待たせる。
    PostRecording = 3,
}

impl Priority {
    /// 低い段から高い段まで、全部。
    pub const ALL: [Self; 4] = [
        Self::ChartPrecompute,
        Self::Reestimate,
        Self::PreviewRequest,
        Self::PostRecording,
    ];

    /// 記録に渡す固定の名前。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ChartPrecompute => "chart_precompute",
            Self::Reestimate => "reestimate",
            Self::PreviewRequest => "preview_request",
            Self::PostRecording => "post_recording",
        }
    }

    pub(crate) const fn index(self) -> usize {
        self as usize
    }
}

/// その計算が**実際に読んだ入力**の印。
///
/// 名前から指紋の文字列への順序つきの集合。 中身は `JobRuntime` が解釈しない
/// ——素材の指紋でも、規則の版でも、対象の版でも、呼び出し側が決める。
/// 公開のときに「今の値」と突き合わせ、違えば `PublishRefused::Stale` で捨てる
/// （エンジンの失敗にはしない）。
///
/// **project 全体の版だけを比べない。** そうすると、無関係な編集でも全部の仕事が
/// 捨てられる（`docs/reports/architecture/00-architecture-report.md` §7、
/// `DEC-PLT-034`）。 計算が読んでいないものを入れると、同じ理由で無駄に捨てる。
///
/// 突き合わせは、仕事の側の項目が全部、`current` に同じ値で在ること。
/// `current` に余分な項目が在っても構わない（その仕事が読んでいない入力は、
/// 変わっても関係がない）。 `current` から項目が消えていたら古い。
///
/// **`Debug` は件数だけを出す。** 指紋は素材のファイル名などから作られうるので、
/// `{:?}` で記録に流れ込ませない（`AGENTS.md` の禁止事項3）。
#[derive(Clone, Default, PartialEq, Eq)]
pub struct BasedOn(BTreeMap<String, String>);

impl BasedOn {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 入力を1つ足す。 同じ名前は上書きする。
    #[must_use]
    pub fn with(mut self, name: impl Into<String>, fingerprint: impl Into<String>) -> Self {
        self.0.insert(name.into(), fingerprint.into());
        self
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// 名前の順に。
    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    /// この印の入力が、`current` で今も同じ値か。
    #[must_use]
    pub fn is_current_in(&self, current: &Self) -> bool {
        self.0
            .iter()
            .all(|(name, fp)| current.0.get(name) == Some(fp))
    }
}

impl fmt::Debug for BasedOn {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BasedOn")
            .field("entries", &self.0.len())
            .finish()
    }
}

/// 仕事が失敗した理由。 code と分類と確定の状態だけ（`DEC-PLT-038`）。
///
/// 表示文も source も持たない。 状態に載せて画面へ渡す語彙で、
/// 画面は code と分類で分岐する。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JobFailure {
    pub code: &'static str,
    pub class: Class,
    pub outcome: Outcome,
}

impl JobFailure {
    /// [`Failure`] から作る。 確定の状態は、持ち主が知っているものを渡す。
    #[must_use]
    pub fn of<E: Failure + ?Sized>(e: &E, outcome: Outcome) -> Self {
        Self {
            code: e.code(),
            class: e.class(),
            outcome,
        }
    }

    /// 呼び出し側の次の手。
    #[must_use]
    pub const fn action(self) -> Action {
        self.class.action()
    }
}

/// 仕事の今の状態（`specs/application/schema/jobs.graphql` の `JobStatus`）。
///
/// 時刻は RFC 3339（`crate::time::now_rfc3339`）。
///
/// 計算が終わって公開を待っている間と、公開の途中は `Running`（進捗 1.0）で見せる。
/// 成功は、公開が確定したときだけ（計算が終わっただけでは成功にしない）。
#[derive(Debug, Clone, PartialEq)]
pub enum JobStatus {
    Queued {
        since: String,
    },
    Running {
        started_at: String,
        /// 0〜1。 測れなければ `None`。
        progress: Option<f32>,
    },
    Succeeded {
        finished_at: String,
        receipt: Receipt,
    },
    Failed {
        finished_at: String,
        failure: JobFailure,
    },
    Cancelled {
        finished_at: String,
        /// 計算そのものを止められたか。 走り始めた外部エンジンは止められない
        /// ことがあり、その場合は `false`（走り切った結果は捨てる）。
        computation_stopped: bool,
    },
}

impl JobStatus {
    /// もう変わらない状態か。
    #[must_use]
    pub const fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Succeeded { .. } | Self::Failed { .. } | Self::Cancelled { .. }
        )
    }
}

/// 仕事1件の読み（`BackgroundJob`）。 保持の期間を過ぎた終わった仕事は引けない。
#[derive(Debug, Clone, PartialEq)]
pub struct JobSnapshot {
    pub id: JobId,
    pub kind: JobKind,
    pub priority: Priority,
    pub attempt: Attempt,
    pub lease: ProjectLease,
    pub status: JobStatus,
}

/// 待っている仕事と走っている仕事の数（`TR-SYN-33`）。
///
/// `pending` は、上限に当たって脇に置いた仕事（`OverflowPolicy::Defer`）も含む
/// ——待っていることに変わりがない。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JobCounts {
    pub pending: usize,
    pub running: usize,
}

/// lane 1本の内訳。 観測と試験のため。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LaneStats {
    pub name: &'static str,
    /// 上限に数える待ち。
    pub queued: usize,
    /// 上限に当たって脇に置いた待ち。 上限には数えない。
    pub deferred: usize,
    pub running: usize,
    /// 待ち＋走っている仕事（固まったと見なした仕事を含む）の申告バイト数。
    pub inflight_bytes: usize,
    /// 期限を過ぎて固まったと見なし、まだ戻ってきていない作業スレッドの数。
    pub hung: usize,
    /// 仕事を取れる作業スレッドの数。
    pub threads: usize,
}

/// 上限に当たったときの扱い（`specs/application/schema/jobs.graphql` の
/// `ResourceBusy` を返すか、まとめるか、脇に置くか）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverflowPolicy {
    /// 断る（`JobError::Busy`）。 誰も待っていない仕事、たとえばチャート。
    Reject,
    /// 同じ鍵の待ちがあれば、それを新しい仕事に置き換える。 古い方は
    /// `Cancelled { computation_stopped: true }` になる。 同じ鍵の待ちが無く、
    /// 上限にも当たっていれば断る。
    Coalesce(CoalesceKey),
    /// **失敗にしない。** 上限を超えたら脇に置き、空いたら昇格させる。
    /// 保存済みのテイクの解析はこれ（`00-architecture-report.md` §7）。
    /// 脇に置いたものは数えるが、上限には含めない。
    Defer,
}

/// [`crate::jobs::JobRuntime::cancel`] の結果。
///
/// `specs/application/schema/jobs.graphql` の `CancelJobOutcome` に写る:
/// `Cancelled` / `Requested` / `Publishing` は `JobCancelRequested`、
/// `AlreadyCommitted` / `AlreadyFinished` は `JobAlreadyFinished`、
/// `NotFound` は `ReferenceNotFound`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// 待ち中（または計算が終わって公開を待っていた）のを取り消した。
    /// もう `Cancelled` で、計算は走っていない（待ち中）か、走り切ったあとで
    /// 結果を捨てた（公開待ち）。
    Cancelled,
    /// 走っている。 合図は立てた。 止まったかは、計算が戻ったときに決まる
    /// （状態が `Cancelled` になり、`computation_stopped` が出る）。
    Requested,
    /// 公開が始まっていて、取り消せない。 受領証が決まるのを待ったが、
    /// 待ち切れなかった（writer を握っている側が長く持っている）。
    Publishing,
    /// 公開が済んでいた。 成功を取消と偽らない。 受領証は公開が返したもの。
    AlreadyCommitted(Receipt),
    /// 失敗か取消で、もう終わっていた。
    AlreadyFinished,
    /// そんな仕事は無い、または保持の期間を過ぎた。
    NotFound,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 優先度の並びが要件どおり() {
        assert!(Priority::PostRecording > Priority::PreviewRequest);
        assert!(Priority::PreviewRequest > Priority::Reestimate);
        assert!(Priority::Reestimate > Priority::ChartPrecompute);
    }

    #[test]
    fn 優先度は4段で全部を低い順に並べる() {
        let mut sorted = Priority::ALL;
        sorted.sort();
        assert_eq!(sorted, Priority::ALL);
        for (i, p) in Priority::ALL.iter().enumerate() {
            assert_eq!(p.index(), i);
        }
    }

    #[test]
    fn 入力が全部同じ値なら現在() {
        let based = BasedOn::new().with("asset", "a1").with("rules", "r7");
        let current = BasedOn::new().with("rules", "r7").with("asset", "a1");
        assert!(based.is_current_in(&current));
    }

    #[test]
    fn 値が変わっていたら古い() {
        let based = BasedOn::new().with("asset", "a1");
        assert!(!based.is_current_in(&BasedOn::new().with("asset", "a2")));
    }

    #[test]
    fn 項目が消えていたら古い() {
        let based = BasedOn::new().with("asset", "a1");
        assert!(!based.is_current_in(&BasedOn::new()));
    }

    #[test]
    fn 読んでいない入力が変わっても古くならない() {
        let based = BasedOn::new().with("asset", "a1");
        let current = BasedOn::new().with("asset", "a1").with("settings", "s9");
        assert!(based.is_current_in(&current));
    }

    #[test]
    fn 何も読まない仕事はいつでも現在() {
        assert!(BasedOn::new().is_current_in(&BasedOn::new()));
        assert!(BasedOn::new().is_current_in(&BasedOn::new().with("x", "y")));
    }

    #[test]
    fn 入力の印は名前の順に並ぶ() {
        let based = BasedOn::new().with("b", "2").with("a", "1");
        let names: Vec<&str> = based.iter().map(|(n, _)| n).collect();
        assert_eq!(names, ["a", "b"]);
        assert_eq!(based.len(), 2);
        assert!(!based.is_empty());
    }

    #[test]
    fn 入力の印は_debug_で指紋を出さない() {
        let based = BasedOn::new().with("秘密の音源", "指紋の本体");
        let shown = format!("{based:?}");
        assert!(
            !shown.contains("秘密") && !shown.contains("指紋"),
            "{shown}"
        );
    }

    #[test]
    fn 終わった状態だけが_terminal() {
        let at = "2026-01-01T00:00:00Z".to_owned();
        assert!(!JobStatus::Queued { since: at.clone() }.is_terminal());
        assert!(
            !JobStatus::Running {
                started_at: at.clone(),
                progress: None
            }
            .is_terminal()
        );
        assert!(
            JobStatus::Succeeded {
                finished_at: at.clone(),
                receipt: Receipt::new(1)
            }
            .is_terminal()
        );
        assert!(
            JobStatus::Cancelled {
                finished_at: at,
                computation_stopped: true
            }
            .is_terminal()
        );
    }
}
