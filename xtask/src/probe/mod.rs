//! 試験を走らせて件数を数えるもの（`DEC-PLT-039`）。
//!
//! 今は cargo と bun の試験 binary を `meta/suites/` の登録と突き合わせる [`receipt`] だけ。
//! 試験に限らない検証の定義と実行の記録へ広げるのは、この下に足す（X04）。

mod receipt;

pub(crate) use receipt::{check_portfolio, test_receipt};
