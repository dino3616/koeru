//! KOERU の実行ファイル。
//!
//! ここはブートストラップ層。 失敗したら回復せず落ちる。
//! `expect` を使ってよいのはこの層だけ（`rust-conventions`）。

// Windows でコンソールウィンドウを出さない。 デバッグビルドでは出す。
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    koeru_app_lib::run();
}
