//! 「前と比べる」ときの基準のリビジョンを決める。
//!
//! 検査が2つの版を比べるとき（採用済みの判断の上書き、ID の付け直し）、新しい側は作業ツリー、
//! 古い側がここで決める基準。 決め方は1つで、手元と CI で同じ規則が働く。
//!
//! 1. `--base <rev>` を渡されたら、`<rev>` と `HEAD` の merge-base
//! 2. 渡されなければ、`origin/main`（無ければ `main`）と `HEAD` の merge-base。 PR の枝は
//!    main から分かれた点になり、PR のチェックアウト（main との merge コミット）は main の先頭になる
//! 3. 2 の merge-base が `HEAD` 自身（いま main の先頭にいる）なら、`HEAD` の親コミット。
//!    main への push は「その push が入れた変更」を比べる
//!
//! **基準が取れないときは、飛ばさずに失敗として返す。** 浅い clone（`fetch-depth: 1`）や
//! main の無い clone で、比べないまま緑になると、比べていないことに誰も気づけない
//! （`DEC-PLT-039`）。 失敗にしないのは、比べる相手が本当に無いときだけ——git のリポジトリの根
//! ではない、コミットが無い、最初のコミット。 理由は呼び出し側が出力に残す。

use std::path::Path;

use super::git;

/// 決めた基準。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Baseline {
    /// この版と比べる。 `how` は、どう決めたか（出力に残す）。
    Revision { sha: String, how: String },
    /// 比べる相手が無い。 `reason` は出力に残す。
    Absent { reason: &'static str },
}

/// 基準を決める。 `explicit` は `--base` の値。
///
/// # Errors
///
/// 基準を決められない（渡された版が解決できない、main が無い、浅い clone で親や分岐点が
/// 引けない、git を起動できない）。 文言は人が読む。
pub(crate) fn resolve(root: &Path, explicit: Option<&str>) -> Result<Baseline, String> {
    if !at_repo_root(root)? {
        return Ok(Baseline::Absent {
            reason: "git のリポジトリの根ではない",
        });
    }
    // コミットが1つも無い（`HEAD` が解決できない）リポジトリには、比べる相手が無い。
    let Ok(head) = rev(root, "HEAD") else {
        return Ok(Baseline::Absent {
            reason: "コミットが無い",
        });
    };

    if let Some(given) = explicit {
        let tip = rev(root, &format!("{given}^{{commit}}"))
            .map_err(|_| format!("--base `{given}` を版として解決できない"))?;
        let sha = merge_base(root, &tip, &head)?;
        return Ok(Baseline::Revision {
            sha,
            how: format!("--base {given} との merge-base"),
        });
    }

    let Some(main) = ["origin/main", "main"]
        .into_iter()
        .find(|name| rev(root, &format!("{name}^{{commit}}")).is_ok())
    else {
        return Err(unavailable(
            root,
            "基準にする `origin/main` も `main` も無い。 `--base <rev>` で渡すか、全履歴を取ってから走らせる",
        ));
    };
    let sha = merge_base(root, main, &head)?;
    if sha != head {
        return Ok(Baseline::Revision {
            sha,
            how: format!("{main} との merge-base"),
        });
    }
    // いま main の先頭にいる。 比べる相手は親コミット。
    match rev(root, "HEAD^") {
        Ok(parent) => Ok(Baseline::Revision {
            sha: parent,
            how: format!("{main} の先頭にいるので親コミット"),
        }),
        Err(_) if is_shallow(root) => Err(unavailable(root, "親コミットが無い")),
        Err(_) => Ok(Baseline::Absent {
            reason: "最初のコミット",
        }),
    }
}

/// 根が git のリポジトリの根そのものか。 別のリポジトリの中の一時ディレクトリ
/// （`target/` の下の fixture など）は、親のリポジトリを基準にしてはならないので、
/// 根ではないものとして扱う。
fn at_repo_root(root: &Path) -> Result<bool, String> {
    match git(root, &["rev-parse", "--show-prefix"]) {
        Ok(prefix) => Ok(prefix.trim().is_empty()),
        // 起動できないのは、git が無いということ。 比べられないので失敗にする。
        Err(e) if e.starts_with("git を起動できない") => Err(e),
        // リポジトリの外。
        Err(_) => Ok(false),
    }
}

