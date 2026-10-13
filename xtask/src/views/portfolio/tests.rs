//! D07c の Portfolio の試験。 受領証は値で組み、false green になる形を1つずつ見る。
//! 実在する `meta/suites/` を読ませる試験が最後に1本ある。

use std::path::Path;

use super::*;
use crate::diagnostic::Report;
use crate::knowledge::{KnowledgeSnapshot, load};
use crate::probe::{Counts, Prerequisites};

fn probe(
    id: &str,
    platforms: &[&str],
    backends: &[&str],
    requires_native: bool,
) -> ProbeDefinition {
    ProbeDefinition {
        id: id.into(),
        runner: "cargo".into(),
        package: "p".into(),
        target: "lib".into(),
        platforms: platforms.iter().map(|s| (*s).to_owned()).collect(),
        backends: backends.iter().map(|s| (*s).to_owned()).collect(),
        requires_native,
        min_cases: 3,
        manual: 0,
        prerequisites: Prerequisites::default(),
    }
}

/// どこでも数える Probe。
fn everywhere(id: &str) -> ProbeDefinition {
    probe(
        id,
        &["macos", "linux", "windows"],
        &["native", "unsupported"],
        false,
    )
}

fn env(platform: &str, backend: &str) -> Option<Environment> {
    Environment::known(platform, backend, "0123abcdef", false)
}

/// 通った受領証。 実行 5 件、見つけたのも 5 件。
fn passed(probe: &str, platform: &str, backend: &str) -> Receipt {
    Receipt {
        probe: probe.into(),
        package: "p".into(),
        target: "lib".into(),
        applies: true,
        status: Status::Passed,
        counts: Counts {
            discovered: 5,
            passed: 5,
            failed: 0,
            ignored: 0,
        },
        actual_work: 5,
        problems: Vec::new(),
        environment: env(platform, backend),
        missing_prerequisites: Vec::new(),
    }
}

fn seen(source: &str, receipt: Receipt) -> Observed {
    Observed {
        source: source.into(),
        receipt,
    }
}

fn state_of(p: &Portfolio, probe: &str, platform: &str, backend: &str) -> CellState {
    p.cells()
        .find(|(row, c)| row.probe == probe && c.platform == platform && c.backend == backend)
        .unwrap_or_else(|| panic!("{probe} の {platform}/{backend} のセルが無い"))
        .1
        .state
}

fn reasons_of(p: &Portfolio, probe: &str, platform: &str, backend: &str) -> Vec<Insufficiency> {
    p.cells()
        .find(|(row, c)| row.probe == probe && c.platform == platform && c.backend == backend)
        .expect("セルがある")
        .1
        .insufficiencies
        .clone()
}

#[test]
fn 受領証が無ければ作れる環境のセルは_missing_で作れない組は_not_applicable() {
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[]);
    // platforms × backends の6個。
    assert_eq!(p.summary().cells, 6);
    for (platform, backend) in [
        ("macos", "native"),
        ("linux", "unsupported"),
        ("windows", "unsupported"),
    ] {
        assert_eq!(
            state_of(&p, "SUITE-X-001", platform, backend),
            CellState::Missing
        );
    }
    // Linux と Windows の native は、どの環境も作らない。 macOS で書いていない OS の席を
    // 強制した組は作れるが、受領証を求めない。
    for (platform, backend) in [
        ("linux", "native"),
        ("windows", "native"),
        ("macos", "unsupported"),
    ] {
        assert_eq!(
            state_of(&p, "SUITE-X-001", platform, backend),
            CellState::NotApplicable
        );
    }
    let s = p.summary();
    assert_eq!((s.missing, s.not_applicable, s.passed), (3, 3, 0));
}

#[test]
fn 求めない組でも届いた受領証は判定する() {
    let mut zero = passed("SUITE-X-001", "macos", "unsupported");
    zero.actual_work = 0;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("a.json", zero)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "unsupported"),
        CellState::Insufficient
    );
    assert!(p.unplaced.is_empty());

    let mut failed = passed("SUITE-X-001", "macos", "unsupported");
    failed.status = Status::Failed;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("a.json", failed)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "unsupported"),
        CellState::Failed
    );
}

