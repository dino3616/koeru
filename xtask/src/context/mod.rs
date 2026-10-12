//! `cargo xtask context` — 現在の semantic state と、比較元からの意味の変化を、1つの出力で返す（D01）。
//!
//! ```text
//! cargo xtask context --root <ID|パス>... [--at <rev>] [--from <rev>] [--format md|json] [--budget <n>]
//! ```
//!
//! 出典は提案書 7（C2）。 Context と比較 Context を別のものとして持たず、同じ計算の2つの見方にする。
//!
//! - [`compile`] — `C(root, revision)`。 ある版で判断に必要な closure（提案書 7.2）を作る
//! - [`compare`] — `diff(C(root, from), C(root, at))`。 旧・新の closure の和集合で比べる
//! - [`render`] — Markdown と JSON。 どちらも同じ [`render::View`] を読む
//! - [`rules`] — 辿る relation の表。 relation の名前はここにしか書かない
//! - [`target`] — D00 に口が出るまでの、仮説の欄の仮の読み
//!
//! `--at` は読む地点で、省略したら作業ツリー。 `--from` は比較元で、省略したら current だけ。
//! **`--from` があっても current state は省かない。** 比較の起点を `--base`、読む地点を `--to` とは
//! 言わない（提案書 7.4）。 コアはオフラインで動く。 Issue / PR の会話（Level 2）は取りに行かず、
//! 出どころの locator だけを出す。
//!
//! 終了コード。 root が読んだどの版にも無い、必須 closure だけで `--budget` を超える、のような
//! 重大な欠落は「不完全」として 0 以外で終わる。 関係する契約を特定できない path（UNMAPPED）は
//! 欠けとして出すだけで、失敗にしない。
//!
//! JSON は D01 が形の持ち主。 `diagnostic` の封筒（`command`・`ok`・`diagnostics`）に、`context` を
//! 足した形で、`context.version`（[`render::FORMAT_VERSION`]）が形の版。 `diagnostics` には不完全の
//! 理由（`error`）と欠け（`note`）が入る。

use std::path::Path;
use std::process::ExitCode;

use crate::diagnostic::{Diagnostic, Severity, render_json, render_json_with};
use crate::repo::{self, RepoView};
use compare::{Changes, compare};
use compile::{
    Address, Context, RootInput, RootState, UnmappedReason, World, apply_budget, compile,
};
use render::{Incomplete, View};

mod compare;
mod compile;
mod render;
mod rules;

#[cfg(test)]
mod tests;

const USAGE: &str = "使い方: cargo xtask context --root <ID|パス>... [--at <rev>] [--from <rev>] [--format md|json] [--budget <n>]";

/// 出す形。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Markdown,
    Json,
}

/// コマンドの引数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Request {
    pub(crate) roots: Vec<String>,
    pub(crate) at: Option<String>,
    pub(crate) from: Option<String>,
    pub(crate) format: Format,
    pub(crate) budget: Option<usize>,
}

/// 出力と、合否。
#[derive(Debug)]
pub(crate) struct Output {
    pub(crate) text: String,
    pub(crate) ok: bool,
}

