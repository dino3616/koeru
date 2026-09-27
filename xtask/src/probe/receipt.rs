//! テストの受領証（`DEC-PLT-039`）。
//!
//! `cargo test` が緑でも、前提を欠いた試験は `return` して「通過」と数えられる。
//! ここでは試験 binary を1本ずつ走らせて件数を数え、`meta/suites/` の登録と突き合わせる。
//!
//! - `check-portfolio` — 試験の target がどれも1つの suite に登録されているか。組み立てない
//! - `test-receipt`    — 組み立てて走らせ、件数を登録と突き合わせ、受領証を書く
//!
//! 数えるのは libtest の安定版の出力（`--list` の `: test` 行と、`test result:` の要約行）。
//! 形が変わったら読めずに落ちる。黙って 0 件にしない。
//!
//! Probe の定義・実行環境・実行結果の型は [`super::model`] が持つ。ここはそれを cargo と
//! bun の実際の起動へつなぐだけ。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use super::model::{Counts, ExecutionContext, NotRunReason, ProbeDefinition, Receipt, Status};
use crate::diagnostic::Report;
use crate::knowledge::{Entry, with_schema};

const PLATFORMS: [&str; 3] = ["macos", "linux", "windows"];
const BACKENDS: [&str; 2] = ["native", "unsupported"];
const RUNNERS: [&str; 2] = ["cargo", "bun"];

/// `meta/suites/` の登録を読んで、形を確かめる。
pub(crate) fn suites(entries: &[Entry], rep: &mut Report) -> Vec<ProbeDefinition> {
    let mut out = Vec::new();
    for e in with_schema(entries, "test-portfolio") {
        for t in e.items() {
            match ProbeDefinition::from_suite(&t) {
                Ok(s) => {
                    for p in &s.platforms {
                        if !PLATFORMS.contains(&p.as_str()) {
                            rep.error(format!("{}: platforms の `{p}` を知らない", s.id));
                        }
                    }
                    for b in &s.backends {
                        if !BACKENDS.contains(&b.as_str()) {
                            rep.error(format!("{}: backends の `{b}` を知らない", s.id));
                        }
                    }
                    if !RUNNERS.contains(&s.runner.as_str()) {
                        rep.error(format!("{}: runner の `{}` を知らない", s.id, s.runner));
                    }
                    if s.platforms.is_empty() {
                        rep.error(format!(
                            "{}: platforms が空。どこでも数えない suite になる",
                            s.id
                        ));
                    }
                    out.push(s);
                }
                Err(msg) => rep.error(format!("{}: {msg}", e.path.display())),
            }
        }
    }
    out
}

/// `cargo metadata --no-deps` の生の JSON。 target の一覧と package 名の両方が要るので、
/// 呼び出しは1箇所にまとめる。
fn cargo_metadata(root: &Path) -> Result<serde_json::Value, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata を起動できない: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata を読めない: {e}"))
}

/// 試験を持ちうる target（`cargo metadata` から）。 (package, target) の組。
fn cargo_targets(root: &Path) -> Result<BTreeSet<(String, String)>, String> {
    let meta = cargo_metadata(root)?;
    let mut set = BTreeSet::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let name = pkg["name"].as_str().unwrap_or_default().to_owned();
        for t in pkg["targets"].as_array().into_iter().flatten() {
            let kinds: Vec<&str> = t["kind"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .collect();
            let target = if kinds.contains(&"test") {
                t["name"].as_str().unwrap_or_default().to_owned()
            } else if kinds.iter().any(|k| k.ends_with("lib")) {
                "lib".to_owned()
            } else if kinds.contains(&"bin") && t["test"].as_bool().unwrap_or(true) {
                // bin も試験 binary になる。 試験を持たない起動口も、0 件が正しい suite として登録する。
                format!("bin:{}", t["name"].as_str().unwrap_or_default())
            } else {
                continue;
            };
            set.insert((name.clone(), target));
        }
    }
    Ok(set)
}

