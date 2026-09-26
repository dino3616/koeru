//! meta と FSL を読んだもの。 検査もコマンドも、読み方はここから引く。
//!
//! 引くのは `repo` と `diagnostic` だけ。 検査やコマンドを引かない。

mod fsl;
mod load;
mod model;
mod refs;

pub(crate) use fsl::{fsl_ids, fsl_sites};
pub(crate) use load::load;
pub(crate) use model::{Entry, id_index, list_of, requirements, str_of, with_schema};
pub(crate) use refs::{citations, find_tr, id_tokens, squash};
