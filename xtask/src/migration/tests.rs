//! 計画 → dry-run → 書き換え → 読み直し を、一時 git リポジトリの上で通して試す。
//!
//! fixture の ID は領域を小文字で書く（`DEC-fix-001`）。 この試験もリポジトリの
//! `check-references` が走査するので、大文字で書くと実在しない ID として落ちる。
//! 一時ディレクトリは `std::env::temp_dir()` の下に作る——`CARGO_TARGET_TMPDIR` は
//! `--lib` の試験では定義されない（`repo/view.rs` の試験と同じ事情）。

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use super::dryrun::{DefaultEquivalence, DryRun, dry_run};
use super::rewrite::fail_rename_at;
use super::*;
use crate::graph::{NodeKind, Registry, RelationClass, RelationRule, SemanticGraph};
use crate::repo::RepoView;

struct Fixture {
    dir: PathBuf,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir()
            .join("xtask-migration-tests")
            .join(name);
        if dir.exists() {
            fs::remove_dir_all(&dir).expect("前の実行の一時ディレクトリを消せる");
        }
        fs::create_dir_all(&dir).expect("一時ディレクトリを作れる");
        let fx = Self { dir };
        fx.git(&["init", "-q"]);
        for (k, v) in [
            ("user.name", "migration-test"),
            ("user.email", "migration-test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            fx.git(&["config", k, v]);
        }
        fx
    }

    /// 判断が2件、FSL が1つ、ソースと文書が1つずつ。 どれも `DEC-fix-001` を引く。
    fn standard(name: &str) -> Self {
        let fx = Self::new(name);
        fx.write(
            "meta/decisions/DEC-fix-001.toml",
            &decision("DEC-fix-001", ""),
        );
        fx.write(
            "meta/decisions/DEC-fix-002.toml",
            &decision("DEC-fix-002", "supersedes = ['DEC-fix-001']"),
        );
        fx.write(
            "specs/a.fsl",
            "  acceptance AC-fix-001 \"見本\" {\n  }\n// DEC-fix-001 を参照\n",
        );
        fx.write("src/lib.rs", "// DEC-fix-001 の判断に従う\n");
        fx.write(
            "docs/note.md",
            "DEC-fix-001 と DEC-fix-001。 DEC-fix-0010 と XDEC-fix-001 は別。\n次の行の DEC-fix-001。\n",
        );
        fx.commit("base");
        fx
    }

    fn root(&self) -> &Path {
        &self.dir
    }

