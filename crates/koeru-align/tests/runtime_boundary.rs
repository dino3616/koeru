//! **自動原音設定の実行時ランタイム境界**を確かめる（`TR-ALN-01`）。
//!
//! > 自動原音設定の実行経路に Python インタプリタ、conda 環境、外部データベースサーバ、
//! > 常駐する別プロセス、ユーザーによる追加インストールを一切含めない。
//! > **インストール直後・ネットワーク切断状態で、追加取得なしに全方式の推定が動作する**
//!
//! # なぜ静的に見るのか
//!
//! Python を起動しないことを実行時に確かめるには「起動しなかった」を観測する必要があり、
//! **通らなかった経路は観測できない。** だから「そもそも経路がコードに無い」を見る。
//! `koeru-app` の `offline.rs` と同じ形。
//!
//! # MFA を採ったのに Python が要らない理由
//!
//! MFA 3.0 は Kaldi のバイナリを呼ぶ方式をやめ、共有ライブラリを直接呼ぶ形になった。
//! **KOERU が使うのはモデル（CC BY 4.0）と Kaldi（Apache-2.0）だけで、
//! MFA というアプリケーションは同梱しない**（`TR-ALN-05`, `DEC-ALN-008`）。

#![allow(clippy::print_stdout)]

use std::path::{Path, PathBuf};

/// 実行経路に含めてはいけないもの（`TR-ALN-01`）。
const FORBIDDEN_RUNTIME: [(&str, &str); 6] = [
    ("python", "Python インタプリタ"),
    ("conda", "conda 環境"),
    ("mamba", "conda 環境"),
    ("postgres", "外部データベースサーバ"),
    ("kalpy", "MFA の Python バインディング"),
    ("montreal_forced_aligner", "MFA のアプリケーション本体"),
];

/// 引いてはいけない crate。**ネットワークも外部プロセスも通らない。**
const FORBIDDEN_CRATES: [&str; 6] = ["reqwest", "hyper", "ureq", "curl", "isahc", "pyo3"];

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

/// **依存に Python バインディングも HTTP クライアントも無い**（`TR-ALN-01`）。
#[test]
fn 依存に外部ランタイムが無い() {
    let text = std::fs::read_to_string(crate_root().join("Cargo.toml")).expect("読めること");
    for name in FORBIDDEN_CRATES {
        assert!(
            !text.contains(name),
            "koeru-align が `{name}` を引いている（TR-ALN-01）"
        );
    }
}

/// **自前のコードが外部プロセスを起動しない**（`TR-ALN-01`）。
///
/// `vendor/` は見ない——上流の Kaldi には使わないツールも入っている。
/// **見るのは KOERU が書いた部分**（`src/`、`shim/`、`build.rs`）。
#[test]
fn 自前のコードが外部プロセスを起動しない() {
    let root = crate_root();
    let mut checked = 0_usize;
    for dir in ["src", "shim"] {
        walk(&root.join(dir), &mut |path, text| {
            checked += 1;
            for pat in [
                "std::process::Command",
                "Command::new",
                "std::process::exit",
            ] {
                assert!(
                    !text.contains(pat),
                    "{} が `{pat}` を含む（TR-ALN-01）",
                    path.display()
                );
            }
            // C++ 側も同じ。
            for pat in ["system(", "popen(", "fork(", "execv"] {
                assert!(
                    !text.contains(pat),
                    "{} が `{pat}` を含む（TR-ALN-01）",
                    path.display()
                );
            }
        });
    }
    assert!(checked > 10, "検査したファイルが少なすぎる: {checked}");
    println!("  {checked} ファイルを見た");
}

/// **同梱物の名前に Python / conda / DB サーバが出てこない**（`TR-ALN-01`）。
///
/// `build.rs` が何を組むかは `MODULES` に列挙してある。
/// **そこへ外部ランタイムが紛れ込んだら落ちる。**
#[test]
fn ビルドが外部ランタイムを引かない() {
    let text = std::fs::read_to_string(crate_root().join("build.rs")).expect("読めること");
    let lower = text.to_lowercase();
    for (needle, what) in FORBIDDEN_RUNTIME {
        assert!(
            !lower.contains(needle),
            "build.rs が {what}（`{needle}`）に触れている（TR-ALN-01）"
        );
    }
}

/// **モデルと辞書は同梱物から読む。実行時に取りに行かない**（`TR-ALN-01`, `TR-PLT-19`）。
///
/// 音素セットと仮名辞書は `include_str!` でコンパイル時に埋め込んである。
/// **URL を組み立ててどこかへ取りに行く経路が無いこと**を見る。
#[test]
fn 実行時に何も取りに行かない() {
    let root = crate_root();
    walk(&root.join("src"), &mut |path, text| {
        for pat in ["http://", "https://"] {
            for line in text.lines() {
                // ドキュメントの参照リンクは通す。**コードに URL が無いことを見る。**
                let is_doc =
                    line.trim_start().starts_with("//") || line.trim_start().starts_with("/*");
                assert!(
                    is_doc || !line.contains(pat),
                    "{} のコードに URL がある: {line}（TR-ALN-01）",
                    path.display()
                );
            }
        }
    });
}

/// **同梱のリソースが揃っている**（`TR-ALN-01` の「追加取得なしに動作する」）。
#[test]
fn 同梱のリソースが揃っている() {
    let res = crate_root().join("resources");
    for name in [
        "mfa-japanese-phones.tsv",
        "kana-phonemes.tsv",
        "presets.toml",
        "models.toml",
    ] {
        assert!(res.join(name).is_file(), "{name} が無い（TR-ALN-01）");
    }

    // **辞書は音素セットの中だけを使う**（`TR-ALN-07`）。
    assert_eq!(koeru_align::phoneme::phone_count(), 86);
    assert_eq!(koeru_align::phoneme::reading_count(), 144);

    // **規約プリセットが方式ごとに揃っている**（`TR-ALN-23`）。
    for m in [
        koeru_core::alias::Method::Single,
        koeru_core::alias::Method::Sequential,
        koeru_core::alias::Method::Cvvc,
    ] {
        assert!(koeru_align::preset::Preset::default_for(m).is_ok());
    }

    // **モデルの台帳が規律を満たしている**（`TR-ALN-31`）。
    let models = koeru_align::ledger::models().expect("読める");
    koeru_align::ledger::check(&models).expect("規律を満たす");
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else if p
            .extension()
            .is_some_and(|x| x == "rs" || x == "cc" || x == "h")
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            f(&p, &text);
        }
    }
}
