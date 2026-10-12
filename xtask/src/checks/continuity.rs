//! `check-meta` の一部: 前の版と比べて、意味が黙って書き換わっていないか。
//!
//! 見るのは2つ（`DEC-PLT-048`）。
//!
//! - 採用済みの判断の `selected` が、同じ ID のまま書き換わった。 意味を変えるなら新しい判断を作って
//!   `supersedes` で繋ぐ。 意味を変えない修正なら、同じ変更の中で `meaning_preserving_edits` に1件足す
//! - 同じ種類のファイルが改名され、ID が変わった。 ID は内容を直しても変わらない
//!
//! 古い側は [`crate::repo::resolve_baseline`] が決める基準、新しい側は作業ツリー。 基準が取れない
//! ときは失敗にする——比べないまま緑になると、比べていないことに誰も気づけない（`DEC-PLT-039`）。
//! どの基準と比べたかは、`meta.baseline` の1行で必ず出す。
//!
//! 基準は `--base <rev>` で変えられる（`<rev>` と HEAD の merge-base）。

use std::path::Path;

use super::CheckContext;
use crate::diagnostic::{Location, Report};
use crate::knowledge::design::{Design, DesignIssue};
use crate::repo::{Baseline, RepoView, resolve_baseline};

/// 引数から `--base <rev>` / `--base=<rev>` を取り出す。
fn base_arg(args: &[String]) -> Result<Option<&str>, String> {
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--base" {
            return match it.next() {
                Some(v) if !v.is_empty() => Ok(Some(v.as_str())),
                _ => Err("使い方: --base <rev>".to_owned()),
            };
        }
        if let Some(v) = a.strip_prefix("--base=") {
            return if v.is_empty() {
                Err("使い方: --base <rev>".to_owned())
            } else {
                Ok(Some(v))
            };
        }
    }
    Ok(None)
}

/// 前の版と比べる。
pub(super) fn check_continuity(ctx: &CheckContext<'_>, rep: &mut Report) {
    let explicit = match base_arg(ctx.args) {
        Ok(v) => v,
        Err(e) => {
            rep.error_code("meta.baseline-unavailable", e);
            return;
        }
    };
    match resolve_baseline(ctx.root, explicit) {
        Err(e) => rep.error_code("meta.baseline-unavailable", e),
        Ok(Baseline::Absent { reason }) => {
            rep.note_code("meta.baseline", format!("比較の基準: なし（{reason}）"));
        }
        Ok(Baseline::Revision { sha, how }) => {
            rep.note_code(
                "meta.baseline",
                format!("比較の基準: {}（{how}）", short(&sha)),
            );
            compare(ctx.root, &sha, rep);
        }
    }
}

fn short(sha: &str) -> &str {
    sha.get(..12).unwrap_or(sha)
}

/// `base` の版と作業ツリーを比べて、問題を `rep` に積む。
fn compare(root: &Path, base: &str, rep: &mut Report) {
    let old_view = match RepoView::revision(root, base) {
        Ok(v) => v,
        Err(e) => {
            rep.error_code(
                "meta.baseline-unavailable",
                format!("比較の基準 {} を読めない: {e}", short(base)),
            );
            return;
        }
    };
    let new_view = RepoView::working_tree(root);
    let renames = match new_view.renames_from(&old_view) {
        Ok(r) => r,
        Err(e) => {
            rep.error_code(
                "meta.baseline-unavailable",
                format!("基準との改名を引けない: {e}"),
            );
            return;
        }
    };
    let old = Design::load(&old_view);
    let new = Design::load(&new_view);

    for issue in old.overwritten_decisions(&new) {
        let DesignIssue::DecisionOverwritten { id } = issue else {
            continue;
        };
        let path = new.object(&id).map(|o| o.origin().path.to_path_buf());
        let file = path
            .as_ref()
            .map_or_else(String::new, |p| format!("{}: ", p.display()));
        rep.error_at(
            "meta.decision-overwritten",
            path.map(|p| Location::new(p, None)),
            format!(
                "{file}採用済みの判断 `{id}` の selected が書き換わった。 意味を変えるなら新しい判断を作って \
                 supersedes で繋ぐ。 意味を変えない修正なら、同じ変更で meaning_preserving_edits に1件足す"
            ),
        );
    }
    for issue in old.changed_identities(&new, &renames) {
        let DesignIssue::IdentityChanged { from, to, .. } = issue else {
            continue;
        };
        let path = new.object(&to).map(|o| o.origin().path.to_path_buf());
        let file = path
            .as_ref()
            .map_or_else(String::new, |p| format!("{}: ", p.display()));
        rep.error_at(
            "meta.identity-changed",
            path.map(|p| Location::new(p, None)),
            format!(
                "{file}`{from}` のファイルが `{to}` に改名された。 ID は内容を直しても変わらない。 \
                 付け直しなら移行の手順で、置き換えなら新しい記録を作って supersedes で繋ぐ"
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn base_は空白区切りでも等号でも読める() {
        assert_eq!(base_arg(&args(&[])), Ok(None));
        assert_eq!(base_arg(&args(&["--base", "main"])), Ok(Some("main")));
        assert_eq!(
            base_arg(&args(&["--base=origin/main"])),
            Ok(Some("origin/main"))
        );
        assert!(base_arg(&args(&["--base"])).is_err());
        assert!(base_arg(&args(&["--base="])).is_err());
        assert!(base_arg(&args(&["--base", ""])).is_err());
    }
}
