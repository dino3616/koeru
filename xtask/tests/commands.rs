//! コマンドの振る舞いを、小さな fixture のリポジトリに対して固定する。
//!
//! 固定したのは main の `931a4ae` の振る舞い。 xtask を組み替えるあいだ（X01 以降）、
//! 意図しない変化をここで落とす。 出力を変えるなら、この試験を変えることが差分に出る。
//!
//! 実リポジトリの出力は突き合わせない。 meta が1件増えるたびに数が変わり、
//! 試験が壊れるのは meta を足した人になる。 ここでは一時ディレクトリに meta と specs の
//! 最小集合を組み、binary を外から叩いて、stdout・stderr・終了コード・書いたものを見る。
//!
//! 通過・失敗・未実行を分けて見る。 未実行は、`test-receipt` の対象外（`not-applicable`）、
//! 組み立たなかった試験 binary、知らない実行器で1件も数えなかった場合の3つ。
//!
//! fixture の ID は、このファイルの中では領域を小文字で書く（`TR-fix-01`）。 このファイルも
//! 実リポジトリの `check-references` が走査するので、大文字で書くと実在しない ID として落ちる。
//! 小文字の領域は ID の形に当たらない。 fixture へ書くとき・引数に渡すとき・出力と比べるときに
//! [`ids`] で大文字へ直す。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// このファイルの中の書き方（`TR-fix-01`）を、fixture の ID（領域が `FIX`）に直す。
fn ids(text: &str) -> String {
    text.replace("-fix", "-FIX")
}

/// 一時ディレクトリに組んだリポジトリ。
struct Fixture {
    root: PathBuf,
}

/// 1回の実行の結果。 パスは `<root>` に置き換え、区切りは `/` に揃えてある。
#[derive(Debug)]
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

impl Fixture {
    /// 空の `meta/` と `specs/` だけを持つリポジトリ。 名前は試験ごとに変える
    /// ——試験は並んで走るので、同じ場所を2つの試験が書き換えないように。
    fn empty(name: &str) -> Self {
        let root = Path::new(env!("CARGO_TARGET_TMPDIR"))
            .join("xtask-commands")
            .join(name);
        if root.exists() {
            fs::remove_dir_all(&root).expect("前の実行の fixture を消せる");
        }
        fs::create_dir_all(root.join("meta")).expect("meta を作れる");
        fs::create_dir_all(root.join("specs")).expect("specs を作れる");
        Self { root }
    }

    /// 検査が全部通る meta と specs と文書。
    fn valid(name: &str) -> Self {
        let fx = Self::empty(name);
        for (rel, text) in VALID {
            fx.write(rel, text);
        }
        fx
    }

    fn write(&self, rel: &str, text: &str) -> &Self {
        let p = self.root.join(ids(rel));
        if let Some(dir) = p.parent() {
            fs::create_dir_all(dir).expect("親を作れる");
        }
        fs::write(&p, ids(text)).expect("書ける");
        self
    }

    fn remove(&self, rel: &str) -> &Self {
        fs::remove_file(self.root.join(ids(rel))).expect("消せる");
        self
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.root.join(ids(rel))).expect("読める")
    }

    fn run(&self, args: &[&str]) -> Run {
        run_in(&self.root, args, &self.root)
    }

    /// fixture の中で git を叩く。 手元の設定（署名・改行の変換）に左右されないよう、
    /// fixture のリポジトリにだけ設定を置く。
    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(&self.root)
            .args(args)
            .output()
            .expect("git を起動できる");
        assert!(
            out.status.success(),
            "git {args:?} が失敗した: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .expect("git の出力が UTF-8")
            .trim()
            .to_owned()
    }

    fn git_init(&self) {
        self.git(&["init", "-q"]);
        self.git(&["checkout", "-q", "-b", "main"]);
        for (k, v) in [
            ("user.name", "fixture"),
            ("user.email", "fixture@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            self.git(&["config", k, v]);
        }
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }
}

fn run_in(cwd: &Path, args: &[&str], root: &Path) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_xtask"))
        .current_dir(cwd)
        .args(args.iter().map(|a| ids(a)))
        // CI の job の要約へ fixture の受領証を書き足さない。
        .env_remove("GITHUB_STEP_SUMMARY")
        // 組み立ては fixture の中に閉じる。 手元で `CARGO_TARGET_DIR` を指していると、
        // fixture の試験 binary が外の target に混ざる。
        .env("CARGO_TARGET_DIR", root.join("target"))
        .output()
        .expect("xtask を起動できる");
    Run {
        code: out.status.code().expect("終了コードがある"),
        stdout: normalize(&String::from_utf8_lossy(&out.stdout), root),
        stderr: normalize(&String::from_utf8_lossy(&out.stderr), root),
    }
}

/// 出力の中の fixture の場所を `<root>` にし、区切りを `/` に揃える。
///
/// macOS では作業ディレクトリが実体のパスで返ってくることがあるので、
/// 正規化したパスも置き換える。
fn normalize(text: &str, root: &Path) -> String {
    let mut out = text.to_owned();
    let mut roots = vec![root.display().to_string()];
    if let Ok(canonical) = fs::canonicalize(root) {
        roots.push(canonical.display().to_string());
    }
    // 長いほうから置き換える。 短いほうが長いほうの先頭と一致していると、半端に残る。
    roots.sort_by_key(|r| std::cmp::Reverse(r.len()));
    for r in roots {
        out = out.replace(&r, "<root>");
    }
    out.replace('\\', "/")
}

/// 期待する出力を、行の並びから作る。 各行に改行を付けてつなぎ、ID を [`ids`] で直す。
///
/// 報告の行は2字下げで始まる。 行継続（`\` 改行）で書くと字下げが消えるので、
/// 行ごとに書く。
fn lines(ls: &[&str]) -> String {
    ids(&ls.iter().map(|l| format!("{l}\n")).collect::<String>())
}

impl Run {
    #[track_caller]
    fn expect(&self, code: i32, stdout: &[&str], stderr: &[&str]) {
        assert_eq!(self.stdout, lines(stdout), "stdout が違う: {self:?}");
        assert_eq!(self.stderr, lines(stderr), "stderr が違う: {self:?}");
        assert_eq!(self.code, code, "終了コードが違う: {self:?}");
    }
}

const REQUIREMENTS: &str = r"schema = 'requirement-set'

[[requirement]]
id = 'TR-fix-01'
title = 'マスターを 44100 Hz で保つ'
confidence = 'Fact'
statement = 'マスターは 44100 Hz で保存する。'
formalized_as = ['REQ-fix-001']

[[requirement]]
id = 'TR-fix-02'
title = '名前を正規化する'
confidence = 'Assumption'
statement = '名前は TR-fix-01 のあとで正規化する。'
depends_on = ['TR-fix-01']
needs_component = false
notes = ['一つ目の注記', '二つ目の注記']
";

const DECISION_1: &str = r"schema = 'decision'
id = 'DEC-fix-001'
constraint_label = '保存の形式'
title = 'WAV で保存する'
status = 'accepted'
owner = 'fixture'
options = ['WAV', 'FLAC']
selected = 'WAV'
rationale = '読み書きが単純。'
review_triggers = ['FLAC が要るようになったとき', '容量が問題になったとき']
affects_requirements = ['TR-fix-01']
";