    fn git(&self, args: &[&str]) -> String {
        let out = Command::new("git")
            .current_dir(&self.dir)
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

    fn write(&self, rel: &str, text: &str) {
        let p = self.dir.join(rel);
        fs::create_dir_all(p.parent().expect("親がある")).expect("親を作れる");
        fs::write(p, text).expect("書ける");
    }

    fn read(&self, rel: &str) -> String {
        fs::read_to_string(self.dir.join(rel)).expect("読める")
    }

    fn exists(&self, rel: &str) -> bool {
        self.dir.join(rel).exists()
    }

    fn commit(&self, message: &str) {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
    }

    fn view(&self) -> RepoView {
        RepoView::working_tree(&self.dir)
    }

    /// ディスクの全ファイル（`.git` を除く）と本文。 「何も書いていない」を比べるのに使う。
    fn tree(&self) -> Vec<(String, String)> {
        let mut out: Vec<(String, String)> = self
            .view()
            .walk("")
            .into_iter()
            .map(|rel| {
                let text = self.read(&rel);
                (rel, text)
            })
            .collect();
        out.sort();
        out
    }

    /// 一時ファイルと退避が残っていない。
    fn assert_no_leftovers(&self) {
        let leftovers: Vec<String> = self
            .view()
            .walk("")
            .into_iter()
            .filter(|r| r.contains(".xtask-migrate."))
            .collect();
        assert!(leftovers.is_empty(), "残っている: {leftovers:?}");
    }
}

fn decision(id: &str, extra: &str) -> String {
    format!(
        "schema = 'decision'\nid = '{id}'\ntitle = '{id}'\nstatus = 'accepted'\nowner = 'fixture'\n\
         options = ['a']\nselected = 'a'\nrationale = '…'\nreview_triggers = ['…']\n{extra}\n"
    )
}

fn registry() -> Registry {
    Registry {
        node_kinds: vec![NodeKind {
            schema: "decision",
            kind: "decision",
        }],
        relations: vec![RelationRule {
            from_schema: "decision",
            field: "supersedes",
            relation: "supersedes",
            class: RelationClass::Reference,
        }],
        locators: Vec::new(),
        fsl_node_kind: Some("fsl-requirement"),
    }
}

fn mapping(pairs: &[(&str, &str)]) -> Mapping {
    Mapping::parse(pairs.iter().copied()).expect("fixture の ID は形が正しい")
}

fn make_plan(fx: &Fixture, m: &Mapping) -> MigrationPlan {
    plan(&fx.view(), m, &registry(), PlanOptions::default()).expect("計画を作れる")
}

fn make_dry(fx: &Fixture, p: &MigrationPlan) -> DryRun {
    dry_run(&fx.view(), p, &registry(), &DefaultEquivalence)
}

fn issues_of(fx: &Fixture, m: &Mapping, options: PlanOptions) -> Vec<PlanIssue> {
    plan(&fx.view(), m, &registry(), options).expect_err("計画を作れない")
}

// ## 計画

#[test]
fn 新しい_id_が既存の記録とぶつかったら計画を作らない() {
    let fx = Fixture::standard("collision-existing");
    // 002 は存在し、この対応表で退かない。
    let issues = issues_of(
        &fx,
        &mapping(&[("DEC-fix-001", "DEC-fix-002")]),
        PlanOptions::default(),
    );
    assert!(
        issues.iter().any(
            |i| matches!(i, PlanIssue::NewIdCollision { new, .. } if new.as_str() == "DEC-fix-002")
        ),
        "{issues:?}"
    );
    // FSL の宣言とぶつかる場合も同じ。
    let issues = issues_of(
        &fx,
        &mapping(&[("DEC-fix-001", "AC-fix-001")]),
        PlanOptions::default(),
    );
    assert!(
        issues
            .iter()
            .any(|i| matches!(i, PlanIssue::NewIdCollision { .. })),
        "{issues:?}"
    );
    // 何も書いていない。
    assert!(!fx.exists("meta/decisions/DEC-fix-003.toml"));
}

#[test]
fn 複数の旧_id_が同じ新_id_へ写るのと存在しない旧_id_を理由つきで返す() {
    let fx = Fixture::standard("collision-mapping");
    let issues = issues_of(
        &fx,
        &mapping(&[
            ("DEC-fix-001", "DEC-fix-010"),
            ("DEC-fix-002", "DEC-fix-010"),
            ("DEC-fix-099", "DEC-fix-011"),
        ]),
        PlanOptions::default(),
    );
    assert!(issues.iter().any(|i| matches!(
        i,
        PlanIssue::MappingCollision { new, olds } if new.as_str() == "DEC-fix-010" && olds.len() == 2
    )));
    assert!(
        issues.iter().any(
            |i| matches!(i, PlanIssue::UnknownSource { old } if old.as_str() == "DEC-fix-099")
        )
    );
    // 文面は読める。
    for i in &issues {
        assert!(!i.to_string().is_empty());
    }
}

#[test]
fn 分類できないファイルの言及は断り_許せば計画に載せて書き換えない() {
    let fx = Fixture::standard("unknown-inbound");
    fx.write("data/index.json", "{\n  \"ref\": \"DEC-fix-001\"\n}\n");
    // UTF-8 として読めないバイナリは、言及があっても数えない（書き換えようが無い）。
    let mut blob = b"\xff\xfe".to_vec();
    blob.extend_from_slice(b"DEC-fix-001");
    fs::write(fx.root().join("data/blob.bin"), blob).expect("書ける");
    fx.commit("json");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010")]);

    let issues = issues_of(&fx, &m, PlanOptions::default());
    assert!(
        issues.iter().any(|i| matches!(
            i,
            PlanIssue::UnknownInboundReference { path, line: 2, .. } if path == "data/index.json"
        )),
        "{issues:?}"
    );

    let p = plan(
        &fx.view(),
        &m,
        &registry(),
        PlanOptions {
            allow_unknown_inbound: true,
        },
    )
    .expect("許せば作れる");
    assert_eq!(p.not_rewritten.len(), 1);
    assert!(p.render().contains("書き換えない言及"));

    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok(), "{:?}", dry.issues);
    apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
    assert_eq!(
        fx.read("data/index.json"),
        "{\n  \"ref\": \"DEC-fix-001\"\n}\n",
        "分類できないファイルは触らない"
    );
}

