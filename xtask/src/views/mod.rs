//! 計算して出す derived view（D06・D07）。
//!
//! 手書きの正本にしない値を、graph と記録から再計算する。 入力は D00 の型つきの口
//! （`knowledge::design`）と graph だけで、legacy の接頭辞や meta の表の並びは引かない。
//!
//! - [`budget`] — 上限・小計・モード別のピーク・余白（D07a）
//! - [`coverage`] — 要件ごとの `formalizes` / `implements` / `targets`（D07b）
//! - [`pattern`] — graph の反復構造を、authority を持たない候補として出す（D06）

// 消費者（D08・Context・CLI）が付くまで、production からは呼ばれない。
// `lib.rs` の `mod graph;` と同じ事情で、許す範囲はこの module に閉じる。
#![allow(dead_code, unused_imports)]

pub(crate) mod budget;
pub(crate) mod coverage;
pub(crate) mod pattern;