const DECISION_2: &str = r"schema = 'decision'
id = 'DEC-fix-002'
constraint_label = '名前の規則'
title = '名前は NFD にする'
status = 'superseded'
superseded_by = 'DEC-fix-003'
owner = 'fixture'
options = ['NFD', 'NFC']
selected = 'NFD'
rationale = '古い判断。'
review_triggers = ['なし']
";

const DECISION_3: &str = r"schema = 'decision'
id = 'DEC-fix-003'
constraint_label = '名前の規則'
title = '名前は NFC にする'
status = 'accepted'
supersedes = ['DEC-fix-002']
owner = 'fixture'
options = ['NFD', 'NFC']
selected = 'NFC'
rationale = '配布先が NFC を前提にしている。'
review_triggers = ['配布先が変わったとき']
";

const QUESTION: &str = r"schema = 'question'
id = 'Q-fix-001'
title = 'FLAC は要るか'
status = 'open'
owner = 'fixture'
why_it_matters = '配布の容量が変わる。'
how_to_close = '利用者に聞く。'
blocks_profiles = ['PROFILE-fix-2']
affects_decisions = ['DEC-fix-001']
";

const PROFILE_1: &str = r"schema = 'profile'
id = 'PROFILE-fix-1'
title = '最初のリリース'
status = 'active'
includes_requirements = ['TR-fix-01', 'TR-fix-02']
includes_fsl = ['REQ-fix-001']
decisions = ['DEC-fix-001']
budgets = ['BUDGET-fix-001']
";

const PROFILE_2: &str = r"schema = 'profile'
id = 'PROFILE-fix-2'
title = '次のリリース'
status = 'planned'
";

const BUDGET: &str = r"schema = 'budget'
id = 'BUDGET-fix-001'
title = 'メモリ'
limit = 1000
unit = 'MB'
scope = '常駐'

[[allocations]]
item = '共通'
value = 300
measured = true

[[allocations]]
item = '収録'
value = 400
mode = '収録中'

[[allocations]]
item = '編集'
value = 500
mode = '編集中'

[[allocations]]
item = '小計'
kind = 'subtotal'
value = 9999

[[allocations]]
item = '未定'
mode = '編集中'
";

const COMPONENTS: &str = r"schema = 'component-ledger'

[[component]]
id = 'CMP-001'
name = 'hound'
purpose = 'WAV の読み書き'
license = 'Apache-2.0'
status = '採用'
decided_by = 'DEC-fix-001'
supports_requirements = ['TR-fix-01']

[[component]]
id = 'CMP-002'
name = 'flac'
purpose = 'FLAC の読み書き'
license = 'MIT'
status = '不適'
decided_by = 'DEC-fix-001'
supports_requirements = ['TR-fix-02']
";

const SUITES: &str = r"schema = 'test-portfolio'

[[suite]]
id = 'SUITE-fix-001'
title = 'fixture'
runner = 'cargo'
package = 'fx'
target = 'lib'
platforms = ['macos', 'linux', 'windows']
min_cases = 1
contracts = ['TR-fix-01', 'REQ-fix-001', 'DEC-fix-001']
";

const FSL: &str = r#"// fixture
@requirement("REQ-fix-001")
acceptance AC-fix-001 {
}
"#;

const GUIDE: &str = r"# 手引き

保存は `TR-fix-01` の「44100 Hz で保存する」に従う。
形式は `DEC-fix-001`、リリースは `PROFILE-fix-1`。
";

const VALID: &[(&str, &str)] = &[
    ("meta/requirements/core.toml", REQUIREMENTS),
    ("meta/decisions/DEC-fix-001.toml", DECISION_1),
    ("meta/decisions/DEC-fix-002.toml", DECISION_2),
    ("meta/decisions/DEC-fix-003.toml", DECISION_3),
    ("meta/questions/Q-fix-001.toml", QUESTION),
    ("meta/profiles/PROFILE-fix-1.toml", PROFILE_1),
    ("meta/profiles/PROFILE-fix-2.toml", PROFILE_2),
    ("meta/budgets/BUDGET-fix-001.toml", BUDGET),
    ("meta/evidence/components.toml", COMPONENTS),
    ("meta/suites/cargo.toml", SUITES),
    ("specs/fixture.fsl", FSL),
    ("docs/guide.md", GUIDE),
];

const USAGE: &[&str] = &[
    "使い方: cargo xtask <check-meta|check-budgets|check-coverage",
    "  check-references|check-profile <ID>",
    "  index-decisions|next-id <接頭辞>|dump-requirements",
    "  touched [<base>]",
    "  check-portfolio|test-receipt [--runner cargo|bun]",
    "  check-schema>",
];

// ## 入口

#[test]
fn 引数が無ければ使い方を出して失敗する() {
    let fx = Fixture::valid("usage");
    fx.run(&[]).expect(1, USAGE, &[]);
    fx.run(&["no-such-command"]).expect(1, USAGE, &[]);
}

#[test]
fn meta_と_specs_を持つ親が無ければ失敗する() {
    let dir = std::env::temp_dir().join(format!("xtask-no-root-{}", std::process::id()));
    fs::create_dir_all(&dir).expect("作れる");
    let run = run_in(&dir, &["check-meta"], &dir);
    fs::remove_dir_all(&dir).expect("消せる");
    run.expect(
        1,
        &[],
        &["リポジトリのルートが見つからない: meta/ と specs/ を持つ親が無い"],
    );
}

/// 作業ディレクトリが下の階層でも、`meta/` と `specs/` を持つ親を根にする。
#[test]
fn 下の階層からでも根を見つける() {
    let fx = Fixture::valid("nested-cwd");
    let run = run_in(&fx.root.join("docs"), &["next-id", "DEC-fix"], &fx.root);
    assert_eq!(run.code, 0, "{run:?}");
    assert!(run.stdout.starts_with(&ids("DEC-fix-004\n")), "{run:?}");
}

// ## check-meta

#[test]
fn check_meta_は整った_meta_を通す() {
    Fixture::valid("check-meta-ok").run(&["check-meta"]).expect(
        0,
        &[
            "  FSL の要求 2 件 / 技術要件 2 件 / 部品台帳 2 件 / 性能目標 0 件",
            "  budget 1 / component-ledger 1 / decision 3 / profile 2 / question 1 / requirement-set 1 / test-portfolio 1",
            "check-meta: ok",
        ],
        &[],
    );
}

