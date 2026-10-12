//! 試験を走らせて件数を数えるもの（`DEC-PLT-039`）。
//!
//! [`model`] が Probe の定義・実行環境・実行結果の型（`ProbeDefinition` /
//! `ExecutionContext` / `Receipt`）を持ち、[`receipt`] が cargo と bun の
//! 試験 binary をそれらの型で走らせて `meta/suites/` の登録と突き合わせる
//! （X04）。今の入力源は SUITE の登録だけで、試験に限らない Probe が増えたら
//! `model::ProbeDefinition` へ別のコンストラクタを足す。

mod model;
mod receipt;

// D02（`verification`）が Receipt を EVID の候補にしてよいかを見るのに、型の名前を引く。
// 型そのものは変えていない。
pub(crate) use model::{
    ExecutionContext, NotRunReason, Prerequisites, ProbeDefinition, Receipt, Status,
};
// `Receipt` を組み立てるのは試験だけ。
#[cfg(test)]
pub(crate) use model::Counts;
pub(crate) use receipt::{check_portfolio, test_receipt};