/// `cargo xtask context` の引数（コマンド名を除く）を受けて、終了コードを返す。
pub(crate) fn run(args: &[String]) -> ExitCode {
    let request = match parse(args) {
        Ok(request) => request,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let root = match repo::repo_root() {
        Ok(root) => root,
        Err(e) => {
            eprintln!("リポジトリのルートが見つからない: {e}");
            return ExitCode::FAILURE;
        }
    };
    let output = match open_and_execute(&root, &request) {
        Ok(output) => output,
        Err(message) => fatal(&root, request.format, &message),
    };
    print!("{}", output.text);
    if output.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// 版を開いて実行する。 版を解決できなければ、そのまま失敗にする。
fn open_and_execute(root: &Path, request: &Request) -> Result<Output, String> {
    let open = |rev: &Option<String>| -> Result<RepoView, String> {
        match rev {
            None => Ok(RepoView::working_tree(root)),
            Some(rev) => RepoView::revision(root, rev).map_err(|e| e.to_string()),
        }
    };
    let at = open(&request.at)?;
    let from = match &request.from {
        None => None,
        Some(rev) => Some(RepoView::revision(root, rev).map_err(|e| e.to_string())?),
    };
    Ok(execute(root, request, &at, from.as_ref()))
}

/// 版を解決できないなどの、Context を作る前の失敗。
fn fatal(root: &Path, format: Format, message: &str) -> Output {
    let text = match format {
        Format::Markdown => format!("context: {message}\n"),
        Format::Json => {
            let diagnostic = Diagnostic {
                severity: Severity::Error,
                code: "context.revision-unresolved",
                location: None,
                message: message.to_owned(),
            };
            format!("{}\n", render_json("context", &[diagnostic], Some(root)))
        }
    };
    Output { text, ok: false }
}

/// 読んだ版から Context を作って出す。 版の開き方に依らないので、試験はメモリ上の view を渡す。
pub(crate) fn execute(
    root: &Path,
    request: &Request,
    at: &RepoView,
    from: Option<&RepoView>,
) -> Output {
    let inputs: Vec<RootInput> = request.roots.iter().map(|r| RootInput::parse(r)).collect();

    let now_world = World::load(at);
    let before_world = from.map(World::load);
    let now_full = compile(&now_world, &inputs);
    let before_full = before_world.as_ref().map(|w| compile(w, &inputs));
    let changes: Option<Changes> = match (&before_world, &before_full) {
        (Some(bw), Some(bc)) => Some(compare(bw, bc, &now_world, &now_full)),
        _ => None,
    };

    let now = match request.budget {
        Some(limit) => apply_budget(&now_full, limit),
        None => now_full,
    };
    let incomplete = incomplete(&now, before_full.as_ref());

    let at_address = Address {
        given: request.at.clone(),
        identity: now_world.identity.clone(),
    };
    let from_address = before_world.as_ref().map(|w| Address {
        given: request.from.clone(),
        identity: w.identity.clone(),
    });
    let view = View {
        at: &at_address,
        from: from_address.as_ref(),
        now: &now,
        before: before_full.as_ref(),
        changes: changes.as_ref(),
        incomplete: &incomplete,
    };
    let ok = incomplete.is_empty();
    let text = match request.format {
        Format::Markdown => render::markdown(&view),
        Format::Json => {
            let diagnostics = diagnostics(&now, &incomplete);
            let payload = Some(("context", render::json_payload(&view)));
            format!(
                "{}\n",
                render_json_with("context", &diagnostics, Some(root), payload)
            )
        }
    };
    Output { text, ok }
}

/// 不完全として終わる理由。 root は、読んだどの版にも実体が無いときだけ。
fn incomplete(now: &Context, before: Option<&Context>) -> Vec<Incomplete> {
    let mut out = Vec::new();
    for (i, root) in now.roots.iter().enumerate() {
        let earlier = before.and_then(|b| b.roots.get(i)).map(|r| &r.state);
        let missing_then = earlier.is_none_or(|s| {
            matches!(
                s,
                RootState::NotFound | RootState::Unmapped(UnmappedReason::FileMissing)
            )
        });
        if !missing_then {
            continue;
        }
        let text = root.input.text();
        match (&root.input, &root.state) {
            (RootInput::Id(_), RootState::NotFound) => {
                out.push(Incomplete::RootNotFound { root: text });
            }
            (RootInput::Path(_), RootState::Unmapped(UnmappedReason::FileMissing)) => {
                out.push(Incomplete::FileMissing { root: text });
            }
            _ => {}
        }
    }
    if let Some(budget) = &now.budget
        && budget.exceeded()
    {
        out.push(Incomplete::RequiredOverBudget {
            required: budget.required,
            limit: budget.limit,
        });
    }
    out
}

/// JSON の `diagnostics`。 不完全の理由は `error`、欠けは `note`。
fn diagnostics(now: &Context, incomplete: &[Incomplete]) -> Vec<Diagnostic> {
    let mut out: Vec<Diagnostic> = incomplete
        .iter()
        .map(|i| Diagnostic {
            severity: Severity::Error,
            code: i.code(),
            location: None,
            message: i.message(),
        })
        .collect();
    let mut note = |code: &'static str, message: String| {
        out.push(Diagnostic {
            severity: Severity::Note,
            code,
            location: None,
            message,
        });
    };
    for root in &now.roots {
        if matches!(root.state, RootState::Unmapped(_)) {
            note(
                "context.unmapped",
                format!(
                    "UNMAPPED: `{}`。 関係する設計契約を特定できていない。 関係する契約が無いという意味ではない",
                    root.input.text()
                ),
            );
        }
    }
    if !now.broken.is_empty() {
        note(
            "context.broken-reference",
            format!("解けない参照・欠けが {} 件ある", now.broken.len()),
        );
    }
    if let Some(budget) = &now.budget
        && !budget.omitted.is_empty()
    {
        note(
            "context.omitted-by-budget",
            format!(
                "予算 {} で周辺 {} 件を削った。 削ったものは出力に含めていない",
                budget.limit,
                budget.omitted.len()
            ),
        );
    }
    out
}

/// 引数を読む。
fn parse(args: &[String]) -> Result<Request, String> {
    let mut roots = Vec::new();
    let (mut at, mut from, mut format, mut budget) = (None, None, Format::Markdown, None);
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => (name, Some(value.to_owned())),
            _ => (arg.as_str(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| it.next().cloned())
                .ok_or_else(|| format!("{name} の値が無い（{what}）\n{USAGE}"))
        };
        match name {
            "--root" => roots.push(value("ID か path")?),
            "--at" => at = Some(value("読む地点の版")?),
            "--from" => from = Some(value("比較元の版")?),
            "--budget" => {
                let raw = value("項目数")?;
                budget = Some(
                    raw.parse::<usize>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| format!("--budget は 1 以上の整数: `{raw}`\n{USAGE}"))?,
                );
            }
            "--format" => {
                format = match value("md か json")?.as_str() {
                    "md" => Format::Markdown,
                    "json" => Format::Json,
                    other => {
                        return Err(format!("--format は md か json: `{other}`\n{USAGE}"));
                    }
                };
            }
            // 起点は `--from`、読む地点は `--at` で固定する（提案書 7.4）。
            "--base" | "--to" => {
                return Err(format!(
                    "{name} は無い。 比較元は `--from`、読む地点は `--at`\n{USAGE}"
                ));
            }
            other => return Err(format!("知らない引数 `{other}`\n{USAGE}")),
        }
    }
    if roots.is_empty() {
        return Err(format!("--root が1つも無い\n{USAGE}"));
    }
    Ok(Request {
        roots,
        at,
        from,
        format,
        budget,
    })
}
