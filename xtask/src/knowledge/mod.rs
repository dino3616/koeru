//! meta と FSL を読んだもの。 検査もコマンドも、読み方はここから引く。
//!
//! 引くのは `repo` と `diagnostic` だけ。 検査やコマンドを引かない。

mod fsl;
mod ids;
mod load;
mod model;
mod refs;
mod snapshot;

pub(crate) use fsl::{fsl_ids, fsl_sites};
pub(crate) use ids::Id;
pub(crate) use load::load;
pub(crate) use model::{Entry, id_index, list_of, requirements, str_of, with_schema};
pub(crate) use refs::{citations, find_tr, id_tokens, squash};
// `Fields` と `Provenance` はまだここでは要らない。 `Record::tables` /
// `Record::provenance` の戻り値としては使えていて、名指しで欲しい消費者
// （X05 / X06 / X07 / D00）が出たときにここへ足す。
pub(crate) use snapshot::{KnowledgeSnapshot, Record};
