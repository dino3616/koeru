//! `cargo xtask design-debt --human [--format human|json]`（提案書 10.3.1、D07c）。
//!
//! 人の観察でしか足りない未解決の仮説を、正本とは別の待ち行列としていつでも取り出す。
//! 導出は D02 の [`human_verification_debt`]、出力の形は [`crate::views::debt`]。
//!
//! **優先度を単一の score にしない。** 並びは仮説の ID 順。 `--human` が無ければ使い方の誤り
//! ——実行の側の不足（受領証が無い・走らせなかった・false green）を同じ一覧に出すかは、まだ
//! 決めていない。 値としては [`crate::views::debt::Debt`] が持つ。
//!
//! 検査ではない。 仮説が積み上がっていても終了コードは 0。

use std::path::Path;
use std::process::ExitCode;

use crate::diagnostic::{OutputFormat, Report};
use crate::knowledge::KnowledgeSnapshot;
use crate::verification::{CapabilityRegistry, Facts, human_verification_debt};
use crate::views::debt::{human_debt_json, render_human_debt};

const USAGE: &str = "使い方: cargo xtask design-debt --human [--format human|json]";

pub(crate) fn design_debt(
    root: &Path,
    snapshot: &KnowledgeSnapshot,
    args: &[String],
    format: OutputFormat,
    mut rep: Report,
) -> ExitCode {
    if args != ["--human"] {
        eprintln!("{USAGE}");
        return ExitCode::FAILURE;
    }
    let (registry, issues) = CapabilityRegistry::from_snapshot(snapshot);
    if !issues.is_empty() {
        // 読めなかった行は表に入っていない。 その種類の仮説は、ここに出ない。
        rep.note(format!(
            "資格の表に誤りが {} 件ある。 check-meta で確かめる。 読めない行の種類の仮説は出ない",
            issues.len()
        ));
    }
    let items = human_verification_debt(&Facts::from_snapshot(snapshot), &registry);
    super::portfolio::finish(
        "design-debt",
        root,
        &render_human_debt(&items),
        human_debt_json(&items),
        format,
        &rep,
    )
}
