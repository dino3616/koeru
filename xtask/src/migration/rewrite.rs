//! 書き換えの transaction。 作業ツリーを中途半端な状態で残さない。
//!
//! 手順は次のとおり。 どこで失敗しても、書く前の状態へ戻す。
//!
//! 1. 計画のあとにファイルが変わっていないか（古い計画を当てない）
//! 2. 触るファイルに未コミットの変更が無いか（いつでも git で戻せる）
//! 3. 新しい本文を同じディレクトリの一時ファイルへ書いて fsync し、元の本文を退避する
//! 4. 一時ファイルを1つずつ rename で差し替える（失敗したら、差し替えた分を退避から戻す）
//! 5. ディスクから読み直した graph が、書く前と rename だけの差で済んでいるか
//!    （済んでいなければ、退避から全部戻す）
//!
//! ディスクへ触るのは、`std::fs` を使うこのファイルだけ。

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use super::dryrun::{DefaultEquivalence, DryRun, EquivalenceIssue, equivalence, frozen};
use super::plan::MigrationPlan;
use crate::graph::Registry;
use crate::repo::{self, RepoView};

/// 書き換えが成立しなかった理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RewriteError {
    /// dry-run が通っていない計画を当てようとした。
    DryRunFailed(Vec<EquivalenceIssue>),
    /// 計画のあとに、対象のファイルが変わっている。
    StalePlan { path: String },
    /// 対象のファイルに、コミットしていない変更がある。
    DirtyWorkingTree { paths: Vec<String> },
    /// git を呼べなかった。
    Git(String),
    /// ファイル操作に失敗した。 `restored` は、書く前へ戻せたか。
    Io { message: String, restored: bool },
    /// 書いたあとに読み直した graph が、rename だけの差で済んでいない。 書く前へ戻した。
    ReloadVerifyFailed(Vec<EquivalenceIssue>),
}

impl std::fmt::Display for RewriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RewriteError::DryRunFailed(issues) => {
                write!(f, "dry-run が通っていない（{} 件）", issues.len())
            }
            RewriteError::StalePlan { path } => {
                write!(f, "{path}: 計画のあとに変わっている。 計画からやり直す")
            }
            RewriteError::DirtyWorkingTree { paths } => write!(
                f,
                "未コミットの変更がある（{}）。 コミットするか退けてからやり直す",
                paths.join(", ")
            ),
            RewriteError::Git(m) => write!(f, "{m}"),
            RewriteError::Io { message, restored } => {
                let tail = if *restored {
                    "書く前へ戻した"
                } else {
                    "書く前へ戻せていない"
                };
                write!(f, "{message}（{tail}）")
            }
            RewriteError::ReloadVerifyFailed(issues) => write!(
                f,
                "書いたあとの読み直しが rename だけの差で済んでいない（{} 件）。 書く前へ戻した",
                issues.len()
            ),
        }
    }
}

impl std::error::Error for RewriteError {}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ApplyOptions {
    /// 触るファイルに未コミットの変更があれば止める。 既定で有効。
    ///
    /// 戻す手段を git に預けるための条件。 外すのは、作業ツリーを自分で
    /// 退避してあるときだけ。
    pub(crate) require_clean: bool,
}

// `derive(Default)` だと `false` になる。 既定は有効にしたい。
impl Default for ApplyOptions {
    fn default() -> Self {
        Self {
            require_clean: true,
        }
    }
}

/// 書き換えた結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Applied {
    /// 新しい本文を持つパス（付け替えた後の名前）。
    pub(crate) written: Vec<String>,
    /// 付け替えで無くなった元のパス。
    pub(crate) removed: Vec<String>,
}

/// 1ファイル分の操作。 付け替えが無ければ `original == target`。
struct Operation {
    original: PathBuf,
    target: PathBuf,
    temp: PathBuf,
    backup: PathBuf,
    content: String,
    rel_target: String,
    rel_original: String,
}

/// `dry` の本文をディスクへ書く。
pub(crate) fn apply(
    root: &Path,
    plan: &MigrationPlan,
    dry: &DryRun,
    registry: &Registry,
    options: ApplyOptions,
) -> Result<Applied, RewriteError> {
    if !dry.is_ok() {
        return Err(RewriteError::DryRunFailed(dry.issues.clone()));
    }

    let working = RepoView::working_tree(root);
    for (path, original) in &plan.snapshots {
        if working.read(path).ok().flatten().as_deref() != Some(original.as_str()) {
            return Err(RewriteError::StalePlan { path: path.clone() });
        }
    }
    // 作業ツリーの view は読むたびにディスクを見る。 書く前の姿は今のうちに固定する。
    let before = frozen(&working);
    if options.require_clean {
        check_clean(root, plan)?;
    }

    let operations = operations(root, plan, dry)?;

    // 一時ファイルと退避を全部作ってから、差し替えに入る。
    // ここまでの失敗は、元のファイルに触れていない。
    for op in &operations {
        if let Err(e) = prepare(op) {
            cleanup(&operations);
            return Err(io_error(&op.rel_target, &e, true));
        }
    }

    for (index, op) in operations.iter().enumerate() {
        if let Err(e) = rename(index, &op.temp, &op.target) {
            restore(&operations);
            cleanup(&operations);
            return Err(io_error(&op.rel_target, &e.to_string(), true));
        }
    }
    // 元のファイルを消すのは、全部を差し替えたあと。 入れ替え（A→B と B→A）では、
    // 一方の元のファイルがもう一方の新しい名前になっているので、順に消すと
    // 書いたばかりのファイルまで消える。
    let targets: Vec<&Path> = operations.iter().map(|o| o.target.as_path()).collect();
    for op in &operations {
        if op.target != op.original
            && !targets.contains(&op.original.as_path())
            && let Err(e) = fs::remove_file(&op.original)
        {
            restore(&operations);
            cleanup(&operations);
            return Err(io_error(&op.rel_original, &e.to_string(), true));
        }
    }

    // 読み直して確かめる。 dry-run と同じ判定を、今度はディスクから読んだ graph に当てる。
    let after = RepoView::working_tree(root);
    let issues = equivalence(&before, &after, plan, registry, &DefaultEquivalence);
    if !issues.is_empty() {
        restore(&operations);
        cleanup(&operations);
        return Err(RewriteError::ReloadVerifyFailed(issues));
    }

    // 確かめ終わってから退避を捨てる。
    cleanup(&operations);
    Ok(Applied {
        written: operations.iter().map(|o| o.rel_target.clone()).collect(),
        removed: operations
            .iter()
            .filter(|o| o.target != o.original)
            .map(|o| o.rel_original.clone())
            .collect(),
    })
}