#[test]
fn 通って仕事をして床を満たした受領証だけが合格() {
    let p = Portfolio::build(
        &[everywhere("SUITE-X-001")],
        &[seen("a.json", passed("SUITE-X-001", "macos", "native"))],
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Passed
    );
    let cell = p
        .cells()
        .find(|(_, c)| c.state == CellState::Passed)
        .expect("合格のセル")
        .1;
    let w = cell.witness.as_ref().expect("受領証を持つ");
    assert_eq!(
        (w.source.as_str(), w.git_sha.as_str()),
        ("a.json", "0123abcdef")
    );
    assert_eq!(p.summary().passed, 1);
}

#[test]
fn 落ちた受領証は_failed() {
    let mut r = passed("SUITE-X-001", "linux", "unsupported");
    r.status = Status::Failed;
    r.counts.failed = 2;
    r.counts.passed = 3;
    r.problems = vec!["2 件が失敗した".into()];
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("l.json", r)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "unsupported"),
        CellState::Failed
    );
}

#[test]
fn 走らせなかった_native_は_skipped_で合格に数えない() {
    for status in [
        Status::Skipped,
        Status::NotRun(NotRunReason::MissingPrerequisite),
        Status::NotRun(NotRunReason::BuildFailed),
    ] {
        let mut r = passed("SUITE-X-001", "macos", "native");
        r.status = status;
        r.counts = Counts::default();
        r.actual_work = 0;
        let p = Portfolio::build(
            &[probe("SUITE-X-001", &["macos"], &["native"], true)],
            &[seen("m.json", r)],
        );
        assert_eq!(
            state_of(&p, "SUITE-X-001", "macos", "native"),
            CellState::Skipped,
            "{status:?}"
        );
        assert_eq!(p.summary().passed, 0);
    }
}

#[test]
fn 実際の仕事が0件の合格は合格に数えない() {
    let mut r = passed("SUITE-X-001", "macos", "native");
    r.counts = Counts {
        discovered: 4,
        passed: 0,
        failed: 0,
        ignored: 4,
    };
    r.actual_work = 0;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("m.json", r)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Insufficient
    );
    assert_eq!(
        reasons_of(&p, "SUITE-X-001", "macos", "native"),
        vec![Insufficiency::NoActualWork { ignored: 4 }]
    );
    assert_eq!(p.summary().passed, 0);
}

#[test]
fn 見つけた件数が床に届かない合格は合格に数えない() {
    let mut r = passed("SUITE-X-001", "macos", "native");
    // 床は 3 件。 2 件しか見つけず、2 件通った。
    r.counts = Counts {
        discovered: 2,
        passed: 2,
        failed: 0,
        ignored: 0,
    };
    r.actual_work = 2;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("m.json", r)]);
    assert_eq!(
        reasons_of(&p, "SUITE-X-001", "macos", "native"),
        vec![Insufficiency::BelowMinCases {
            discovered: 2,
            executed: 2,
            min_cases: 3
        }]
    );
    assert_eq!(p.summary().insufficient, 1);
    assert_eq!(p.summary().passed, 0);
}

#[test]
fn native_が要る_probe_を_native_でない_backend_で走らせた合格は合格に数えない() {
    let native = probe(
        "SUITE-X-001",
        &["macos", "linux"],
        &["native", "unsupported"],
        true,
    );
    let p = Portfolio::build(
        std::slice::from_ref(&native),
        &[seen(
            "l.json",
            passed("SUITE-X-001", "linux", "unsupported"),
        )],
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "unsupported"),
        CellState::Insufficient
    );
    assert_eq!(
        reasons_of(&p, "SUITE-X-001", "linux", "unsupported"),
        vec![Insufficiency::NativeRequired]
    );
    // 走らせていなければ、native でない backend の受領証は求めない。
    let p = Portfolio::build(&[native], &[]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "unsupported"),
        CellState::NotApplicable
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Missing
    );
    // native が macOS にしか無いので、`linux/native` も求めない。
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "native"),
        CellState::NotApplicable
    );
}

#[test]
fn 前提が無かったと言う合格は合格に数えない() {
    let mut r = passed("SUITE-X-001", "macos", "native");
    r.missing_prerequisites = vec!["fixtures/voice.wav".into(), "models/mfa".into()];
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("m.json", r)]);
    assert_eq!(
        reasons_of(&p, "SUITE-X-001", "macos", "native"),
        vec![Insufficiency::MissingPrerequisite(vec![
            "fixtures/voice.wav".into(),
            "models/mfa".into()
        ])]
    );
    assert_eq!(p.summary().passed, 0);
}

