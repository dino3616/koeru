//! 失敗の契約を source から確かめる（`DEC-PLT-038`）。
//!
//! 型では言えないものを見る。 文言に値を差し込んでいないこと、code が型をまたいで
//! 一意なこと、code の形。 どれも破っても組み立ては通り、画面にもトレースにも
//! 黙って出る。

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// `#[error("…")]` に差し込んでよい欄。 どれも数で、利用者のデータではない。
///
/// 何番目のノートか、1行に何単位か、が分からないと直しようがない失敗だけがここに来る。
/// 名前や綴りや入力値を差し込む欄を足さない。
const DISPLAY_FIELDS_ALLOWED: [&str; 3] = ["index", "got", "max"];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("リポジトリの根があること")
}

fn sources() -> Vec<(PathBuf, String)> {
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()));
        for e in entries.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                walk(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p)
                    .unwrap_or_else(|e| panic!("{} を読めない: {e}", p.display()));
                out.push((p, text));
            }
        }
    }
    // 走査する crate は `crates/` の下で `Cargo.toml` を持つもの全部。 名前を並べると、
    // 足した crate だけが素通りする——`koeru-model` を切り出したとき、実装が 28 個に
    // 減ったことでしか気づけなかった。**踏んだ。**
    let crates = repo_root().join("crates");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&crates)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", crates.display()));
    for e in entries.filter_map(Result::ok) {
        if e.path().join("Cargo.toml").is_file() {
            walk(&e.path().join("src"), &mut out);
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `start` にある開き括弧・波括弧に対応する閉じ括弧までの中身。 文字列の中は数えない。
fn balanced(text: &str, start: usize) -> &str {
    let bytes = text.as_bytes();
    let (open, close) = match bytes[start] {
        b'(' => (b'(', b')'),
        b'{' => (b'{', b'}'),
        other => panic!("括弧ではない: {}", other as char),
    };
    let mut depth = 0_i32;
    let mut i = start;
    let mut in_str = false;
    while i < bytes.len() {
        let b = bytes[i];
        if in_str {
            if b == b'\\' {
                i += 1;
            } else if b == b'"' {
                in_str = false;
            }
        } else if b == b'"' {
            in_str = true;
        } else if b == open {
            depth += 1;
        } else if b == close {
            depth -= 1;
            if depth == 0 {
                return &text[start + 1..i];
            }
        }
        i += 1;
    }
    panic!("括弧が閉じていない");
}

/// 文字列リテラルの中身を並べる。
fn string_literals(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c != '"' {
            continue;
        }
        let mut s = String::new();
        while let Some(c) = chars.next() {
            match c {
                '\\' => {
                    if let Some(e) = chars.next() {
                        s.push(e);
                    }
                }
                '"' => break,
                _ => s.push(c),
            }
        }
        out.push(s);
    }
    out
}

/// 書式の文字列が差し込む欄の名前。 `{{` は波括弧そのもの。
fn placeholders(fmt: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut chars = fmt.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                continue;
            }
            let name: String = chars.by_ref().take_while(|c| *c != '}').collect();
            out.push(name.split(':').next().unwrap_or_default().trim().to_owned());
        }
    }
    out
}

/// `impl … Failure for 型 {` の型名と、その `fn code` が返す文字列。
fn failure_codes(text: &str) -> Vec<(String, Vec<String>)> {
    let mut out = Vec::new();
    for marker in ["impl koeru_failure::Failure for ", "impl Failure for "] {
        for (at, _) in text.match_indices(marker) {
            let rest = &text[at + marker.len()..];
            let ty = rest
                .split(|c: char| c.is_whitespace() || c == '{')
                .next()
                .unwrap_or_default()
                .to_owned();
            let Some(open) = rest.find('{') else { continue };
            let body = balanced(rest, open);
            let Some(code_at) = body.find("fn code(&self)") else {
                continue;
            };
            let Some(brace) = body[code_at..].find('{') else {
                continue;
            };
            let code_body = balanced(&body[code_at..], brace);
            out.push((ty, string_literals(code_body)));
        }
    }
    out
}