#[test]
fn 同じファイルの複数の出現を拾い_長い_id_の一部は拾わない() {
    let fx = Fixture::standard("multiple-refs");
    let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
    let in_note: Vec<&Edit> = p
        .edits
        .iter()
        .filter(|e| e.path == "docs/note.md")
        .collect();
    // 1行目に2つ、2行目に1つ。 `DEC-fix-0010` と `XDEC-fix-001` は拾わない。
    assert_eq!(
        in_note.iter().map(|e| e.line).collect::<Vec<_>>(),
        [1, 1, 2]
    );
    assert!(in_note[0].col_start < in_note[1].col_start);
    assert!(in_note.iter().all(|e| e.kind == EditKind::Mention));

    let dry = make_dry(&fx, &p);
    assert_eq!(
        dry.rewritten["docs/note.md"],
        "DEC-fix-010 と DEC-fix-010。 DEC-fix-0010 と XDEC-fix-001 は別。\n次の行の DEC-fix-010。\n"
    );
}

#[test]
fn fsl_と_toml_とソースの参照を1つの計画で書き換える() {
    let fx = Fixture::standard("all-kinds");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010"), ("AC-fix-001", "AC-fix-010")]);
    let p = make_plan(&fx, &m);

    let kinds = |path: &str| -> Vec<EditKind> {
        p.edits
            .iter()
            .filter(|e| e.path == path)
            .map(|e| e.kind)
            .collect()
    };
    assert!(kinds("meta/decisions/DEC-fix-001.toml").contains(&EditKind::Definition));
    assert!(kinds("meta/decisions/DEC-fix-002.toml").contains(&EditKind::RelationValue));
    assert!(kinds("specs/a.fsl").contains(&EditKind::FslDeclaration));
    assert!(kinds("specs/a.fsl").contains(&EditKind::Mention));
    assert_eq!(kinds("src/lib.rs"), [EditKind::Mention]);
    assert_eq!(
        p.renames,
        [FileRename {
            from: "meta/decisions/DEC-fix-001.toml".to_owned(),
            to: "meta/decisions/DEC-fix-010.toml".to_owned(),
        }]
    );

    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok(), "{:?}", dry.issues);
    // dry-run は書かない。
    assert!(fx.exists("meta/decisions/DEC-fix-001.toml"));

    let applied = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
    assert!(
        applied
            .removed
            .contains(&"meta/decisions/DEC-fix-001.toml".to_owned())
    );
    assert!(!fx.exists("meta/decisions/DEC-fix-001.toml"));
    assert!(
        fx.read("meta/decisions/DEC-fix-010.toml")
            .contains("id = 'DEC-fix-010'")
    );
    assert!(
        fx.read("meta/decisions/DEC-fix-002.toml")
            .contains("supersedes = ['DEC-fix-010']")
    );
    assert!(fx.read("specs/a.fsl").contains("acceptance AC-fix-010 "));
    assert!(fx.read("specs/a.fsl").contains("// DEC-fix-010 を参照"));
    assert_eq!(fx.read("src/lib.rs"), "// DEC-fix-010 の判断に従う\n");
    fx.assert_no_leftovers();

    // 読み直した graph は、読み込みの失敗も行き先の無い edge も持たない。
    let (graph, issues) = SemanticGraph::build(&fx.view(), &registry());
    assert!(issues.is_empty(), "{issues:?}");
    assert!(graph.validate().is_empty());
}

#[test]
fn 計画は決定的で_render_は並びが固定される() {
    let fx = Fixture::standard("deterministic");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010"), ("AC-fix-001", "AC-fix-010")]);
    let a = make_plan(&fx, &m);
    let b = make_plan(&fx, &m);
    assert_eq!(a.edits, b.edits);
    assert_eq!(a.render(), b.render());

    let keys: Vec<(&str, usize, usize)> = a
        .edits
        .iter()
        .map(|e| (e.path.as_str(), e.line, e.col_start))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort_unstable();
    assert_eq!(keys, sorted);

    let rendered = a.render();
    assert!(rendered.contains("docs/note.md\n  1:1 DEC-fix-001 -> DEC-fix-010 (言及)"));
    assert!(rendered.ends_with("件のファイル名の付け替え\n"));
}

// ## dry-run と書き換え

#[test]
fn 入れ替えは原文だけを見て同時に行う() {
    let fx = Fixture::standard("swap");
    let m = mapping(&[
        ("DEC-fix-001", "DEC-fix-002"),
        ("DEC-fix-002", "DEC-fix-001"),
    ]);
    let p = make_plan(&fx, &m);
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok(), "{:?}", dry.issues);

    apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
    // 旧 002（001 を置き換える側）が 001 になり、旧 001 が 002 になる。
    let new_001 = fx.read("meta/decisions/DEC-fix-001.toml");
    let new_002 = fx.read("meta/decisions/DEC-fix-002.toml");
    assert!(new_001.contains("id = 'DEC-fix-001'"));
    assert!(new_001.contains("supersedes = ['DEC-fix-002']"));
    assert!(new_002.contains("id = 'DEC-fix-002'"));
    assert!(!new_002.contains("supersedes"));
    assert_eq!(fx.read("src/lib.rs"), "// DEC-fix-002 の判断に従う\n");
    fx.assert_no_leftovers();
}

