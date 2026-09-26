//! KOERU のドメイン層。GUI と OS に依存しない。
//!
//! 型を足すときは `rust-conventions` skill に従うこと。
//!
//! 移行中（`DEC-PLT-034`）。 入出力を持たない規則は `koeru-model` へ移した。
//! 下の再輸出は既存の `koeru_core::alias` などの経路を通すためだけにある。
//! 新しい規則は `koeru-model` に足す。 外部形式の構文と符号化は `koeru-formats` へ移し、
//! `text` と `frq` はファイルに触る口だけを持って残りを再輸出する。

pub use koeru_model::{
    alias, calibration, channel, guide, inventory, leak, mora, names, order, oto, pace, plan,
    presamp, preset, reclist, song, tone, voice, waveform,
};

pub mod analysis;
pub mod capture;
pub mod db;
pub mod frq;
pub mod handoff;
pub mod project;
pub mod release;
pub mod relocate;
pub mod schema;
pub mod subbank;
pub mod text;
pub mod ust;
