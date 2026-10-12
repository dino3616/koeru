//! `koeru-synth` が計算だけを持ち、旧ドメイン層・DB・ファイルを
//! 引き込まないことを確かめる（`DEC-PLT-034`）。
//!
//! 型では言えない。 `koeru-core` を1行足しても組み立ては通り、SQLite と圧縮と YAML が
//! 合成器の依存に混ざる。 合成を別プロセスや WASM へ寄せる日に初めて落ちる。
//! それより前に、依存とソースの両方をここで見る。

use std::path::{Path, PathBuf};

/// 引いてよい workspace の crate。 **並べていないものは通さない。**
///
/// - `koeru-failure` — 失敗の語彙
/// - `koeru-formats` — 周波数表の格子（`frq::HOP_SIZE`）。 受け取るのはバイト列と数
/// - `koeru-model` — 5値の型（`oto::Oto`）
///
/// `koeru-audio` は開発時の試験（WAV を読む）にだけ使うので `[dev-dependencies]` にあり、
/// ここには並べない。 本体が引き始めたらこの検査が落ちる。
const WORKSPACE_ALLOWED: [&str; 3] = ["koeru-failure", "koeru-formats", "koeru-model"];

/// 戻ってきてはいけない crate。
///
/// - `koeru-core` — SQLite・ZIP・YAML・ファイルを持つ旧ドメイン層。 合成器が引くと、
///   音を出すために DB の依存を全部引くことになる
/// - `koeru-package` / `koeru-app` / `koeru-runtime` / `koeru-align` — 上の層と横の engine。
///   エンジンは互いを知らず、組み合わせるのは Application の仕事
/// - 残りは外側の crate が持つもの（SQLite・圧縮・画像・YAML・Tauri・GraphQL）
const DEPENDENCIES_FORBIDDEN: [&str; 14] = [
    "koeru-core",
    "koeru-package",
    "koeru-app",
    "koeru-runtime",
    "koeru-align",
    "diesel",
    "diesel_migrations",
    "libsqlite3-sys",
    "zip",
    "image",
    "yaml_serde",
    "tauri",
    "specta",
    "async-graphql",
];

/// 本体のコードが触ってはいけない標準ライブラリの口。
///
/// 受け取るのは波形と数で、ファイルを開くのは呼び出し側。 時計も入れる。
/// 同じ入力から同じ音を返す計算が現在時刻を読むと、再実行で答えが割れる。
/// `std::path` は名前の組み替えだけなので通す。
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
fn 許可した_workspace_crate_しか引かない() {
    let deps = manifest_dependencies();
    let extra: Vec<&String> = deps
        .iter()
        .filter(|d| d.starts_with("koeru-") && !WORKSPACE_ALLOWED.contains(&d.as_str()))
        .collect();
    assert!(
        extra.is_empty(),
        "許可していない workspace の依存: {extra:?}。 計算だけの crate なので、\
         規則は `koeru-model`、外部形式は `koeru-formats` に置く"
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
    assert!(files >= 4, "走査したファイルが少なすぎる: {files}");
    assert!(found.is_empty(), "ファイルか時計に触っている: {found:?}");
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 依存とソースの読み方が壊れていない() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# 注記\nthiserror.workspace = true\nkoeru-failure = { path = \"../koeru-failure\" }\n\n[dev-dependencies]\nkoeru-audio = { path = \"../koeru-audio\" }\n\n[build-dependencies]\ncc.workspace = true\n";
    assert_eq!(dependencies(manifest), ["thiserror", "koeru-failure", "cc"]);

    let src = "use std::fs;\n#[cfg(test)]\nmod tests { use std::time::Instant; }\n";
    assert_eq!(product_lines(src).collect::<Vec<_>>(), ["use std::fs;"]);

    let clock = "let t = std::time::Instant::now();";
    assert!(
        STD_FORBIDDEN.iter().any(|f| clock.contains(f)),
        "時計を拾う"
    );
    let path = "use std::path::PathBuf;";
    assert!(
        !STD_FORBIDDEN.iter().any(|f| path.contains(f)),
        "名前の組み替えだけの `std::path` は通す"
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
/// 試験は時間を測ってよい。
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
