//! `test-receipt` の Probe / Receipt の境目を、小さな fixture のリポジトリに対して固定する
//! （X04、`DEC-PLT-039`）。
//!
//! `xtask/tests/commands.rs`（X00・X01）は既存のコマンドの出力を1バイトも変えないことを
//! 見る試験で、ここでは触らない。ここが見るのは、`probe::model` が区別する状態
//! （`Passed` / `Failed` / `NotApplicable` / `NotRun`）が実際にその区別どおりに現れるか——
//! 前提を欠いた試験や組み立たない suite が「通過」に紛れ込まないか。
//!
//! fixture は commands.rs の `Fixture` と、cargo の crate を組む `cargo_fixture`
//! （`xtask/tests/commands.rs:1264` あたり）を参考に、このファイルの中だけで持つ。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// 一時ディレクトリに組んだ、meta と cargo のワークスペースだけを持つリポジトリ。
struct Fixture {
    root: PathBuf,
}

struct Run {
    code: i32,
    stdout: String,
}

impl Fixture {
    /// 名前は試験ごとに変える——試験は並んで走るので、同じ場所を2つの試験が
    /// 書き換えないように。
    fn new(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("xtask-probe")
            .join(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("前の実行の fixture を消せる");
        }
        fs::create_dir_all(root.join("meta/suites")).expect("meta を作れる");
        fs::create_dir_all(root.join("specs")).expect("specs を作れる");
        Self { root }
    }

    fn write(&self, rel: &str, text: &str) -> &Self {
        let p = self.root.join(rel);
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).expect("親を作れる");
        }
        fs::write(&p, text).expect("書ける");
        self
    }

    /// crate を `dir/` に置き、`meta/suites/cargo.toml` を書く。 package 名は
    /// ディレクトリ名と別に指定できる——`package_name` の退行試験がこれを使う。
    fn cargo_crate(&self, dir: &str, package: &str, lib_rs: &str) -> &Self {
        self.write(
            "Cargo.toml",
            &format!("[workspace]\nmembers = [\"{dir}\"]\nresolver = \"2\"\n"),
        )
        .write(
            &format!("{dir}/Cargo.toml"),
            &format!(
                "[package]\nname = \"{package}\"\nversion = \"0.0.0\"\nedition = \"2021\"\npublish = false\n"
            ),
        )
        .write(&format!("{dir}/src/lib.rs"), lib_rs)
        .write(".gitignore", "target/\nCargo.lock\n")
    }

    fn run(&self, args: &[&str]) -> Run {
        self.run_with_env(args, &[])
    }

    fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_xtask"));
        cmd.current_dir(&self.root)
            .args(args)
            // CI の job の要約へ fixture の受領証を書き足さない。
            .env_remove("GITHUB_STEP_SUMMARY")
            // 組み立ては fixture の中に閉じる。手元の `CARGO_TARGET_DIR` に混ざらないように。
            .env("CARGO_TARGET_DIR", self.root.join("target"));
        for (k, v) in env {
            cmd.env(k, v);
        }
        let out = cmd.output().expect("xtask を起動できる");
        Run {
            code: out.status.code().expect("終了コードがある"),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        }
    }

    fn receipt(&self, runner: &str, platform: &str, backend: &str) -> serde_json::Value {
        let text = fs::read_to_string(self.root.join(format!(
            "target/receipts/{runner}-{platform}-{backend}.json"
        )))
        .expect("受領証を読める");
        serde_json::from_str(&text).expect("受領証は JSON")
    }
}

fn portfolio(suites: &[String]) -> String {
    format!("schema = 'test-portfolio'\n\n{}", suites.concat())
}

fn suite(id: &str, package: &str, target: &str, extra: &str) -> String {
    format!(
        "[[suite]]\nid = '{id}'\ntitle = '{id}'\nrunner = 'cargo'\npackage = '{package}'\ntarget = '{target}'\nplatforms = ['macos', 'linux', 'windows']\n{extra}\n"
    )
}

/// この環境で `test-receipt` が書く受領証の名前。 `probe::model::ExecutionContext::detect`
/// と同じ規則で決める——このファイル自身が「書いていない OS 向け」の検証
/// （`RUSTFLAGS` を強制した `cargo test --workspace`）の中で走ると、ここで起動する
/// xtask の子プロセスもその `RUSTFLAGS` を継承するので、素朴に「macOS なら native」と
/// 決め打つと受領証のファイル名を外す。
fn platform_and_backend() -> (&'static str, &'static str) {
    let platform = std::env::consts::OS;
    let forced = std::env::var("RUSTFLAGS")
        .unwrap_or_default()
        .contains("koeru_force_unsupported_backend");
    let backend = if platform == "macos" && !forced {
        "native"
    } else {
        "unsupported"
    };
    (platform, backend)
}

const LIB_TWO_TESTS: &str = r#"#[cfg(test)]
mod tests {
    #[test]
    fn one() {}

    #[test]
    fn two() {}
}
"#;

const LIB_EMPTY: &str = "// 試験を持たない lib。\n";

// ## 0 件で見つかった binary
//
// `min_cases = 0` の登録なら、試験を1本も持たない lib でも「通過」になる——
// 0 件そのものが問題ではなく、登録した下限を下回ることが問題になる。

