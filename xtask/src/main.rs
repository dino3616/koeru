//! `cargo xtask` の入口。 振り分けから先は [`xtask::cli`] が持つ。

use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    xtask::cli::run(&args)
}
