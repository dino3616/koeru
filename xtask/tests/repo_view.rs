//! `touched` の不具合の退行試験。
//!
//! `RepoView` そのものの単体試験は `xtask/src/repo/view.rs` の `#[cfg(test)]` に
//! 置いてある——`repo` は crate 外から見えない private module なので、外側の
//! 統合試験からは `RepoView` を直接呼べない。ここに置けるのは、compiled binary の
//! 出力から観測できるものだけ。
//!
//! `commands.rs` は X03・X04 と並行で触っているので、fixture はここに小さく持つ
//! （`commands.rs` の `Fixture` を参考にしたが、コピーではない）。
//!
//! このファイルの中では ID の領域を小文字で書く（`TR-fix-01`）。 実リポジトリの
//! `check-references` がこの `.rs` も走査するので、大文字で書くと実在しない ID
//! として落ちる。 fixture へ書くとき・出力と比べるときに [`ids`] で大文字へ直す。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// このファイルの中の書き方（`TR-fix-01`）を、fixture の ID（領域が `FIX`）に直す。
fn ids(text: &str) -> String {
    text.replace("-fix", "-FIX")
}

/// 一時ディレクトリに組んだリポジトリ。
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("xtask-repo-view")
            .join(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("前の実行の fixture を消せる");
        }
        fs::create_dir_all(root.join("meta/requirements")).expect("meta を作れる");
        fs::create_dir_all(root.join("specs")).expect("specs を作れる");
        Self { root }
    }

    fn write(&self, rel: &str, text: &str) -> &Self {
        let p = self.root.join(ids(rel));
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).expect("親を作れる");
        }
        fs::write(&p, ids(text)).expect("書ける");
        self
    }

    /// fixture の中で git を叩く。 手元の設定に左右されないよう、
    /// fixture のリポジトリにだけ設定を置く。
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(&self.root)
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

    fn git_init(&self) {
        self.git(&["init", "-q"]);
        self.git(&["checkout", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            self.git(&["config", k, v]);
        }
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn run(&self, args: &[&str]) -> String {
        let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
            .current_dir(&self.root)
            .args(args.iter().map(|a| ids(a)))
            .env_remove("GITHUB_STEP_SUMMARY")
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .output()
            .expect("xtask を起動できる");
        assert!(
            out.status.success(),
            "xtask {args:?} が失敗した: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).into_owned()
    }
}

const CORE: &str = r"schema = 'requirement-set'

[[requirement]]
id = 'TR-fix-01'
title = 't'
confidence = 'Fact'
statement = 'old one.'

[[requirement]]
id = 'TR-fix-02'
title = 't2'
confidence = 'Fact'
statement = 'other.'
";

/// `base...HEAD` の新しい側は HEAD の版の本文で引く。
///
/// 直す前は、`base...HEAD` の新しい側の行も作業ツリーの本文（`own_now`）で
/// 引いていた。 コミット後に同じ meta ファイルの上のほうへ行を足すと、
/// 下にある項目の行番号がずれる。
///
/// ここでは `TR-fix-01` の `statement` を1行書き換えてコミットしたあと、
/// コミットしていないまま同じファイルの先頭に空行を6行足す——`TR-fix-01` の
/// 宣言そのものは動かない（先頭からの相対位置は同じ）が、絶対の行番号はずれる。
///
/// **消した側（`削除`）は行番号がずれても正しく引ける**——`base` の本文で
/// 引いているので、後からの書き換えの影響を受けない。 直す前のコードで
/// 壊れるのは新しい側（`書き換え`）だけ。 ずれた行番号を作業ツリーの本文に
/// 当てると、どの項目にも当たらず`書き換え`の引用が消える
/// ——直す前のコードで実際に確かめてから直した（PR に書く）。
#[test]
fn touched_は_base_head_の新しい側をhead_の本文で引く() {
    let fx = Fixture::new("bug-line-shift");
    fx.write("meta/requirements/core.toml", CORE);
    fx.git_init();
    fx.commit("base");

    // `main` を base のまま残す。 base と HEAD が同じコミットでは、
    // `base...HEAD` の差分がそもそも空になる。
    fx.git(&["checkout", "-q", "-b", "feature"]);
    fx.write(
        "meta/requirements/core.toml",
        &CORE.replace("old one.", "new one."),
    );
    fx.commit("statement changed");

    // コミットしていないまま、ファイルの先頭に空行を足して行番号をずらす。
    let shifted = format!("{}{}", "\n".repeat(6), CORE.replace("old one.", "new one."));
    fx.write("meta/requirements/core.toml", &shifted);

    let out = fx.run(&["touched"]);
    assert!(
        out.contains(&ids("## TR-fix-01")),
        "TR-fix-01 は（削除）側だけでも出る: {out}"
    );
    // 直す前は「（削除）」しか付かず、「（書き換え）」が消えていた。
    assert!(
        out.contains("core.toml（書き換え）"),
        "行がずれても、コミットで変えた TR-fix-01 は「書き換え」で引けるはず: {out}"
    );
}