#[test]
fn 連鎖も原文だけを見て1回で写る() {
    let fx = Fixture::standard("chain");
    fx.write(
        "meta/decisions/DEC-fix-003.toml",
        &decision("DEC-fix-003", ""),
    );
    fx.commit("003");
    // 001→002→003 と退いていく。 002 も 003 も、この対応表の旧 ID。
    let m = mapping(&[
        ("DEC-fix-003", "DEC-fix-004"),
        ("DEC-fix-002", "DEC-fix-003"),
        ("DEC-fix-001", "DEC-fix-002"),
    ]);
    let p = make_plan(&fx, &m);
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok(), "{:?}", dry.issues);
    apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
    assert!(
        fx.read("meta/decisions/DEC-fix-004.toml")
            .contains("id = 'DEC-fix-004'")
    );
    assert!(
        fx.read("meta/decisions/DEC-fix-003.toml")
            .contains("supersedes = ['DEC-fix-002']")
    );
    assert_eq!(fx.read("src/lib.rs"), "// DEC-fix-002 の判断に従う\n");
}

#[test]
fn 計画のあとに対象が変わっていたら書かない() {
    let fx = Fixture::standard("stale");
    let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok());

    fx.write("docs/note.md", "計画のあとに書き足した。 DEC-fix-001\n");
    let before = fx.tree();
    let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
    assert_eq!(
        err,
        RewriteError::StalePlan {
            path: "docs/note.md".to_owned()
        }
    );
    assert_eq!(fx.tree(), before, "何も書いていない");
    fx.assert_no_leftovers();
}

#[test]
fn 対象が消えていても古い計画として断る() {
    let fx = Fixture::standard("stale-removed");
    let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
    let dry = make_dry(&fx, &p);
    fs::remove_file(fx.root().join("src/lib.rs")).expect("消せる");
    let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
    assert!(matches!(err, RewriteError::StalePlan { path } if path == "src/lib.rs"));
}

#[test]
fn dry_run_が通らない計画は書かない() {
    let fx = Fixture::standard("failed-dry-run");
    // registry が relation と見なさない欄の言及。 書き換えると内容だけが変わり、
    // rename の差では説明できない。
    fx.write(
        "meta/decisions/DEC-fix-003.toml",
        &decision("DEC-fix-003", "note = 'DEC-fix-001 を見よ'"),
    );
    fx.commit("003");
    let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
    let dry = make_dry(&fx, &p);
    assert!(!dry.is_ok());
    assert!(
        dry.issues.iter().any(|i| matches!(
            i,
            EquivalenceIssue::UnexplainedChange(k) if format!("{k:?}").contains("DEC-fix-003")
        )),
        "{:?}",
        dry.issues
    );

    let before = fx.tree();
    let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
    assert!(matches!(err, RewriteError::DryRunFailed(_)));
    assert_eq!(fx.tree(), before, "何も書いていない");
    fx.assert_no_leftovers();

    // 直して計画からやり直せば通る。
    fx.write(
        "meta/decisions/DEC-fix-003.toml",
        &decision("DEC-fix-003", "supersedes = ['DEC-fix-001']"),
    );
    fx.commit("fix");
    let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok(), "{:?}", dry.issues);
    apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
}

#[test]
fn 未コミットの変更があれば既定では書かない() {
    let fx = Fixture::standard("dirty");
    fx.write("docs/note.md", "未コミット。 DEC-fix-001\n");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010")]);
    // 計画はコミット前の本文で作る。 計画のあとには変わっていない。
    let p = make_plan(&fx, &m);
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok());

    let before = fx.tree();
    let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
    assert_eq!(
        err,
        RewriteError::DirtyWorkingTree {
            paths: vec!["docs/note.md".to_owned()]
        }
    );
    assert_eq!(fx.tree(), before);

    // 外せば書ける。
    apply(
        fx.root(),
        &p,
        &dry,
        &registry(),
        ApplyOptions {
            require_clean: false,
        },
    )
    .expect("require_clean を外せば書ける");
    assert_eq!(fx.read("docs/note.md"), "未コミット。 DEC-fix-010\n");
}

