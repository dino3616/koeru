//! KOERU の合成。
//!
//! **WORLD を基礎とする**（`DEC-SYN-001`）。ニューラルボコーダへの置き換えは採らない。
//! F0 推定だけ SwiftF0 に差し替える（`DEC-SYN-004`）。
//!
//! **UTAU 互換 resampler は自前で書く**（`DEC-SYN-005`）。`worldline` は
//! OpenUtau のコードで取れないため。仕様は `TR-SYN-08` が引数一式を定義済み。

pub mod oto;
pub mod resampler;
pub mod segment;
pub mod world;