/// `AppError::new("code", …)` の code。 境界で初めて分かる失敗。
fn boundary_codes(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for (at, _) in text.match_indices("AppError::new(") {
        let args = balanced(text, at + "AppError::new".len());
        if let Some(first) = args.trim_start().strip_prefix('"')
            && let Some(end) = first.find('"')
        {
            out.push(first[..end].to_owned());
        }
    }
    out
}

fn is_code(s: &str) -> bool {
    let parts: Vec<&str> = s.split('.').collect();
    parts.len() >= 2
        && parts.iter().all(|p| {
            !p.is_empty()
                && p.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

#[test]
fn 失敗の文言に値を差し込まない() {
    let mut bad = Vec::new();
    let mut seen = 0_usize;
    for (path, text) in sources() {
        for (at, _) in text.match_indices("#[error(") {
            let args = balanced(&text, at + "#[error".len());
            if args.trim() == "transparent" {
                continue;
            }
            seen += 1;
            for fmt in string_literals(args) {
                for name in placeholders(&fmt) {
                    if !DISPLAY_FIELDS_ALLOWED.contains(&name.as_str()) {
                        let line = text[..at].matches('\n').count() + 1;
                        bad.push(format!("{}:{line}: `{{{name}}}`", path.display()));
                    }
                }
            }
        }
    }
    assert!(
        bad.is_empty(),
        "文言に値を差し込んでいる。値は型つきの欄で持つ（`DEC-PLT-038`）:\n  {}",
        bad.join("\n  ")
    );
    assert!(
        seen > 100,
        "#[error] が {seen} 件しか見つからない。走査の先を確かめる"
    );
}

#[test]
fn code_は型をまたいで一意() {
    let mut owners: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut malformed = Vec::new();
    let mut impls = 0_usize;
    for (path, text) in sources() {
        let file = path
            .strip_prefix(repo_root())
            .map_or_else(|_| path.display().to_string(), |p| p.display().to_string());
        for (ty, codes) in failure_codes(&text) {
            impls += 1;
            for code in codes {
                if !is_code(&code) {
                    malformed.push(format!("{file} の {ty}: `{code}`"));
                }
                owners
                    .entry(code)
                    .or_default()
                    .insert(format!("{file} の {ty}"));
            }
        }
        // 境界の code は、同じ意味の失敗を別の場所で作ることがある（`app.poisoned`）。
        // 境界どうしは1人の持ち主として数え、型とだけぶつからないことを見る。
        for code in boundary_codes(&text) {
            if !is_code(&code) {
                malformed.push(format!("{file} の AppError::new: `{code}`"));
            }
            owners
                .entry(code)
                .or_default()
                .insert("koeru-app の境界".to_owned());
        }
    }
    let shared: Vec<String> = owners
        .iter()
        .filter(|(_, o)| o.len() > 1)
        .map(|(code, o)| {
            format!(
                "`{code}`: {}",
                o.iter().cloned().collect::<Vec<_>>().join(" / ")
            )
        })
        .collect();
    assert!(
        shared.is_empty(),
        "同じ code を複数の持ち主が名乗っている。呼び出し側が区別できない:\n  {}",
        shared.join("\n  ")
    );
    assert!(
        malformed.is_empty(),
        "code は `領域.名前` の点区切り（小文字・数字・_）:\n  {}",
        malformed.join("\n  ")
    );
    assert!(
        impls >= 30,
        "Failure の実装が {impls} 個しか見つからない。走査の先を確かめる"
    );
    assert!(
        owners.len() >= 120,
        "code が {} 個しか見つからない",
        owners.len()
    );
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 読み方そのものが壊れていない() {
    assert_eq!(
        placeholders("{index} 番目 {{literal}} {actual:?}"),
        ["index", "actual"]
    );
    let src = r#"
impl koeru_failure::Failure for X {
    fn class(&self) -> Class { Class::Internal }
    fn code(&self) -> &'static str {
        match self { Self::A => "x.a", Self::B(e) => e.code() }
    }
}
fn f() { AppError::new("app.x", Class::Rejected, format!("{} 件", 3)); }
"#;
    assert_eq!(
        failure_codes(src),
        [("X".to_owned(), vec!["x.a".to_owned()])]
    );
    assert_eq!(boundary_codes(src), ["app.x"]);
    assert!(is_code("recording.macos.unit_failed"));
    assert!(!is_code("Recording.x") && !is_code("nodot"));
}
