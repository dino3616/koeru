//! 受領証（`target/receipts/*.json`）の JSON の書き方と読み方。
//!
//! [`render`] が `test-receipt` の書く形を作り、[`parse`] が読み戻す。 読むのは
//! `cargo xtask portfolio` が、CI の job ごとの受領証（macOS・Linux・Windows）を並べて
//! 突き合わせるため。
//!
//! 欄は足すだけで、既存の欄の意味は変えていない。
//!
//! - 受領証の最上位の `git` / `dirty` / `platform` / `backend` / `runner` と、suite ごとの
//!   `suite` / `package` / `target` / `applies` / `discovered` / `passed` / `failed` /
//!   `ignored` / `result` / `problems` は、これまでどおり。
//! - suite ごとに `environment`（`platform` / `backend` / `git` / `dirty`）、
//!   `missing_prerequisites`、`result` が `not-run` のときの `not_run_reason` を足した。
//!
//! 読むときの環境は、suite の `environment` があればそれ、無ければ最上位の4欄。 どちらも
//! そろっていなければ「環境不明」（[`Receipt::environment`] が `None`）。 環境不明の受領証を
//! 読んで落とすことはしない。 合格に数えないのは、突き合わせる側の仕事。
//!
//! 件数と `result` は必須。 読めない欄を 0 や既定の状態に倒すと、形が壊れた受領証が
//! 「通過」に紛れ込む。 読めなければ、どの欄が読めなかったかを言って断る。

use serde_json::{Value, json};

use super::model::{Counts, Environment, ExecutionContext, NotRunReason, Receipt, Status};

/// 1枚の受領証ファイルを読んだもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ReceiptDocument {
    /// 走らせた runner（`cargo` / `bun`）。 無い古い形では `None`。
    pub(crate) runner: Option<String>,
    pub(crate) receipts: Vec<Receipt>,
}

/// 受領証の JSON を作る。 `receipts` は [`Receipt::countable`] を通ったものだけを渡す。
pub(crate) fn render(runner: &str, ctx: &ExecutionContext, receipts: &[Receipt]) -> Value {
    let suites: Vec<Value> = receipts.iter().map(suite).collect();
    json!({
        "git": ctx.git_sha,
        "dirty": ctx.dirty,
        "platform": ctx.platform,
        "backend": ctx.backend,
        "runner": runner,
        "suites": suites,
    })
}

fn suite(v: &Receipt) -> Value {
    let mut out = json!({
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
        "missing_prerequisites": v.missing_prerequisites,
    });
    if let Status::NotRun(reason) = v.status {
        out["not_run_reason"] = json!(reason.as_str());
    }
    if let Some(env) = &v.environment {
        out["environment"] = json!({
            "platform": env.platform,
            "backend": env.backend,
            "git": env.git_sha,
            "dirty": env.dirty,
        });
    }
    out
}

/// 受領証の JSON を読む。
///
/// # Errors
///
/// JSON として読めない、`suites` が無い、suite の必須の欄が読めない、`result` を知らない。
pub(crate) fn parse(text: &str) -> Result<ReceiptDocument, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| format!("JSON として読めない: {e}"))?;
    let suites = doc
        .get("suites")
        .and_then(Value::as_array)
        .ok_or("`suites` の配列が無い")?;
    let top = environment_of(&doc, "git");
    let mut receipts = Vec::with_capacity(suites.len());
    for (i, s) in suites.iter().enumerate() {
        receipts.push(receipt(s, top.as_ref()).map_err(|e| format!("suites[{i}]: {e}"))?);
    }
    Ok(ReceiptDocument {
        runner: doc.get("runner").and_then(Value::as_str).map(str::to_owned),
        receipts,
    })
}

/// `platform` / `backend` / `dirty` と、版の欄（`git_key`）から環境を作る。
fn environment_of(v: &Value, git_key: &str) -> Option<Environment> {
    Environment::known(
        v.get("platform")?.as_str()?,
        v.get("backend")?.as_str()?,
        v.get(git_key)?.as_str()?,
        v.get("dirty")?.as_bool()?,
    )
}

fn receipt(s: &Value, top: Option<&Environment>) -> Result<Receipt, String> {
    let text = |k: &str| {
        s.get(k)
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| format!("文字列の `{k}` が読めない"))
    };
    let count = |k: &str| {
        s.get(k)
            .and_then(Value::as_u64)
            .ok_or_else(|| format!("非負の整数の `{k}` が読めない"))
    };
    let counts = Counts {
        discovered: count("discovered")?,
        passed: count("passed")?,
        failed: count("failed")?,
        ignored: count("ignored")?,
    };
    let result = text("result")?;
    let status = match result.as_str() {
        "passed" => Status::Passed,
        "failed" => Status::Failed,
        "not-applicable" => Status::NotApplicable,
        "skipped" => Status::Skipped,
        "not-run" => {
            let reason = text("not_run_reason")
                .ok()
                .and_then(|r| NotRunReason::parse(&r))
                .ok_or("`not-run` の `not_run_reason` が読めない")?;
            Status::NotRun(reason)
        }
        other => return Err(format!("`result` の `{other}` を知らない")),
    };
    // suite が自分の環境を持てばそれ。 持たなければ最上位の欄。
    let environment = match s.get("environment") {
        Some(e) => environment_of(e, "git"),
        None => top.cloned(),
    };
    Ok(Receipt {
        probe: text("suite")?,
        package: text("package")?,
        target: text("target")?,
        applies: s
            .get("applies")
            .and_then(Value::as_bool)
            .ok_or("真偽値の `applies` が読めない")?,
        status,
        actual_work: counts.passed + counts.failed,
        counts,
        problems: strings(s, "problems"),
        environment,
        missing_prerequisites: strings(s, "missing_prerequisites"),
    })
}

