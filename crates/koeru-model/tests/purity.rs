//! `koeru-model` が入出力を持ち込まないことを確かめる（`DEC-PLT-034`）。
//!
//! 型では言えない。 `std::fs` を1行足しても組み立ては通り、WASM で呼ぶ日（M6）に
//! 初めて落ちる。 それより前に、依存とソースの両方をここで見る。

use std::path::{Path, PathBuf};

/// 引いてよい crate。 **並べていないものは通さない。**
///
/// 足すなら、native と WASM の両方で組み立ち、入出力を持たないことを確かめてから。
/// SQLite（`diesel`）・圧縮（`zip`）・画像・Tauri・GraphQL・外部形式の読み書き
/// （`encoding_rs` / `yaml_serde` / `toml_edit`）は、ここではなく外側の crate が持つ。
const DEPENDENCIES_ALLOWED: [&str; 5] = [
    "koeru-failure",
    "sha2",
    "thiserror",
    "tracing",
    "unicode-normalization",
];

/// 本体のコードが触ってはいけない標準ライブラリの口。
///
/// 時計も入れる。 同じ入力から同じ判断を返す kernel が現在時刻を読むと、
/// 編集のドラッグ中の予測（WASM）と確定（native）で答えが割れる。 `Duration` は
/// 値なので通す（確認に掛かる見込み時間、`TR-ALN-25`）。
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
        "許可していない依存: {extra:?}。 入出力を持つものは外側の crate に置く"
    );
}

#[test]
fn 本体のコードが入出力と時計に触らない() {
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
    assert!(files >= 20, "走査したファイルが少なすぎる: {files}");
    assert!(found.is_empty(), "入出力か時計に触っている: {found:?}");
}

/// `build.rs` を持たない。 組み立ての途中で外を読むと、同じソースから別の答えが出る。
#[test]
fn 組み立ての手順を持たない() {
    assert!(!crate_dir().join("build.rs").exists());
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 依存とソースの読み方が壊れていない() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# 注記\nsha2.workspace = true\nkoeru-failure = { path = \"../koeru-failure\" }\n\n[dev-dependencies]\nzip = \"1\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
    assert_eq!(dependencies(manifest), ["sha2", "koeru-failure", "libc"]);

    let src = "use std::fs;\n#[cfg(test)]\nmod tests { use std::time::Instant; }\n";
    assert_eq!(product_lines(src).collect::<Vec<_>>(), ["use std::fs;"]);

    let clock = "let t = std::time::Instant::now();";
    assert!(
        STD_FORBIDDEN.iter().any(|f| clock.contains(f)),
        "時計を拾う"
    );
    let budget = "use std::time::Duration;";
    assert!(
        !STD_FORBIDDEN.iter().any(|f| budget.contains(f)),
        "値の Duration は通す"
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
/// 試験は時間を測ってよい（被覆の計算が 50ms に収まるか、など）。
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
