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
//! doctest は試験 binary にならないので、`cargo test --doc` を別に走らせ、crate ごとに
//! `target = 'doc'` の suite として数える。
//!
//! Probe の定義・実行環境・実行結果の型は [`super::model`] が持つ。ここはそれを cargo と
//! bun の実際の起動へつなぐだけ。

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use super::document;
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
            if has_doctests(t) {
                set.insert((name.clone(), DOCTEST_TARGET.to_owned()));
            }
            set.insert((name.clone(), target));
        }
    }
    Ok(set)
}

/// doctest の suite が名乗る target。 doctest は lib に付くが、lib の試験 binary とは
/// 別に走るので、別の suite に数える。
const DOCTEST_TARGET: &str = "doc";

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
            // doctest の有無は組み立てないと分からない。 持つのに登録が無いものは
            // `test-receipt` が落とす。
            None if key.1 == DOCTEST_TARGET => {}
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
        targets.iter().filter(|(_, t)| t != DOCTEST_TARGET).count()
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
        environment: None,
        missing_prerequisites: Vec::new(),
    }
}

/// suite ごとの判定。
fn judge(s: &ProbeDefinition, counts: Counts, ctx: &ExecutionContext) -> Receipt {
    let applies = s.applies(ctx);
    let problems = collect_problems(s, &counts, applies);
    make_receipt(s, counts, applies, problems).in_environment(ctx)
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
    out.extend(doctest_receipts(root, &by_key, ctx, &mut seen, rep));
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
            out.push(Receipt::not_run(s, NotRunReason::BuildFailed).in_environment(ctx));
        }
    }
    out
}

/// doctest を数えて判定する。 `seen` には数えた suite を足す。
///
/// doctest は `--no-run` の JSON に試験 binary として出てこないので、別に走らせる。
/// 登録は doctest を持つ crate にだけ求める——持たない crate を登録させても、
/// 0 件の suite が並ぶだけになる。
fn doctest_receipts(
    root: &Path,
    by_key: &BTreeMap<(&str, &str), &ProbeDefinition>,
    ctx: &ExecutionContext,
    seen: &mut BTreeSet<String>,
    rep: &mut Report,
) -> Vec<Receipt> {
    let (sections, ok) = match run_doctests(root) {
        Ok(r) => r,
        Err(e) => {
            rep.error(e);
            return Vec::new();
        }
    };
    let mut out = Vec::new();
    let mut failed_somewhere = false;
    for (package, c) in &sections {
        failed_somewhere |= c.failed > 0;
        match by_key.get(&(package.as_str(), DOCTEST_TARGET)) {
            Some(probe) => {
                seen.insert(probe.id.clone());
                out.push(judge(probe, *c, ctx));
            }
            None if c.discovered > 0 => rep.error(format!(
                "{package} の doctest（{} 件）がどの suite にも登録されていない。\
                 `meta/suites/` に target = 'doc' で足す",
                c.discovered
            )),
            None => {}
        }
    }
    // 件数に失敗が出ていないのに落ちたのは、doctest が組み立たなかったとき。
    if !ok && !failed_somewhere {
        rep.error("cargo test --doc が失敗を返した。doctest が組み立たなかった");
    }
    out
}

/// crate ごとの doctest の件数（package 名と、合算した件数）。 2つ目は終了コードが成功か。
///
/// `-p` で crate を1つずつ選ぶと feature の統合が変わり、依存を組み直す。 だから
/// ワークスペースを1回で走らせ、cargo が標準エラーに出す `Doc-tests <crate>` の見出しで
/// 区切る。 見出しと件数が別の出力に出るので、両方を同じ pipe に流して順序を保つ。
fn run_doctests(root: &Path) -> Result<(Vec<(String, Counts)>, bool), String> {
    let packages = lib_packages(root)?;
    let (mut reader, writer) =
        std::io::pipe().map_err(|e| format!("doctest の出力を受ける pipe を作れない: {e}"))?;
    let mut cmd = Command::new("cargo");
    cmd.args([
        "test",
        "--workspace",
        "--all-features",
        "--doc",
        "--no-fail-fast",
    ])
    .current_dir(root)
    .stdout(
        writer
            .try_clone()
            .map_err(|e| format!("pipe を複製できない: {e}"))?,
    )
    .stderr(writer);
    let mut child = cmd
        .spawn()
        .map_err(|e| format!("cargo test --doc を起動できない: {e}"))?;
    // 書き込み側を手放さないと、子が終わっても読み終わらない。
    drop(cmd);
    let mut text = String::new();
    std::io::Read::read_to_string(&mut reader, &mut text)
        .map_err(|e| format!("cargo test --doc の出力を読めない: {e}"))?;
    let status = child
        .wait()
        .map_err(|e| format!("cargo test --doc を待てない: {e}"))?;
    print!("{text}");
    let sections = doctest_sections(&text, &packages).ok_or(
        "cargo test --doc の出力に `Doc-tests` の見出しか `test result:` が無い。\
         形が変わったか、途中で落ちた",
    )?;
    Ok((sections, status.success()))
}

