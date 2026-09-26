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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::diagnostic::Report;
use crate::knowledge::{Entry, list_of, str_of, with_schema};
use crate::repo::git;

/// 1件の suite の登録。
#[derive(Debug, Clone)]
pub(crate) struct Suite {
    pub(crate) id: String,
    runner: String,
    package: String,
    target: String,
    platforms: Vec<String>,
    backends: Vec<String>,
    min_cases: u64,
    manual: u64,
}

impl Suite {
    fn from_table(t: &toml::Table) -> Result<Self, String> {
        let id = str_of(t, "id").ok_or("id が無い")?.to_owned();
        let need = |k: &str| {
            str_of(t, k)
                .map(str::to_owned)
                .ok_or_else(|| format!("{id}: `{k}` が無い"))
        };
        let int = |k: &str| t.get(k).and_then(toml::Value::as_integer);
        let backends = list_of(t, "backends");
        Ok(Self {
            runner: need("runner")?,
            package: need("package")?,
            target: need("target")?,
            platforms: list_of(t, "platforms"),
            backends: if backends.is_empty() {
                vec!["native".into(), "unsupported".into()]
            } else {
                backends
            },
            min_cases: int("min_cases")
                .and_then(|n| u64::try_from(n).ok())
                .ok_or_else(|| format!("{id}: `min_cases` が無い、または負"))?,
            manual: int("manual")
                .and_then(|n| u64::try_from(n).ok())
                .unwrap_or(0),
            id,
        })
    }

    /// この環境で件数を求めるか。
    fn applies(&self, ctx: &Context) -> bool {
        self.platforms.iter().any(|p| p == ctx.platform)
            && self.backends.iter().any(|b| b == ctx.backend)
    }
}

/// 実行した環境。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Context {
    platform: &'static str,
    backend: &'static str,
}

impl Context {
    fn detect() -> Self {
        let platform = std::env::consts::OS;
        // 音声のバックエンドは macOS にしか無い。 他の OS と、強制した組み立ては
        // 「書いていない OS」の席が選ばれる。
        let forced = std::env::var("RUSTFLAGS")
            .unwrap_or_default()
            .contains("koeru_force_unsupported_backend");
        let backend = if platform == "macos" && !forced {
            "native"
        } else {
            "unsupported"
        };
        Self { platform, backend }
    }
}

const PLATFORMS: [&str; 3] = ["macos", "linux", "windows"];
const BACKENDS: [&str; 2] = ["native", "unsupported"];
const RUNNERS: [&str; 2] = ["cargo", "bun"];

