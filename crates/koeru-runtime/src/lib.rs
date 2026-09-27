//! KOERU の Application 層（`DEC-PLT-034`）。
//!
//! project の単一の書き手（[`ProjectRuntime::writer`]）と、一貫した読み
//! （[`ProjectRuntime::read`] が返す [`ProjectReadSession`]）を持つ。 永続化・
//! 回復・単一の書き手・一貫した読みがここの持ち分で、Tauri・GraphQL・callback の
//! 処理は持たない——それらは `koeru-app`（移行中）と、将来の `koeru-graphql` /
//! `koeru-desktop` が持つ（`DEC-PLT-034` の crate 表）。
//!
//! # T05 のどこにあたるか
//!
//! `docs/reports/architecture/03-task-dag.md` の T05
//! （`ProjectRuntime` / coherent read session）のうち、この crate（T05b-1）が
//! 持つのは:
//!
//! - [`ProjectLease`] — 開いた project への貸与。閉じて開き直すと別の値になり、
//!   同じ値を二度と振らない
//! - [`ProjectRuntime`] — 貸与・単一の書き手・読み手接続のプールを持つ。
//!   第二の `Studio` にはしない
//! - [`ProjectReadSession`] — 1つの版に固定した読みで、facet ごとに遅延解決する
//!   （`DEC-PLT-044`）
//!
//! `koeru-app` の `Studio` をこれ経由に繋ぎ直すのは T05b-2（別のエージェントの作業）。
//! ここではまだ `crates/koeru-app/src/{studio.rs,commands.rs,lib.rs}` を書き換えない。
//!
//! `editor`（M6 / T13）と `distribution`（WAV 走査が要る。T08 / T09）の facet は
//! まだここに無い。 `session` モジュールに理由を書いてある。

pub mod error;
pub mod lease;
pub mod review;
pub mod runtime;
pub mod session;

pub use error::RuntimeError;
pub use lease::ProjectLease;
pub use runtime::{ProjectRuntime, WriterGuard};
pub use session::{ProjectReadSession, RecordingFacet, RepertoireFacet, ReviewFacet};
