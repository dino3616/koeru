//! 検査の結果。 集めることと、人に見せることを分ける。
//!
//! 検査は [`Report`] に [`Diagnostic`] を積むだけで、書式を知らない。 人に見せる形は
//! [`render_human`]、機械が読む形は [`render_json`] が1箇所で決める。 検査の側はどちらも
//! 知らず、どちらで出すかは入口（`cli`）が [`Report::set_output`] で決める。
//!
//! 文言（`message`）は人に見せる文のまま持つ。 `code` と `location` は文言の代わりではなく、
//! 機械が同じ失敗を同じものとして数えるための構造で、人向けの出力には1字も出ない。
//! code を決めていない呼び出し（[`Report::error`] / [`Report::note`]）は [`GENERIC_CODE`]
//! になる——検査を載せ替えるときに、規則ごとに決めて足す。

use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::{Value, json};

/// code を決めていない診断が名乗る値。
pub const GENERIC_CODE: &str = "generic";

/// 落とすか、知らせるだけか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    /// 1件でもあればコマンドは失敗する。
    Error,
    /// 数や内訳。 合否を変えない。
    Note,
}

impl Severity {
    fn as_str(self) -> &'static str {
        match self {
            Self::Error => "error",
            Self::Note => "note",
        }
    }
}

/// 診断が指す場所。 `line` は1始まりで、分かるときだけ持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Location {
    pub path: PathBuf,
    pub line: Option<usize>,
}

impl Location {
    pub fn new(path: impl Into<PathBuf>, line: Option<usize>) -> Self {
        Self {
            path: path.into(),
            line,
        }
    }
}

/// 検査が出した1件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub severity: Severity,
    /// 規則ごとに決まる安定した名前（`meta.unknown-reference` のような、`.` 区切り）。
    pub code: &'static str,
    pub location: Option<Location>,
    pub message: String,
}

/// 出す形。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

/// 1回のコマンドが集めた診断。 読み込みの段の失敗もここに積まれて、コマンドへ渡る。
#[derive(Debug, Default)]
pub struct Report {
    diagnostics: Vec<Diagnostic>,
    format: OutputFormat,
    /// JSON の `location.path` をこの根からの相対にする。 人向けの出力には効かない。
    root: Option<PathBuf>,
}

impl Report {
    pub fn error(&mut self, msg: impl Into<String>) {
        self.push(Severity::Error, GENERIC_CODE, None, msg);
    }

    pub fn note(&mut self, msg: impl Into<String>) {
        self.push(Severity::Note, GENERIC_CODE, None, msg);
    }

    pub fn error_code(&mut self, code: &'static str, msg: impl Into<String>) {
        self.push(Severity::Error, code, None, msg);
    }

    pub fn note_code(&mut self, code: &'static str, msg: impl Into<String>) {
        self.push(Severity::Note, code, None, msg);
    }

    pub fn error_at(
        &mut self,
        code: &'static str,
        location: Option<Location>,
        msg: impl Into<String>,
    ) {
        self.push(Severity::Error, code, location, msg);
    }

    pub fn note_at(
        &mut self,
        code: &'static str,
        location: Option<Location>,
        msg: impl Into<String>,
    ) {
        self.push(Severity::Note, code, location, msg);
    }

