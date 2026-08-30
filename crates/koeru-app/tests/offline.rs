//! **試唱の全経路がオフラインで動く**ことを確かめる（`TR-SYN-32`, `TR-PLT-20`）。
//!
//! 直接ネットワークを止めて試すことはできないので、**そもそも外へ出る経路が
//! コードに無いこと**と、**同梱物だけで動くこと**の2つで確かめる。
//!
//! # 何を見ているか
//!
//! 1. 依存の木に HTTP クライアントが入っていない
//! 2. ソースに外部プロセスの起動が無い（`TR-SYN-01` の「外部プロセスを起動しない」）
//! 3. 合成コア・phonemizer・録音リストが、同梱物だけで動く

#![allow(clippy::print_stdout)]

use std::path::Path;

/// 引いてはいけない crate。
const NETWORK_CRATES: [&str; 9] = [
    "reqwest",
    "hyper",
    "ureq",
    "curl",
    "isahc",
    "attohttpc",
    "surf",
    "tokio-tungstenite",
    "tungstenite",
];

/// **合成の経路は HTTP クライアントを引かない**（`TR-SYN-32`）。
///
/// `koeru-app` は Tauri を引き、Tauri は `reqwest` を引く（asset protocol のため）。
/// **これは KOERU が通信することを意味しない。** 止めるべきなのは
/// 「合成・録音・ドメインの経路が外へ出ること」なので、そこを見る。
#[test]
fn 合成の経路がhttpクライアントを引かない() {
    let root = repo_root();
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio"] {
        let manifest = root.join("crates").join(crate_name).join("Cargo.toml");
        let text = std::fs::read_to_string(&manifest).expect("読めること");
        for name in NETWORK_CRATES {
            assert!(
                !text.contains(name),
                "**{crate_name} が {name} を引いている。** 処理はローカル完結で、声をサーバへ送らない"
            );
        }
    }
    println!("合成・録音・ドメインの経路に HTTP クライアントは無い");
}

/// **KOERU 自身のコードが HTTP クライアントを使わない**（`TR-SYN-32`, `TR-PLT-20`）。
///
/// Tauri が引いているものを、こちらから呼ばない。
#[test]
fn 自分のコードがhttpクライアントを呼ばない() {
    let root = repo_root();
    let mut found = Vec::new();
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio", "koeru-app"] {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            for name in NETWORK_CRATES {
                let ident = name.replace('-', "_");
                if text.contains(&format!("{ident}::")) {
                    found.push(format!("{} が {name} を呼んでいる", path.display()));
                }
            }
        });
    }
    assert!(found.is_empty(), "{found:?}");
    println!("自分のコードは HTTP クライアントを呼んでいない");
}

/// **初回起動時のダウンロードを行わない**（`TR-SYN-32`, `TR-PLT-20`）。
///
/// 配るものの中に、外を指す設定が無いことを見る。
#[test]
fn 設定が外を指していない() {
    let root = repo_root();
    let conf =
        std::fs::read_to_string(root.join("crates/koeru-app/tauri.conf.json")).expect("読めること");

    // **フロントは同梱したファイルから読む。** リモートを指さない。
    assert!(
        conf.contains("\"frontendDist\": \"ui/dist/client\""),
        "配るフロントが同梱物であること"
    );
    // **更新機構を持たない。** 持つと、起動のたびに外へ出る。
    assert!(!conf.contains("updater"), "更新機構を持たないこと");
    // **開発用の口はローカルだけ。**
    assert!(
        !conf.contains("devUrl") || conf.contains("http://localhost:1420"),
        "開発用の口がローカルであること"
    );
    println!("設定は外を指していない");
}

/// **外部プロセスを起動しない**（`TR-SYN-01`）。
/// **外部プロセスを起動しない**（`TR-SYN-01`）。
///
/// `.exe`、Wine、Python インタプリタのいずれも起動しない。
/// **本人が明示的に指した resampler だけが例外**（`TR-SYN-35`）で、
/// それは実装されるときにこの一覧へ足す。
#[test]
fn 合成の経路に外部プロセスの起動が無い() {
    let root = repo_root();
    let mut found = Vec::new();
    for crate_name in ["koeru-core", "koeru-synth", "koeru-audio", "koeru-app"] {
        let src = root.join("crates").join(crate_name).join("src");
        walk(&src, &mut |path, text| {
            if text.contains("std::process::Command") || text.contains("process::Command") {
                found.push(path.display().to_string());
            }
        });
    }
    assert!(
        found.is_empty(),
        "**外部プロセスの起動が見つかった**: {found:?}"
    );
    println!("外部プロセスの起動は無い");
}

/// **同梱物だけで、録音リストと課題曲と phonemizer が動く**（`TR-SYN-32`, `TR-PLT-20`）。
///
/// 初回起動時のダウンロードを行わない。
#[test]
fn 同梱物だけで一通り動く() {
    use koeru_core::alias::Method;
    use koeru_core::inventory::UnitSet;
    use koeru_core::{mora, reclist, ust};

    // 録音リスト。**第三者の録音リストファイルを同梱しない**（TR-RCL-02）。
    let list = reclist::generate_single(UnitSet::Core, 5).expect("生成できること");
    assert!(!list.is_empty());

    // 課題曲。**同梱はパブリックドメインの伝承曲だけ**（TR-RCL-12）。
    let songs = ust::bundled_songs();
    assert_eq!(songs.len(), 1);

    // phonemizer。**辞書を外から取らない**（TR-SYN-11）。
    let m = mora::parse("さくらさくら", UnitSet::Core).expect("読めること");
    let need = koeru_core::alias::required_aliases(Method::Single, &m, UnitSet::Core);
    assert!(!need.is_empty());

    // 合成コア。**同梱した WORLD**（TR-SYN-05）。
    let x: Vec<f64> = (0..4410)
        .map(|i| (2.0 * std::f64::consts::PI * 220.0 * f64::from(i) / 44_100.0).sin() * 0.5)
        .collect();
    let cond = koeru_synth::f0::conditions(koeru_synth::f0::Purpose::Preview, None);
    let (f0, _) = koeru_synth::f0::estimate(&x, 44_100, &cond);
    assert!(!f0.is_empty(), "同梱した合成コアが動くこと");

    println!(
        "録音リスト {} 行 / 課題曲 {} 曲 / 必要単位 {} 個",
        list.len(),
        songs.len(),
        need.len()
    );
}

fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("リポジトリの根があること")
}

fn walk(dir: &Path, f: &mut impl FnMut(&Path, &str)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.filter_map(Result::ok) {
        let p = e.path();
        if p.is_dir() {
            walk(&p, f);
        } else if p.extension().is_some_and(|x| x == "rs")
            && let Ok(text) = std::fs::read_to_string(&p)
        {
            f(&p, &text);
        }
    }
}
