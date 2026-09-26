//! 作業ツリーと、任意の git の版を同じ口で読む。
//!
//! [`RepoView::working_tree`] はファイルシステムをそのまま読み、[`RepoView::revision`]
//! は解決した SHA から git の plumbing で読む。上位（`touched` など）はどちらの版かを
//! 気にせず、`read` / `exists` / `walk` / `identity` の4つだけを呼べばよい。
//!
//! パスはすべて repo の根からの相対で、区切りは `/` に揃えてある——版から読むときは
//! git がもともとそう返し、作業ツリーから読むときは組み立てた文字列を明示的に揃える。

use std::fs;
use std::path::{Path, PathBuf};

use super::process::{self, ProcessError};
use super::{SKIPPED_DIRS, nul_paths};

/// 読み・存在確認・版の解決が失敗した理由。
#[derive(Debug)]
pub(crate) enum ViewError {
    /// `rev` を SHA へ解決できない（`git rev-parse` が通らない）。
    NoSuchRevision(String),
    /// 読んだ内容が UTF-8 として読めない。
    NotUtf8(String),
    /// 上記以外の失敗（I/O、git の想定外の失敗）。
    Failed(String),
}

impl std::fmt::Display for ViewError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ViewError::NoSuchRevision(rev) => write!(f, "版 `{rev}` を解決できない"),
            ViewError::NotUtf8(e) => write!(f, "内容が UTF-8 ではない: {e}"),
            ViewError::Failed(e) => write!(f, "{e}"),
        }
    }
}

/// この view が指しているものそのもの。
///
/// **今はまだ試験だけが呼ぶ。** `identity` の消費者（X03・X04）はこの先に乗る。
#[derive(Debug, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum RevisionRef {
    /// 作業ツリー。 `head` は直近のコミット、`dirty` は未コミットの変更
    /// （未追跡のファイルも含む）があるか。
    WorkingTree { head: String, dirty: bool },
    /// 解決済みの版の SHA。
    Revision { sha: String },
}

/// 作業ツリーと、解決済みの版を同じ形で読む。
#[derive(Debug)]
pub(crate) enum RepoView {
    WorkingTree { root: PathBuf },
    Revision { root: PathBuf, sha: String },
}

impl RepoView {
    /// 作業ツリーをそのまま読む view。
    pub(crate) fn working_tree(root: &Path) -> Self {
        RepoView::WorkingTree {
            root: root.to_path_buf(),
        }
    }

    /// `rev`（SHA・ブランチ名・タグ名など）を解決して固定した view。
    ///
    /// 解決した SHA を持つので、以降の呼び出しの途中でブランチが動いても揺れない。
    pub(crate) fn revision(root: &Path, rev: &str) -> Result<Self, ViewError> {
        let spec = format!("{rev}^{{commit}}");
        let sha = process::run_text(root, "git", &["rev-parse", "--verify", &spec])
            .map_err(|e| match e {
                ProcessError::NotUtf8(m) => ViewError::NotUtf8(m),
                // `rev-parse --verify` は、解決できない rev をここで返す。
                ProcessError::ExitStatus { .. } => ViewError::NoSuchRevision(rev.to_owned()),
                ProcessError::Spawn(m) => ViewError::Failed(m),
            })?
            .trim()
            .to_owned();
        Ok(RepoView::Revision {
            root: root.to_path_buf(),
            sha,
        })
    }

    /// `rel` の中身。 無ければ `None`。
    pub(crate) fn read(&self, rel: &str) -> Result<Option<String>, ViewError> {
        match self {
            RepoView::WorkingTree { root } => read_working_tree(root, rel),
            RepoView::Revision { root, sha } => read_revision(root, sha, rel),
        }
    }

    /// `rel` がこの版に存在するか。
    ///
    /// **今はまだ試験だけが呼ぶ。** 消費者（X03・X04）はこの先に乗る。
    #[allow(dead_code)]
    pub(crate) fn exists(&self, rel: &str) -> bool {
        match self {
            RepoView::WorkingTree { root } => root.join(rel).exists(),
            RepoView::Revision { root, sha } => {
                let spec = format!("{sha}:{rel}");
                process::run(root, "git", &["cat-file", "-e", &spec]).is_ok()
            }
        }
    }