#[test]
fn rename_が途中で失敗したら書く前へ戻す() {
    let fx = Fixture::standard("rollback-rename");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010"), ("AC-fix-001", "AC-fix-010")]);
    let p = make_plan(&fx, &m);
    let dry = make_dry(&fx, &p);
    assert!(dry.is_ok());
    assert!(dry.rewritten.len() >= 4, "複数のファイルを触る");

    let before = fx.tree();
    for fail_at in 0..dry.rewritten.len() {
        fail_rename_at(Some(fail_at));
        let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
        fail_rename_at(None);
        assert!(
            matches!(err, RewriteError::Io { restored: true, .. }),
            "{fail_at} 番目: {err:?}"
        );
        assert_eq!(fx.tree(), before, "{fail_at} 番目で失敗しても書く前と同じ");
        fx.assert_no_leftovers();
    }
    // 注入をやめれば、同じ計画がそのまま通る。
    apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
}

#[test]
fn 読み直しが合わなければ書く前へ戻す() {
    let fx = Fixture::standard("rollback-reload");
    let m = mapping(&[("DEC-fix-001", "DEC-fix-010")]);
    let p = make_plan(&fx, &m);
    let mut dry = make_dry(&fx, &p);
    assert!(dry.is_ok());
    // dry-run が正しいと言いながら、1ファイルだけ書き換えを落とした本文を渡す。
    // 002 は、ここで消える `DEC-fix-001` を指したまま残る。
    let original = fx.read("meta/decisions/DEC-fix-002.toml");
    dry.rewritten
        .insert("meta/decisions/DEC-fix-002.toml".to_owned(), original);

    let before = fx.tree();
    let err = apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).unwrap_err();
    match err {
        RewriteError::ReloadVerifyFailed(issues) => {
            assert!(
                issues
                    .iter()
                    .any(|i| matches!(i, EquivalenceIssue::UnresolvedEdge { .. })),
                "{issues:?}"
            );
        }
        other => panic!("読み直しで落ちるはず: {other:?}"),
    }
    assert_eq!(fx.tree(), before, "書く前へ戻した");
    fx.assert_no_leftovers();
}

#[test]
fn 読み込めない状態の_meta_では計画を作らない() {
    let fx = Fixture::standard("broken-meta");
    fx.write("meta/decisions/broken.toml", "これは toml ではない = = =\n");
    let issues = issues_of(
        &fx,
        &mapping(&[("DEC-fix-001", "DEC-fix-010")]),
        PlanOptions::default(),
    );
    assert!(issues.iter().any(|i| matches!(i, PlanIssue::Load(_))));
}

#[test]
fn 収集ファイルの記録はファイル名を付け替えない() {
    let fx = Fixture::new("collection");
    fx.write(
        "meta/requirements/fix.toml",
        "schema = 'requirement-set'\n\n[[requirement]]\nid = 'TR-fix-01'\ntitle = '1件目'\nconfidence = 'Fact'\nstatement = '…'\n",
    );
    fx.commit("base");
    let reg = Registry {
        node_kinds: vec![NodeKind {
            schema: "requirement-set",
            kind: "requirement",
        }],
        ..Registry::default()
    };
    let m = mapping(&[("TR-fix-01", "TR-fix-02")]);
    let p = plan(&fx.view(), &m, &reg, PlanOptions::default()).expect("作れる");
    assert!(p.renames.is_empty());
    let dry = dry_run(&fx.view(), &p, &reg, &DefaultEquivalence);
    assert!(dry.is_ok(), "{:?}", dry.issues);
    apply(fx.root(), &p, &dry, &reg, ApplyOptions::default()).expect("書ける");
    assert!(
        fx.read("meta/requirements/fix.toml")
            .contains("id = 'TR-fix-02'")
    );
}

#[test]
fn 作業ツリーの_symlink_は辿らない() {
    #[cfg(unix)]
    {
        let fx = Fixture::standard("symlink");
        std::os::unix::fs::symlink("docs/note.md", fx.root().join("LINK.md")).expect("張れる");
        fx.commit("link");
        let p = make_plan(&fx, &mapping(&[("DEC-fix-001", "DEC-fix-010")]));
        assert!(p.edits.iter().all(|e| e.path != "LINK.md"));
        let dry = make_dry(&fx, &p);
        apply(fx.root(), &p, &dry, &registry(), ApplyOptions::default()).expect("書ける");
        assert!(
            fs::symlink_metadata(fx.root().join("LINK.md"))
                .expect("ある")
                .file_type()
                .is_symlink(),
            "symlink のまま"
        );
    }
}
