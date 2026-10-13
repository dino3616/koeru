//! 計算して出す derived view（D06・D07）。
//!
//! 手書きの正本にしない値を、graph と記録から再計算する。 入力は D00 の型つきの口
//! （`knowledge::design`）と graph だけで、legacy の接頭辞や meta の表の並びは引かない。
//!
//! - [`budget`] — 上限・小計・モード別のピーク・余白（D07a）
//! - [`coverage`] — 要件ごとの `formalizes` / `implements` / `targets`（D07b）
//! - [`pattern`] — graph の反復構造を、authority を持たない候補として出す（D06）
//! - [`portfolio`] — 要る Probe と実行された Receipt を突き合わせ、不足を false green にしない（D07c）
//! - [`debt`] — 人の観察が要る仮説と、Portfolio の不足を、種類ごとの債務として出す（D07c）
//!
//! 出力の形（JSON・端末向け）は D07 が持つ（実装計画 3.2）。 `portfolio` と `debt` は
//! `cargo xtask portfolio` と `cargo xtask design-debt [--human]` が呼ぶ。 値を作るのはここで、
//! 受領証のディレクトリを読むのと、出力の封筒（`command` / `ok` / `diagnostics`）は
//! `commands` が持つ。

// 消費者（D08・Context・CLI）が付くまで、production からは呼ばれない。
// `lib.rs` の `mod graph;` と同じ事情で、許す範囲はこの module に閉じる。
#![allow(dead_code, unused_imports)]

pub(crate) mod budget;
pub(crate) mod coverage;
pub(crate) mod debt;
pub(crate) mod pattern;
pub(crate) mod portfolio;
