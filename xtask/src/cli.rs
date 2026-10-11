//! 引数の振り分けと、コマンドに渡すものの組み立て。
//!
//! `check-*` は [`checks::CHECKS`] の登録から引く。 検査を足すときはここを触らず、
//! `checks` に1ファイルと登録を1行足す。 それ以外のコマンドを足すときは [`dispatch`] に
//! 1行と、[`USAGE`] に名前を足す。 検査の中身はここに書かない。
//!
//! 機械が読む出力: `--format json` をどこに置いても（コマンドの前でも後ろでも）、
//! `check-*` と `check-portfolio` は `{ command, ok, diagnostics }` の1行の JSON を出す。
//! 終了コードは人向けの出力と同じ。 ほかのコマンドは自分の出力を持つので、指定すると失敗する。
//! 既定（指定なし、`--format human`）の出力は変わらない。 [`USAGE`] は出力の一部として
//! 固定されているので、この指定はここに書く。

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::checks::{self, CheckContext};
use crate::diagnostic::{OutputFormat, Report};
use crate::knowledge::{self, Entry, KnowledgeSnapshot};
use crate::{commands, probe, repo};

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
    /// `entries` を型つきで読み、FSL の索引を足したもの。 検査が共有する。
    snapshot: KnowledgeSnapshot,
    report: Report,
}

impl Workspace {
    fn open(format: OutputFormat) -> Result<Self, String> {
        let root = repo::repo_root()?;
        let mut report = Report::default();
        report.set_output(format, &root);
        let entries = knowledge::load(&root, &mut report);
        // 重複 ID の一覧は捨てる。 報告するのは `check-meta` だけで、それは自分の読みで
        // 数える——ここで積むと、どのコマンドにも重複が混ざる。
        let (snapshot, _duplicates) = KnowledgeSnapshot::from_entries(&entries);
        let snapshot = snapshot.with_fsl(&root);
        Ok(Self {
            root,
            entries,
            snapshot,
            report,
        })
    }
}

/// `cargo xtask` の引数（プログラム名を除く）を受けて、終了コードを返す。
pub fn run(args: &[String]) -> ExitCode {
    let (format, args) = match split_format(args) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let ws = match Workspace::open(format) {
        Ok(ws) => ws,
        Err(e) => {
            eprintln!("リポジトリのルートが見つからない: {e}");
            return ExitCode::FAILURE;
        }
    };
    if format == OutputFormat::Json && !supports_json(args.first().map(String::as_str)) {
        eprintln!("--format json は check-* と check-portfolio だけで使える");
        return ExitCode::FAILURE;
    }
    dispatch(ws, &args)
}

/// `--format <human|json>` を引数から外して、指定を返す。 位置は問わない。
fn split_format(args: &[String]) -> Result<(OutputFormat, Vec<String>), String> {
    let mut format = OutputFormat::Human;
    let mut rest = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let value = if a == "--format" {
            match it.next() {
                Some(v) => Some(v.as_str()),
                None => return Err("使い方: --format <human|json>".to_owned()),
            }
        } else {
            a.strip_prefix("--format=")
        };
        match value {
            None => rest.push(a.clone()),
            Some("human") => format = OutputFormat::Human,
            Some("json") => format = OutputFormat::Json,
            Some(_) => return Err("使い方: --format <human|json>".to_owned()),
        }
    }
    Ok((format, rest))
}

fn supports_json(command: Option<&str>) -> bool {
    command.is_some_and(|c| c == "check-portfolio" || checks::find(c).is_some())
}

fn dispatch(ws: Workspace, args: &[String]) -> ExitCode {
    let Workspace {
        root,
        entries,
        snapshot,
        report: rep,
    } = ws;
    let command = args.first().map(String::as_str);
    if let Some(check) = command.and_then(checks::find) {
        return run_check(check, &root, &entries, &snapshot, &args[1..], rep);
    }
    dispatch_other(&root, &entries, args, rep)
}

fn run_check(
    check: &checks::Check,
    root: &Path,
    entries: &[Entry],
    snapshot: &KnowledgeSnapshot,
    args: &[String],
    mut rep: Report,
) -> ExitCode {
    if args.len() < check.required_args {
        eprintln!("{}", check.usage);
        return ExitCode::FAILURE;
    }
    let ctx = CheckContext {
        root,
        entries,
        snapshot,
        args,
    };
    (check.run)(&ctx, &mut rep);
    rep.finish(check.name)
}

/// 検査ではないコマンドと、登録を持たない `check-portfolio`。
fn dispatch_other(root: &Path, entries: &[Entry], args: &[String], rep: Report) -> ExitCode {
    match args.first().map(String::as_str) {
        Some("index-decisions") => {
            let check = args.iter().any(|a| a == "--check");
            commands::index_decisions(root, entries, check, rep)
        }
        Some("check-portfolio") => probe::check_portfolio(root, entries, rep),
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

#[cfg(test)]
mod tests {
    use super::*;

    fn args(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn 指定が無ければ人向けで引数はそのまま() {
        let (f, rest) = split_format(&args(&["check-meta"])).expect("読める");
        assert_eq!(f, OutputFormat::Human);
        assert_eq!(rest, args(&["check-meta"]));
    }

    #[test]
    fn format_はコマンドの前でも後ろでも読める() {
        for a in [
            &["--format", "json", "check-profile", "P"][..],
            &["check-profile", "--format", "json", "P"][..],
            &["check-profile", "P", "--format=json"][..],
        ] {
            let (f, rest) = split_format(&args(a)).expect("読める");
            assert_eq!(f, OutputFormat::Json);
            assert_eq!(rest, args(&["check-profile", "P"]));
        }
    }

    #[test]
    fn 知らない形式と値の無い指定は拒む() {
        assert!(split_format(&args(&["check-meta", "--format", "xml"])).is_err());
        assert!(split_format(&args(&["check-meta", "--format"])).is_err());
        assert!(split_format(&args(&["--format=yaml"])).is_err());
    }

    #[test]
    fn json_に対応するのは登録した検査と_check_portfolio() {
        assert!(supports_json(Some("check-meta")));
        assert!(supports_json(Some("check-schema")));
        assert!(supports_json(Some("check-portfolio")));
        assert!(!supports_json(Some("next-id")));
        assert!(!supports_json(Some("touched")));
        assert!(!supports_json(None));
    }
}
