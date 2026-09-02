//! submodule の WORDL（`vendor/world`）をビルドする。
//!
//! **submodule で調達する**（`DEC-PLT-016`）。以前は同梱していたが（`DEC-SYN-006`）、
//! Kaldi を submodule にした時点で「`git clone` + `cargo build` だけで通る」性質は
//! どのみち失われるので、**調達の形を1つに揃えた。**
//!
//! ライセンスは BSD-3-Clause。**各ファイルの著作権表示は上流のまま。**

use std::path::Path;

/// ビルドする翻訳単位。
///
/// **`codec.cpp` と `synthesisrealtime.cpp` は入れない。** 使っていない。
const SOURCES: [&str; 9] = [
    "cheaptrick.cpp",
    "common.cpp",
    "d4c.cpp",
    "dio.cpp",
    "fft.cpp",
    "harvest.cpp",
    "matlabfunctions.cpp",
    "stonemask.cpp",
    "synthesis.cpp",
];

fn main() {
    let src = Path::new("vendor/world/src");
    println!("cargo:rerun-if-changed=vendor/world");

    // **submodule が未初期化のときは、C++ のコンパイルエラーではなく手順を出す**
    // （`DEC-PLT-016`）。ヘッダが無いだけで数百行の診断が出ても、原因が読めない。
    assert!(
        src.join("dio.cpp").is_file(),
        "WORLD の submodule が初期化されていない。\n    \
         git submodule update --init --recursive\n\
         を実行すること（DEC-PLT-016）。"
    );

    let mut build = cc::Build::new();
    build
        .cpp(true)
        .std("c++11")
        .include(src)
        // 同梱コードの警告はこちらの責任ではない。**自前のコードの警告は消さない。**
        .warnings(false);

    for f in SOURCES {
        build.file(src.join(f));
    }
    build.compile("world");
}