    fn push(
        &mut self,
        severity: Severity,
        code: &'static str,
        location: Option<Location>,
        msg: impl Into<String>,
    ) {
        self.diagnostics.push(Diagnostic {
            severity,
            code,
            location,
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

    /// 出す形と、JSON の場所の基準にする根を決める。
    pub fn set_output(&mut self, format: OutputFormat, root: &Path) {
        self.format = format;
        self.root = Some(root.to_path_buf());
    }

    /// 決めた形で書き出して、終了コードを返す。 形によらず、終了コードは同じ規則。
    pub fn finish(&self, command: &str) -> ExitCode {
        match self.format {
            OutputFormat::Human => print!("{}", render_human(command, &self.diagnostics)),
            OutputFormat::Json => println!(
                "{}",
                render_json(command, &self.diagnostics, self.root.as_deref())
            ),
        }
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

/// 機械が読む形。 積んだ順のまま、知らせも失敗も1本の配列に入れる。
///
/// `ok` は失敗が1件も無いこと。 `location.path` は `root` の下なら根からの相対で、
/// 区切りは `/` に揃える。 `root` の外のパスはそのまま。
pub fn render_json(command: &str, diagnostics: &[Diagnostic], root: Option<&Path>) -> String {
    render_json_with(command, diagnostics, root, None)
}

/// [`render_json`] に、コマンドごとの payload を1つ添えた形。 `payload` は（欄の名前、値）で、
/// 封筒の欄（`command`・`ok`・`diagnostics`）と並ぶ。 payload の形はそのコマンドが持つ。
///
/// 封筒の欄と同じ名前は載せられない——封筒が先で、payload は黙って捨てる（機械が `ok` を
/// 読むので、payload が上書きできてはいけない）。
pub fn render_json_with(
    command: &str,
    diagnostics: &[Diagnostic],
    root: Option<&Path>,
    payload: Option<(&str, Value)>,
) -> String {
    let items: Vec<Value> = diagnostics
        .iter()
        .map(|d| {
            let location = d.location.as_ref().map(|l| {
                json!({
                    "path": relative(&l.path, root),
                    "line": l.line,
                })
            });
            json!({
                "severity": d.severity.as_str(),
                "code": d.code,
                "message": d.message,
                "location": location,
            })
        })
        .collect();
    let ok = !diagnostics.iter().any(|d| d.severity == Severity::Error);
    let mut envelope = json!({
        "command": command,
        "ok": ok,
        "diagnostics": items,
    });
    if let (Some((name, value)), Some(fields)) = (payload, envelope.as_object_mut()) {
        fields.entry(name).or_insert(value);
    }
    envelope.to_string()
}

fn relative(path: &Path, root: Option<&Path>) -> String {
    let shown = root
        .and_then(|r| path.strip_prefix(r).ok())
        .unwrap_or(path)
        .display()
        .to_string();
    shown.replace('\\', "/")
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

    /// code と場所は人向けの出力に出ない。 載せ替えで文言が変わらないことの土台。
    #[test]
    fn code_と場所は人向けの出力を変えない() {
        let mut plain = Report::default();
        plain.error("文言");
        let mut rich = Report::default();
        rich.error_at("x.y", Some(Location::new("meta/a.toml", Some(3))), "文言");
        assert_eq!(
            render_human("check-x", plain.diagnostics()),
            render_human("check-x", rich.diagnostics())
        );
    }

    #[test]
    fn code_を決めない診断は汎用の_code_になる() {
        let mut rep = Report::default();
        rep.error("a");
        rep.note("b");
        assert!(rep.diagnostics().iter().all(|d| d.code == GENERIC_CODE));
        assert!(rep.diagnostics().iter().all(|d| d.location.is_none()));
    }

    #[test]
    fn json_の封筒は積んだ順で_ok_を持つ() {
        let mut rep = Report::default();
        rep.error_at(
            "meta.x",
            Some(Location::new("/repo/meta/a.toml", Some(7))),
            "失敗",
        );
        rep.note_code("meta.summary", "数");
        let v: Value = serde_json::from_str(&render_json(
            "check-meta",
            rep.diagnostics(),
            Some(Path::new("/repo")),
        ))
        .expect("JSON として読める");
        assert_eq!(v["command"], "check-meta");
        assert_eq!(v["ok"], false);
        let d = v["diagnostics"].as_array().expect("配列");
        assert_eq!(d.len(), 2);
        assert_eq!(d[0]["severity"], "error");
        assert_eq!(d[0]["code"], "meta.x");
        assert_eq!(d[0]["message"], "失敗");
        assert_eq!(d[0]["location"]["path"], "meta/a.toml");
        assert_eq!(d[0]["location"]["line"], 7);
        assert_eq!(d[1]["severity"], "note");
        assert_eq!(d[1]["location"], Value::Null);
    }

    #[test]
    fn 失敗が無ければ_ok_は_true() {
        let v: Value = serde_json::from_str(&render_json("check-x", &[], None)).expect("読める");
        assert_eq!(v["ok"], true);
        assert_eq!(v["diagnostics"], json!([]));
        // 知らせだけでも失敗ではない。
        let mut rep = Report::default();
        rep.note("数");
        let v: Value =
            serde_json::from_str(&render_json("check-x", rep.diagnostics(), None)).expect("読める");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn payload_は封筒の欄と並び_封筒の欄は上書きされない() {
        let mut rep = Report::default();
        rep.note_code("x.y", "数");
        let plain = render_json("check-x", rep.diagnostics(), None);
        assert_eq!(
            render_json_with("check-x", rep.diagnostics(), None, None),
            plain,
            "payload が無ければ render_json と同じ"
        );
        let with = render_json_with(
            "check-x",
            rep.diagnostics(),
            None,
            Some(("context", json!({ "version": 1 }))),
        );
        let v: Value = serde_json::from_str(&with).expect("読める");
        let mut without: Value = serde_json::from_str(&plain).expect("読める");
        assert_eq!(v["context"], json!({ "version": 1 }));
        // payload を足した欄のほかは、封筒そのまま。
        without["context"] = json!({ "version": 1 });
        assert_eq!(v, without);
        // 欄の並びは決定的（鍵の順）。 payload の位置で封筒の出力が揺れない。
        let keys: Vec<&String> = v.as_object().expect("表").keys().collect();
        assert_eq!(keys, ["command", "context", "diagnostics", "ok"]);
        // 封筒の欄と同じ名前は、封筒が勝つ。
        let clobber = render_json_with(
            "check-x",
            rep.diagnostics(),
            None,
            Some(("ok", json!("payload"))),
        );
        let v: Value = serde_json::from_str(&clobber).expect("読める");
        assert_eq!(v["ok"], true);
    }

    #[test]
    fn 根の外のパスと行の無い場所はそのまま出す() {
        let mut rep = Report::default();
        rep.error_at("x", Some(Location::new("/elsewhere/a.md", None)), "m");
        let v: Value = serde_json::from_str(&render_json(
            "check-x",
            rep.diagnostics(),
            Some(Path::new("/repo")),
        ))
        .expect("読める");
        assert_eq!(v["diagnostics"][0]["location"]["path"], "/elsewhere/a.md");
        assert_eq!(v["diagnostics"][0]["location"]["line"], Value::Null);
    }
}