/// lib の crate 名（`koeru_audio`）→ package 名（`koeru-audio`）。
fn lib_packages(root: &Path) -> Result<BTreeMap<String, String>, String> {
    let meta = cargo_metadata(root)?;
    let mut map = BTreeMap::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let name = pkg["name"].as_str().unwrap_or_default();
        for t in pkg["targets"].as_array().into_iter().flatten() {
            if has_doctests(t) {
                let lib = t["name"].as_str().unwrap_or_default().replace('-', "_");
                map.insert(lib, name.to_owned());
            }
        }
    }
    Ok(map)
}

/// doctest を持ちうる target か。 lib で、`doctest = false` にしていないもの。
fn has_doctests(target: &serde_json::Value) -> bool {
    let is_lib = target["kind"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .any(|k| k.ends_with("lib"));
    is_lib && target["doctest"].as_bool().unwrap_or(false)
}

/// `Doc-tests <crate>` の見出しごとに、`running N tests` と `test result:` を足し合わせる。
///
/// edition 2024 では、まとめて組める doctest と `compile_fail` などの単独のものが別々に
/// 走り、1つの crate に要約行が2本出る。 最後の1本だけを読むと片方を落とす。
/// 見出しの無い crate や、要約行の欄が読めない crate があれば `None`。
fn doctest_sections(
    text: &str,
    packages: &BTreeMap<String, String>,
) -> Option<Vec<(String, Counts)>> {
    let mut out: Vec<(String, Counts, bool)> = Vec::new();
    for raw in text.lines() {
        let line = strip_ansi(raw);
        let line = line.trim();
        if let Some(lib) = line.strip_prefix("Doc-tests ") {
            let lib = lib.trim();
            let package = packages.get(lib).cloned().unwrap_or_else(|| lib.to_owned());
            out.push((package, Counts::default(), false));
            continue;
        }
        let Some((_, c, summarized)) = out.last_mut() else {
            continue;
        };
        if let Some(rest) = line.strip_prefix("running ") {
            if let Some(n) = rest
                .split_whitespace()
                .next()
                .and_then(|n| n.parse::<u64>().ok())
            {
                c.discovered += n;
            }
        } else if line.starts_with("test result:") {
            let s = libtest_counts(line)?;
            c.passed += s.passed;
            c.failed += s.failed;
            c.ignored += s.ignored;
            *summarized = true;
        }
    }
    if out.is_empty() || out.iter().any(|(_, _, s)| !s) {
        return None;
    }
    Some(out.into_iter().map(|(p, c, _)| (p, c)).collect())
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
                out.push(make_receipt(s, c, applies, problems).in_environment(ctx));
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
    let receipt = document::render(runner, ctx, receipts);
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
    fn doctest_は見出しごとに要約行を全部足す() {
        let packages = BTreeMap::from([
            ("koeru_audio".to_owned(), "koeru-audio".to_owned()),
            ("koeru_core".to_owned(), "koeru-core".to_owned()),
        ]);
        let out = "\
\u{1b}[1m\u{1b}[32m   Doc-tests\u{1b}[0m koeru_audio

running 2 tests
test a ... ok
test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.10s

running 5 tests
test b - compile fail ... ok
test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 8.31s

   Doc-tests koeru_core

running 0 tests

test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
";
        let s = doctest_sections(out, &packages).expect("読める");
        assert_eq!(s.len(), 2);
        assert_eq!(s[0].0, "koeru-audio");
        assert_eq!((s[0].1.discovered, s[0].1.passed), (7, 7));
        assert_eq!((s[1].0.as_str(), s[1].1.discovered), ("koeru-core", 0));
    }

    #[test]
    fn doctest_の要約行が欠けたら読めないと言う() {
        let packages = BTreeMap::new();
        // 見出しの後で落ちて要約行が出なかった crate を、0 件として数えない。
        assert!(doctest_sections("   Doc-tests x\n\nrunning 3 tests\n", &packages).is_none());
        assert!(doctest_sections("no output", &packages).is_none());
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
            requires_native: false,
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
    fn 判定した受領証は走らせた環境を持つ() {
        let mut ctx = ctx();
        ctx.git_sha = "abc123".into();
        let c = Counts {
            discovered: 1,
            passed: 1,
            failed: 0,
            ignored: 0,
        };
        let env = judge(&probe(1, 0), c, &ctx)
            .environment
            .expect("版が取れていれば環境を持つ");
        assert_eq!(
            (
                env.platform.as_str(),
                env.backend.as_str(),
                env.git_sha.as_str()
            ),
            ("linux", "unsupported", "abc123")
        );
        // git の外で走らせて版が取れなければ、環境不明のまま書く。
        assert_eq!(judge(&probe(1, 0), c, &self::ctx()).environment, None);
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