/// 読み込みの段の失敗（形を名乗らない・知らない形・置き場所の違い・形と中身の不一致）。
#[test]
fn check_meta_は形の崩れた_meta_を落とす() {
    let fx = Fixture::valid("check-meta-shape");
    fx.write("meta/decisions/nameless.toml", "id = 'DEC-fix-900'\n")
        .write("meta/questions/weird.toml", "schema = 'nonsense'\n")
        .write("meta/profiles/misplaced.toml", DECISION_1)
        .write(
            "meta/decisions/DEC-fix-009.toml",
            r"schema = 'decision'
id = 'DEC-fix-010'
title = 'あ'
status = 'accepted'
options = []
selected = ''
rationale = ''
review_triggers = []

[[optoins]]
x = 1
",
        )
        .write(
            "meta/budgets/BUDGET-fix-002.toml",
            "schema = 'budget'\nid = 'BUDGET-fix-002'\ntitle = 'あ'\nlimit = 1\nunit = 'MB'\nscope = 'あ'\n",
        )
        .write(
            "meta/budgets/targets.toml",
            "schema = 'target-set'\n\n[[targt]]\nid = 'TGT-001'\n\n[[target]]\nid = 'X-001'\nitem = 'a'\n",
        )
        .write("meta/suites/empty.toml", "schema = 'test-portfolio'\n");
    // 読み込みの失敗が先に、パスの順で並ぶ。 形の検査の失敗はその後に、読めたものの順で。
    fx.run(&["check-meta"]).expect(
        1,
        &[
            "  FSL の要求 2 件 / 技術要件 2 件 / 部品台帳 2 件 / 性能目標 1 件",
            "  budget 2 / component-ledger 1 / decision 4 / profile 2 / question 1 / requirement-set 1 / target-set 1 / test-portfolio 2",
            "  NG <root>/meta/decisions/nameless.toml: `schema` が無い。ファイルは自分の形を名乗る必要がある",
            "  NG <root>/meta/profiles/misplaced.toml: schema `decision` は meta/decisions/ に置く",
            "  NG <root>/meta/questions/weird.toml: 知らない schema `nonsense`",
            "  NG <root>/meta/budgets/BUDGET-fix-002.toml: schema `budget` は `[[allocations]]` を1件以上持つ必要がある",
            "  NG <root>/meta/budgets/targets.toml: schema `target-set` の知らない `[[targt]]` がある。`[[target]]` の打ち間違いではないか",
            "  NG <root>/meta/budgets/targets.toml: [[target]] の 0 件目に `goal` が無い",
            "  NG <root>/meta/budgets/targets.toml: [[target]] の id `X-001` は `TGT-` で始まる必要がある",
            "  NG <root>/meta/decisions/DEC-fix-009.toml: 必須項目 `owner` が無い",
            "  NG <root>/meta/decisions/DEC-fix-009.toml: ファイル名が id `DEC-fix-010` と一致しない",
            "  NG <root>/meta/decisions/DEC-fix-009.toml: schema `decision` の知らない `[[optoins]]` がある。許すのは [] だけ",
            "  NG <root>/meta/suites/empty.toml: schema `test-portfolio` は `[[suite]]` を1件以上持つ必要がある",
            "check-meta: 11 件",
        ],
        &[],
    );
}

