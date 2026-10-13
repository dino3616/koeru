//! 外部コマンドを走らせる、唯一の口。
//!
//! `repo::git` をはじめ、外部コマンドを起動する場所はここへ寄せる。 標準出力を
//! bytes のまま返す [`run`] と、UTF-8 の文字列まで変換する [`run_text`] を持つ。
//! 失敗は3つに分けて返す——起動できない、終了コードが 0 でない、出力が UTF-8 でない。
//! 呼び出し側は `match` で分岐でき、`Display` へ畳んで初めて文言になる。

use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

/// 外部コマンドの標準出力。 文字列化は [`Output::text`] で行う。
pub(crate) struct Output {
    /// bytes のまま。 `git cat-file --batch` のように、複数の本文が並ぶ出力は呼び出し側が切る。
    pub(crate) stdout: Vec<u8>,
}

impl Output {
    /// 標準出力を UTF-8 の文字列として読む。
    pub(crate) fn text(self) -> Result<String, ProcessError> {
        String::from_utf8(self.stdout).map_err(|e| ProcessError::NotUtf8(e.to_string()))
    }
}

/// 外部コマンドの実行が失敗した理由。
#[derive(Debug)]
pub(crate) enum ProcessError {
    /// 起動できなかった（実行ファイルが無い、権限が無い等）。
    Spawn(String),
    /// 終了コードが 0 でなかった。標準エラーを添える。
    ExitStatus { stderr: String },
    /// 標準出力が UTF-8 として読めなかった。
    NotUtf8(String),
}

impl std::fmt::Display for ProcessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ProcessError::Spawn(e) => write!(f, "起動できない: {e}"),
            ProcessError::ExitStatus { stderr } => write!(f, "終了コードが 0 でない: {stderr}"),
            ProcessError::NotUtf8(e) => write!(f, "出力が UTF-8 ではない: {e}"),
        }
    }
}

/// `program` を `cwd` で走らせ、標準出力を bytes のまま返す。
pub(crate) fn run(cwd: &Path, program: &str, args: &[&str]) -> Result<Output, ProcessError> {
    let out = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .output()
        .map_err(|e| ProcessError::Spawn(e.to_string()))?;
    if !out.status.success() {
        return Err(ProcessError::ExitStatus {
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(Output { stdout: out.stdout })
}

/// [`run`] に、標準入力を渡す。 1本のプロセスに複数の問いを流して、起動の回数を減らすためのもの
/// （`git cat-file --batch`）。
///
/// 入力は別のスレッドで書く。 出力が溢れるほど大きいとき、書き終えてから読もうとすると、子が
/// 出力を詰まらせたまま入力を読まなくなって、互いに待ち続ける。
pub(crate) fn run_with_stdin(
    cwd: &Path,
    program: &str,
    args: &[&str],
    stdin: &[u8],
) -> Result<Output, ProcessError> {
    let mut child = Command::new(program)
        .current_dir(cwd)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ProcessError::Spawn(e.to_string()))?;
    let mut pipe = child
        .stdin
        .take()
        .ok_or_else(|| ProcessError::Spawn("標準入力を開けない".to_owned()))?;
    let out = std::thread::scope(|scope| {
        // 子が先に終わって入力を読まなくなると書き込みは失敗するが、その失敗は終了コードに出る。
        let writer = scope.spawn(move || pipe.write_all(stdin));
        let out = child.wait_with_output();
        let _ = writer.join();
        out
    })
    .map_err(|e| ProcessError::Spawn(e.to_string()))?;
    if !out.status.success() {
        return Err(ProcessError::ExitStatus {
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_owned(),
        });
    }
    Ok(Output { stdout: out.stdout })
}

/// [`run`] して、標準出力を文字列まで変換する。
pub(crate) fn run_text(cwd: &Path, program: &str, args: &[&str]) -> Result<String, ProcessError> {
    run(cwd, program, args)?.text()
}