fn rev(root: &Path, spec: &str) -> Result<String, String> {
    git(root, &["rev-parse", "--verify", "--quiet", spec]).map(|s| s.trim().to_owned())
}

fn merge_base(root: &Path, a: &str, b: &str) -> Result<String, String> {
    git(root, &["merge-base", a, b])
        .map(|s| s.trim().to_owned())
        .map_err(|_| unavailable(root, &format!("`{a}` と `HEAD` の分岐点が引けない")))
}

fn is_shallow(root: &Path) -> bool {
    git(root, &["rev-parse", "--is-shallow-repository"]).is_ok_and(|s| s.trim() == "true")
}

/// 基準が取れなかったときの文言。 浅い clone ならそう言う——原因の大半はこれ。
fn unavailable(root: &Path, what: &str) -> String {
    let hint = if is_shallow(root) {
        "。 浅い clone（`fetch-depth` が有限）で履歴が足りない。 全履歴を取る（CI なら checkout の `fetch-depth: 0`）"
    } else {
        ""
    };
    format!("比較の基準を決められない: {what}{hint}")
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::process::Command;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;

    /// 試験ごとに使い捨てる git リポジトリ。 `tempfile` は引かない（`repo::view` の試験と同じ形）。
    struct Repo(PathBuf);

    impl Drop for Repo {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(dir)
            .args(args)
            .output()
            .expect("git を起動できる");
        assert!(
            out.status.success(),
            "git {args:?} が失敗した: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .trim()
            .to_owned()
    }

    impl Repo {
        fn new(name: &str) -> Self {
            static COUNTER: AtomicU64 = AtomicU64::new(0);
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "koeru-xtask-baseline-{name}-{}-{n}",
                std::process::id()
            ));
            fs::remove_dir_all(&dir).ok();
            fs::create_dir_all(&dir).expect("作れる");
            let repo = Self(dir);
            repo.init();
            repo
        }

        fn init(&self) {
            run(&self.0, &["init", "-q"]);
            run(&self.0, &["checkout", "-q", "-b", "main"]);
            configure(&self.0);
        }

        fn git(&self, args: &[&str]) -> String {
            run(&self.0, args)
        }

        fn commit(&self, file: &str, body: &str) -> String {
            fs::write(self.0.join(file), body).expect("書ける");
            self.git(&["add", "-A"]);
            self.git(&["commit", "-q", "-m", file]);
            self.git(&["rev-parse", "HEAD"])
        }
    }

    fn configure(dir: &Path) {
        for (k, v) in [
            ("user.name", "baseline-test"),
            ("user.email", "baseline-test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            run(dir, &["config", k, v]);
        }
    }

    fn revision(sha: &str, how: &str) -> Result<Baseline, String> {
        Ok(Baseline::Revision {
            sha: sha.to_owned(),
            how: how.to_owned(),
        })
    }

    /// 比べる相手が本当に無いものは、失敗にしない。 理由を持って返す。
    #[test]
    fn 比べる相手が無いものは_absent() {
        // git のリポジトリではない。
        let plain =
            std::env::temp_dir().join(format!("koeru-xtask-baseline-plain-{}", std::process::id()));
        fs::create_dir_all(&plain).expect("作れる");
        // 一時ディレクトリが別のリポジトリの中にある環境でも、根ではないので同じ。
        assert!(matches!(
            resolve(&plain, None),
            Ok(Baseline::Absent {
                reason: "git のリポジトリの根ではない"
            })
        ));
        fs::remove_dir_all(&plain).ok();

        // コミットが無い。
        let repo = Repo::new("unborn");
        assert_eq!(
            resolve(&repo.0, None),
            Ok(Baseline::Absent {
                reason: "コミットが無い"
            })
        );
        // 最初のコミット。
        repo.commit("a", "1");
        assert_eq!(
            resolve(&repo.0, None),
            Ok(Baseline::Absent {
                reason: "最初のコミット"
            })
        );
    }

    /// リポジトリの中のディレクトリは、根ではない。 親のリポジトリを基準にしない。
    #[test]
    fn リポジトリの中のディレクトリは根ではない() {
        let repo = Repo::new("subdir");
        repo.commit("a", "1");
        repo.commit("b", "2");
        let sub = repo.0.join("fixture");
        fs::create_dir_all(&sub).expect("作れる");
        assert_eq!(
            resolve(&sub, None),
            Ok(Baseline::Absent {
                reason: "git のリポジトリの根ではない"
            })
        );
    }

    /// main の先頭にいれば親コミット。 main から分かれた枝なら、分かれた点。
    #[test]
    fn main_の先頭は親_枝は分岐点() {
        let repo = Repo::new("main-and-branch");
        let first = repo.commit("a", "1");
        let second = repo.commit("b", "2");
        assert_eq!(
            resolve(&repo.0, None),
            revision(&first, "main の先頭にいるので親コミット")
        );

        repo.git(&["checkout", "-q", "-b", "feature"]);
        repo.commit("c", "3");
        repo.commit("d", "4");
        assert_eq!(
            resolve(&repo.0, None),
            revision(&second, "main との merge-base")
        );
        // main が先に進んでいても、分かれた点。
        repo.git(&["checkout", "-q", "main"]);
        repo.commit("e", "5");
        repo.git(&["checkout", "-q", "feature"]);
        assert_eq!(
            resolve(&repo.0, None),
            revision(&second, "main との merge-base")
        );
    }

    /// 明示した基準は、その版と HEAD の merge-base。 `HEAD` を渡せば作業ツリーの未コミット分だけ。
    #[test]
    fn 明示した基準を使う() {
        let repo = Repo::new("explicit");
        let first = repo.commit("a", "1");
        let second = repo.commit("b", "2");
        assert_eq!(
            resolve(&repo.0, Some(&first)),
            revision(&first, &format!("--base {first} との merge-base"))
        );
        assert_eq!(
            resolve(&repo.0, Some("HEAD")),
            revision(&second, "--base HEAD との merge-base")
        );
        // 解決できない版は失敗。 飛ばさない。
        let err = resolve(&repo.0, Some("no-such-ref")).expect_err("落ちる");
        assert!(err.contains("`no-such-ref`"), "{err}");
    }

    /// `origin/main` があれば、`main` より先にそちらを使う。
    #[test]
    fn origin_main_を先に使う() {
        let origin = Repo::new("origin-source");
        let first = origin.commit("a", "1");
        origin.commit("b", "2");

        let clone = Repo::new("origin-clone");
        clone.git(&["remote", "add", "origin", origin.0.to_str().expect("UTF-8")]);
        clone.git(&["fetch", "-q", "origin", "main"]);
        // ローカルの main は origin/main より古い（最初のコミットだけ）。
        clone.git(&["reset", "-q", "--hard", &first]);
        clone.git(&["checkout", "-q", "-b", "feature", "origin/main"]);
        clone.commit("c", "3");
        let tip = origin.git(&["rev-parse", "HEAD"]);
        assert_eq!(
            resolve(&clone.0, None),
            revision(&tip, "origin/main との merge-base")
        );
    }

    /// main が無ければ失敗。 比べずに通さない。
    #[test]
    fn main_が無ければ失敗する() {
        let repo = Repo::new("no-main");
        repo.commit("a", "1");
        repo.git(&["checkout", "-q", "-b", "other"]);
        repo.git(&["branch", "-q", "-D", "main"]);
        let err = resolve(&repo.0, None).expect_err("落ちる");
        assert!(err.starts_with("比較の基準を決められない"), "{err}");
        assert!(err.contains("origin/main"), "{err}");
    }

    /// 浅い clone で、親も分岐点も引けなければ失敗。 原因が浅い clone だと言う。
    #[test]
    fn 浅い_clone_で基準が取れなければ失敗する() {
        let origin = Repo::new("shallow-source");
        origin.commit("a", "1");
        origin.commit("b", "2");
        origin.commit("c", "3");

        let dir = std::env::temp_dir().join(format!(
            "koeru-xtask-baseline-shallow-{}",
            std::process::id()
        ));
        fs::remove_dir_all(&dir).ok();
        let url = format!("file://{}", origin.0.display());
        run(
            &std::env::temp_dir(),
            &[
                "clone",
                "-q",
                "--depth",
                "1",
                &url,
                dir.to_str().expect("UTF-8"),
            ],
        );
        configure(&dir);
        let result = resolve(&dir, None);
        fs::remove_dir_all(&dir).ok();
        let err = result.expect_err("落ちる");
        assert!(err.starts_with("比較の基準を決められない"), "{err}");
        assert!(err.contains("浅い clone"), "{err}");
        assert!(err.contains("fetch-depth: 0"), "{err}");
    }
}