/// TOML として読めないファイル。 文言の後ろ半分は toml crate の診断なので、版で変わる。
/// 固定するのは先頭と、落ちることだけ。
#[test]
fn check_meta_は_toml_として読めないファイルを落とす() {
    let fx = Fixture::valid("check-meta-toml");
    fx.write("meta/evidence/broken.toml", "schema = \n");
    let run = fx.run(&["check-meta"]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(
        run.stdout.contains(
            "\n  NG <root>/meta/evidence/broken.toml: TOML として読めない: TOML parse error"
        ),
        "{run:?}"
    );
    assert!(run.stdout.ends_with("check-meta: 1 件\n"), "{run:?}");
}

/// 読み込んだあとの意味の検査（参照先の実在、置き換えの両側、所属、重複）。
#[test]
fn check_meta_は参照の切れた_meta_を落とす() {
    let fx = Fixture::valid("check-meta-refs");
    fx.write(
        "meta/requirements/core.toml",
        &format!(
            "{REQUIREMENTS}
[[requirement]]
id = 'TR-fix-03'
title = '孤児'
confidence = 'Maybe'
statement = ''
depends_on = ['TR-fix-77']
formalized_as = ['REQ-fix-404']
notes = ['TR-fix-88 を見る']

[[requirement]]
id = 'TR-fix-01'
title = '重複'
confidence = 'Fact'
statement = '重複した'
"
        ),
    )
    .write(
        "meta/decisions/DEC-fix-003.toml",
        &DECISION_3.replace(
            "supersedes = ['DEC-fix-002']",
            "supersedes = ['DEC-fix-001', 'DEC-fix-404']",
        ),
    )
    .write(
        "meta/decisions/DEC-fix-001.toml",
        &DECISION_1.replace(
            "affects_requirements = ['TR-fix-01']",
            "affects_requirements = ['TR-fix-01', 'TR-fix-01', 'TR-fix-55']\naffects_fsl = ['REQ-fix-999']\naffects_budgets = ['BUDGET-fix-404']\nundecided_in = ['specs/fixture.fsl', 'specs/missing.fsl']",
        ),
    )
    .write(
        "meta/profiles/PROFILE-fix-2.toml",
        &format!("{PROFILE_2}includes_requirements = ['TR-fix-02']\n"),
    )
    .write(
        "meta/evidence/components.toml",
        &format!(
            "{COMPONENTS}
[[component]]
id = 'CMP-003'
name = 'x'
purpose = 'x'
license = 'MIT'
status = '採用'

[[component]]
id = 'CMP-004'
name = 'y'
purpose = 'y'
license = 'MIT'
status = '採用候補'
decided_by = 'DEC-fix-404'
"
        ),
    )
    .write(
        "meta/suites/cargo.toml",
        &SUITES.replace("'DEC-fix-001']", "'DEC-fix-001', 'SUITE-fix-404']"),
    );
    fx.run(&["check-meta"]).expect(
        1,
        &[
            "  FSL の要求 2 件 / 技術要件 3 件 / 部品台帳 4 件 / 性能目標 0 件",
            "  budget 1 / component-ledger 1 / decision 3 / profile 2 / question 1 / requirement-set 1 / test-portfolio 1",
            "  NG <root>/meta/requirements/core.toml: id `TR-fix-01` が重複している",
            "  NG TR-fix-03: 確度 `Maybe` は Fact / Assumption / Unknown のいずれでもない",
            "  NG TR-fix-03: 本文が空",
            "  NG TR-fix-03: depends_on の `TR-fix-77` は登録簿に存在しない",
            "  NG TR-fix-03: formalized_as の `REQ-fix-404` は FSL に存在しない",
            "  NG TR-fix-03 が参照する `TR-fix-88` は登録簿に存在しない",
            "  NG DEC-fix-003 が DEC-fix-001 を置き換えているのに、DEC-fix-001 の status が `accepted` のまま",
            "  NG DEC-fix-001 に superseded_by = 'DEC-fix-003' が無い",
            "  NG DEC-fix-003 の supersedes が指す DEC-fix-404 が無い",
            "  NG DEC-fix-404 に superseded_by = 'DEC-fix-003' が無い",
            "  NG DEC-fix-002 は DEC-fix-003 に置き換えられたと言うが、DEC-fix-003 の supersedes に無い",
            "  NG TR-fix-02 が複数のマイルストーンに属している: PROFILE-fix-1, PROFILE-fix-2",
            "  NG どのマイルストーンにも属さない要件が 1 件ある: TR-fix-03 ...",
            "  NG <root>/meta/evidence/components.toml: `CMP-003` は採否を決めているのに decided_by が無い。理由と撤回条件を持つ判断記録へ繋ぐこと",
            "  NG <root>/meta/evidence/components.toml: decided_by の `DEC-fix-404` という判断記録は存在しない",
            "  NG <root>/meta/requirements/core.toml: id `TR-fix-01` が <root>/meta/requirements/core.toml と重複している",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: affects_requirements に `TR-fix-01` が2度出ている",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: affects_requirements の `TR-fix-55` は技術要件に存在しない",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: affects_fsl の `REQ-fix-999` はFSL の要求に存在しない",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: affects_budgets の `BUDGET-fix-404` という meta は存在しない",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: specs/fixture.fsl に未決の印が無い",
            "  NG <root>/meta/decisions/DEC-fix-001.toml: specs/missing.fsl が読めない",
            "  NG <root>/meta/decisions/DEC-fix-003.toml: supersedes の `DEC-fix-404` という meta は存在しない",
            "  NG <root>/meta/suites/cargo.toml: SUITE-fix-001 の contracts の `SUITE-fix-404` は存在しない",
            "check-meta: 24 件",
        ],
        &[],
    );
}

/// 読み込みの段の失敗は、どのコマンドでも落ちる。 ただし入口の2つだけは例外。
#[test]
fn 読み込みの失敗はコマンドを問わず落とす() {
    let fx = Fixture::valid("load-error");
    fx.write("meta/decisions/nameless.toml", "id = 'DEC-fix-900'\n");
    // 自分の検査は最後まで走り、読み込みの失敗が報告に混ざる。
    fx.run(&["check-budgets"]).expect(
        1,
        &[
            "  BUDGET-fix-001: 山 800MB（編集中） / 上限 1000MB（80%）工程 4 件 / 実測済みでない 3 / 数値未設定 1",
            "      収録中: 700MB（共通 300MB を含む）",
            "      編集中: 800MB（共通 300MB を含む）",
            "  NG <root>/meta/decisions/nameless.toml: `schema` が無い。ファイルは自分の形を名乗る必要がある",
            "check-budgets: 1 件",
        ],
        &[],
    );
    // 番号は出す。 終了コードだけが失敗になる。
    fx.run(&["next-id", "DEC-fix"]).expect(
        1,
        &[
            "DEC-fix-004",
            "  `DEC-fix` は 3 件、最大は DEC-fix-003",
            "  NG <root>/meta/decisions/nameless.toml: `schema` が無い。ファイルは自分の形を名乗る必要がある",
            "next-id: 1 件",
        ],
        &[],
    );
    // 使い方と `dump-requirements` は、読み込みの失敗を報告せずに終わる。
    fx.run(&[]).expect(1, USAGE, &[]);
    let dump = fx.run(&["dump-requirements"]);
    assert_eq!(dump.code, 0, "{dump:?}");
    assert!(dump.stderr.is_empty(), "{dump:?}");
}

// ## check-budgets

#[test]
fn check_budgets_はモードごとの山を上限と比べる() {
    let fx = Fixture::valid("check-budgets");
    // 小計行は数えない。 値の無い行は 0 として数え、数値未設定として出す。
    fx.run(&["check-budgets"]).expect(
        0,
        &[
            "  BUDGET-fix-001: 山 800MB（編集中） / 上限 1000MB（80%）工程 4 件 / 実測済みでない 3 / 数値未設定 1",
            "      収録中: 700MB（共通 300MB を含む）",
            "      編集中: 800MB（共通 300MB を含む）",
            "check-budgets: ok",
        ],
        &[],
    );
    fx.write(
        "meta/budgets/BUDGET-fix-001.toml",
        &BUDGET.replace("limit = 1000", "limit = 700"),
    );
    fx.run(&["check-budgets"]).expect(
        1,
        &[
            "  BUDGET-fix-001: 山 800MB（編集中） / 上限 700MB（114%）工程 4 件 / 実測済みでない 3 / 数値未設定 1",
            "      収録中: 700MB（共通 300MB を含む）",
            "      編集中: 800MB（共通 300MB を含む）",
            "  NG BUDGET-fix-001: 編集中 の合計 800MB が上限 700MB を超えている（100MB 超過）",
            "check-budgets: 1 件",
        ],
        &[],
    );
    fx.write(
        "meta/budgets/BUDGET-fix-001.toml",
        &BUDGET.replace("limit = 1000", "limit = '千'"),
    );
    fx.run(&["check-budgets"]).expect(
        1,
        &[
            "  NG <root>/meta/budgets/BUDGET-fix-001.toml: `limit` が整数でない",
            "check-budgets: 1 件",
        ],
        &[],
    );
}

// ## check-coverage

#[test]
fn check_coverage_は支えの無い要件を落とす() {
    let fx = Fixture::valid("check-coverage");
    // `不適` の部品は支えない。 `needs_component = false` の要件は数えない。
    fx.run(&["check-coverage"]).expect(
        0,
        &[
            "  採る見込みの部品 1 件 / うち要件を指しているもの 1 件",
            "  要件 2 件 / 外部部品が要らないと宣言 1 件 / 支える部品がある 1 件 / 無い 0 件",
            "check-coverage: ok",
        ],
        &[],
    );
    fx.write(
        "meta/requirements/core.toml",
        &format!(
            "{REQUIREMENTS}
[[requirement]]
id = 'TR-fix-03'
title = '支えが無い'
confidence = 'Unknown'
statement = '誰も支えない。'
"
        ),
    )
    .write(
        "meta/evidence/components.toml",
        &COMPONENTS.replace("status = '不適'", "status = '参照のみ'"),
    );
    // `参照のみ` の部品は支える側に数える。
    fx.run(&["check-coverage"]).expect(
        1,
        &[
            "  採る見込みの部品 2 件 / うち要件を指しているもの 2 件",
            "  要件 3 件 / 外部部品が要らないと宣言 1 件 / 支える部品がある 2 件 / 無い 1 件",
            "  NG TR-fix-03 を支える部品が1つも無い",
            "  NG TR-fix-02 は needs_component = false なのに、支える部品が宣言されている",
            "check-coverage: 2 件",
        ],
        &[],
    );
}

// ## check-references

#[test]
fn check_references_は実体の無い_id_と古い引用を落とす() {
    let fx = Fixture::valid("check-references");
    fx.run(&["check-references"]).expect(
        0,
        &[
            "  ID 参照 3 件、実体 14 種",
            "  引用 1 件",
            "check-references: ok",
        ],
        &[],
    );
    fx.write(
        "src/lib.rs",
        "//! `TR-fix-99` と `DEC-fix-404` を引く。\n//! `TR-fix-01` の「48000 Hz で保存する」\n// `REQ-fix-001` と `AC-fix-001` は FSL にある。\n// `TR-fix02` は形が違うので拾わない。\n",
    )
    // 生成物と走査外の拡張子は見ない。
    .write("src/bindings.gen.ts", "// `TR-fix-98`\n")
    .write("notes.txt", "`TR-fix-97`\n")
    .write("target/x.md", "`TR-fix-96`\n");
    fx.run(&["check-references"]).expect(
        1,
        &[
            "  ID 参照 8 件、実体 14 種",
            "  引用 2 件",
            "  NG DEC-fix-404 の実体が無い（src/lib.rs:1）",
            "  NG TR-fix-99 の実体が無い（src/lib.rs:1）",
            "  NG src/lib.rs:2: TR-fix-01 に「48000 Hz で保存する」という文字列が無い",
            "check-references: 3 件",
        ],
        &[],
    );
}

// ## check-profile

#[test]
fn check_profile_は未決の問いが塞ぐプロファイルを落とす() {
    let fx = Fixture::valid("check-profile");
    fx.run(&["check-profile", "PROFILE-fix-1"]).expect(
        0,
        &[
            "  PROFILE-fix-1: FSL の要求 1 件 / 決定 1 件 / 予算 1 件",
            "check-profile: ok",
        ],
        &[],
    );
    fx.run(&["check-profile", "PROFILE-fix-2"]).expect(
        1,
        &[
            "  PROFILE-fix-2: FSL の要求 0 件 / 決定 0 件 / 予算 0 件",
            "  NG Q-fix-001 が未決のまま PROFILE-fix-2 を塞いでいる: FLAC は要るか",
            "check-profile: 1 件",
        ],
        &[],
    );
    fx.run(&["check-profile", "PROFILE-fix-9"]).expect(
        1,
        &[
            "  NG `PROFILE-fix-9` というプロファイルが無い",
            "check-profile: 1 件",
        ],
        &[],
    );
    fx.run(&["check-profile"])
        .expect(1, &[], &["使い方: cargo xtask check-profile <PROFILE-ID>"]);
    // 閉じた問いは塞がない。
    fx.write(
        "meta/questions/Q-fix-001.toml",
        &QUESTION.replace("status = 'open'", "status = 'closed'"),
    );
    fx.run(&["check-profile", "PROFILE-fix-2"]).expect(
        0,
        &[
            "  PROFILE-fix-2: FSL の要求 0 件 / 決定 0 件 / 予算 0 件",
            "check-profile: ok",
        ],
        &[],
    );
}

// ## index-decisions

#[test]
fn index_decisions_は索引を書き_check_で古さを見る() {
    let fx = Fixture::valid("index-decisions");
    // 索引が無いうちは古い。
    fx.run(&["index-decisions", "--check"]).expect(
        1,
        &[
            "  判断記録 3 件",
            "  NG meta/decisions/README.md が古い。`cargo xtask index-decisions` で作り直す",
            "index-decisions: 1 件",
        ],
        &[],
    );
    fx.run(&["index-decisions"]).expect(
        0,
        &["  判断記録 3 件を索引にした", "index-decisions: ok"],
        &[],
    );
    assert_eq!(
        fx.read("meta/decisions/README.md"),
        lines(&[
            "# 判断記録の索引",
            "",
            "`schema = 'decision'` のファイルの一覧。この索引は手で書かない。",
            "`cargo xtask index-decisions` が `meta/decisions/*.toml` から作る。",
            "中身を直すのは各 TOML 側で、索引は作り直す。",
            "",
            "読み方と規律は [../README.md](../README.md)。置き換えの関係（`supersedes` /",
            "`superseded_by` / `status = 'superseded'`）は `cargo xtask check-meta` が双方向で検査する。",
            "",
            "| ID | 何についての判断か | 決めたこと | 状態 |",
            "|---|---|---|---|",
            "| [DEC-fix-001](DEC-fix-001.toml) | 保存の形式 | WAV で保存する | accepted |",
            "| [DEC-fix-002](DEC-fix-002.toml) | 名前の規則 | 名前は NFD にする | superseded |",
            "| [DEC-fix-003](DEC-fix-003.toml) | 名前の規則 | 名前は NFC にする | accepted |",
            "",
            "3 件。",
        ])
    );
    fx.run(&["index-decisions", "--check"]).expect(
        0,
        &["  判断記録 3 件", "index-decisions: ok"],
        &[],
    );
    fx.write(
        "meta/decisions/DEC-fix-001.toml",
        &DECISION_1.replace("WAV で保存する", "WAV で残す"),
    );
    fx.run(&["index-decisions", "--check"]).expect(
        1,
        &[
            "  判断記録 3 件",
            "  NG meta/decisions/README.md が古い。`cargo xtask index-decisions` で作り直す",
            "index-decisions: 1 件",
        ],
        &[],
    );
}

// ## next-id

#[test]
fn next_id_は既にある番号の次を桁を揃えて出す() {
    let fx = Fixture::valid("next-id");
    fx.run(&["next-id", "DEC-fix"]).expect(
        0,
        &[
            "DEC-fix-004",
            "  `DEC-fix` は 3 件、最大は DEC-fix-003",
            "next-id: ok",
        ],
        &[],
    );
    // 収集ファイルの中の ID も数える。 桁は既にあるものに合わせる。
    fx.run(&["next-id", "TR-fix"]).expect(
        0,
        &[
            "TR-fix-03",
            "  `TR-fix` は 2 件、最大は TR-fix-02",
            "next-id: ok",
        ],
        &[],
    );
    // 1件も無ければ3桁から始める。
    fx.run(&["next-id", "NEW-fix"]).expect(
        0,
        &["NEW-fix-001", "  `NEW-fix` はまだ1件も無い", "next-id: ok"],
        &[],
    );
    fx.run(&["next-id"]).expect(
        1,
        &[],
        &["使い方: cargo xtask next-id <接頭辞>   例: DEC-PLT"],
    );
    // 抜けは言うが、埋めない。
    fx.write(
        "meta/decisions/DEC-fix-005.toml",
        &DECISION_1.replace("DEC-fix-001", "DEC-fix-005"),
    );
    fx.run(&["next-id", "DEC-fix"]).expect(
        0,
        &[
            "DEC-fix-006",
            "  抜けている番号がある: DEC-fix-004",
            "  `DEC-fix` は 4 件、最大は DEC-fix-005",
            "next-id: ok",
        ],
        &[],
    );
}

// ## dump-requirements

#[test]
fn dump_requirements_は区切り文字で要件を書き出す() {
    let fx = Fixture::valid("dump-requirements");
    let run = fx.run(&["dump-requirements"]);
    // US(0x1f) 区切りの欄、RS(0x1e) 区切りのレコード。 欄は ID・題・確信度・
    // depends_on（`,` 区切り）・本文・注記（1件1欄）。 末尾に改行は無い。
    assert_eq!(
        run.stdout,
        ids(&[
            "TR-fix-01\u{1f}マスターを 44100 Hz で保つ\u{1f}Fact\u{1f}\u{1f}マスターは 44100 Hz で保存する。\u{1e}",
            "TR-fix-02\u{1f}名前を正規化する\u{1f}Assumption\u{1f}TR-fix-01\u{1f}名前は TR-fix-01 のあとで正規化する。\u{1f}一つ目の注記\u{1f}二つ目の注記\u{1e}",
        ]
        .concat()),
        "{run:?}"
    );
    assert_eq!(run.stderr, "", "{run:?}");
    assert_eq!(run.code, 0, "{run:?}");
}

// ## touched

#[test]
fn touched_は差分が引く_id_を本文ごと出す() {
    let fx = Fixture::valid("touched");
    fx.write("src/lib.rs", "pub fn a() {}\n").write(
        "meta/requirements/extra.toml",
        "schema = 'requirement-set'\n\n[[requirement]]\nid = 'TR-fix-03'\ntitle = '消える'\nconfidence = 'Fact'\nstatement = '消える要件。'\n",
    );
    fx.git_init();
    fx.write(".gitignore", "target/\n");
    fx.commit("base");
    let base = fx.git(&["rev-parse", "HEAD"]);

    fx.git(&["checkout", "-q", "-b", "feature"]);
    fx.write(
        "src/lib.rs",
        "/// 44100 Hz で保つ（`TR-fix-01`）。 形は `REQ-fix-001`。\n/// `TR-fix-99` は無い。\npub fn a() {}\n",
    )
    .write(
        "meta/requirements/core.toml",
        &REQUIREMENTS.replace("のあとで正規化する", "の前に正規化する"),
    )
    .remove("meta/requirements/extra.toml")
    .write("config.yml", "on: push\n");
    fx.commit("feature");
    // コミットしていない変更と、追跡していないファイルも見る。
    fx.write(
        "docs/guide.md",
        &format!("{GUIDE}\n判断は `DEC-fix-001` を見る。\n"),
    )
    .write("docs/new.md", "`Q-fix-001` が残っている。\n")
    .write("notes.txt", "メモ\n");

    // 消えた要件は base の原文を指す。 書き換えた要件は、変わった行の旧側と新側の両方で引く。
    let run = fx.run(&["touched"]);
    let expected = lines(&[
        "# main...HEAD が触れた契約",
        "",
        "## DEC-fix-001 — WAV で保存する",
        "",
        "正本: `meta/decisions/DEC-fix-001.toml`",
        "引いている場所: docs/guide.md:4, docs/guide.md:6",
        "状態: accepted ／ 選んだ案: WAV",
        "",
        "覆る条件:",
        "- FLAC が要るようになったとき",
        "- 容量が問題になったとき",
        "",
        "## PROFILE-fix-1 — 最初のリリース",
        "",
        "正本: `meta/profiles/PROFILE-fix-1.toml`",
        "引いている場所: docs/guide.md:4",
        "状態: active",
        "",
        "## Q-fix-001 — FLAC は要るか",
        "",
        "正本: `meta/questions/Q-fix-001.toml`",
        "引いている場所: docs/new.md:1",
        "状態: open",
        "",
        "配布の容量が変わる。",
        "",
        "閉じ方: 利用者に聞く。",
        "",
        "## REQ-fix-001",
        "",
        "正本: `specs/fixture.fsl:2`（FSL）",
        "引いている場所: src/lib.rs:1",
        "",
        "## TR-fix-01 — マスターを 44100 Hz で保つ",
        "",
        "正本: `meta/requirements/core.toml`",
        "引いている場所: docs/guide.md:3, src/lib.rs:1",
        "確信度: Fact",
        "",
        "マスターは 44100 Hz で保存する。",
        "",
        "## TR-fix-02 — 名前を正規化する",
        "",
        "正本: `meta/requirements/core.toml`",
        "引いている場所: meta/requirements/core.toml（削除）, meta/requirements/core.toml（書き換え）",
        "確信度: Assumption",
        "",
        "名前は TR-fix-01 の前に正規化する。",
        "",
        "## TR-fix-03",
        "",
        "**この変更で消えた。** 元の本文は `git show <base>:<path>`。",
        "消えた場所: meta/requirements/extra.toml（削除）",
        "",
        "## TR-fix-99",
        "",
        "**実体が無い。**（`check-references` が落とす）",
        "引いている場所: src/lib.rs:2",
        "",
        "## ID を1つも引いていない変更ファイル",
        "",
        "ここは契約との対応が機械では出ない。読んで決める。",
        "",
        "- `config.yml`",
        "- `notes.txt`",
        "",
        "  変更 7 ファイル、うち引用があったのは 3、触れた ID 8 件",
        "touched: ok",
    ])
    .replace("<base>", &base);
    assert_eq!(run.stdout, expected, "{run:?}");
    assert_eq!(run.stderr, "", "{run:?}");
    assert_eq!(run.code, 0, "{run:?}");

    // base を変えられる。 自分自身との差分は、コミットしていない分だけ。
    fx.run(&["touched", "HEAD"]).expect(
        0,
        &[
            "# HEAD...HEAD が触れた契約",
            "",
            "## DEC-fix-001 — WAV で保存する",
            "",
            "正本: `meta/decisions/DEC-fix-001.toml`",
            "引いている場所: docs/guide.md:4, docs/guide.md:6",
            "状態: accepted ／ 選んだ案: WAV",
            "",
            "覆る条件:",
            "- FLAC が要るようになったとき",
            "- 容量が問題になったとき",
            "",
            "## PROFILE-fix-1 — 最初のリリース",
            "",
            "正本: `meta/profiles/PROFILE-fix-1.toml`",
            "引いている場所: docs/guide.md:4",
            "状態: active",
            "",
            "## Q-fix-001 — FLAC は要るか",
            "",
            "正本: `meta/questions/Q-fix-001.toml`",
            "引いている場所: docs/new.md:1",
            "状態: open",
            "",
            "配布の容量が変わる。",
            "",
            "閉じ方: 利用者に聞く。",
            "",
            "## TR-fix-01 — マスターを 44100 Hz で保つ",
            "",
            "正本: `meta/requirements/core.toml`",
            "引いている場所: docs/guide.md:3",
            "確信度: Fact",
            "",
            "マスターは 44100 Hz で保存する。",
            "",
            "## ID を1つも引いていない変更ファイル",
            "",
            "ここは契約との対応が機械では出ない。読んで決める。",
            "",
            "- `notes.txt`",
            "",
            "  変更 3 ファイル、うち引用があったのは 2、触れた ID 4 件",
            "touched: ok",
        ],
        &[],
    );
}

#[test]
fn touched_は引けない_base_で失敗する() {
    let fx = Fixture::valid("touched-bad-base");
    fx.git_init();
    fx.commit("base");
    let run = fx.run(&["touched", "no-such-ref"]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(
        run.stdout
            .starts_with("  NG git diff --name-only -z no-such-ref...HEAD が失敗した: "),
        "{run:?}"
    );
    assert!(run.stdout.ends_with("touched: 1 件\n"), "{run:?}");
}

// ## check-schema

const SCHEMA: &str = r#"schema { query: Query mutation: Mutation subscription: Subscription }

scalar ProjectLease
scalar OperationId
scalar Revision
scalar EventCursor
scalar ThingId

enum FailureClass { REJECTED CONFLICT }

interface Problem { code: String! class: FailureClass! }
type ProjectLeaseExpired implements Problem { code: String! class: FailureClass! }
type OperationIdReused implements Problem { code: String! class: FailureClass! }
type RevisionConflict implements Problem { code: String! class: FailureClass! }
type EventGap { resumeAfter: EventCursor! }
type MutationReceipt { operationId: OperationId! revision: Revision! }

type Thing { id: ThingId! name: String! }
union ThingResult = Thing | ProjectLeaseExpired
type Query { thing(lease: ProjectLease!): ThingResult! }

input RenameThingInput {
  lease: ProjectLease!
  operationId: OperationId!
  baseRevision: Revision!
  thing: ThingId!
  name: String!
}
type RenameThingPayload { receipt: MutationReceipt! outcome: RenameThingOutcome! }
union RenameThingOutcome = ThingRenamed | ProjectLeaseExpired | OperationIdReused | RevisionConflict
type ThingRenamed { thing: Thing! }
type Mutation { renameThing(input: RenameThingInput!): RenameThingPayload! }

union ThingEvent = ThingChanged | EventGap | ProjectLeaseExpired
type ThingChanged { cursor: EventCursor! }
type Subscription { thingEvents(lease: ProjectLease!, after: EventCursor): ThingEvent! }
"#;

const OPERATION: &str = r"query ReadThing($lease: ProjectLease!) {
  thing(lease: $lease) {
    __typename
    ... on Thing { id name }
  }
}
";

const CAPABILITY_MAP: &str = r"schema = 'capability-map'
[vocabulary]
capabilities = ['thing.read', 'thing.modify']
costs = ['cheap-read', 'durable-write']
[fields]
'Query.thing' = { capability = 'thing.read', cost = 'cheap-read' }
'Mutation.renameThing' = { capability = 'thing.modify', cost = 'durable-write' }
'Subscription.thingEvents' = { capability = 'thing.read', cost = 'cheap-read' }
";

#[test]
fn check_schema_は契約と見本と能力の表を検査する() {
    let fx = Fixture::valid("check-schema");
    fx.write("specs/application/schema/shared.graphql", SCHEMA)
        .write("specs/application/operations/read.graphql", OPERATION)
        .write("specs/application/capabilities.toml", CAPABILITY_MAP);
    fx.run(&["check-schema"]).expect(
        0,
        &[
            "  operation 1 件（query 1 / mutation 0 / subscription 0）、fragment 0 件、文書 1 本",
            "  見本の operation が使っていない root の欄 2 個: Mutation.renameThing, Subscription.thingEvents",
            "  型 23 個、root の欄 3 個、SDL 1 本",
            "check-schema: ok",
        ],
        &[],
    );

    // 能力の表が root の欄を覆っていない。
    fx.write(
        "specs/application/capabilities.toml",
        &CAPABILITY_MAP.replace(
            "'Subscription.thingEvents' = { capability = 'thing.read', cost = 'cheap-read' }\n",
            "",
        ),
    );
    fx.run(&["check-schema"]).expect(
        1,
        &[
            "  operation 1 件（query 1 / mutation 0 / subscription 0）、fragment 0 件、文書 1 本",
            "  見本の operation が使っていない root の欄 2 個: Mutation.renameThing, Subscription.thingEvents",
            "  型 23 個、root の欄 3 個、SDL 1 本",
            "  NG specs/application/capabilities.toml: root の欄 `Subscription.thingEvents` に能力が無い",
            "check-schema: 1 件",
        ],
        &[],
    );

    // 見本が1本も無い。
    fx.write("specs/application/capabilities.toml", CAPABILITY_MAP)
        .remove("specs/application/operations/read.graphql");
    fx.run(&["check-schema"]).expect(
        1,
        &[
            "  型 23 個、root の欄 3 個、SDL 1 本",
            "  NG specs/application/operations に operation の文書が1本も無い",
            "check-schema: 1 件",
        ],
        &[],
    );
}

#[test]
fn check_schema_は_sdl_の置き場所が無ければ落とす() {
    let run = Fixture::valid("check-schema-missing").run(&["check-schema"]);
    assert_eq!(run.code, 1, "{run:?}");
    assert!(
        run.stdout
            .starts_with("  NG <root>/specs/application/schema を読めない: "),
        "{run:?}"
    );
    assert!(run.stdout.ends_with("check-schema: 1 件\n"), "{run:?}");
}

// ## check-portfolio と test-receipt
//
// cargo を本当に走らせる。 fixture は依存の無い crate 1つで、試験 binary は2本
// （lib と `tests/extra.rs`）。 lib は通る試験2件と手動の試験1件を持つ。
//
// crate は package と同じ名前のディレクトリ `fx/` に置く。 `test-receipt` は組み立ての
// JSON の `package_id` から名前を取るが、今の cargo の形（`path+file:///…/fx#0.0.0`）では
// ディレクトリの名前を読んでいる。 名前が違うと、どの試験 binary も「登録されていない」になる。

const FX_WORKSPACE: &str = r#"# 外のワークスペースに入らない。
[workspace]
members = ["fx"]
resolver = "2"
"#;

const FX_MANIFEST: &str = r#"[package]
name = "fx"
version = "0.0.0"
edition = "2021"
publish = false
"#;

const FX_LIB: &str = r#"#[cfg(test)]
mod tests {
    #[test]
    fn one() {}

    #[test]
    fn two() {}

    #[test]
    #[ignore = "手動"]
    fn manual() {}
}
"#;

const FX_EXTRA: &str = "#[test]\nfn extra() {}\n";

fn cargo_fixture(name: &str) -> Fixture {
    let fx = Fixture::empty(name);
    fx.write("Cargo.toml", FX_WORKSPACE)
        .write("fx/Cargo.toml", FX_MANIFEST)
        .write("fx/src/lib.rs", FX_LIB)
        .write("fx/tests/extra.rs", FX_EXTRA)
        .write(".gitignore", "target/\nCargo.lock\n");
    fx
}

fn suite(id: &str, runner: &str, target: &str, platforms: &str, extra: &str) -> String {
    format!(
        "[[suite]]\nid = '{id}'\ntitle = '{id}'\nrunner = '{runner}'\npackage = 'fx'\ntarget = '{target}'\nplatforms = {platforms}\n{extra}\n"
    )
}

fn portfolio(suites: &[String]) -> String {
    format!("schema = 'test-portfolio'\n\n{}", suites.concat())
}

const ALL: &str = "['macos', 'linux', 'windows']";

/// この環境（`test-receipt` が見るのと同じ規則で決まる）。
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

/// この環境ではない platform。 対象外の suite を作るのに使う。
fn other_platform() -> &'static str {
    ["macos", "linux", "windows"]
        .into_iter()
        .find(|p| *p != std::env::consts::OS)
        .expect("ほかの platform がある")
}

