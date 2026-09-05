//! 画面へ渡す型と呼び出し口が、Rust 側と一致していることを固定する。
//!
//! `ui/src/lib/bindings.gen.ts` は生成物で、正本は Rust 側の
//! コマンド定義（`DEC-PLT-019`）。手で直さない。
//!
//! 生成し直すのはここ。`cargo test -p koeru-app --test bindings` を走らせると
//! 書き出し、`--check`（CI）では差分があれば落ちる。

use std::path::{Path, PathBuf};

fn bindings_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("ui/src/lib/bindings.gen.ts")
}

/// 生成した TS を返す。書き出しはせず、文字列として取る。
fn generate() -> String {
    let dest = std::env::temp_dir().join(format!("koeru-bindings-{}.ts", std::process::id()));
    koeru_app_lib::builder()
        .export(specta_typescript::Typescript::default(), &dest)
        .expect("bindings を生成できること");
    let out = std::fs::read_to_string(&dest).expect("生成した bindings を読めること");
    let _ = std::fs::remove_file(&dest);
    out
}

/// 生成物が最新であること。
///
/// コマンドを1つ足して `bindings.gen.ts` を作り直し忘れると、画面側は
/// 古い型のまま通ってしまう。 型が合っているように見えて実際は合っていない
/// ——`invoke` は実行時にしか失敗しないので、ここで止める。
///
/// 落ちたら `KOERU_WRITE_BINDINGS=1 cargo test -p koeru-app --test bindings`。
#[test]
fn 画面のbindingsがrustと一致する() {
    let generated = generate();
    let path = bindings_path();

    if std::env::var_os("KOERU_WRITE_BINDINGS").is_some() {
        std::fs::write(&path, &generated).expect("bindings を書けること");
        return;
    }

    let current = std::fs::read_to_string(&path).unwrap_or_default();
    assert!(
        current == generated,
        "{} が古い。`KOERU_WRITE_BINDINGS=1 cargo test -p koeru-app --test bindings` で作り直す",
        path.display(),
    );
}
