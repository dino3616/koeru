//! `koeru-package` が旧ドメイン層とエンジンを引き込まないことを確かめる
//! （`DEC-PLT-034`）。
//!
//! 型では言えない。 `koeru-core` や `koeru-align` を1行足しても組み立ては通り、
//! 書き出しの依存に SQLite や Kaldi が混ざる。 書き出しを Application の計画に
//! 載せ替える日（T08）に初めて落ちる。 それより前に、依存とソースの両方をここで見る。

use std::path::{Path, PathBuf};

/// 引いてよい workspace の crate。 **並べていないものは通さない。**
///
/// - `koeru-failure` — 失敗の語彙
/// - `koeru-formats` — `oto.ini`・周波数表・文字符号化・WAV・ZIP の構文と符号化
/// - `koeru-model` — 書き出しの可否・被覆・下位方式への計画など、入出力を持たない規則
const WORKSPACE_ALLOWED: [&str; 3] = ["koeru-failure", "koeru-formats", "koeru-model"];

/// 戻ってきてはいけない crate。
///
/// - `koeru-core` — SQLite・ZIP・YAML・ファイルを持つ旧ドメイン層。 書き出しの組み立てが
///   引くと、木を作るために DB の依存を全部引くことになる
/// - `koeru-audio` / `koeru-align` / `koeru-synth` — ファイルや OS の音声 API を持つ層と
///   計算だけのエンジン。 書き出しが引いてよいのは外部形式の構文と符号化
///   （`koeru-formats`）で、再輸出を経由しない。 `oto.ini` のためだけに Kaldi を、
///   WAV のためだけに録音の層を引いていた
/// - `koeru-app` / `koeru-runtime` — 上の層
/// - `zip` — 配布 ZIP の符号化は `koeru-formats` が持つ
/// - 残りは外側の crate が持つもの（SQLite・YAML・Tauri・GraphQL）。
///   `image` は音源アイコンの変換（`icon.rs`）で今はここが持つので並べない
const DEPENDENCIES_FORBIDDEN: [&str; 13] = [
    "koeru-core",
    "koeru-audio",
    "koeru-align",
    "koeru-synth",
    "zip",
    "koeru-app",
    "koeru-runtime",
    "diesel",
    "diesel_migrations",
    "libsqlite3-sys",
    "yaml_serde",
    "tauri",
    "specta",
];

/// 本体のコードがどのファイルでも触ってはいけない標準ライブラリの口と時計。
///
/// `std::io` は縛らない。 `icon.rs` がメモリ上の `Cursor` を使う。
/// `std::path` は名前の組み替えだけなので通す。
const STD_FORBIDDEN: [&str; 5] = [
    "std::net",
    "std::process",
    "std::thread",
    "Instant",
    "SystemTime",
];

/// 本体のコードがどのファイルでも触ってはいけない口。 例外は無い。
///
/// ZIP を書いて読み戻す手順（`TR-PKG-52`）は、符号化を `koeru-formats`、ファイルを
/// 開く側を `koeru-app` の `export_io` が持つ。
const STD_FS_ENV: [&str; 2] = ["std::fs", "std::env"];

#[test]
fn 許可した_workspace_crate_しか引かない() {
    let deps = manifest_dependencies();
    let extra: Vec<&String> = deps
        .iter()
        .filter(|d| d.starts_with("koeru-") && !WORKSPACE_ALLOWED.contains(&d.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "許可していない workspace の依存: {extra:?}。 規則は `koeru-model`、\
         外部形式は `koeru-formats` に置く"
    );
}

#[test]
fn 禁じた_crate_を引かない() {
    let deps = manifest_dependencies();
    let bad: Vec<&String> = deps
        .iter()
        .filter(|d| DEPENDENCIES_FORBIDDEN.contains(&d.as_str()))
        .collect();
    assert!(
        bad.is_empty(),
        "引いてはいけない依存: {bad:?}（`DEC-PLT-034` の crate 表を見る）"
    );
}

#[test]
fn 本体のコードが外へ出る口と時計に触らない() {
    let src = crate_dir().join("src");
    let mut found = Vec::new();
    let mut files = 0_usize;
    walk(&src, &mut |path, text| {
        files += 1;
        let relative = relative_to(&src, path);
        for (i, line) in product_lines(text).enumerate() {
            let code = line.split("//").next().unwrap_or_default();
            for name in STD_FORBIDDEN.iter().chain(STD_FS_ENV.iter()) {
                if code.contains(name) {
                    found.push(format!("{relative}:{}: {name}", i + 1));
                }
            }
        }
    });
    assert!(files >= 8, "走査したファイルが少なすぎる: {files}");
    assert!(found.is_empty(), "外へ出る口か時計に触っている: {found:?}");
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 依存とソースの読み方が壊れていない() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# 注記\nzip.workspace = true\nkoeru-audio = { path = \"../koeru-audio\" }\n\n[dev-dependencies]\nkoeru-align = { path = \"../koeru-align\" }\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
    assert_eq!(dependencies(manifest), ["zip", "koeru-audio", "libc"]);

    let src = "use std::fs;\n#[cfg(test)]\nmod tests { use std::time::Instant; }\n";
    assert_eq!(product_lines(src).collect::<Vec<_>>(), ["use std::fs;"]);

    let clock = "let t = std::time::Instant::now();";
    assert!(
        STD_FORBIDDEN.iter().any(|f| clock.contains(f)),
        "時計を拾う"
    );
    // どのファイルにも例外が無い。 `archive.rs` も同じ扱い。
    let file = "let f = std::fs::File::create(dest)?;";
    assert!(
        STD_FS_ENV.iter().any(|f| file.contains(f)),
        "ファイルを拾う"
    );
    let path = "use std::path::PathBuf;";
    assert!(
        !STD_FORBIDDEN
            .iter()
            .chain(STD_FS_ENV.iter())
            .any(|f| path.contains(f)),
        "名前の組み替えだけの `std::path` は通す"
    );

    assert_eq!(
        relative_to(Path::new("/a/src"), Path::new("/a/src/archive.rs")),
        "archive.rs"
    );
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn manifest_dependencies() -> Vec<String> {
    let manifest = crate_dir().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", manifest.display()));
    let deps = dependencies(&text);
    assert!(
        !deps.is_empty(),
        "依存を1つも読めていない。読み方が壊れていないか"
    );
    deps
}

/// `[dependencies]` と `[target.*.dependencies]` と `[build-dependencies]` に並んだ名前。
/// `[dev-dependencies]` は試験の中だけで使うので見ない
/// （`crates/koeru-model/tests/purity.rs` と同じ読み方）。
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
/// 試験は一時ディレクトリを作ってよい。
fn product_lines(text: &str) -> impl Iterator<Item = &str> {
    text.lines().take_while(|l| l.trim() != "#[cfg(test)]")
}

fn relative_to(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