    /// `dir` の下の相対パスをすべて返す。 `SKIPPED_DIRS` は飛ばす——版と作業ツリーで
    /// 同じ規則になる（走査対象に正本が無いディレクトリだから、というのが `SKIPPED_DIRS`
    /// 自身の理由で、それはどちらの版を読んでいても変わらない）。
    ///
    /// **今はまだ試験だけが呼ぶ。** 消費者（X03・X04）はこの先に乗る。
    #[allow(dead_code)]
    pub(crate) fn walk(&self, dir: &str) -> Vec<String> {
        match self {
            RepoView::WorkingTree { root } => walk_working_tree(root, dir),
            RepoView::Revision { root, sha } => walk_revision(root, sha, dir),
        }
    }

    /// この view が指しているものそのもの。
    ///
    /// **今はまだ試験だけが呼ぶ。** 消費者（X03・X04）はこの先に乗る。
    #[allow(dead_code)]
    pub(crate) fn identity(&self) -> Result<RevisionRef, ViewError> {
        match self {
            RepoView::Revision { sha, .. } => Ok(RevisionRef::Revision { sha: sha.clone() }),
            RepoView::WorkingTree { root } => {
                let head = process::run_text(root, "git", &["rev-parse", "HEAD"])
                    .map_err(|e| ViewError::Failed(e.to_string()))?
                    .trim()
                    .to_owned();
                // `--porcelain` は既定で未追跡も含む。 1行でもあれば dirty。
                let status = process::run_text(root, "git", &["status", "--porcelain"])
                    .map_err(|e| ViewError::Failed(e.to_string()))?;
                Ok(RevisionRef::WorkingTree {
                    head,
                    dirty: !status.trim().is_empty(),
                })
            }
        }
    }
}

fn read_working_tree(root: &Path, rel: &str) -> Result<Option<String>, ViewError> {
    match fs::read(root.join(rel)) {
        Ok(bytes) => String::from_utf8(bytes)
            .map(Some)
            .map_err(|e| ViewError::NotUtf8(e.to_string())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ViewError::Failed(e.to_string())),
    }
}

fn read_revision(root: &Path, sha: &str, rel: &str) -> Result<Option<String>, ViewError> {
    let spec = format!("{sha}:{rel}");
    match process::run(root, "git", &["cat-file", "-p", &spec]) {
        Ok(out) => out
            .text()
            .map(Some)
            .map_err(|e| ViewError::NotUtf8(e.to_string())),
        // `cat-file -p` は、その版にパスが無いときも終了コード非0で落ちる。
        // sha 自体は構築時に解決済みなので、ここへ来る非0はほぼ「その版に無い」。
        Err(ProcessError::ExitStatus { .. }) => Ok(None),
        Err(e) => Err(ViewError::Failed(e.to_string())),
    }
}

