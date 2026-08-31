//! OS ごとの音声入出力。
//!
//! **抽象レイヤを挟まない**（`DEC-REC-001`）。共通のトレイトで包むのは、
//! `TR-REC-08`〜`12` が要求する制御がバックエンド固有で、包んだ時点で
//! 出せなくなるものが出るため。**共通化するのは呼び出し側の手順であって、API ではない。**

#[cfg(target_os = "macos")]
pub mod macos;

// **どの OS でもコンパイルする。** macOS でしか型が合わない形にしておくと、
// 気づくのが他 OS の CI になり、直すたびに1往復かかる。
pub mod unsupported;

/// いまの OS のバックエンド。
///
/// **これは抽象レイヤではない。** 型を揃えているだけで、
/// 各 OS 固有の制御はそれぞれのモジュールに直に生えている（`DEC-REC-001`）。
///
/// **書いていない OS には空の実装を置く。** そうしないと `koeru-app` が
/// macOS でしかコンパイルできず、他 OS の CI が「アプリが組み立たない」ところで止まって、
/// **その先にあるドメイン層の回帰にも気づけなくなる。**
/// **書いていない OS 向けの組み立てを、macOS からも検査できるようにする。**
///
/// クロスコンパイルには C のツールチェーンが要り、手元では通せない。
/// このフラグを立てると `current` がスタブを指すので、
/// 型のずれが手元で分かる。気づくのを他 OS の CI に頼ると、直すたびに1往復かかる。
///
/// ```bash
/// RUSTFLAGS='--cfg koeru_force_unsupported_backend' cargo check --workspace --all-targets
/// ```
///
/// **cargo の feature にしない。** feature は加算的であるべきで、
/// `--all-features` で挙動が変わるものを混ぜると、**macOS の CI がスタブを検査しはじめる。**
#[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
pub use macos as current;

#[cfg(any(not(target_os = "macos"), koeru_force_unsupported_backend))]
pub use unsupported as current;
