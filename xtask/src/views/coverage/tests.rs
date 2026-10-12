//! D07b の Requirement Coverage の試験。 fixture の ID は `fix` を領域にする
//! （`check-references` が拾わない綴り）。 実在する meta を読ませる試験が最後に1本ある。

use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::knowledge::design::Design;
use crate::repo::RepoView;

const REQUIREMENTS: &str = "schema = 'requirement-set'
domain = 'fix'
prefix = 'TR-fix'

[[requirement]]
id = 'TR-fix-01'
title = '全部覆われている'
confidence = 'Fact'
statement = '本文'
formalized_as = ['REQ-fix-001']

[[requirement]]
id = 'TR-fix-02'
title = '不適と候補にしか結びつかない'
confidence = 'Fact'
statement = '本文'

[[requirement]]
id = 'TR-fix-03'
title = 'Probe だけ'
confidence = 'Fact'
statement = '本文'

[[requirement]]
id = 'TR-fix-04'
title = '形式だけ'
confidence = 'Fact'
statement = '本文'
formalized_as = ['FB-fix-001']

[[requirement]]
id = 'TR-fix-05'
title = '外部部品は要らないと宣言している。 何も覆っていない'
confidence = 'Fact'
statement = '本文'
needs_component = false

[[requirement]]
id = 'TR-fix-06'
title = '何も覆っていない'
confidence = 'Fact'
statement = '本文'
";

const COMPONENTS: &str = "schema = 'component-ledger'

[[component]]
id = 'CMP-fix-001'
name = '採用'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'DEC-fix-001'
domains = ['fix']
supports_requirements = ['TR-fix-01']

[[component]]
id = 'CMP-fix-002'
name = '不適'
purpose = '用途'
license = 'GPL'
status = '不適'
decided_by = 'DEC-fix-001'
domains = ['fix']
supports_requirements = ['TR-fix-02']

[[component]]
id = 'CMP-fix-003'
name = '採用候補'
purpose = '用途'
license = 'MIT'
status = '採用候補'
domains = ['fix']
supports_requirements = ['TR-fix-02']
";

const DECISION: &str = "schema = 'decision'
id = 'DEC-fix-001'
title = '採否'
status = 'accepted'
owner = 'x'
options = ['a']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
";

/// 部品ではない資源（調査）も `supports_requirements` を張る。 部品の欄には混ざらない。
const EVIDENCE: &str = "schema = 'evidence'
id = 'EVID-fix-001'
title = '調査'
kind = 'measurement'
source = '出典'
provenance = '出どころ'
confidence = 'Fact'
supports_requirements = ['TR-fix-02']
";

const SUITES: &str = "schema = 'test-portfolio'

[[suite]]
id = 'SUITE-fix-001'
title = '試験1'
runner = 'cargo'
package = 'xtask'
target = 'lib'
platforms = ['macos']
min_cases = 1
contracts = ['TR-fix-01', 'TR-fix-03', 'DEC-fix-001']
";

const FSL: &str = "model Fix {
  @requirement(\"REQ-fix-001\", \"本文\")
  forbidden FB-fix-001 \"禁止\"
}
";

fn files() -> BTreeMap<String, String> {
    [
        ("meta/requirements/fix.toml", REQUIREMENTS),
        ("meta/evidence/components.toml", COMPONENTS),
        ("meta/evidence/EVID-fix-001.toml", EVIDENCE),
        ("meta/decisions/DEC-fix-001.toml", DECISION),
        ("meta/suites/cargo.toml", SUITES),
        ("specs/fix.fsl", FSL),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_owned(), b.to_owned()))
    .collect()
}

fn coverage_of(files: &BTreeMap<String, String>) -> Coverage {
    let design = Design::load(&RepoView::in_memory(Path::new("/fixture"), files.clone()));
    Coverage::build(&design, &design.graph())
}

fn id(raw: &str) -> Id {
    raw.parse().expect("ID の形")
}

fn ids(raws: &[&str]) -> Vec<Id> {
    raws.iter().map(|r| id(r)).collect()
}

