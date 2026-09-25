//! `koeru-formats` がファイルにも方針にも触らないことを確かめる（`DEC-PLT-034`）。
//!
//! 型では言えない。 `std::fs` を1行足しても組み立ては通り、構文の試験も通る。
//! 呼び出し側がファイルを読んで渡す形が崩れると、同じ構文を GraphQL や WASM から
//! 呼べなくなる。 それより前に、依存とソースの両方をここで見る。

use std::path::{Path, PathBuf};

/// 引いてよい crate。 **並べていないものは通さない。**
///
/// `koeru-model` は `oto.ini` の5値の型（`Oto`）のためだけに引く。 採用・台帳・確信度の
/// 規則は持ち込まない。 SQLite（`diesel`）・圧縮（`zip`）・Tauri・GraphQL は外側の crate が持つ。
const DEPENDENCIES_ALLOWED: [&str; 6] = [
    "encoding_rs",
    "koeru-failure",
    "koeru-model",
    "thiserror",
    "tracing",
    "unicode-normalization",
];

/// 本体のコードが触ってはいけない標準ライブラリの口。
///
/// 受け取るのはバイト列と文字列。 `std::path` は名前の綴りを組み替えるだけなので通す
/// （WAV 名から `.frq` 名を作る、`TR-PKG-05`）。
const STD_FORBIDDEN: [&str; 8] = [
    "std::fs",
    "std::io",
    "std::net",
    "std::process",
    "std::thread",
    "std::env",
    "Instant",
    "SystemTime",
];

#[test]
fn 許可した_crate_しか引かない() {
    let manifest = crate_dir().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", manifest.display()));
    let deps = dependencies(&text);
    assert!(
        !deps.is_empty(),
        "依存を1つも読めていない。読み方が壊れていないか"
    );
    let extra: Vec<&String> = deps
        .iter()
        .filter(|d| !DEPENDENCIES_ALLOWED.contains(&d.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "許可していない依存: {extra:?}。 ファイルや方針を持つものは外側の crate に置く"
    );
}

#[test]
fn 本体のコードがファイルと時計に触らない() {
    let mut found = Vec::new();
    let mut files = 0_usize;
    walk(&crate_dir().join("src"), &mut |path, text| {
        files += 1;
        for (i, line) in product_lines(text).enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for name in STD_FORBIDDEN {
                if code.contains(name) {
                    found.push(format!("{}:{}: {name}", path.display(), i + 1));
                }
            }
        }
    });
    assert!(files >= 5, "走査したファイルが少なすぎる: {files}");
    assert!(found.is_empty(), "ファイルか時計に触っている: {found:?}");
}

/// `build.rs` を持たない。 組み立ての途中で外を読むと、同じソースから別の答えが出る。
#[test]
fn 組み立ての手順を持たない() {
    assert!(!crate_dir().join("build.rs").exists());
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 依存とソースの読み方が壊れていない() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# 注記\nencoding_rs.workspace = true\nkoeru-failure = { path = \"../koeru-failure\" }\n\n[dev-dependencies]\nzip = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
    assert_eq!(
        dependencies(manifest),
        ["encoding_rs", "koeru-failure", "libc"]
    );

    let src = "use std::io::Write as _;\n#[cfg(test)]\nmod tests { use std::fs; }\n";
    assert_eq!(
        product_lines(src).collect::<Vec<_>>(),
        ["use std::io::Write as _;"]
    );
    assert!(
        STD_FORBIDDEN.iter().any(|f| src.contains(f)),
        "書き込みの口を拾う"
    );
    let naming = "use std::path::{Path, PathBuf};";
    assert!(
        !STD_FORBIDDEN.iter().any(|f| naming.contains(f)),
        "名前を組み替えるだけの std::path は通す"
    );
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// `[dependencies]` と `[target.*.dependencies]` と `[build-dependencies]` に並んだ名前。
/// `[dev-dependencies]` は試験の中だけで使うので見ない。
fn dependencies(manifest: &str) -> Vec<String> {
    let mut names = Vec::new();
    let mut inside = false;
    for line in manifest.lines().map(str::trim) {
        if line.starts_with('[') {
            inside = line == "[dependencies]"
                || line == "[build-dependencies]"
                || (line.starts_with("[target.") && line.ends_with(".dependencies]"));
            continue;
        }
        if !inside || line.is_empty() || line.starts_with('#') {
            continue;
        }
        let key = line
            .split(['=', '.'])
            .next()
            .unwrap_or_default()
            .trim()
            .trim_matches('"');
        if !key.is_empty() {
            names.push(key.to_owned());
        }
    }
    names
}

/// 試験の module より前の行。 本体の後ろに `#[cfg(test)] mod tests` を置く慣習に頼っている。
/// 試験はファイルを作ってよい。
fn product_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().take_while(|l| l.trim() != "#[cfg(test)]")
}

fn walk(dir: &Path, f: &mut dyn FnMut(&Path, &str)) {
    let entries =
        std::fs::read_dir(dir).unwrap_or_else(|e| panic!("{} を読めない: {e}", dir.display()));
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, f);
        } else if path.extension().is_some_and(|e| e == "rs") {
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("{} を読めない: {e}", path.display()));
            f(&path, &text);
        }
    }
}