fn walk_working_tree(root: &Path, dir: &str) -> Vec<String> {
    // 空文字は根そのもの。`root.join("")` でも同じ結果になるが、
    // 空のパス要素を足さない形のほうが読める。
    let start = if dir.is_empty() {
        root.to_path_buf()
    } else {
        root.join(dir)
    };
    let mut out = Vec::new();
    let mut stack = vec![start];
    while let Some(d) = stack.pop() {
        let Ok(rd) = fs::read_dir(&d) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if SKIPPED_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(p);
                continue;
            }
            if let Ok(rel) = p.strip_prefix(root) {
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    out.sort();
    out
}

fn walk_revision(root: &Path, sha: &str, dir: &str) -> Vec<String> {
    // `git ls-tree` は空のパス指定を受け付けない。 根そのものは `.` で指す。
    let pathspec = if dir.is_empty() { "." } else { dir };
    let Ok(out) = process::run(
        root,
        "git",
        &["ls-tree", "-r", "-z", "--name-only", sha, "--", pathspec],
    ) else {
        return Vec::new();
    };
    let Ok(text) = out.text() else {
        return Vec::new();
    };
    let mut result: Vec<String> = nul_paths(&text)
        .filter(|rel| {
            !Path::new(rel)
                .components()
                .any(|c| SKIPPED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
        })
        .collect();
    result.sort();
    result
}

/// `git diff --name-only` の結果。 変更されたファイルの一覧で、内容は返さない。
///
/// `touched` が「何が変わったか」を出すたびに呼ぶ、いちばん軽い問い合わせ。
pub(crate) fn diff_files(root: &Path, spec: &str) -> Result<Vec<String>, String> {
    let out = super::git(root, &["diff", "--name-only", "-z", spec])?;
    Ok(nul_paths(&out).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    /// 一時ディレクトリに組んだ、小さな git リポジトリ。
    ///
    /// `tempfile` は引かない。 `CARGO_TARGET_TMPDIR` は unit 試験（`--lib`）では
    /// コンパイル時に定義されない——`tests/` 配下の統合試験だけが持つ変数で、
    /// この試験は `view.rs` に同居する unit 試験なので使えない。**踏んだ。**
    /// 代わりに OS の一時ディレクトリの下に試験ごとの名前で作る。
    struct Repo {
        dir: PathBuf,
    }

    impl Repo {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join("xtask-repo-view-tests")
                .join(name);
            if dir.exists() {
                fs::remove_dir_all(&dir).expect("前の実行の一時ディレクトリを消せる");
            }
            fs::create_dir_all(&dir).expect("一時ディレクトリを作れる");
            let repo = Self { dir };
            repo.git(&["init", "-q"]);
            for (k, v) in [
                ("user.name", "view-test"),
                ("user.email", "view-test@example.invalid"),
                ("commit.gpgsign", "false"),
                ("core.autocrlf", "false"),
            ] {
                repo.git(&["config", k, v]);
            }
            repo
        }

        fn root(&self) -> &Path {
            &self.dir
        }

        fn git(&self, args: &[&str]) -> String {
            let out = Command::new("git")
                .current_dir(self.root())
                .args(args)
                .output()
                .expect("git を起動できる");
            assert!(
                out.status.success(),
                "git {args:?} が失敗した: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout)
                .expect("git の出力が UTF-8")
                .trim()
                .to_owned()
        }

        fn write(&self, rel: &str, text: &str) {
            let p = self.root().join(rel);
            if let Some(parent) = p.parent() {
                fs::create_dir_all(parent).expect("親を作れる");
            }
            fs::write(p, text).expect("書ける");
        }

        fn remove(&self, rel: &str) {
            fs::remove_file(self.root().join(rel)).expect("消せる");
        }

        fn commit(&self, message: &str) -> String {
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", message]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    /// 存在しない ref は型つきの失敗になる。 文字列を見なくても分岐できる。
    #[test]
    fn 存在しない_ref_は型つきの失敗になる() {
        let repo = Repo::new("no-such-revision");
        repo.write("a.txt", "x\n");
        repo.commit("base");
        let err = RepoView::revision(repo.root(), "no-such-ref").unwrap_err();
        assert!(matches!(err, ViewError::NoSuchRevision(rev) if rev == "no-such-ref"));
    }

    /// 消したファイル。 古い版は読めて、新しい版（作業ツリーも含む）では無い。
    #[test]
    fn 消したファイルは古い版だけに残る() {
        let repo = Repo::new("deleted-file");
        repo.write("keep.txt", "残る\n");
        repo.write("gone.txt", "消える\n");
        let base = repo.commit("base");
        repo.remove("gone.txt");
        repo.commit("remove");

        let old = RepoView::revision(repo.root(), &base).unwrap();
        assert_eq!(old.read("gone.txt").unwrap(), Some("消える\n".to_owned()));
        assert!(old.exists("gone.txt"));

        let now = RepoView::working_tree(repo.root());
        assert_eq!(now.read("gone.txt").unwrap(), None);
        assert!(!now.exists("gone.txt"));

        let head = RepoView::revision(repo.root(), "HEAD").unwrap();
        assert_eq!(head.read("gone.txt").unwrap(), None);
    }

    /// 名前を変えたファイル。 旧パスは古い版にしか無く、新パスは新しい版にしか無い。
    #[test]
    fn 名前を変えたファイルは旧新どちらの版にも中身のまま現れない() {
        let repo = Repo::new("renamed-file");
        repo.write("old_name.txt", "同じ中身\n");
        let base = repo.commit("base");
        repo.remove("old_name.txt");
        repo.write("new_name.txt", "同じ中身\n");
        repo.commit("rename");

        let old = RepoView::revision(repo.root(), &base).unwrap();
        assert_eq!(
            old.read("old_name.txt").unwrap(),
            Some("同じ中身\n".to_owned())
        );
        assert_eq!(old.read("new_name.txt").unwrap(), None);

        let head = RepoView::revision(repo.root(), "HEAD").unwrap();
        assert_eq!(head.read("old_name.txt").unwrap(), None);
        assert_eq!(
            head.read("new_name.txt").unwrap(),
            Some("同じ中身\n".to_owned())
        );
    }

    /// 同じパスでも、版が違えば中身が違う。
    #[test]
    fn 古い版の中身は版ごとに固定されている() {
        let repo = Repo::new("content-differs-by-revision");
        repo.write("a.txt", "v1\n");
        let base = repo.commit("base");
        repo.write("a.txt", "v2\n");
        repo.commit("update");

        let old = RepoView::revision(repo.root(), &base).unwrap();
        let head = RepoView::revision(repo.root(), "HEAD").unwrap();
        assert_eq!(old.read("a.txt").unwrap(), Some("v1\n".to_owned()));
        assert_eq!(head.read("a.txt").unwrap(), Some("v2\n".to_owned()));
    }

    /// 未コミットの変更（dirty）と未追跡のファイルを、作業ツリーの識別が拾う。
    #[test]
    fn dirty_な作業ツリーと未追跡ファイルを見分ける() {
        let repo = Repo::new("dirty-working-tree");
        repo.write("a.txt", "v1\n");
        repo.commit("base");

        let clean = RepoView::working_tree(repo.root()).identity().unwrap();
        assert_eq!(
            clean,
            RevisionRef::WorkingTree {
                head: repo.git(&["rev-parse", "HEAD"]),
                dirty: false,
            }
        );

        // 追跡しているファイルをコミットせずに書き換える。
        repo.write("a.txt", "v1 だが未コミット\n");
        let dirty = RepoView::working_tree(repo.root()).identity().unwrap();
        assert!(matches!(
            dirty,
            RevisionRef::WorkingTree { dirty: true, .. }
        ));

        // 戻して、代わりに未追跡のファイルだけを置く。
        repo.write("a.txt", "v1\n");
        repo.git(&["checkout", "--", "a.txt"]);
        repo.write("untracked.txt", "追跡していない\n");
        let dirty_by_untracked = RepoView::working_tree(repo.root()).identity().unwrap();
        assert!(matches!(
            dirty_by_untracked,
            RevisionRef::WorkingTree { dirty: true, .. }
        ));
    }

    /// `walk` は `SKIPPED_DIRS` を飛ばし、版と作業ツリーで同じ集合になる。
    #[test]
    fn walk_は_skipped_dirs_を飛ばし版と作業ツリーで揃う() {
        let repo = Repo::new("walk-skips-skipped-dirs");
        repo.write("src/a.rs", "fn a() {}\n");
        repo.write("src/b.rs", "fn b() {}\n");
        // `target` や `node_modules` は開発機の全体設定でよく無視される。
        // ここでは同じ `SKIPPED_DIRS` の一員でも、狙って無視されにくい名前を使う。
        repo.write("vendor/build.log", "ログ\n");
        repo.write("generated/asset.js", "// 生成物\n");
        let sha = repo.commit("base");

        let want = vec!["src/a.rs".to_owned(), "src/b.rs".to_owned()];

        let working = RepoView::working_tree(repo.root()).walk("");
        assert_eq!(working, want, "作業ツリーの走査");

        let revision = RepoView::revision(repo.root(), &sha).unwrap().walk("");
        assert_eq!(revision, want, "版の走査");
    }

    /// 変更されたファイルの一覧。 内容までは持たない、いちばん軽い問い合わせ。
    #[test]
    fn diff_files_は変更したファイル名だけを返す() {
        let repo = Repo::new("diff-files");
        repo.write("a.txt", "v1\n");
        repo.write("b.txt", "v1\n");
        let base = repo.commit("base");
        repo.write("a.txt", "v2\n");
        repo.commit("update a");

        let mut changed = diff_files(repo.root(), &format!("{base}..HEAD")).unwrap();
        changed.sort();
        assert_eq!(changed, vec!["a.txt".to_owned()]);
    }
}
