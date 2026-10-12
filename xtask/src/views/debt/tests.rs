//! D07c の Verification Debt の試験。 人の観察が要る仮説と、Portfolio の不足が、種類ごとに
//! 分かれて出ることを見る。

use super::*;
use crate::probe::{Counts, Environment, ProbeDefinition, Receipt, Status};
use crate::verification::{EvidenceStanding, ProbeStanding};
use crate::views::portfolio::Observed;

fn human(hypothesis: &str) -> HumanDebtItem {
    HumanDebtItem {
        hypothesis: hypothesis.into(),
        kind: "semantic-comprehension".into(),
        why_human: vec!["first-contact-comprehension".into()],
        decisions: vec!["DEC-fix-001".into()],
        requirements: vec!["TR-fix-01".into()],
        evidence: vec![EvidenceStanding {
            id: "EVID-fix-001".into(),
            method: Some("agent-review".into()),
            stance: EvidenceStance::Supporting,
            human_qualified: false,
        }],
        origins: 1,
        shortfall: Shortfall::NoHumanObservation,
        probes: vec![ProbeStanding {
            id: "PROBE-P00001".into(),
            method: Some("first-contact-comprehension".into()),
            can_close_shortfall: true,
        }],
    }
}

fn definition(id: &str) -> ProbeDefinition {
    ProbeDefinition {
        id: id.into(),
        runner: "cargo".into(),
        package: "p".into(),
        target: "lib".into(),
        platforms: vec!["macos".into(), "linux".into()],
        backends: vec!["unsupported".into()],
        requires_native: false,
        min_cases: 1,
        manual: 0,
        prerequisites: crate::probe::Prerequisites::default(),
    }
}

fn receipt(probe: &str, platform: &str, status: Status, actual_work: u64) -> Observed {
    Observed {
        source: "r.json".into(),
        receipt: Receipt {
            probe: probe.into(),
            package: "p".into(),
            target: "lib".into(),
            applies: true,
            status,
            counts: Counts {
                discovered: actual_work,
                passed: actual_work,
                failed: 0,
                ignored: 0,
            },
            actual_work,
            problems: Vec::new(),
            environment: Environment::known(platform, "unsupported", "abc", false),
            missing_prerequisites: Vec::new(),
        },
    }
}

#[test]
fn 人の観察が要る仮説と実行の不足が種類ごとに分かれて出る() {
    // X-001 は linux で通り、macos は受領証が無い。 X-002 は linux で 0 件を開いて通り、
    // macos は走らせなかった。
    let portfolio = Portfolio::build(
        &[definition("SUITE-X-001"), definition("SUITE-X-002")],
        &[
            receipt("SUITE-X-001", "linux", Status::Passed, 3),
            receipt("SUITE-X-002", "linux", Status::Passed, 0),
            receipt("SUITE-X-002", "macos", Status::Skipped, 0),
            // 環境不明。
            Observed {
                source: "old.json".into(),
                receipt: Receipt {
                    environment: None,
                    ..receipt("SUITE-X-001", "macos", Status::Passed, 3).receipt
                },
            },
        ],
    );
    let debt = Debt::build(vec![human("HYP-A00001")], &portfolio);
    assert_eq!(debt.human.len(), 1);
    let ids = |v: &[CellDebt]| -> Vec<String> {
        v.iter()
            .map(|c| format!("{}:{}", c.probe, c.cell.platform))
            .collect()
    };
    assert_eq!(ids(&debt.missing), ["SUITE-X-001:macos"]);
    assert_eq!(ids(&debt.skipped), ["SUITE-X-002:macos"]);
    assert_eq!(ids(&debt.false_green), ["SUITE-X-002:linux"]);
    assert_eq!(debt.unplaced.len(), 1);
    assert_eq!(debt.len(), 5);
    // 通ったセルは債務ではない。
    assert!(
        debt.missing
            .iter()
            .chain(&debt.skipped)
            .chain(&debt.false_green)
            .all(|c| c.probe != "SUITE-X-001" || c.cell.platform != "linux")
    );
}

#[test]
fn 落ちたセルは債務に入れない() {
    let portfolio = Portfolio::build(
        &[definition("SUITE-X-001")],
        &[
            receipt("SUITE-X-001", "linux", Status::Failed, 3),
            receipt("SUITE-X-001", "macos", Status::Passed, 3),
        ],
    );
    let debt = Debt::build(Vec::new(), &portfolio);
    assert_eq!(debt.len(), 0);
}

#[test]
fn 債務は仮説の欄のうち記録にあるものだけを出し_優先度を持たない() {
    let item = human("HYP-A00001");
    let text = render_human_debt(std::slice::from_ref(&item));
    for expected in [
        "HYP-A00001",
        "semantic-comprehension",
        "first-contact-comprehension",
        "DEC-fix-001",
        "TR-fix-01",
        "EVID-fix-001",
        "PROBE-P00001",
    ] {
        assert!(text.contains(expected), "{expected} が無い: {text}");
    }
    // 記録に欄が無い項目は、欄があるかのように出さない。
    for absent in ["blast", "reversib", "score"] {
        assert!(!text.contains(absent), "{absent} を出している: {text}");
    }
    assert!(text.contains("優先度は付けない"));
    let json = human_debt_json(&[item]);
    assert_eq!(json["count"], 1);
    assert_eq!(json["human"][0]["hypothesis"], "HYP-A00001");
    assert_eq!(json["human"][0]["has_valid_probe"], true);
    assert!(json["human"][0].get("score").is_none());
    assert!(json["human"][0].get("blast_radius").is_none());
}

#[test]
fn 一覧の出力は種類ごとの節に分かれる() {
    let portfolio = Portfolio::build(
        &[definition("SUITE-X-001")],
        &[receipt("SUITE-X-001", "linux", Status::Passed, 0)],
    );
    let debt = Debt::build(vec![human("HYP-A00001")], &portfolio);
    let text = debt.render_human();
    for section in ["[human]", "[missing]", "[false-green]"] {
        assert!(text.contains(section), "{section} が無い: {text}");
    }
    assert!(!text.contains("[skipped]"));
    let json = debt.to_json();
    assert_eq!(json["human"].as_array().map(Vec::len), Some(1));
    assert_eq!(json["missing"].as_array().map(Vec::len), Some(1));
    assert_eq!(
        json["false_green"][0]["insufficiencies"][0],
        "no-actual-work"
    );
}
