//! Probe の定義・実行環境・実行結果の型（X04）。
//!
//! 今の唯一の入力源は `meta/suites/` の登録（`test-portfolio`）で、
//! [`ProbeDefinition::from_suite`] がそれを読む。試験に限らない検証
//! （Loom・Miri・性能計測のような）を足すときは、ここへ別のコンストラクタを増やす。
//!
//! 状態は [`Status`] が5つに分ける。`NotRun` の理由は [`NotRunReason`]。
//! 受領証（`target/receipts/*.json`）は今のところ `Passed` / `Failed` /
//! `NotApplicable` しか書き出さない——`NotRun` は Report の誤りとして出しており、
//! その出し方は `probe::receipt` の側でも変えていない。[`Receipt::countable`] が
//! 受領証と要約のどちらにも積まない境目を持つ。

use std::path::Path;

use crate::knowledge::{list_of, str_of};
use crate::repo;

/// 1件の Probe の定義。 今は SUITE の登録を読んだものしか無い。
#[derive(Debug, Clone)]
pub(crate) struct ProbeDefinition {
    pub(crate) id: String,
    pub(crate) runner: String,
    pub(crate) package: String,
    pub(crate) target: String,
    pub(crate) platforms: Vec<String>,
    pub(crate) backends: Vec<String>,
    pub(crate) min_cases: u64,
    pub(crate) manual: u64,
    /// 実行前に要るもの。 今の SUITE の登録にはここへ足す欄が無いので、
    /// 型にだけ用意してある（読むのは既存の欄だけでよい）。まだ何も読まないので
    /// 常に空——構築するだけで、読む側はまだ無い。
    #[allow(dead_code)]
    pub(crate) prerequisites: Prerequisites,
}

/// 実行前に要るもの（fixture・LFS の実体・モデルのパス・環境変数）。
///
/// どれも今の `meta/suites/` の登録には欄が無い。 将来 Probe がこれらを
/// 宣言する段になったら、[`ProbeDefinition::from_suite`] に読み口を足す。
/// それまでは常に空で、読む側も無い。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) struct Prerequisites {
    pub(crate) fixtures: Vec<String>,
    pub(crate) lfs_assets: Vec<String>,
    pub(crate) model_paths: Vec<String>,
    pub(crate) env_vars: Vec<String>,
}

impl ProbeDefinition {
    /// `meta/suites/` の1件の登録を Probe の定義として読む互換の口。
    ///
    /// 今のところ入力源はこれだけ。 SUITE 以外の入力源（Loom・Miri のような
    /// 手で登録する Probe）が増えたら、ここに別のコンストラクタを足す。
    pub(crate) fn from_suite(t: &toml::Table) -> Result<Self, String> {
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
            prerequisites: Prerequisites::default(),
            id,
        })
    }

    /// この環境で件数を求めるか。
    pub(crate) fn applies(&self, ctx: &ExecutionContext) -> bool {
        self.platforms.iter().any(|p| p == ctx.platform)
            && self.backends.iter().any(|b| b == ctx.backend)
    }
}

/// 実行された環境。 受領証の識別に使う。
#[derive(Debug, Clone)]
pub(crate) struct ExecutionContext {
    pub(crate) platform: &'static str,
    pub(crate) backend: &'static str,
    pub(crate) git_sha: String,
    pub(crate) dirty: bool,
    /// runner の版。 安く取れるものだけ入れる。 cargo の版を毎回問い合わせると
    /// `test-receipt` の起動が1回増えるので、今は取っていない（常に `None`）。
    /// 走らせる段になったら埋める。
    #[allow(dead_code)] // 今はどの runner も版を出さない。将来のための欄。
    pub(crate) runner_version: Option<String>,
}

impl ExecutionContext {
    pub(crate) fn detect(root: &Path) -> Self {
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
        let git_sha = repo::git(root, &["rev-parse", "HEAD"])
            .unwrap_or_default()
            .trim()
            .to_owned();
        let dirty = repo::git(root, &["status", "--porcelain"]).is_ok_and(|s| !s.trim().is_empty());
        Self {
            platform,
            backend,
            git_sha,
            dirty,
            runner_version: None,
        }
    }
}

