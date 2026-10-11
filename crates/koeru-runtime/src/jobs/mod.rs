//! 背後の仕事（`JobRuntime`）。 待ち・試行・取消・期限・公開の線形化点を
//! 1つの手続きに集める（`DEC-PLT-034`、`DEC-PLT-047`、T07a）。
//!
//! # 何を持つか
//!
//! - [`JobRuntime`] — アプリの寿命で1つ。 lane（作業スレッドの束）ごとに有界の
//!   待ちを持ち、優先度（[`Priority`]、`TR-SYN-34`）の順に回す。 エンジンを知らず、
//!   計算は closure で受け取る
//! - [`JobSpec`] — 依頼。 貸与・種類・優先度・読んだ入力の印（[`BasedOn`]）・
//!   申告バイト数・上限に当たったときの扱い（[`OverflowPolicy`]）・期限・計算
//! - 結果の受け取りは2通り。 待つ（[`JobTicket::wait`]）か、回す
//!   （[`JobRuntime::submit_notify`] の完了 callback）
//! - 公開は別の手続き（[`JobRuntime::publish`]）。 **受理の線形化点**で、
//!   台帳へ書くのは呼び出し側の `admit` だけ
//!
//! # 決めていないこと
//!
//! 上限・期限・保持の値はすべて仮で、実測していない（`Q-PLT-006`、
//! [`config`] の定数）。 待ち数をどう画面へ流すか（`jobEvents`）と、解析の非同期の
//! 見せ方（`AnalysisPending`）はここでは決めない（T09 / T12）。 `crates/koeru-app` の
//! `workers.rs` からの置き換えと `Studio` への接続は T07b-2。

pub mod config;
mod error;
mod ids;
mod model;
mod runtime;
mod spec;
mod state;
mod trace;

pub use config::{JobConfig, JobConfigError, LaneConfig};
pub use error::{Cancelled, ComputeError, JobError, PublishError, PublishRefused};
pub use ids::{Attempt, CoalesceKey, JobId, ParseJobIdError, Receipt};
pub use model::{
    BasedOn, CancelOutcome, JobCounts, JobFailure, JobKind, JobSnapshot, JobStatus, LaneStats,
    OverflowPolicy, Priority,
};
pub use runtime::JobRuntime;
pub use spec::{CancelProbe, Completed, JobHandle, JobSpec, JobTicket, Published};
