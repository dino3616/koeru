//! 同梱した WORLD（`vendor/world`）をビルドする。
//!
//! **同梱にしたのは、`git clone` + `cargo build` だけで通るようにするため**
//! （`DEC-SYN-006`）。submodule だと寄稿者と CI に手順が1つ増える。
//! WORLD は参照実装で更新が稀、かつ 336KB と小さい。
//!
//! ライセンスは BSD-3-Clause。**各ファイルの著作権表示をそのまま残している。**

fn main() {
    let src = std::path::Path::new("vendor/world/src");
    println!("cargo:rerun-if-changed=vendor/world");

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        .include(src)
        // 同梱コードの警告はこちらの責任ではない。**自前のコードの警告は消さない。**
        .warnings(false);

    for f in [
        "cheaptrick.cpp",
        "common.cpp",
        "d4c.cpp",
        "dio.cpp",
        "fft.cpp",
        "harvest.cpp",
        "matlabfunctions.cpp",
        "stonemask.cpp",
        "synthesis.cpp",
    ] {
        build.file(src.join(f));
    }
    build.compile("world");
}