/// 1本の試験 binary（または将来の Probe）の件数。
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Counts {
    pub(crate) discovered: u64,
    pub(crate) passed: u64,
    pub(crate) failed: u64,
    pub(crate) ignored: u64,
}

/// 実行しなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NotRunReason {
    /// 試験 binary が組み立たなかった。
    BuildFailed,
    /// fixture・モデル・環境変数のような前提が欠けた。
    ///
    /// 今の cargo/bun の runner はまだこの理由を出していない——前提が欠けた試験は
    /// libtest から見れば「通った」試験と区別が付かず、`min_cases` の不足という
    /// 形で [`Status::Failed`] に落ちる（`DEC-PLT-039`）。 前提を宣言できる
    /// Probe が増えたら、判定した側からここを返す。
    #[allow(dead_code)]
    MissingPrerequisite,
    /// 知らない runner。
    ///
    /// 今は runner 単位の誤りとして Report に積むだけで、suite ごとの
    /// Receipt は作っていない。
    #[allow(dead_code)]
    UnknownRunner,
}

/// 実行結果の状態。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Status {
    Passed,
    Failed,
    /// Probe そのものを走らせない選択。
    ///
    /// 今の cargo/bun の runner はまだ出さない——無視した試験（`#[ignore]`）は
    /// [`Counts::ignored`] に残るだけで、Probe 単位の状態は `Passed` / `Failed`
    /// のままになる。
    #[allow(dead_code)]
    Skipped,
    NotApplicable,
    NotRun(NotRunReason),
}

/// 1件の Probe の実行結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Receipt {
    pub(crate) probe: String,
    pub(crate) package: String,
    pub(crate) target: String,
    /// この環境の対象か。 `status` とは別に持つ——組み立ての誤り（`failed` の
    /// 判定の一部）と「対象外」は独立な軸で、対象外の Probe でも登録の形が
    /// 崩れていれば `Failed` になる。
    pub(crate) applies: bool,
    pub(crate) status: Status,
    pub(crate) counts: Counts,
    /// 実際に仕事をした数。 今は `passed + failed` を最低限持つ。
    pub(crate) actual_work: u64,
    pub(crate) problems: Vec<String>,
}

impl Receipt {
    /// 試験 binary が組み立たなかった Probe。
    ///
    /// 受領証（JSON）には積まない——`NotRun` は今のところ Report の誤りとして
    /// 出しているだけで、その出し方は変えていない。[`Receipt::countable`] が
    /// この Receipt を要約と受領証のどちらからも外す。
    pub(crate) fn not_run(probe: &ProbeDefinition, reason: NotRunReason) -> Self {
        Self {
            probe: probe.id.clone(),
            package: probe.package.clone(),
            target: probe.target.clone(),
            applies: false,
            status: Status::NotRun(reason),
            counts: Counts::default(),
            actual_work: 0,
            problems: Vec::new(),
        }
    }

    /// 受領証と要約に数えるか。 `NotRun` はどちらにも積まない。
    pub(crate) fn countable(&self) -> bool {
        !matches!(self.status, Status::NotRun(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe() -> ProbeDefinition {
        ProbeDefinition {
            id: "SUITE-X-001".into(),
            runner: "cargo".into(),
            package: "p".into(),
            target: "lib".into(),
            platforms: vec!["linux".into()],
            backends: vec!["native".into()],
            min_cases: 1,
            manual: 0,
            prerequisites: Prerequisites::default(),
        }
    }

    #[test]
    fn not_run_は受領証にも要約にも数えない() {
        let r = Receipt::not_run(&probe(), NotRunReason::BuildFailed);
        assert_eq!(r.status, Status::NotRun(NotRunReason::BuildFailed));
        assert!(!r.countable());
    }

    #[test]
    fn 走らせた結果は数える() {
        for status in [
            Status::Passed,
            Status::Failed,
            Status::NotApplicable,
            Status::Skipped,
        ] {
            let r = Receipt {
                probe: "SUITE-X-001".into(),
                package: "p".into(),
                target: "lib".into(),
                applies: true,
                status,
                counts: Counts::default(),
                actual_work: 0,
                problems: Vec::new(),
            };
            assert!(r.countable(), "{status:?} を数え損ねた");
        }
    }
}