#[test]
fn check_portfolio_は試験の_target_と登録を突き合わせる() {
    let fx = cargo_fixture("check-portfolio");
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[
            suite("SUITE-FX-001", "cargo", "lib", ALL, "min_cases = 2"),
            suite("SUITE-FX-002", "cargo", "extra", ALL, "min_cases = 1"),
        ]),
    );
    fx.run(&["check-portfolio"]).expect(
        0,
        &[
            "  suite 2 件 / cargo の試験 target 2 個",
            "check-portfolio: ok",
        ],
        &[],
    );

    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[
            suite("SUITE-FX-001", "cargo", "lib", ALL, "min_cases = 2"),
            suite("SUITE-FX-002", "cargo", "lib", "['plan9']", "min_cases = 1"),
            suite("SUITE-FX-003", "cargo", "ghost", ALL, "min_cases = 1"),
            suite("SUITE-FX-004", "bun", "test", ALL, "min_cases = 1"),
            suite("SUITE-FX-005", "cargo", "lib", "[]", ""),
        ]),
    );
    // 形の崩れた登録は数えない。 bun の登録は突き合わせの外。
    fx.run(&["check-portfolio"]).expect(
        1,
        &[
            "  suite 4 件 / cargo の試験 target 2 個",
            "  NG <root>/meta/suites/cargo.toml: [[suite]] の 4 件目に `min_cases` が無い",
            "  NG SUITE-FX-002: platforms の `plan9` を知らない",
            "  NG <root>/meta/suites/cargo.toml: SUITE-FX-005: `min_cases` が無い、または負",
            "  NG fx の extra がどの suite にも登録されていない。`meta/suites/` に足す",
            "  NG fx の lib を複数の suite が名乗っている: SUITE-FX-001, SUITE-FX-002",
            "  NG SUITE-FX-003: fx の ghost という target は無い",
            "check-portfolio: 6 件",
        ],
        &[],
    );
}