/// crate のディレクトリ → package 名の表（`cargo metadata` の `manifest_path` から）。
///
/// `cargo test --message-format=json` の `package_id` は cargo の版で形が割れる
/// （`path+file:///…/koeru-core#0.0.0` と `koeru-core 0.0.0 (path+file:///…)`）。
/// 前者はディレクトリ名を package 名として読むしかないが、crate のディレクトリ名と
/// `[package] name` が違うとそれは実際の package 名と一致しない。 `cargo metadata` は
/// 両方の版で `manifest_path` を持つので、そちらから引ける表を先に作り、
/// 表に無いときだけ [`package_name`] の文字列読みに落ちる。
fn package_names_by_manifest(root: &Path) -> Result<BTreeMap<PathBuf, String>, String> {
    let meta = cargo_metadata(root)?;
    let mut map = BTreeMap::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let name = pkg["name"].as_str().unwrap_or_default();
        let manifest = pkg["manifest_path"].as_str().unwrap_or_default();
        if !name.is_empty() && !manifest.is_empty() {
            map.insert(PathBuf::from(manifest), name.to_owned());
        }
    }
    Ok(map)
}

/// 試験の target がどれも、ちょうど1つの suite に登録されているか。
pub(crate) fn check_portfolio(root: &Path, entries: &[Entry], mut rep: Report) -> ExitCode {
    let suites = suites(entries, &mut rep);
    let targets = match cargo_targets(root) {
        Ok(t) => t,
        Err(e) => {
            rep.error(e);
            return rep.finish("check-portfolio");
        }
    };
    let mut registered: BTreeMap<(String, String), Vec<String>> = BTreeMap::new();
    for s in suites.iter().filter(|s| s.runner == "cargo") {
        registered
            .entry((s.package.clone(), s.target.clone()))
            .or_default()
            .push(s.id.clone());
    }
    for key in &targets {
        match registered.get(key) {
            None => rep.error(format!(
                "{} の {} がどの suite にも登録されていない。`meta/suites/` に足す",
                key.0, key.1
            )),
            Some(ids) if ids.len() > 1 => rep.error(format!(
                "{} の {} を複数の suite が名乗っている: {}",
                key.0,
                key.1,
                ids.join(", ")
            )),
            Some(_) => {}
        }
    }
    for (key, ids) in &registered {
        if !targets.contains(key) {
            rep.error(format!(
                "{}: {} の {} という target は無い",
                ids.join(", "),
                key.0,
                key.1
            ));
        }
    }
    rep.note(format!(
        "suite {} 件 / cargo の試験 target {} 個",
        suites.len(),
        targets.len()
    ));
    if suites.is_empty() {
        rep.error("suite が1件も無い。`meta/suites/` を確かめる");
    }
    rep.finish("check-portfolio")
}

/// 組み立てた試験 binary（`cargo test --no-run` の JSON から）。
#[derive(Debug)]
struct Executable {
    package: String,
    target: String,
    path: PathBuf,
    dir: PathBuf,
}

fn build_executables(root: &Path) -> Result<Vec<Executable>, String> {
    // `package_id` の文字列読みが外れる crate（ディレクトリ名と package 名が違う）を、
    // `cargo metadata` の実体で補う。
    let names = package_names_by_manifest(root)?;
    let out = Command::new("cargo")
        .args([
            "test",
            "--workspace",
            "--all-features",
            "--no-run",
            "--message-format=json",
        ])
        .current_dir(root)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("cargo test --no-run を起動できない: {e}"))?;
    if !out.status.success() {
        return Err("試験を組み立てられない".into());
    }
    let mut exes = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        let Ok(msg) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if msg["reason"] != "compiler-artifact"
            || !msg["profile"]["test"].as_bool().unwrap_or(false)
        {
            continue;
        }
        let Some(exe) = msg["executable"].as_str() else {
            continue;
        };
        let t = &msg["target"];
        let kinds: Vec<&str> = t["kind"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(serde_json::Value::as_str)
            .collect();
        let name = t["name"].as_str().unwrap_or_default();
        let target = if kinds.contains(&"test") {
            name.to_owned()
        } else if kinds.iter().any(|k| k.ends_with("lib")) {
            "lib".to_owned()
        } else {
            format!("bin:{name}")
        };
        let manifest = PathBuf::from(msg["manifest_path"].as_str().unwrap_or_default());
        let package = names
            .get(&manifest)
            .cloned()
            .unwrap_or_else(|| package_name(&msg["package_id"]));
        exes.push(Executable {
            package,
            target,
            path: PathBuf::from(exe),
            dir: manifest.parent().map(Path::to_path_buf).unwrap_or_default(),
        });
    }
    Ok(exes)
}

