//! 合否を返す検査。 1コマンド1ファイル。
//!
//! 検査は `knowledge` から読み、`diagnostic::Report` に積んで終わる。 ほかの検査も
//! `commands` も引かない。

mod budgets;
mod coverage;
mod meta;
mod profile;
mod references;
mod schema;

pub(crate) use budgets::check_budgets;
pub(crate) use coverage::check_coverage;
pub(crate) use meta::check_meta;
pub(crate) use profile::check_profile;
pub(crate) use references::check_references;
pub(crate) use schema::check_schema;
