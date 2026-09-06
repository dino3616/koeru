//! submodule の Kaldi（`vendor/kaldi`）から、GMM 推論に要る部分だけをビルドする。
//!
//! # なぜ `configure` と `make` を使わないのか
//!
//! Kaldi の完全ビルドは 30〜60 分かかり、CI でも毎回それを払うことになる
//! （`DEC-PLT-016` の review_trigger「CI の submodule 取得がビルド時間の支配項に
//! なったとき」に直ちに当たる）。KOERU が使うのは強制アライメントだけなので、
//! 必要な8モジュールを `cc` で直接組む。
//!
//! # OpenFst を引かない
//!
//! これが成立するのは調べた結果であって、願望ではない。
//!
//! - `base/kaldi-types.h` は `OPENFST_VER >= 10800` なら `<stdint.h>` の
//!   typedef を使う。上流自身が用意している分岐なので、定義すれば OpenFst が要らない
//! - `hmm/transition-model.h` は `<fst/fst-decl.h>` を引くが、FST の型を1つも使っていない。
//!   空のスタブを置く。使い始めたらビルドが壊れる——それが狙い
//! - 本当に OpenFst が要るのは `tree/tree-renderer.cc`（GraphViz 可視化）、
//!   `hmm/tree-accu.cc`（学習用）、`hmm/hmm-utils.cc`（FST 構築）の3つだけ。
//!   どれも推論には要らない（`hmm-utils` の `SplitToPhones` 相当はシムで書く）
//!
//! # BLAS
//!
//! macOS は Accelerate（`HAVE_CLAPACK`）。他 OS はまだ書いていない——
//! `koeru-audio` と同じく、書いていない OS では素直に落ちるようにしてある。

use std::path::{Path, PathBuf};

/// Kaldi のモジュールと、そこから外すファイル。
///
/// 外す理由はどれも「推論に要らない」。 減らすためではなく、
/// OpenFst を引かないため。
const MODULES: [(&str, &[&str]); 8] = [
    ("base", &[]),
    ("matrix", &[]),
    ("util", &[]),
    // GraphViz で木を描く。可視化なので要らない。
    ("tree", &["tree-renderer.cc"]),
    ("gmm", &[]),
    // `tree-accu` は学習用、`hmm-utils` は FST 構築。どちらも OpenFst を引く。
    ("hmm", &["tree-accu.cc", "hmm-utils.cc"]),
    ("transform", &[]),
    ("feat", &[]),
];

fn main() {
    // 書いていない OS では Kaldi を組まない（`src/mfa/unsupported.rs` が選ばれる）。
    // 組もうとして落ちると、他 OS の CI が「クレートが組み立たない」ところで止まり、
    // その先のドメイン層の回帰にも気づけなくなる。一度やった（`koeru-audio`）。
    let forced_unsupported = std::env::var("CARGO_ENCODED_RUSTFLAGS")
        .unwrap_or_default()
        .contains("koeru_force_unsupported_backend");
    if !cfg!(target_os = "macos") || forced_unsupported {
        println!("cargo:rerun-if-env-changed=CARGO_ENCODED_RUSTFLAGS");
        return;
    }

    let vendor = Path::new("vendor/kaldi/src");
    println!("cargo:rerun-if-changed=vendor/kaldi");
    println!("cargo:rerun-if-changed=shim");

    // submodule が未初期化のときは、C++ のエラーではなく手順を出す（`DEC-PLT-016`）。
    assert!(
        vendor.join("base/kaldi-error.cc").is_file(),
        "Kaldi の submodule が初期化されていない。\n    \
         git submodule update --init --recursive\n\
         を実行すること（DEC-PLT-016）。"
    );

    let out = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR がある"));
    write_generated_headers(&out);

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++17")
        .include(vendor)
        .include(&out)
        // Kaldi の警告はこちらの責任ではない。自前のコードの警告は消さない。
        .warnings(false)
        .define("KALDI_DOUBLEPRECISION", "0")
        .define("HAVE_POSIX_MEMALIGN", None)
        // 上流自身の分岐に乗る。 これで `<fst/types.h>` の代わりに `<stdint.h>` を使う。
        .define("OPENFST_VER", "10800");

    // Accelerate。`kaldi-blas.h` が `HAVE_CLAPACK` で Accelerate を引く。
    // 他 OS は上で戻っているので、ここへは来ない。
    build.define("HAVE_CLAPACK", None);
    println!("cargo:rustc-link-lib=framework=Accelerate");

    for (module, skip) in MODULES {
        for f in sources(&vendor.join(module), skip) {
            build.file(f);
        }
    }
    build.file("shim/koeru_kaldi.cc");
    build.compile("koeru_kaldi");
}

/// Kaldi の Makefile が作るヘッダと、OpenFst の空スタブを置く。
fn write_generated_headers(out: &Path) {
    // `base/version.h` は Kaldi の Makefile が git から作る。
    let base = out.join("base");
    std::fs::create_dir_all(&base).expect("生成先を作れる");
    std::fs::write(
        base.join("version.h"),
        concat!(
            "// KOERU の build.rs が作った。Kaldi の Makefile が作るものの代わり。\n",
            "#define KALDI_VERSION \"5.5-koeru\"\n",
            "#define KALDI_GIT_HEAD \"vendored\"\n",
        ),
    )
    .expect("version.h を書ける");

    // `hmm/transition-model.h` が引いているが、FST の型を1つも使っていない。
    let fst = out.join("fst");
    std::fs::create_dir_all(&fst).expect("生成先を作れる");
    std::fs::write(
        fst.join("fst-decl.h"),
        concat!(
            "// Kaldi の hmm/transition-model.h が引いているが、FST の型を1つも使っていない。\n",
            "// 使い始めたらここが空なのでビルドが壊れる。それが狙い。\n",
            "#pragma once\n",
        ),
    )
    .expect("fst-decl.h を書ける");
}

/// そのモジュールの `.cc`。試験は入れない。
fn sources(dir: &Path, skip: &[&str]) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("モジュールを読める")
        .filter_map(std::result::Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e == "cc"))
        .filter(|p| {
            let name = p
                .file_name()
                .and_then(std::ffi::OsStr::to_str)
                .unwrap_or("");
            !name.contains("-test") && !skip.contains(&name)
        })
        .collect();
    // 並びを固定する。 `read_dir` の順は環境で変わり、
    // リンク順が変われば再現性が落ちる（`TR-ALN-29`）。
    v.sort();
    v
}