/// `package_id` から crate の名前を取り出す。
///
/// 形は cargo の版で2通りある。 `path+file:///…/koeru-core#0.0.0` と
/// `koeru-core 0.0.0 (path+file:///…)`。 前者はディレクトリ名を package 名として
/// 読むので、ディレクトリ名と `[package] name` が違う crate では外れる。
/// [`package_names_by_manifest`] の表に無いときだけ、ここへ落ちる保険。
fn package_name(id: &serde_json::Value) -> String {
    let id = id.as_str().unwrap_or_default();
    if let Some((head, _)) = id.split_once(' ') {
        return head.to_owned();
    }
    let path = id.split('#').next().unwrap_or_default();
    path.rsplit('/').next().unwrap_or_default().to_owned()
}

/// `--list` の行から、試験の数を数える。
fn discover(exe: &Executable) -> Result<u64, String> {
    let out = Command::new(&exe.path)
        .args(["--list", "--format", "terse"])
        .current_dir(&exe.dir)
        .output()
        .map_err(|e| format!("{} を起動できない: {e}", exe.path.display()))?;
    if !out.status.success() {
        return Err(format!("{} の --list が失敗した", exe.path.display()));
    }
    Ok(String::from_utf8_lossy(&out.stdout)
        .lines()
        .filter(|l| l.ends_with(": test"))
        .count() as u64)
}

/// 走らせて、要約行から件数を読む。 出力はそのまま流す。
fn run(exe: &Executable) -> Result<Counts, String> {
    let out = Command::new(&exe.path)
        .current_dir(&exe.dir)
        .stderr(std::process::Stdio::inherit())
        .output()
        .map_err(|e| format!("{} を起動できない: {e}", exe.path.display()))?;
    let stdout = String::from_utf8_lossy(&out.stdout);
    print!("{stdout}");
    libtest_counts(&stdout).ok_or_else(|| {
        format!(
            "{} の出力に `test result:` が無い。libtest の形が変わったか、途中で落ちた",
            exe.path.display()
        )
    })
}

/// `test result: ok. 9 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; …` を読む。
///
/// 欄が1つでも読めなければ `None`。 読めない欄を 0 に倒すと、形が変わったときに
/// 全部が 0 件の「通過」になる。
fn libtest_counts(stdout: &str) -> Option<Counts> {
    let summary = stdout
        .lines()
        .rev()
        .find(|l| l.starts_with("test result:"))?;
    let (_, rest) = summary.split_once(". ")?;
    let mut fields = BTreeMap::new();
    for part in rest.split(';') {
        let mut it = part.split_whitespace();
        if let (Some(n), Some(key)) = (it.next(), it.next())
            && let Ok(n) = n.parse::<u64>()
        {
            fields.insert(key.to_owned(), n);
        }
    }
    Some(Counts {
        discovered: 0,
        passed: *fields.get("passed")?,
        failed: *fields.get("failed")?,
        ignored: *fields.get("ignored")?,
    })
}

/// 件数から、床割れ・無視過多・件数不一致を集める。 `status` の判定はここでは
/// 決めない——bun の runner は「終了コードだけが失敗」を、この一覧が空だったときに
/// 限って足すので、その判定より前に呼ぶ。
fn collect_problems(s: &ProbeDefinition, counts: &Counts, applies: bool) -> Vec<String> {
    let mut problems = Vec::new();
    let executed = counts.passed + counts.failed;
    if counts.failed > 0 {
        problems.push(format!("{} 件が失敗した", counts.failed));
    }
    if applies && executed < s.min_cases {
        problems.push(format!(
            "実行したのが {executed} 件で、登録の下限 {} 件に届かない",
            s.min_cases
        ));
    }
    // 無視は手動のハーネスだけ。 登録より多いのは、黙って外した試験がある合図。
    if counts.ignored > s.manual {
        problems.push(format!(
            "{} 件を無視した。登録は手動 {} 件まで",
            counts.ignored, s.manual
        ));
    }
    if counts.discovered != executed + counts.ignored {
        problems.push(format!(
            "見つけた {} 件と、実行 {executed} 件 ＋ 無視 {} 件が合わない",
            counts.discovered, counts.ignored
        ));
    }
    problems
}

