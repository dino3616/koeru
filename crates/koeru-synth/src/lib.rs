//! KOERU の合成。
//!
//! WORLD を基礎とする（`DEC-SYN-001`, `TR-SYN-05`, `TR-SYN-06`, `TR-SYN-07`）。
//! ニューラルボコーダへの置き換えは採らない——
//! 「あなたの声そのもの」が「生成された声」に変わる。
//!
//! 同梱する。 `vendor/world` に BSD-3-Clause のまま置き、LICENSE も残す（`TR-SYN-07`）。
//! F0 推定だけ SwiftF0 に差し替える（`DEC-SYN-004`）。
//!
//! UTAU 互換 resampler は自前で書く（`DEC-SYN-005`）。`worldline` は
//! OpenUtau のコードで取れないため。仕様は `TR-SYN-08` が引数一式を定義済み。

pub mod f0;
pub mod phrase;
pub mod resampler;
pub mod world;