#[test]
fn 理由は重なって全部出る() {
    let mut r = passed("SUITE-X-001", "linux", "unsupported");
    r.counts = Counts::default();
    r.actual_work = 0;
    r.missing_prerequisites = vec!["fixtures/voice.wav".into()];
    let native = probe("SUITE-X-001", &["linux"], &["unsupported"], true);
    let p = Portfolio::build(&[native], &[seen("l.json", r)]);
    let codes: Vec<&str> = reasons_of(&p, "SUITE-X-001", "linux", "unsupported")
        .iter()
        .map(Insufficiency::code)
        .collect();
    assert_eq!(
        codes,
        ["native-required", "no-actual-work", "missing-prerequisite"]
    );
}

#[test]
fn 環境が不明な古い受領証はセルを埋めず_合格にも数えない() {
    let mut old = passed("SUITE-X-001", "macos", "native");
    old.environment = None;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("old.json", old)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Missing
    );
    assert_eq!(p.summary().passed, 0);
    assert_eq!(p.unplaced.len(), 1);
    assert_eq!(p.unplaced[0].reason, UnplacedReason::UnknownEnvironment);
    assert_eq!(p.unplaced[0].source, "old.json");
    assert_eq!(p.summary().unplaced, 1);
}

#[test]
fn 複数の受領証を重ねると各セルが正しく埋まる() {
    let probes = [everywhere("SUITE-X-001"), everywhere("SUITE-X-002")];
    let mac = [
        seen("mac/cargo.json", passed("SUITE-X-001", "macos", "native")),
        // macOS の native では、X-002 は 0 件で通った。
        seen("mac/cargo.json", {
            let mut r = passed("SUITE-X-002", "macos", "native");
            r.counts = Counts::default();
            r.actual_work = 0;
            r
        }),
    ];
    let linux = [
        seen(
            "linux/cargo.json",
            passed("SUITE-X-001", "linux", "unsupported"),
        ),
        seen(
            "linux/cargo.json",
            passed("SUITE-X-002", "linux", "unsupported"),
        ),
    ];
    let both: Vec<Observed> = mac.iter().chain(linux.iter()).cloned().collect();
    let p = Portfolio::build(&probes, &both);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Passed
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "unsupported"),
        CellState::Passed
    );
    assert_eq!(
        state_of(&p, "SUITE-X-002", "macos", "native"),
        CellState::Insufficient
    );
    assert_eq!(
        state_of(&p, "SUITE-X-002", "linux", "unsupported"),
        CellState::Passed
    );
    // Windows の artifact は渡していない。
    assert_eq!(
        state_of(&p, "SUITE-X-001", "windows", "unsupported"),
        CellState::Missing
    );
    // 渡す順には依らない。
    let reversed: Vec<Observed> = both.iter().rev().cloned().collect();
    assert_eq!(Portfolio::build(&probes, &reversed), p);
    let s = p.summary();
    assert_eq!((s.passed, s.insufficient), (3, 1));
}

#[test]
fn 同じセルに複数届けば落ちたものを残し_合格は不足に勝つ() {
    let probes = [everywhere("SUITE-X-001")];
    let mut zero = passed("SUITE-X-001", "macos", "native");
    zero.counts = Counts::default();
    zero.actual_work = 0;
    let good = passed("SUITE-X-001", "macos", "native");
    let mut bad = passed("SUITE-X-001", "macos", "native");
    bad.status = Status::Failed;

    let p = Portfolio::build(
        &probes,
        &[seen("a.json", zero.clone()), seen("b.json", good.clone())],
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Passed
    );
    let p = Portfolio::build(
        &probes,
        &[
            seen("a.json", good),
            seen("b.json", bad),
            seen("c.json", zero),
        ],
    );
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Failed
    );
    let cell = p
        .cells()
        .find(|(_, c)| c.state == CellState::Failed)
        .expect("ある")
        .1;
    assert_eq!(cell.received, 3);
}

#[test]
fn 登録は対象と言う環境で対象外と言う受領証は合格にも対象外にも数えない() {
    let mut r = passed("SUITE-X-001", "linux", "unsupported");
    r.status = Status::NotApplicable;
    r.applies = false;
    r.counts = Counts::default();
    r.actual_work = 0;
    let p = Portfolio::build(&[everywhere("SUITE-X-001")], &[seen("l.json", r)]);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "linux", "unsupported"),
        CellState::Insufficient
    );
    assert_eq!(
        reasons_of(&p, "SUITE-X-001", "linux", "unsupported"),
        vec![Insufficiency::ApplicabilityDisagrees]
    );
}

