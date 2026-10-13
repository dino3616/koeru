//! `cargo xtask design-debt [--human | --receipts <dir>...] [--format human|json]`
//! （提案書 10.3.1、D07c）。
//!
//! まだ確かめていないことの待ち行列を、正本とは別に、いつでも取り出す。 出力の形は
//! [`crate::views::debt`]。
//!
//! - `--human` — 人の観察でしか足りない未解決の仮説だけ（提案書の形）。 導出は D02 の
//!   [`human_verification_debt`]
//! - 無指定 — 全種類を、種類ごとの節に分けて出す。 人の観察が要る仮説に、実行の側の不足
//!   （受領証が無い・走らせなかった・false green・セルに置けない受領証）を足す。 受領証は
//!   `portfolio` と同じ `--receipts`（省略時は `target/receipts/`）から読む。 `--human` に
//!   `--receipts` は付けられない——受領証を読まないので、付けても黙って無視することになる
//!
//! **優先度を単一の score にしない。** 並びは種類ごとに分け、種類の中では ID 順。
//!
//! 検査ではない。 債務が積み上がっていても終了コードは 0。 受領証が読めないときだけ非 0
//! （`portfolio` と同じ）。

use std::path::Path;
use std::process::ExitCode;

use super::portfolio::{finish, load_receipts, receipt_dirs};
use crate::diagnostic::{OutputFormat, Report};
use crate::knowledge::{Entry, KnowledgeSnapshot};
use crate::verification::{CapabilityRegistry, Facts, human_verification_debt};
use crate::views::debt::{Debt, human_debt_json, render_human_debt};
use crate::views::portfolio::{self, Portfolio};

const USAGE: &str =
    "使い方: cargo xtask design-debt [--human | --receipts <dir>...] [--format human|json]";

pub(crate) fn design_debt(
    root: &Path,
    entries: &[Entry],
    snapshot: &KnowledgeSnapshot,
    args: &[String],
    format: OutputFormat,
    mut rep: Report,
) -> ExitCode {
    let human_only = args == ["--human"];
    let dirs = if human_only {
        Vec::new()
    } else {
        match receipt_dirs(args, USAGE) {
            Ok(d) => d,
            Err(usage) => {
                eprintln!("{usage}");
                return ExitCode::FAILURE;
            }
        }
    };
    let (registry, issues) = CapabilityRegistry::from_snapshot(snapshot);
    if !issues.is_empty() {
        // 読めなかった行は表に入っていない。 その種類の仮説は、ここに出ない。
        rep.note(format!(
            "資格の表に誤りが {} 件ある。 check-meta で確かめる。 読めない行の種類の仮説は出ない",
            issues.len()
        ));
    }
    let human = human_verification_debt(&Facts::from_snapshot(snapshot), &registry);
    if human_only {
        return finish(
            "design-debt",
            root,
            &render_human_debt(&human),
            human_debt_json(&human),
            format,
            &rep,
        );
    }
    let observed = load_receipts(root, &dirs, &mut rep);
    let probes = portfolio::definitions(entries, snapshot, &mut rep);
    let debt = Debt::build(human, &Portfolio::build(&probes, &observed));
    finish(
        "design-debt",
        root,
        &debt.render_human(),
        debt.to_json(),
        format,
        &rep,
    )
}
