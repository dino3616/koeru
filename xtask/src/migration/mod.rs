//! ID の cutover を、検索置換ではなく「計画 → dry-run → 書き換え → 読み直し」で行う（X06）。
//!
//! - [`mapping`] — 旧 ID → 新 ID の対応表。 何を写すかは D08 が持つ
//! - [`scan`] — 旧 ID の出現を、meta・FSL・ソースと文書から拾う
//! - [`plan`] — 衝突と書き換え方の分からない参照を見て、計画を作る
//! - [`dryrun`] — 書く前に、書いた後の graph を作って判定する
//! - [`rewrite`] — 作業ツリーへ書く transaction。 どこで失敗しても書く前へ戻す
//!
//! 呼ぶ経路はまだ無い。 対応表を渡してこれを呼ぶコマンドは D08 が足す。

mod dryrun;
mod mapping;
mod plan;
mod rewrite;
mod scan;

#[cfg(test)]
mod tests;

pub(crate) use dryrun::{
    DefaultEquivalence, DryRun, EquivalenceCheck, EquivalenceIssue, dry_run, equivalence,
};
pub(crate) use mapping::{Mapping, MappingError};
pub(crate) use plan::{FileRename, MigrationPlan, PlanIssue, PlanOptions, plan};
pub(crate) use rewrite::{Applied, ApplyOptions, RewriteError, apply};
pub(crate) use scan::{Edit, EditKind, UnknownInboundRef};
