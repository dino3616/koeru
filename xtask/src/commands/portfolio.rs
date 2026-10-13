//! `cargo xtask portfolio [--receipts <dir>]... [--format human|json]`（D07c）。
//!
//! 要る Probe（`meta/suites/` の登録と、記録に実行の契約を書いた Probe）と、実行された
//! Receipt を突き合わせて、セルごとの状態と不足を出す。 突き合わせは
//! [`crate::views::portfolio`]、ここは受領証のディレクトリを読むことと出力の封筒だけを持つ。
//!
//! **検査ではない。** 不足があっても終了コードは 0。 合否は `test-receipt` が持つ
//! （`DEC-PLT-039`）。 落とすのは、受領証のディレクトリや受領証が読めないとき——読めない受領証を
//! 黙って飛ばすと、そのセルは missing に見えて、読めなかったことが消える。
//!
//! `--receipts` は何度でも付けられる。 CI の macOS・Linux・Windows の artifact を並べて渡す。
//! 省略すると、`test-receipt` が書く `target/receipts/`。 省略したときにそれが無ければ、
//! まだ何も走らせていないので、受領証が1枚も無いものとして扱う（全セルが missing になる）。
//! 付けたディレクトリが無ければ落とす。

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::Value;

use crate::diagnostic::{OutputFormat, Report, render_json};
use crate::knowledge::{Entry, KnowledgeSnapshot};
use crate::probe::parse_receipt_document;
use crate::views::portfolio::{self, Observed, Portfolio};

const USAGE: &str = "使い方: cargo xtask portfolio [--receipts <dir>]... [--format human|json]";

/// `--receipts` を集める。 知らない引数は、`usage` を返す使い方の誤り。
pub(super) fn receipt_dirs(args: &[String], usage: &str) -> Result<Vec<PathBuf>, String> {
    let mut dirs = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "--receipts" {
            dirs.push(PathBuf::from(it.next().ok_or(usage)?));
        } else if let Some(v) = a.strip_prefix("--receipts=") {
            dirs.push(PathBuf::from(v));
        } else {
            return Err(usage.to_owned());
        }
    }
    Ok(dirs)
}

/// 受領証を読む。 `dirs` が空なら `target/receipts/`（無くても誤りにしない）。
pub(super) fn load_receipts(root: &Path, dirs: &[PathBuf], rep: &mut Report) -> Vec<Observed> {
    let mut observed = Vec::new();
    if dirs.is_empty() {
        read_dir(
            root,
            &root.join("target/receipts"),
            true,
            rep,
            &mut observed,
        );
    }
    for dir in dirs {
        read_dir(root, dir, false, rep, &mut observed);
    }
    observed
}

/// ディレクトリの `*.json` を、ファイル名の昇順で読む。
///
/// 読めないものは `rep` に積んで、読めたものだけを返す。 `lenient_missing` は、ディレクトリが
/// 無いことを誤りにしない（省略時の `target/receipts/`）。
fn read_dir(
    root: &Path,
    dir: &Path,
    lenient_missing: bool,
    rep: &mut Report,
    out: &mut Vec<Observed>,
) {
    let shown = |p: &Path| {
        p.strip_prefix(root)
            .unwrap_or(p)
            .to_string_lossy()
            .replace('\\', "/")
    };
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if lenient_missing && e.kind() == ErrorKind::NotFound => {
            rep.note(format!(
                "{} が無い。 受領証がまだ1枚も無いものとして扱う",
                shown(dir)
            ));
            return;
        }
        Err(e) => {
            rep.error(format!(
                "受領証のディレクトリ {} を読めない: {e}",
                shown(dir)
            ));
            return;
        }
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in entries {
        match entry {
            Ok(e) => {
                let p = e.path();
                if p.extension().is_some_and(|x| x == "json") && p.is_file() {
                    files.push(p);
                }
            }
            Err(e) => rep.error(format!("{} の一覧を読めない: {e}", shown(dir))),
        }
    }
    files.sort();
    for file in files {
        let source = shown(&file);
        let text = match fs::read_to_string(&file) {
            Ok(t) => t,
            Err(e) => {
                rep.error(format!("{source} を読めない: {e}"));
                continue;
            }
        };
        match parse_receipt_document(&text) {
            Ok(doc) => out.extend(doc.receipts.into_iter().map(|receipt| Observed {
                source: source.clone(),
                receipt,
            })),
            Err(e) => rep.error(format!("{source}: {e}")),
        }
    }
}

pub(crate) fn portfolio(
    root: &Path,
    entries: &[Entry],
    snapshot: &KnowledgeSnapshot,
    args: &[String],
    format: OutputFormat,
    mut rep: Report,
) -> ExitCode {
    let dirs = match receipt_dirs(args, USAGE) {
        Ok(d) => d,
        Err(usage) => {
            eprintln!("{usage}");
            return ExitCode::FAILURE;
        }
    };
    let observed = load_receipts(root, &dirs, &mut rep);
    let probes = portfolio::definitions(entries, snapshot, &mut rep);
    let view = Portfolio::build(&probes, &observed);
    finish(
        "portfolio",
        root,
        &view.render_human(),
        view.to_json(),
        format,
        &rep,
    )
}

/// 値を、決めた形で出す。 人向けは本文のあとに `Report` の1行、機械向けは `check-*` と同じ
/// 封筒（`command` / `ok` / `diagnostics`）に `view` を足す。
pub(super) fn finish(
    command: &str,
    root: &Path,
    human: &str,
    view: Value,
    format: OutputFormat,
    rep: &Report,
) -> ExitCode {
    match format {
        OutputFormat::Human => {
            print!("{human}");
            // 診断は封筒と同じ並びで出る。 本文を先に出してから、1行の合否を足す。
            rep.finish(command)
        }
        OutputFormat::Json => {
            let mut envelope: Value =
                serde_json::from_str(&render_json(command, rep.diagnostics(), Some(root)))
                    .expect("render_json は JSON を返す");
            envelope[command.replace('-', "_")] = view;
            println!("{envelope}");
            if rep.has_errors() {
                ExitCode::FAILURE
            } else {
                ExitCode::SUCCESS
            }
        }
    }
}