/// 件数と問題の一覧から [`Receipt`] を組む。 `status` は問題が確定したあとに決める。
fn make_receipt(
    s: &ProbeDefinition,
    counts: Counts,
    applies: bool,
    problems: Vec<String>,
) -> Receipt {
    let status = if !problems.is_empty() {
        Status::Failed
    } else if applies {
        Status::Passed
    } else {
        Status::NotApplicable
    };
    Receipt {
        probe: s.id.clone(),
        package: s.package.clone(),
        target: s.target.clone(),
        applies,
        status,
        actual_work: counts.passed + counts.failed,
        counts,
        problems,
    }
}

/// suite ごとの判定。
fn judge(s: &ProbeDefinition, counts: Counts, ctx: &ExecutionContext) -> Receipt {
    let applies = s.applies(ctx);
    let problems = collect_problems(s, &counts, applies);
    make_receipt(s, counts, applies, problems)
}

/// `cargo xtask test-receipt [--runner cargo|bun]`
pub(crate) fn test_receipt(
    root: &Path,
    entries: &[Entry],
    args: &[String],
    mut rep: Report,
) -> ExitCode {
    let runner = args
        .windows(2)
        .find(|w| w[0] == "--runner")
        .map_or("cargo", |w| w[1].as_str());
    let ctx = ExecutionContext::detect(root);
    let probes = suites(entries, &mut rep);
    let receipts = match runner {
        "cargo" => cargo_receipts(root, &probes, &ctx, &mut rep),
        "bun" => bun_receipts(root, &probes, &ctx, &mut rep),
        other => {
            rep.error(format!("runner の `{other}` を知らない（cargo か bun）"));
            Vec::new()
        }
    };
    for v in &receipts {
        for p in &v.problems {
            rep.error(format!("{}（{} の {}）: {p}", v.probe, v.package, v.target));
        }
    }
    // `NotRun`（組み立たなかった suite）はここまでで Report の誤りとして報告済み。
    // 受領証にも要約の件数にも積まない——今のところ「見つからなかった」ことは
    // その誤りの行だけで表す。
    let countable: Vec<Receipt> = receipts.into_iter().filter(Receipt::countable).collect();
    let executed: u64 = countable.iter().map(|v| v.actual_work).sum();
    let ignored: u64 = countable.iter().map(|v| v.counts.ignored).sum();
    rep.note(format!(
        "{} / {} / {runner}: suite {} 件、実行 {executed} 件、無視 {ignored} 件",
        ctx.platform,
        ctx.backend,
        countable.len()
    ));
    if countable.is_empty() {
        rep.error("1件も数えられなかった");
    }
    match write_receipt(root, runner, &ctx, &countable) {
        Ok(path) => rep.note(format!("受領証: {}", path.display())),
        Err(e) => rep.error(format!("受領証を書けない: {e}")),
    }
    rep.finish("test-receipt")
}

fn cargo_receipts(
    root: &Path,
    probes: &[ProbeDefinition],
    ctx: &ExecutionContext,
    rep: &mut Report,
) -> Vec<Receipt> {
    let exes = match build_executables(root) {
        Ok(e) => e,
        Err(e) => {
            rep.error(e);
            return Vec::new();
        }
    };
    let by_key: BTreeMap<(&str, &str), &ProbeDefinition> = probes
        .iter()
        .filter(|s| s.runner == "cargo")
        .map(|s| ((s.package.as_str(), s.target.as_str()), s))
        .collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for exe in &exes {
        let Some(probe) = by_key.get(&(exe.package.as_str(), exe.target.as_str())) else {
            rep.error(format!(
                "{} の {} がどの suite にも登録されていない",
                exe.package, exe.target
            ));
            continue;
        };
        seen.insert(probe.id.clone());
        let counts =
            discover(exe).and_then(|discovered| run(exe).map(|c| Counts { discovered, ..c }));
        match counts {
            Ok(c) => out.push(judge(probe, c, ctx)),
            Err(e) => rep.error(format!("{}: {e}", probe.id)),
        }
    }
    // この環境で数えるはずの suite が、組み立てにも出てこなかった。
    for s in probes
        .iter()
        .filter(|s| s.runner == "cargo" && s.applies(ctx))
    {
        if !seen.contains(&s.id) {
            rep.error(format!(
                "{}: {} の {} の試験 binary が組み立たなかった",
                s.id, s.package, s.target
            ));
            out.push(Receipt::not_run(s, NotRunReason::BuildFailed));
        }
    }
    out
}

