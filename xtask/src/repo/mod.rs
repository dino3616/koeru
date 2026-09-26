//! リポジトリそのもの。 根の見つけ方、走査から外す場所、git の呼び出し。
//!
//! 作業ツリーと任意の版を同じ口で読むのは [`view::RepoView`]（X02）。
//! 外部コマンドの起動そのものは [`process`] に1つに寄せてあり、`git` はそれを使う。

use std::path::{Path, PathBuf};

mod process;
mod view;

pub(crate) use view::{RepoView, diff_files};

pub(crate) const META_DIR: &str = "meta";
pub(crate) const SPEC_DIR: &str = "specs";

/// ID の正本が無いディレクトリ。走査からも変更の集計からも外す。
///
/// 生成物と調達物には ID の正本が無い。`.claude` を外す理由だけが違う。
/// あの下には Claude Code がワークツリー——このリポジトリの別チェックアウト——を
/// 生やす。ID の正本はそこにも在るが、それは別の版の正本で、いま読んだ `meta/` とは
/// 揃わない。片方にしか無い ID が「実体が無い」として出る。見るのは手元の1本だけにする。
///
/// skill は落ちない。 `.claude/skills/` の中身は `.agents/skills/` への symlink で、
/// 実体のほうは走査に残る。外すことで、同じ本文を2度数えていたのをやめることにもなる。
pub(crate) const SKIPPED_DIRS: &[&str] = &[
    ".git",
    ".claude",
    // nix-direnv が張る store への symlink 置き場（`DEC-PLT-033`）。
    ".direnv",
    "target",
    "node_modules",
    "vendor",
    "models",
    "dist",
    "generated",
];

pub(crate) fn repo_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if dir.join(META_DIR).is_dir() && dir.join(SPEC_DIR).is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!("{META_DIR}/ と {SPEC_DIR}/ を持つ親が無い"));
        }
    }
}

pub(crate) fn git(root: &Path, args: &[&str]) -> Result<String, String> {
    // 非 ASCII のパスを C 形式で引用させない。 引用されたまま使うと、
    // 差分の見出しともファイル名とも一致せず、そのファイルが黙って落ちる。
    let mut full: Vec<&str> = vec!["-c", "core.quotePath=false"];
    full.extend_from_slice(args);
    match process::run_text(root, "git", &full) {
        Ok(s) => Ok(s),
        Err(process::ProcessError::Spawn(e)) => Err(format!("git を起動できない: {e}")),
        Err(process::ProcessError::ExitStatus { stderr }) => {
            Err(format!("git {} が失敗した: {stderr}", args.join(" ")))
        }
        Err(process::ProcessError::NotUtf8(e)) => Err(format!("git の出力が UTF-8 ではない: {e}")),
    }
}

/// NUL 区切りで返ってきたパスの並び。
///
/// 改行区切りで受けると、改行を含むパスで崩れる。 引用させない設定と
/// 合わせて、Git が持っているとおりの名前をそのまま受け取る。
pub(crate) fn nul_paths(out: &str) -> impl Iterator<Item = String> + '_ {
    out.split('\0').filter(|s| !s.is_empty()).map(str::to_owned)
}