/// 受領証を読む。 suite の並びは組み立ての順で決まり、実行ごとに変わりうるので揃える。
fn receipt(fx: &Fixture, runner: &str) -> serde_json::Value {
    let (platform, backend) = platform_and_backend();
    let text = fx.read(&format!(
        "target/receipts/{runner}-{platform}-{backend}.json"
    ));
    let mut v: serde_json::Value = serde_json::from_str(&text).expect("受領証は JSON");
    if let Some(suites) = v["suites"].as_array_mut() {
        suites.sort_by_key(|s| s["suite"].as_str().unwrap_or_default().to_owned());
    }
    v
}

/// 報告の部分だけ。 試験 binary の出力（所要時間を含む）はそのまま流れてくるので、
/// 最後の `test result:` の行と、それに続く libtest の空行より後ろを見る。
fn report_part(stdout: &str) -> String {
    let lines: Vec<&str> = stdout.lines().collect();
    let start = lines
        .iter()
        .rposition(|l| l.starts_with("test result:"))
        .map_or(0, |i| i + 1);
    lines[start..]
        .iter()
        .skip_while(|l| l.is_empty())
        .map(|l| format!("{l}\n"))
        .collect()
}

#[test]
fn test_receipt_は実行した件数で通過と失敗と未実行を分ける() {
    let (platform, backend) = platform_and_backend();
    let fx = cargo_fixture("test-receipt");
    let other = format!("['{}']", other_platform());

    // ## 通過。 対象外の suite は件数を求めず、`not-applicable` として残す。
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[
            suite(
                "SUITE-FX-001",
                "cargo",
                "lib",
                ALL,
                "min_cases = 2\nmanual = 1",
            ),
            suite("SUITE-FX-002", "cargo", "extra", &other, "min_cases = 5"),
        ]),
    );
    fx.git_init();
    fx.commit("base");
    let sha = fx.git(&["rev-parse", "HEAD"]);
    let receipt_line = format!("  受領証: <root>/target/receipts/cargo-{platform}-{backend}.json");
    let run = fx.run(&["test-receipt"]);
    assert_eq!(
        report_part(&run.stdout),
        lines(&[
            &format!("  {platform} / {backend} / cargo: suite 2 件、実行 3 件、無視 1 件"),
            &receipt_line,
            "test-receipt: ok",
        ]),
        "{run:?}"
    );
    assert_eq!(run.code, 0, "{run:?}");
    assert_eq!(
        receipt(&fx, "cargo"),
        serde_json::json!({
            "git": sha,
            "dirty": false,
            "platform": platform,
            "backend": backend,
            "runner": "cargo",
            "suites": [
                {
                    "suite": "SUITE-FX-001", "package": "fx", "target": "lib",
                    "applies": true, "discovered": 3, "passed": 2, "failed": 0, "ignored": 1,
                    "result": "passed", "problems": [],
                },
                {
                    "suite": "SUITE-FX-002", "package": "fx", "target": "extra",
                    "applies": false, "discovered": 1, "passed": 1, "failed": 0, "ignored": 0,
                    "result": "not-applicable", "problems": [],
                },
            ],
        })
    );

    // ## 失敗。 下限に届かない・登録より多い無視・登録の無い試験 binary・
    // 組み立たなかった suite（未実行）。
    fx.write(
        "meta/suites/cargo.toml",
        &portfolio(&[
            suite("SUITE-FX-001", "cargo", "lib", ALL, "min_cases = 5"),
            suite("SUITE-FX-003", "cargo", "ghost", ALL, "min_cases = 1"),
        ]),
    );
    let run = fx.run(&["test-receipt"]);
    assert_eq!(
        report_part(&run.stdout),
        lines(&[
            &format!("  {platform} / {backend} / cargo: suite 1 件、実行 2 件、無視 1 件"),
            &receipt_line,
            "  NG fx の extra がどの suite にも登録されていない",
            "  NG SUITE-FX-003: fx の ghost の試験 binary が組み立たなかった",
            "  NG SUITE-FX-001（fx の lib）: 実行したのが 2 件で、登録の下限 5 件に届かない",
            "  NG SUITE-FX-001（fx の lib）: 1 件を無視した。登録は手動 0 件まで",
            "test-receipt: 4 件",
        ]),
        "{run:?}"
    );
    assert_eq!(run.code, 1, "{run:?}");
    assert_eq!(
        receipt(&fx, "cargo"),
        serde_json::json!({
            "git": sha,
            "dirty": true,
            "platform": platform,
            "backend": backend,
            "runner": "cargo",
            "suites": [
                {
                    "suite": "SUITE-FX-001", "package": "fx", "target": "lib",
                    "applies": true, "discovered": 3, "passed": 2, "failed": 0, "ignored": 1,
                    "result": "failed",
                    "problems": [
                        "実行したのが 2 件で、登録の下限 5 件に届かない",
                        "1 件を無視した。登録は手動 0 件まで",
                    ],
                },
            ],
        })
    );

    // ## 知らない実行器。 1件も数えず、それでも受領証は書く。
    fx.run(&["test-receipt", "--runner", "nope"]).expect(
        1,
        &[
            &format!("  {platform} / {backend} / nope: suite 0 件、実行 0 件、無視 0 件"),
            &format!("  受領証: <root>/target/receipts/nope-{platform}-{backend}.json"),
            "  NG runner の `nope` を知らない（cargo か bun）",
            "  NG 1件も数えられなかった",
            "test-receipt: 2 件",
        ],
        &[],
    );
    assert_eq!(receipt(&fx, "nope")["suites"], serde_json::json!([]));
}