fn strings(s: &Value, key: &str) -> Vec<String> {
    s.get(key)
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> ExecutionContext {
        ExecutionContext {
            platform: "macos",
            backend: "native",
            git_sha: "abc123".into(),
            dirty: false,
            runner_version: None,
        }
    }

    fn passed() -> Receipt {
        Receipt {
            probe: "SUITE-X-001".into(),
            package: "p".into(),
            target: "lib".into(),
            applies: true,
            status: Status::Passed,
            counts: Counts {
                discovered: 4,
                passed: 3,
                failed: 0,
                ignored: 1,
            },
            actual_work: 3,
            problems: Vec::new(),
            environment: ctx().environment(),
            missing_prerequisites: Vec::new(),
        }
    }

    #[test]
    fn 書いた受領証を読み戻せる() {
        let mut gap = passed();
        gap.probe = "SUITE-X-002".into();
        gap.missing_prerequisites = vec!["fixtures/voice.wav".into()];
        let value = render("cargo", &ctx(), &[passed(), gap.clone()]);
        let back = parse(&value.to_string()).expect("読める");
        assert_eq!(back.runner.as_deref(), Some("cargo"));
        assert_eq!(back.receipts, vec![passed(), gap]);
    }

    #[test]
    fn 既存の欄は最上位も_suite_もそのまま出す() {
        let value = render("cargo", &ctx(), &[passed()]);
        for key in ["git", "dirty", "platform", "backend", "runner", "suites"] {
            assert!(value.get(key).is_some(), "最上位の `{key}` が消えた");
        }
        let s = &value["suites"][0];
        for key in [
            "suite",
            "package",
            "target",
            "applies",
            "discovered",
            "passed",
            "failed",
            "ignored",
            "result",
            "problems",
        ] {
            assert!(s.get(key).is_some(), "suite の `{key}` が消えた");
        }
        assert_eq!(s["result"], "passed");
        assert_eq!(s["environment"]["platform"], "macos");
        assert_eq!(s["environment"]["git"], "abc123");
    }

    #[test]
    fn 環境の欄が無い古い形は環境不明として読む() {
        let old = r#"{"suites":[{"suite":"SUITE-X-001","package":"p","target":"lib",
            "applies":true,"discovered":2,"passed":2,"failed":0,"ignored":0,
            "result":"passed","problems":[]}]}"#;
        let doc = parse(old).expect("読める");
        assert_eq!(doc.receipts[0].environment, None);
        assert_eq!(doc.receipts[0].status, Status::Passed);
        assert!(doc.receipts[0].missing_prerequisites.is_empty());
    }

    #[test]
    fn suite_に環境が無ければ最上位の欄を使う() {
        let text = r#"{"git":"abc","dirty":true,"platform":"linux","backend":"unsupported",
            "suites":[{"suite":"S","package":"p","target":"lib","applies":true,
            "discovered":1,"passed":1,"failed":0,"ignored":0,"result":"passed","problems":[]}]}"#;
        let env = parse(text).expect("読める").receipts[0]
            .environment
            .clone()
            .expect("最上位から読める");
        assert_eq!(
            (env.platform.as_str(), env.backend.as_str(), env.dirty),
            ("linux", "unsupported", true)
        );
    }

    #[test]
    fn 版が空の環境は不明() {
        let text = r#"{"git":"","dirty":false,"platform":"linux","backend":"unsupported",
            "suites":[{"suite":"S","package":"p","target":"lib","applies":true,
            "discovered":1,"passed":1,"failed":0,"ignored":0,"result":"passed","problems":[]}]}"#;
        assert_eq!(parse(text).expect("読める").receipts[0].environment, None);
    }

    #[test]
    fn not_run_は理由つきで読み戻す() {
        let mut r = passed();
        r.status = Status::NotRun(NotRunReason::MissingPrerequisite);
        r.counts = Counts::default();
        r.actual_work = 0;
        let back = parse(&render("cargo", &ctx(), std::slice::from_ref(&r)).to_string())
            .expect("読める");
        assert_eq!(back.receipts, vec![r]);
    }

    #[test]
    fn 読めない欄は黙って_0_や既定に倒さない() {
        assert!(parse("not json").is_err());
        assert!(parse("{}").is_err());
        let no_count = r#"{"suites":[{"suite":"S","package":"p","target":"lib","applies":true,
            "discovered":1,"passed":1,"failed":0,"result":"passed"}]}"#;
        assert!(parse(no_count).expect_err("断る").contains("ignored"));
        let unknown = r#"{"suites":[{"suite":"S","package":"p","target":"lib","applies":true,
            "discovered":1,"passed":1,"failed":0,"ignored":0,"result":"green"}]}"#;
        assert!(parse(unknown).expect_err("断る").contains("green"));
        let no_reason = r#"{"suites":[{"suite":"S","package":"p","target":"lib","applies":false,
            "discovered":0,"passed":0,"failed":0,"ignored":0,"result":"not-run"}]}"#;
        assert!(parse(no_reason).is_err());
    }
}