fn row<'a>(coverage: &'a Coverage, raw: &str) -> &'a RequirementCoverage {
    let wanted = id(raw);
    coverage
        .requirements
        .iter()
        .find(|r| r.requirement == wanted)
        .expect("要件")
}

#[test]
fn 採用の部品だけが_implements_に数えられる() {
    let coverage = coverage_of(&files());
    // 採用の部品は数える。
    assert_eq!(
        row(&coverage, "TR-fix-01").implements.by,
        ids(&["CMP-fix-001"])
    );
    // 不適・採用候補は要件と結びついていても数えない。 別に残る。 調査は部品の欄に混ざらない。
    let r = row(&coverage, "TR-fix-02");
    assert!(r.implements.by.is_empty());
    assert_eq!(r.unadopted, ids(&["CMP-fix-002", "CMP-fix-003"]));
    // 採否の語を採用に変えれば、数えられる。
    let adopted = COMPONENTS.replace("status = '採用候補'", "status = '採用'");
    let mut f = files();
    f.insert("meta/evidence/components.toml".to_owned(), adopted);
    let coverage = coverage_of(&f);
    let r = row(&coverage, "TR-fix-02");
    assert_eq!(r.implements.by, ids(&["CMP-fix-003"]));
    assert_eq!(r.unadopted, ids(&["CMP-fix-002"]));
}

#[test]
fn formalizes_implements_targets_は別々に出る() {
    let coverage = coverage_of(&files());
    let all = row(&coverage, "TR-fix-01");
    assert_eq!(all.formalizes.by, ids(&["REQ-fix-001"]));
    assert_eq!(all.implements.by, ids(&["CMP-fix-001"]));
    assert_eq!(all.targets.by, ids(&["SUITE-fix-001"]));

    // どれか1つだけ覆っている要件は、ほかの関係を覆っているようには見えない。
    let probe_only = row(&coverage, "TR-fix-03");
    assert!(!probe_only.formalizes.is_covered());
    assert!(!probe_only.implements.is_covered());
    assert_eq!(probe_only.targets.by, ids(&["SUITE-fix-001"]));

    let formal_only = row(&coverage, "TR-fix-04");
    assert_eq!(formal_only.formalizes.by, ids(&["FB-fix-001"]));
    assert!(!formal_only.implements.is_covered());
    assert!(!formal_only.targets.is_covered());
    assert!(!formal_only.is_uncovered());
}

#[test]
fn formalizes_は一部の被覆として出る() {
    let coverage = coverage_of(&files());
    let all = row(&coverage, "TR-fix-01");
    assert_eq!(all.formalizes.relation, "formalizes");
    assert!(all.formalizes.partial);
    assert_eq!(all.implements.relation, "implements");
    assert!(!all.implements.partial);
    assert_eq!(all.targets.relation, "targets");
    assert!(!all.targets.partial);
}

#[test]
fn 外部部品が要らないという宣言は被覆を変えない() {
    let coverage = coverage_of(&files());
    // `needs_component = false` でも、書いていなくても、何も覆っていなければ同じ。
    for raw in ["TR-fix-05", "TR-fix-06"] {
        let r = row(&coverage, raw);
        assert!(r.is_uncovered(), "{raw}");
        assert!(r.unadopted.is_empty(), "{raw}");
    }
}

#[test]
fn 件数は関係ごとに数える() {
    let summary = coverage_of(&files()).summary();
    assert_eq!(
        summary,
        CoverageSummary {
            requirements: 6,
            formalized: 2,
            implemented: 1,
            targeted: 2,
            uncovered: 3,
        }
    );
}

