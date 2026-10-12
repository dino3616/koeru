//! 合否を主にしないコマンド。 ID の払い出し、判断記録の索引、要件の書き出し、
//! 変更が触れた契約の案内、検証の view（`portfolio`・`design-debt`、D07c）。
//! 1コマンド1ファイル。
//!
//! `knowledge` と `repo` から読む。 view の値は `views` が作る。 `checks` を引かない。

mod design_debt;
mod dump_requirements;
mod index_decisions;
mod next_id;
mod portfolio;
mod touched;

pub(crate) use design_debt::design_debt;
pub(crate) use dump_requirements::dump_requirements;
pub(crate) use index_decisions::index_decisions;
pub(crate) use next_id::next_id;
pub(crate) use portfolio::portfolio;
pub(crate) use touched::touched;