#[test]
fn 対象外の環境の対象外の受領証は黙って捨てる() {
    // macOS だけの Probe を Linux で走らせると、対象外の受領証が届く。 それは普通。
    let mut r = passed("SUITE-X-001", "linux", "unsupported");
    r.status = Status::NotApplicable;
    r.applies = false;
    r.counts = Counts::default();
    r.actual_work = 0;
    let p = Portfolio::build(
        &[probe("SUITE-X-001", &["macos"], &["native"], true)],
        &[seen("l.json", r)],
    );
    assert!(p.unplaced.is_empty());
    assert_eq!(p.summary().cells, 1);
    assert_eq!(
        state_of(&p, "SUITE-X-001", "macos", "native"),
        CellState::Missing
    );
}

#[test]
fn セルに置けない受領証は理由を付けて分ける() {
    let macos_only = probe("SUITE-X-001", &["macos"], &["native"], true);
    // 登録に無い Probe。
    let stranger = passed("SUITE-X-999", "macos", "native");
    // どの環境も作らない組。
    let impossible = passed("SUITE-X-001", "linux", "native");
    // 対象にしていない環境で落ちた。
    let mut outside = passed("SUITE-X-001", "linux", "unsupported");
    outside.status = Status::Failed;
    outside.applies = false;
    let p = Portfolio::build(
        &[macos_only],
        &[
            seen("a.json", stranger),
            seen("b.json", impossible),
            seen("c.json", outside),
        ],
    );
    let reasons: Vec<(&str, UnplacedReason)> = p
        .unplaced
        .iter()
        .map(|u| (u.source.as_str(), u.reason))
        .collect();
    assert_eq!(
        reasons,
        [
            ("b.json", UnplacedReason::UnreachableEnvironment),
            ("c.json", UnplacedReason::OutsideDeclaration),
            ("a.json", UnplacedReason::UnknownProbe),
        ]
    );
    assert_eq!(p.summary().passed, 0);
}

#[test]
fn 要約はセルの数を状態ごとに数える() {
    let probes = [everywhere("SUITE-X-001")];
    let p = Portfolio::build(
        &probes,
        &[seen("m.json", passed("SUITE-X-001", "macos", "native"))],
    );
    let s = p.summary();
    assert_eq!(s.cells, 6);
    assert_eq!(
        s.passed + s.failed + s.skipped + s.not_applicable + s.missing + s.insufficient,
        s.cells
    );
    assert!(s.line().contains("executed-and-passed 1"));
    // Linux と Windows の unsupported。 macOS の unsupported は求めない。
    assert_eq!(s.to_json()["missing"], 2);
}

#[test]
fn 出力は値から作れる() {
    let p = Portfolio::build(
        &[everywhere("SUITE-X-001")],
        &[seen("m.json", passed("SUITE-X-001", "macos", "native"))],
    );
    let json = p.to_json();
    assert_eq!(json["summary"]["executed-and-passed"], 1);
    assert_eq!(
        json["probes"][0]["cells"][0]["state"],
        "executed-and-passed"
    );
    assert_eq!(json["probes"][0]["cells"][0]["witness"]["source"], "m.json");
    let text = p.render_human();
    assert!(text.contains("SUITE-X-001"));
    assert!(text.contains("macos"));
    assert!(text.contains("missing"));
}

/// 実在する登録を読んで、受領証が空なら全セルが missing か not-applicable になる。
#[test]
fn 実在する_suites_で空の受領証から_portfolio_が作れる() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let mut rep = Report::default();
    let entries = load(root, &mut rep);
    let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
    let defs = definitions(&entries, &snapshot, &mut rep);
    assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
    assert!(!defs.is_empty(), "suite が1件も読めない");
    let p = Portfolio::build(&defs, &[]);
    let s = p.summary();
    assert_eq!(s.probes, defs.len());
    assert!(s.cells >= s.probes);
    assert_eq!(
        s.missing + s.not_applicable,
        s.cells,
        "受領証が無いのに埋まった"
    );
    assert!(s.missing > 0);
    assert_eq!((s.passed, s.insufficient, s.unplaced), (0, 0, 0));
    // native が要る suite は、native でない backend のセルを持たない。
    for row in p.probes.iter().filter(|r| r.requires_native) {
        assert!(row.cells.iter().all(|c| c.backend == "native"), "{row:?}");
    }
}