fn check_clean(root: &Path, plan: &MigrationPlan) -> Result<(), RewriteError> {
    if plan.snapshots.is_empty() {
        return Ok(());
    }
    let mut args = vec!["status", "--porcelain", "--"];
    args.extend(plan.snapshots.keys().map(String::as_str));
    let status = repo::git(root, &args).map_err(RewriteError::Git)?;
    // `--porcelain` の1行は `XY <path>`。
    let dirty: Vec<String> = status
        .lines()
        .filter_map(|line| line.get(3..).map(str::to_owned))
        .collect();
    if dirty.is_empty() {
        Ok(())
    } else {
        Err(RewriteError::DirtyWorkingTree { paths: dirty })
    }
}

fn operations(
    root: &Path,
    plan: &MigrationPlan,
    dry: &DryRun,
) -> Result<Vec<Operation>, RewriteError> {
    let mut out = Vec::new();
    for (rel, content) in &dry.rewritten {
        let rel_target = plan
            .renames
            .iter()
            .find(|r| &r.from == rel)
            .map_or_else(|| rel.clone(), |r| r.to.clone());
        let original = root.join(rel);
        let target = root.join(&rel_target);
        out.push(Operation {
            temp: sibling(&target, "tmp")
                .ok_or_else(|| io_error(&rel_target, "ファイル名が無い", true))?,
            backup: sibling(&original, "bak")
                .ok_or_else(|| io_error(rel, "ファイル名が無い", true))?,
            original,
            target,
            content: content.clone(),
            rel_target,
            rel_original: rel.clone(),
        });
    }
    Ok(out)
}

/// 同じディレクトリの `.<name>.xtask-migrate.<ext>`。 rename を同じ filesystem の中で終わらせる。
fn sibling(path: &Path, ext: &str) -> Option<PathBuf> {
    let name = path.file_name()?.to_str()?;
    Some(path.with_file_name(format!(".{name}.xtask-migrate.{ext}")))
}

fn io_error(path: &str, cause: &str, restored: bool) -> RewriteError {
    RewriteError::Io {
        message: format!("{path}: {cause}"),
        restored,
    }
}

/// 一時ファイルへ書いて fsync し、元の本文を退避する。
fn prepare(op: &Operation) -> Result<(), String> {
    let mut f = fs::File::create(&op.temp).map_err(|e| e.to_string())?;
    f.write_all(op.content.as_bytes())
        .map_err(|e| e.to_string())?;
    f.sync_all().map_err(|e| e.to_string())?;
    fs::copy(&op.original, &op.backup).map_err(|e| e.to_string())?;
    Ok(())
}

/// 書く前の状態へ戻す。
///
/// 付け替えで増えたファイルを消し、退避を元の場所へ書き戻す。 まだ差し替えていない
/// 元のファイルへ退避を書き戻しても、同じ本文なので害は無い。
fn restore(operations: &[Operation]) {
    let originals: Vec<&Path> = operations.iter().map(|o| o.original.as_path()).collect();
    for op in operations {
        if !originals.contains(&op.target.as_path()) {
            let _ = fs::remove_file(&op.target);
        }
    }
    for op in operations {
        let _ = fs::copy(&op.backup, &op.original);
    }
}

/// 一時ファイルと退避を消す。
fn cleanup(operations: &[Operation]) {
    for op in operations {
        let _ = fs::remove_file(&op.temp);
        let _ = fs::remove_file(&op.backup);
    }
}

#[cfg(test)]
thread_local! {
    /// 試験が rename の失敗を注入する。 何番目の rename を落とすか。
    ///
    /// ディレクトリを読み取り専用にして落とす方法は OS で挙動が違う。 呼び出しの
    /// 順番で落とせば、どこでも同じ。
    static FAIL_RENAME_AT: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(super) fn fail_rename_at(index: Option<usize>) {
    FAIL_RENAME_AT.with(|c| c.set(index));
}

fn rename(index: usize, from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(test)]
    if FAIL_RENAME_AT.with(std::cell::Cell::get) == Some(index) {
        return Err(std::io::Error::other("試験が注入した rename の失敗"));
    }
    #[cfg(not(test))]
    let _ = index;
    fs::rename(from, to)
}