/// UI の story 試験。 vitest の要約行（`Tests  207 passed (207)`）を読む。
fn bun_receipts(
    root: &Path,
    probes: &[ProbeDefinition],
    ctx: &ExecutionContext,
    rep: &mut Report,
) -> Vec<Receipt> {
    let mut out = Vec::new();
    for s in probes.iter().filter(|s| s.runner == "bun") {
        let dir = root.join(&s.package);
        let res = Command::new("bun")
            .args(["run", &s.target])
            .current_dir(&dir)
            .stderr(std::process::Stdio::inherit())
            .output();
        let output = match res {
            Ok(o) => o,
            Err(e) => {
                rep.error(format!("{}: bun を起動できない: {e}", s.id));
                continue;
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        print!("{stdout}");
        match vitest_counts(&stdout) {
            Some(c) => {
                let applies = s.applies(ctx);
                let mut problems = collect_problems(s, &c, applies);
                if !output.status.success() && problems.is_empty() {
                    problems.push("bun が失敗を返した".into());
                }
                out.push(make_receipt(s, c, applies, problems));
            }
            None => rep.error(format!(
                "{}: vitest の要約行（`Tests …`）が無い。形が変わったか、途中で落ちた",
                s.id
            )),
        }
    }
    out
}

/// `Tests  205 passed | 2 skipped (207)` のような行から件数を読む。
fn vitest_counts(stdout: &str) -> Option<Counts> {
    let line = stdout
        .lines()
        .map(strip_ansi)
        .find(|l| l.trim_start().starts_with("Tests "))?;
    let body = line.trim_start().strip_prefix("Tests")?.trim();
    let (parts, total) = body.rsplit_once('(')?;
    let total: u64 = total.trim_end_matches(')').trim().parse().ok()?;
    let mut c = Counts {
        discovered: total,
        ..Counts::default()
    };
    for part in parts.split('|') {
        let mut it = part.split_whitespace();
        let (Some(n), Some(what)) = (it.next(), it.next()) else {
            continue;
        };
        let n: u64 = n.parse().ok()?;
        match what {
            "passed" => c.passed = n,
            "failed" => c.failed = n,
            "skipped" | "todo" => c.ignored += n,
            _ => {}
        }
    }
    Some(c)
}

fn strip_ansi(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// 受領証を `target/receipts/` に書く。 CI では job の要約にも出す。
///
/// `NotRun` は呼び出し側（[`test_receipt`]）がすでに除いているので、ここに来る
/// 受領証は `Passed` / `Failed` / `NotApplicable`（と、まだ実際には作らない
/// `Skipped`）だけになる。
fn write_receipt(
    root: &Path,
    runner: &str,
    ctx: &ExecutionContext,
    receipts: &[Receipt],
) -> Result<PathBuf, String> {
    let suites: Vec<serde_json::Value> = receipts
        .iter()
        .map(|v| {
            serde_json::json!({
                "suite": v.probe,
                "package": v.package,
                "target": v.target,
                "applies": v.applies,
                "discovered": v.counts.discovered,
                "passed": v.counts.passed,
                "failed": v.counts.failed,
                "ignored": v.counts.ignored,
                "result": match v.status {
                    Status::Passed => "passed",
                    Status::Failed => "failed",
                    Status::NotApplicable => "not-applicable",
                    Status::Skipped => "skipped",
                    Status::NotRun(_) => "not-run",
                },
                "problems": v.problems,
            })
        })
        .collect();
    let receipt = serde_json::json!({
        "git": ctx.git_sha,
        "dirty": ctx.dirty,
        "platform": ctx.platform,
        "backend": ctx.backend,
        "runner": runner,
        "suites": suites,
    });
    let dir = root.join("target/receipts");
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{runner}-{}-{}.json", ctx.platform, ctx.backend));
    let text = serde_json::to_string_pretty(&receipt).map_err(|e| e.to_string())?;
    fs::write(&path, text).map_err(|e| e.to_string())?;

    if let Ok(summary) = std::env::var("GITHUB_STEP_SUMMARY") {
        let mut md = format!(
            "### 受領証（{} / {} / {runner}）\n\n| suite | target | 実行 | 無視 | 結果 |\n|---|---|---|---|---|\n",
            ctx.platform, ctx.backend
        );
        for v in receipts {
            let result = match v.status {
                Status::Failed => "失敗",
                Status::Passed => "通過",
                Status::NotApplicable => "対象外",
                Status::Skipped => "無視",
                Status::NotRun(_) => "未実行",
            };
            let _ = writeln!(
                md,
                "| {} | {} {} | {} | {} | {result} |",
                v.probe,
                v.package,
                v.target,
                v.counts.passed + v.counts.failed,
                v.counts.ignored
            );
        }
        let _ = fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(summary)
            .and_then(|mut f| std::io::Write::write_all(&mut f, md.as_bytes()));
    }
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::super::model::Prerequisites;
    use super::*;

    #[test]
    fn libtest_の要約行を読む() {
        let out = "running 3 tests\ntest a ... ok\n\ntest result: ok. 2 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 0.01s\n";
        let c = libtest_counts(out).expect("読める");
        assert_eq!((c.passed, c.failed, c.ignored), (2, 0, 1));
        // 欄が読めなければ 0 に倒さず、読めないと言う。
        assert!(libtest_counts("test result: ok. 2 succeeded").is_none());
        assert!(libtest_counts("no summary").is_none());
    }

    #[test]
    fn crate_の名前を取り出す() {
        assert_eq!(
            package_name(&serde_json::json!("path+file:///x/crates/koeru-core#0.0.0")),
            "koeru-core"
        );
        assert_eq!(
            package_name(&serde_json::json!("koeru-core 0.0.0 (path+file:///x)")),
            "koeru-core"
        );
    }

    #[test]
    fn vitest_の要約行を読む() {
        let c = vitest_counts("\u{1b}[2m      Tests \u{1b}[22m 205 passed | 2 skipped (207)\n")
            .expect("読める");
        assert_eq!((c.discovered, c.passed, c.ignored), (207, 205, 2));
        assert!(vitest_counts("no summary").is_none());
    }

    fn probe(min: u64, manual: u64) -> ProbeDefinition {
        ProbeDefinition {
            id: "SUITE-X-001".into(),
            runner: "cargo".into(),
            package: "p".into(),
            target: "lib".into(),
            platforms: vec!["macos".into(), "linux".into()],
            backends: vec!["native".into(), "unsupported".into()],
            min_cases: min,
            manual,
            prerequisites: Prerequisites::default(),
        }
    }

    const CTX_PLATFORM: &str = "linux";
    const CTX_BACKEND: &str = "unsupported";

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            platform: CTX_PLATFORM,
            backend: CTX_BACKEND,
            git_sha: String::new(),
            dirty: false,
            runner_version: None,
        }
    }

    #[test]
    fn 実行が0件なら通さない() {
        let ctx = ctx();
        let v = judge(&probe(1, 0), Counts::default(), &ctx);
        assert!(!v.problems.is_empty(), "0件で通った");
        assert_eq!(v.status, Status::Failed);
    }

    #[test]
    fn 登録より多く無視したら通さない() {
        let ctx = ctx();
        let c = Counts {
            discovered: 3,
            passed: 2,
            failed: 0,
            ignored: 1,
        };
        assert!(!judge(&probe(1, 0), c, &ctx).problems.is_empty());
        assert!(judge(&probe(1, 1), c, &ctx).problems.is_empty());
    }

    #[test]
    fn 対象外は問題が無ければ_not_applicable() {
        let ctx = ExecutionContext {
            platform: "windows",
            backend: "unsupported",
            git_sha: String::new(),
            dirty: false,
            runner_version: None,
        };
        let c = Counts {
            discovered: 2,
            passed: 2,
            failed: 0,
            ignored: 0,
        };
        let v = judge(&probe(2, 0), c, &ctx);
        assert!(!v.applies);
        assert_eq!(v.status, Status::NotApplicable);
    }

    #[test]
    fn 組み立たない_suite_は_not_run_として区別できる() {
        let s = probe(1, 0);
        let r = Receipt::not_run(&s, NotRunReason::BuildFailed);
        assert_eq!(r.status, Status::NotRun(NotRunReason::BuildFailed));
        assert!(!r.countable());
    }
}