/// `meta/suites/` の登録を読んで、形を確かめる。
pub(crate) fn suites(entries: &[Entry], rep: &mut Report) -> Vec<Suite> {
    let mut out = Vec::new();
    for e in with_schema(entries, "test-portfolio") {
        for t in e.items() {
            match Suite::from_table(&t) {
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

/// 試験を持ちうる target（`cargo metadata` から）。 (package, target) の組。
fn cargo_targets(root: &Path) -> Result<BTreeSet<(String, String)>, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata を起動できない: {e}"))?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    let meta: serde_json::Value = serde_json::from_slice(&out.stdout)
        .map_err(|e| format!("cargo metadata を読めない: {e}"))?;
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

/// 1本の試験 binary の件数。
#[derive(Debug, Default, Clone, Copy)]
struct Counts {
    discovered: u64,
    passed: u64,
    failed: u64,
    ignored: u64,
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
        exes.push(Executable {
            package: package_name(&msg["package_id"]),
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
/// `koeru-core 0.0.0 (path+file:///…)`。
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

/// suite ごとの判定。
#[derive(Debug)]
struct Verdict {
    suite: String,
    package: String,
    target: String,
    counts: Counts,
    /// この環境で件数を求めたか。 求めないなら記録だけ。
    applies: bool,
    problems: Vec<String>,
}

fn judge(s: &Suite, counts: Counts, ctx: &Context) -> Verdict {
    let mut problems = Vec::new();
    let applies = s.applies(ctx);
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
    Verdict {
        suite: s.id.clone(),
        package: s.package.clone(),
        target: s.target.clone(),
        counts,
        applies,
        problems,
    }
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
    let ctx = Context::detect();
    let suites = suites(entries, &mut rep);
    let verdicts = match runner {
        "cargo" => cargo_verdicts(root, &suites, &ctx, &mut rep),
        "bun" => bun_verdicts(root, &suites, &ctx, &mut rep),
        other => {
            rep.error(format!("runner の `{other}` を知らない（cargo か bun）"));
            Vec::new()
        }
    };
    for v in &verdicts {
        for p in &v.problems {
            rep.error(format!("{}（{} の {}）: {p}", v.suite, v.package, v.target));
        }
    }
    let executed: u64 = verdicts
        .iter()
        .map(|v| v.counts.passed + v.counts.failed)
        .sum();
    let ignored: u64 = verdicts.iter().map(|v| v.counts.ignored).sum();
    rep.note(format!(
        "{} / {} / {runner}: suite {} 件、実行 {executed} 件、無視 {ignored} 件",
        ctx.platform,
        ctx.backend,
        verdicts.len()
    ));
    if verdicts.is_empty() {
        rep.error("1件も数えられなかった");
    }
    match write_receipt(root, runner, &ctx, &verdicts) {
        Ok(path) => rep.note(format!("受領証: {}", path.display())),
        Err(e) => rep.error(format!("受領証を書けない: {e}")),
    }
    rep.finish("test-receipt")
}

fn cargo_verdicts(root: &Path, suites: &[Suite], ctx: &Context, rep: &mut Report) -> Vec<Verdict> {
    let exes = match build_executables(root) {
        Ok(e) => e,
        Err(e) => {
            rep.error(e);
            return Vec::new();
        }
    };
    let by_key: BTreeMap<(&str, &str), &Suite> = suites
        .iter()
        .filter(|s| s.runner == "cargo")
        .map(|s| ((s.package.as_str(), s.target.as_str()), s))
        .collect();
    let mut seen = BTreeSet::new();
    let mut out = Vec::new();
    for exe in &exes {
        let Some(suite) = by_key.get(&(exe.package.as_str(), exe.target.as_str())) else {
            rep.error(format!(
                "{} の {} がどの suite にも登録されていない",
                exe.package, exe.target
            ));
            continue;
        };
        seen.insert(suite.id.clone());
        let counts =
            discover(exe).and_then(|discovered| run(exe).map(|c| Counts { discovered, ..c }));
        match counts {
            Ok(c) => out.push(judge(suite, c, ctx)),
            Err(e) => rep.error(format!("{}: {e}", suite.id)),
        }
    }
    // この環境で数えるはずの suite が、組み立てにも出てこなかった。
    for s in suites
        .iter()
        .filter(|s| s.runner == "cargo" && s.applies(ctx))
    {
        if !seen.contains(&s.id) {
            rep.error(format!(
                "{}: {} の {} の試験 binary が組み立たなかった",
                s.id, s.package, s.target
            ));
        }
    }
    out
}

/// UI の story 試験。 vitest の要約行（`Tests  207 passed (207)`）を読む。
fn bun_verdicts(root: &Path, suites: &[Suite], ctx: &Context, rep: &mut Report) -> Vec<Verdict> {
    let mut out = Vec::new();
    for s in suites.iter().filter(|s| s.runner == "bun") {
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
                let mut v = judge(s, c, ctx);
                if !output.status.success() && v.problems.is_empty() {
                    v.problems.push("bun が失敗を返した".into());
                }
                out.push(v);
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
fn write_receipt(
    root: &Path,
    runner: &str,
    ctx: &Context,
    verdicts: &[Verdict],
) -> Result<PathBuf, String> {
    let sha = git(root, &["rev-parse", "HEAD"]).unwrap_or_default();
    let dirty = git(root, &["status", "--porcelain"]).is_ok_and(|s| !s.trim().is_empty());
    let suites: Vec<serde_json::Value> = verdicts
        .iter()
        .map(|v| {
            serde_json::json!({
                "suite": v.suite,
                "package": v.package,
                "target": v.target,
                "applies": v.applies,
                "discovered": v.counts.discovered,
                "passed": v.counts.passed,
                "failed": v.counts.failed,
                "ignored": v.counts.ignored,
                "result": if !v.problems.is_empty() { "failed" } else if v.applies { "passed" } else { "not-applicable" },
                "problems": v.problems,
            })
        })
        .collect();
    let receipt = serde_json::json!({
        "git": sha.trim(),
        "dirty": dirty,
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
        for v in verdicts {
            let result = if !v.problems.is_empty() {
                "失敗"
            } else if v.applies {
                "通過"
            } else {
                "対象外"
            };
            let _ = writeln!(
                md,
                "| {} | {} {} | {} | {} | {result} |",
                v.suite,
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

    fn suite(min: u64, manual: u64) -> Suite {
        Suite {
            id: "SUITE-X-001".into(),
            runner: "cargo".into(),
            package: "p".into(),
            target: "lib".into(),
            platforms: vec!["macos".into(), "linux".into()],
            backends: vec!["native".into(), "unsupported".into()],
            min_cases: min,
            manual,
        }
    }

    const CTX: Context = Context {
        platform: "linux",
        backend: "unsupported",
    };

    #[test]
    fn 実行が0件なら通さない() {
        let v = judge(&suite(1, 0), Counts::default(), &CTX);
        assert!(!v.problems.is_empty(), "0件で通った");
    }

    #[test]
    fn 登録より多く無視したら通さない() {
        let c = Counts {
            discovered: 3,
            passed: 2,
            failed: 0,
            ignored: 1,
        };
        assert!(!judge(&suite(1, 0), c, &CTX).problems.is_empty());
        assert!(judge(&suite(1, 1), c, &CTX).problems.is_empty());
    }
}
