//! `koeru-runtime` が Tauri・GraphQL・各エンジンの具体実装を引き込まないことを
//! 確かめる（`DEC-PLT-034`）。
//!
//! この crate は Application 層で、SQLite・ファイル・時計には触ってよい
//! ——`koeru-model` / `koeru-formats`（`crates/koeru-model/tests/purity.rs` /
//! `crates/koeru-formats/tests/purity.rs`）の純度試験とは違う規律で、
//! あちらの許可リスト方式（並べていない依存を全部落とす）はここでは採らない。
//! ここが見るのは、Tauri・specta・各エンジン（`koeru-audio` / `koeru-synth` /
//! `koeru-align`）・`async-graphql` の具体実装を引いていないことだけ
//! （`docs/reports/architecture/03-task-dag.md` T05 の
//! 「Owned / affected」「Must not change」）。

use std::path::PathBuf;

/// 引いてはいけない crate。
///
/// - `tauri` / `specta` — 境界（`koeru-app`、将来の `koeru-desktop`）が持つ
///   （`DEC-PLT-034` の crate 表）
/// - `koeru-audio` / `koeru-synth` / `koeru-align` — 計算だけを持つ crate。
///   ここから直接引くと、Application 層が個別エンジンの具体形に縛られる
/// - `async-graphql` — `koeru-graphql` が持つ（`DEC-PLT-035`）
const DEPENDENCIES_FORBIDDEN: [&str; 6] = [
    "tauri",
    "specta",
    "koeru-audio",
    "koeru-synth",
    "koeru-align",
    "async-graphql",
];

#[test]
fn 禁じた_crate_を引かない() {
    let manifest = crate_dir().join("Cargo.toml");
    let text = std::fs::read_to_string(&manifest)
        .unwrap_or_else(|e| panic!("{} を読めない: {e}", manifest.display()));
    let deps = dependencies(&text);
    assert!(
        !deps.is_empty(),
        "依存を1つも読めていない。読み方が壊れていないか"
    );
    let bad: Vec<&String> = deps
        .iter()
        .filter(|d| DEPENDENCIES_FORBIDDEN.contains(&d.as_str()))
        .collect();
    assert!(
        bad.is_empty(),
        "引いてはいけない依存: {bad:?}（`DEC-PLT-034` の crate 表を見る）"
    );
}

/// 検査そのものを検査する（`DEC-PLT-039`）。 読み方が壊れると、上の検査は黙って通る。
#[test]
fn 依存の読み方が壊れていない() {
    let manifest = "[package]\nname = \"x\"\n\n[dependencies]\n# 注記\nthiserror.workspace = true\nkoeru-core = { path = \"../koeru-core\" }\n\n[dev-dependencies]\ntauri = \"2\"\n\n[target.'cfg(unix)'.dependencies]\nlibc = \"0.2\"\n";
    assert_eq!(dependencies(manifest), ["thiserror", "koeru-core", "libc"]);
}

fn crate_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
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