#[test]
fn 見つけた試験が_0_件でも登録の下限が_0_なら通す() {
    let fx = Fixture::new("zero-found");
    fx.cargo_crate("fx", "fx", LIB_EMPTY);
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[suite("SUITE-FX-101", "fx", "lib", "min_cases = 0")]),
    );
    let run = fx.run(&["test-receipt"]);
    assert_eq!(run.code, 0, "{}", run.stdout);
    let (platform, backend) = platform_and_backend();
    let receipt = fx.receipt("cargo", platform, backend);
    let s = &receipt["suites"][0];
    assert_eq!(s["discovered"], 0);
    assert_eq!(s["passed"], 0);
    assert_eq!(s["result"], "passed");
}

// ## 前提を欠いて早く return する試験
//
// libtest から見れば、前提が無くて早期に `return` した試験も「通った」試験と
// 区別が付かない——ここが `min_cases`（DEC-PLT-039）を置いた理由そのもの。
// 実際にモデルや fixture が揃えば3件目が増えるはずの suite を、2件のまま
// 登録の下限だけ3にしたときに落ちることを確かめる。通過と数えないこと。

#[test]
fn 前提を欠いた試験は_min_cases_の下限で落ちる() {
    let fx = Fixture::new("early-return");
    fx.cargo_crate("fx", "fx", LIB_TWO_TESTS);
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[suite("SUITE-FX-102", "fx", "lib", "min_cases = 3")]),
    );
    let run = fx.run(&["test-receipt"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    assert!(
        run.stdout
            .contains("実行したのが 2 件で、登録の下限 3 件に届かない"),
        "{}",
        run.stdout
    );
    let (platform, backend) = platform_and_backend();
    let receipt = fx.receipt("cargo", platform, backend);
    assert_eq!(receipt["suites"][0]["result"], "failed");
}

// ## 組み立たない suite（NotRun）
//
// 存在しない target を登録すると、試験 binary は組み立てられない。これは
// `passed` でも `failed` でもなく、受領証にも積まれない——`NotRun` は
// Report の誤りとしてだけ出る（`probe::model::Status::NotRun`）。

#[test]
fn 組み立たない_suite_は通過にならず受領証にも載らない() {
    let fx = Fixture::new("build-failed");
    fx.cargo_crate("fx", "fx", LIB_TWO_TESTS);
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[suite("SUITE-FX-103", "fx", "ghost", "min_cases = 1")]),
    );
    let run = fx.run(&["test-receipt"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    assert!(
        run.stdout
            .contains("SUITE-FX-103: fx の ghost の試験 binary が組み立たなかった"),
        "{}",
        run.stdout
    );
    let (platform, backend) = platform_and_backend();
    let receipt = fx.receipt("cargo", platform, backend);
    assert_eq!(
        receipt["suites"].as_array().expect("配列"),
        &Vec::<serde_json::Value>::new()
    );
}

// ## unsupported backend の文脈で native だけの suite が NotApplicable
//
// `backends = ['native']` の登録は、この環境が unsupported backend（macOS 以外、
// または `koeru_force_unsupported_backend` を強制したとき）なら対象外になる。
// 対象外は「失敗」ではない——`applies: false` かつ `result: "not-applicable"`。
// platform に依らず確かめられるよう、子プロセスの `RUSTFLAGS` で強制する。

#[test]
fn native_限定の_suite_は_unsupported_backend_で対象外になる() {
    let fx = Fixture::new("native-only");
    fx.cargo_crate("fx", "fx", LIB_TWO_TESTS);
    fx.write(
        "meta/suites/cargo.toml",
        "schema = 'test-portfolio'\n\n[[suite]]\nid = 'SUITE-FX-104'\ntitle = 'SUITE-FX-104'\nrunner = 'cargo'\npackage = 'fx'\ntarget = 'lib'\nplatforms = ['macos', 'linux', 'windows']\nbackends = ['native']\nmin_cases = 2\n",
    );
    let run = fx.run_with_env(
        &["test-receipt"],
        &[("RUSTFLAGS", "--cfg koeru_force_unsupported_backend")],
    );
    assert_eq!(run.code, 0, "{}", run.stdout);
    let platform = std::env::consts::OS;
    let receipt = fx.receipt("cargo", platform, "unsupported");
    let s = &receipt["suites"][0];
    assert_eq!(s["applies"], false);
    assert_eq!(s["result"], "not-applicable");
}

// ## 知らない runner
//
// 1件も数えず、それでも受領証は書く（既存の commands.rs の試験と同じ形。
// ここでは probe の観点——受領証の `suites` が空になることだけを見る）。

#[test]
fn 知らない_runner_は_1件も数えない() {
    let fx = Fixture::new("unknown-runner");
    fx.cargo_crate("fx", "fx", LIB_TWO_TESTS);
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[suite("SUITE-FX-105", "fx", "lib", "min_cases = 2")]),
    );
    let run = fx.run(&["test-receipt", "--runner", "nope"]);
    assert_eq!(run.code, 1, "{}", run.stdout);
    assert!(
        run.stdout.contains("runner の `nope` を知らない"),
        "{}",
        run.stdout
    );
    assert!(
        run.stdout.contains("1件も数えられなかった"),
        "{}",
        run.stdout
    );
    let (platform, backend) = platform_and_backend();
    let receipt = fx.receipt("nope", platform, backend);
    assert_eq!(receipt["suites"].as_array().expect("配列").len(), 0);
}
