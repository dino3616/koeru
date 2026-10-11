//! 合否を返す検査。 1コマンド1ファイル。
//!
//! 検査は [`CheckContext`] から読み、`diagnostic::Report` に積んで終わる。 ほかの検査も
//! `commands` も引かない。 報告の書き出しと終了コードは入口（`cli`）が持つので、検査は
//! `rep.finish` を呼ばない。
//!
//! 検査を足すときは、このディレクトリに1ファイルと、[`CHECKS`] に1行を足す。
//! `cli` は触らない。
//!
//! ```text
//! // checks/my_gate.rs
//! pub(crate) fn check_my_gate(ctx: &CheckContext<'_>, rep: &mut Report) {
//!     for r in ctx.snapshot.by_schema("decision") { /* 失敗は rep.error_at(code, 場所, 文言) */ }
//! }
//! // checks/mod.rs の CHECKS に
//! Check::new("check-my-gate", my_gate::check_my_gate),
//! ```
//!
//! - `check-meta` / `check-references` / `check-schema` — X01〜X05 の土台（snapshot・graph・診断）の上
//! - `check-budgets` / `check-coverage` — 旧い meta（`BUDGET` / `CMP`）を `Entry` のまま読む
//!   互換 view。 D08 の切り替えまで

use std::path::Path;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, KnowledgeSnapshot};

mod budgets;
mod coverage;
mod legacy_registry;
mod meta;
mod profile;
mod references;
mod schema;

/// 検査が読むもの。 コマンドを選ぶ前に1度だけ組み立てて、どの検査にも同じものを渡す。
pub(crate) struct CheckContext<'a> {
    pub(crate) root: &'a Path,
    /// 旧い消費者（`BUDGET` / `CMP` の互換 view と、ファイル単位の数え方）が読む。
    pub(crate) entries: &'a [Entry],
    /// meta と FSL を型つきで読んだもの。 新しい検査はこちらから読む。
    pub(crate) snapshot: &'a KnowledgeSnapshot,
    /// コマンドの名前より後ろの引数。
    pub(crate) args: &'a [String],
}

/// 登録する検査1つ。
pub(crate) struct Check {
    /// CLI のコマンド名。
    pub(crate) name: &'static str,
    /// 必要な引数の数と、足りないときに出す使い方。 引数が要らなければ `(0, "")`。
    pub(crate) required_args: usize,
    pub(crate) usage: &'static str,
    pub(crate) run: fn(&CheckContext<'_>, &mut Report),
}

impl Check {
    const fn new(name: &'static str, run: fn(&CheckContext<'_>, &mut Report)) -> Self {
        Self {
            name,
            required_args: 0,
            usage: "",
            run,
        }
    }

    const fn with_args(mut self, required: usize, usage: &'static str) -> Self {
        self.required_args = required;
        self.usage = usage;
        self
    }
}

/// `cargo xtask check-*` の一覧。 コマンドの名前の重複は試験が見る。
pub(crate) const CHECKS: &[Check] = &[
    Check::new("check-meta", meta::check_meta),
    Check::new("check-budgets", budgets::check_budgets),
    Check::new("check-coverage", coverage::check_coverage),
    Check::new("check-references", references::check_references),
    Check::new("check-profile", profile::check_profile)
        .with_args(1, "使い方: cargo xtask check-profile <PROFILE-ID>"),
    Check::new("check-schema", schema::check_schema),
];

/// コマンド名から検査を引く。
pub(crate) fn find(name: &str) -> Option<&'static Check> {
    CHECKS.iter().find(|c| c.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::diagnostic::Severity;

    #[test]
    fn コマンド名は重複せず_check_で始まる() {
        let names: BTreeSet<&str> = CHECKS.iter().map(|c| c.name).collect();
        assert_eq!(names.len(), CHECKS.len());
        assert!(CHECKS.iter().all(|c| c.name.starts_with("check-")));
    }

    #[test]
    fn 名前で引ける() {
        assert_eq!(find("check-meta").map(|c| c.name), Some("check-meta"));
        assert!(find("check-nothing").is_none());
        assert!(find("next-id").is_none());
    }

    #[test]
    fn 引数が要る検査は使い方を持つ() {
        for c in CHECKS {
            assert_eq!(c.required_args > 0, !c.usage.is_empty(), "{}", c.name);
        }
        assert_eq!(find("check-profile").map(|c| c.required_args), Some(1));
    }

    /// 登録した検査は、同じ文脈で呼ぶと `Report` に積む。 登録の行だけで走る。
    #[test]
    fn 登録した関数が文脈から呼ばれる() {
        fn gate(ctx: &CheckContext<'_>, rep: &mut Report) {
            rep.error_code("test.gate", format!("{} 件", ctx.entries.len()));
        }
        let check = Check::new("check-test-gate", gate);
        let snapshot = KnowledgeSnapshot::default();
        let ctx = CheckContext {
            root: Path::new("."),
            entries: &[],
            snapshot: &snapshot,
            args: &[],
        };
        let mut rep = Report::default();
        (check.run)(&ctx, &mut rep);
        assert_eq!(rep.diagnostics().len(), 1);
        assert_eq!(rep.diagnostics()[0].code, "test.gate");
        assert_eq!(rep.diagnostics()[0].severity, Severity::Error);
    }
}
