//! 引数の振り分けと、コマンドに渡すものの組み立て。
//!
//! コマンドを足すときは [`dispatch`] に1行と、[`USAGE`] に名前を足す。 検査の中身は
//! ここに書かない。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{self, Entry};
use crate::{checks, commands, probe, repo};

const USAGE: &str = "使い方: cargo xtask <check-meta|check-budgets|check-coverage
  check-references|check-profile <ID>
  index-decisions|next-id <接頭辞>|dump-requirements
  touched [<base>]
  check-portfolio|test-receipt [--runner cargo|bun]
  check-schema>";

/// どのコマンドにも渡すもの。
///
/// meta はコマンドを選ぶ前に読む。 読み込みの段の失敗は報告に積まれて、選んだコマンドの
/// 結果に混ざる——どのコマンドを走らせても、形の崩れた meta があれば落ちる。
struct Workspace {
    root: PathBuf,
    entries: Vec<Entry>,
    report: Report,
}

impl Workspace {
    fn open() -> Result<Self, String> {
        let root = repo::repo_root()?;
        let mut report = Report::default();
        let entries = knowledge::load(&root, &mut report);
        Ok(Self {
            root,
            entries,
            report,
        })
    }
}

/// `cargo xtask` の引数（プログラム名を除く）を受けて、終了コードを返す。
pub fn run(args: &[String]) -> ExitCode {
    let ws = match Workspace::open() {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("リポジトリのルートが見つからない: {e}");
            return ExitCode::FAILURE;
        }
    };
    dispatch(&ws.root, &ws.entries, args, ws.report)
}

fn dispatch(root: &Path, entries: &[Entry], args: &[String], rep: Report) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("check-meta") => checks::check_meta(root, entries, rep),
        Some("check-budgets") => checks::check_budgets(entries, rep),
        Some("check-coverage") => checks::check_coverage(entries, rep),
        Some("check-references") => checks::check_references(root, entries, rep),
        Some("check-profile") => match args.get(1) {
            Some(id) => checks::check_profile(entries, id, rep),
            None => {
                eprintln!("使い方: cargo xtask check-profile <PROFILE-ID>");
                ExitCode::FAILURE
            }
        },
        Some("index-decisions") => {
            let check = args.iter().any(|a| a == "--check");
            commands::index_decisions(root, entries, check, rep)
        }
        Some("check-portfolio") => probe::check_portfolio(root, entries, rep),
        Some("check-schema") => checks::check_schema(root, rep),
        Some("test-receipt") => probe::test_receipt(root, entries, &args[1..], rep),
        // 既定は `main`。PR レビューは main との差分を見るので、引数なしで足りる。
        Some("touched") => commands::touched(
            root,
            entries,
            args.get(1).map_or("main", String::as_str),
            rep,
        ),
        Some("next-id") => match args.get(1) {
            Some(prefix) => commands::next_id(entries, prefix, rep),
            None => {
                eprintln!("使い方: cargo xtask next-id <接頭辞>   例: DEC-PLT");
                ExitCode::FAILURE
            }
        },
        Some("dump-requirements") => commands::dump_requirements(entries),
        _ => {
            println!("{USAGE}");
            ExitCode::FAILURE
        }
    }
}