#[test]
fn 何も覆われていない要件だけの_repository() {
    let mut f = files();
    for path in [
        "meta/evidence/components.toml",
        "meta/evidence/EVID-fix-001.toml",
        "meta/decisions/DEC-fix-001.toml",
        "meta/suites/cargo.toml",
        "specs/fix.fsl",
    ] {
        f.remove(path);
    }
    // `formalized_as` も外す。 指す先の FSL が無い。
    f.insert(
        "meta/requirements/fix.toml".to_owned(),
        REQUIREMENTS
            .replace("formalized_as = ['REQ-fix-001']\n", "")
            .replace("formalized_as = ['FB-fix-001']\n", ""),
    );
    let coverage = coverage_of(&f);
    assert_eq!(coverage.requirements.len(), 6);
    assert!(coverage.requirements.iter().all(|r| r.is_uncovered()));
    assert!(coverage.requirements.iter().all(|r| r.unadopted.is_empty()));
    let summary = coverage.summary();
    assert_eq!(
        (
            summary.formalized,
            summary.implemented,
            summary.targeted,
            summary.uncovered
        ),
        (0, 0, 0, 6)
    );
}

#[test]
fn 要件が無ければ空() {
    let coverage = coverage_of(&BTreeMap::new());
    assert!(coverage.requirements.is_empty());
    assert_eq!(coverage.summary().uncovered, 0);
}

#[test]
fn 上限の要件も要件として並び_確かめる_probe_を持てる() {
    let budget = "schema = 'budget'
id = 'BUDGET-fix-001'
title = '予算'
limit = 1500
unit = 'MB'
scope = '条件'
";
    let suites = SUITES.replace(
        "contracts = ['TR-fix-01', 'TR-fix-03', 'DEC-fix-001']",
        "contracts = ['BUDGET-fix-001']",
    );
    let mut f = files();
    f.insert(
        "meta/budgets/BUDGET-fix-001.toml".to_owned(),
        budget.to_owned(),
    );
    f.insert("meta/suites/cargo.toml".to_owned(), suites);
    let coverage = coverage_of(&f);
    let r = row(&coverage, "BUDGET-fix-001");
    assert_eq!(r.title.as_deref(), Some("予算"));
    assert_eq!(r.targets.by, ids(&["SUITE-fix-001"]));
}

/// 実在する meta を読ませる。 全要件が1行ずつ載り、3つの関係の件数は graph の edge と一致する。
#[test]
fn 実在する_meta_の全要件が_view_を通る() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let design = Design::load(&RepoView::working_tree(root));
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    let coverage = Coverage::build(&design, &graph);

    // 種類が要件の node が1件も落ちず、重複もしない。
    let mut expected: Vec<Id> = graph
        .nodes()
        .filter(|n| Kind::of(n) == Some(Kind::Requirement))
        .filter_map(|n| match &n.key {
            NodeKey::Object(id) => Some(id.clone()),
            NodeKey::Locator { .. } => None,
        })
        .collect();
    expected.sort();
    let listed: Vec<Id> = coverage
        .requirements
        .iter()
        .map(|r| r.requirement.clone())
        .collect();
    assert!(!listed.is_empty());
    assert_eq!(listed, expected);

    // 関係ごとの被覆の総数が、graph の edge の数と一致する。 要件が受ける edge は、
    // 要件でない相手へ張る関係を除いて取りこぼさない。
    let edges = |name: &str| {
        graph
            .all_relations()
            .filter(|r| {
                r.name == name && graph.node(&r.to).and_then(Kind::of) == Some(Kind::Requirement)
            })
            .count()
    };
    let counted = |axis: fn(&RequirementCoverage) -> &Axis| {
        coverage
            .requirements
            .iter()
            .map(|r| axis(r).by.len())
            .sum::<usize>()
    };
    assert_eq!(counted(|r| &r.formalizes), edges("formalizes"));
    assert_eq!(counted(|r| &r.implements), edges("implements"));
    assert_eq!(counted(|r| &r.targets), edges("targets"));
    // 実在する要件には、形式にされているものも、実現する部品を持つものも、確かめる Probe を持つものもある。
    let summary = coverage.summary();
    assert!(summary.formalized > 0 && summary.implemented > 0 && summary.targeted > 0);
    assert_eq!(summary.requirements, expected.len());
}
