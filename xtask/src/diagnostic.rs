//! 検査の結果。 集めることと、人に見せることを分ける。
//!
//! 検査は [`Report`] に [`Diagnostic`] を積むだけで、書式を知らない。 人に見せる形は
//! [`render_human`] が1箇所で決める。 機械が読む形を足すときは、ここに別の renderer を
//! 並べる。 検査の側は変えない。
//!
//! 文言はまだ文字列のまま持つ。 code や場所を型で持たせるのは、検査を載せ替えるとき
//! （X07）に、検査ごとに決める。

use std::fmt::Write as _;
use std::process::ExitCode;

/// 落とすか、知らせるだけか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// 1件でもあればコマンドは失敗する。
    Error,
    /// 数や内訳。 合否を変えない。
    Note,
}

/// 検査が出した1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    pub message: String,
}

/// 1回のコマンドが集めた診断。 読み込みの段の失敗もここに積まれて、コマンドへ渡る。
#[derive(Debug, Default)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
}

impl Report {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.push(Severity::Error, msg);
    }

    pub fn note(&mut self, msg: impl Into<String>) {
        self.push(Severity::Note, msg);
    }

    fn push(&mut self, severity: Severity, msg: impl Into<String>) {
        self.diagnostics.push(Diagnostic {
            severity,
            message: msg.into(),
        });
    }

    /// 積んだ順の診断。
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error)
    }

    /// 人向けに書き出して、終了コードを返す。
    pub fn finish(&self, command: &str) -> ExitCode {
        print!("{}", render_human(command, &self.diagnostics));
        if self.has_errors() {
            ExitCode::FAILURE
        } else {
            ExitCode::SUCCESS
        }
    }
}

/// 端末に出す形。
///
/// 知らせが先、失敗が後。 積んだ順は種類の中でだけ保つ——検査は数を数え終えてから
/// 知らせを積むことが多く、積んだ順のまま出すと失敗の一覧の後ろに総数が来る。
/// 最後の1行がコマンドの名前と合否で、失敗なら件数。
pub fn render_human(command: &str, diagnostics: &[Diagnostic]) -> String {
    let mut out = String::new();
    let of = |severity| diagnostics.iter().filter(move |d| d.severity == severity);
    for d in of(Severity::Note) {
        let _ = writeln!(out, "  {}", d.message);
    }
    let errors = of(Severity::Error).count();
    if errors == 0 {
        let _ = writeln!(out, "{command}: ok");
    } else {
        for d in of(Severity::Error) {
            let _ = writeln!(out, "  NG {}", d.message);
        }
        let _ = writeln!(out, "{command}: {errors} 件");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 知らせが先で失敗が後() {
        let mut rep = Report::default();
        rep.error("一つ目");
        rep.note("数");
        rep.error("二つ目");
        assert_eq!(
            render_human("check-x", rep.diagnostics()),
            "  数\n  NG 一つ目\n  NG 二つ目\ncheck-x: 2 件\n"
        );
        assert!(rep.has_errors());
    }

    #[test]
    fn 失敗が無ければ_ok() {
        let mut rep = Report::default();
        rep.note("数");
        assert_eq!(
            render_human("check-x", rep.diagnostics()),
            "  数\ncheck-x: ok\n"
        );
        assert!(!rep.has_errors());
        assert_eq!(render_human("check-x", &[]), "check-x: ok\n");
    }
}
