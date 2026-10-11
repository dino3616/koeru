//! D00 の試験。 fixture の ID は `fix` を領域にする（`check-references` が拾わない綴り）。
//! 実在する meta を読ませる試験が最後に1本ある。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::legacy::{CONTRACT_SCHEMA, Role, spec};
use super::*;
use crate::diagnostic::Report;
use crate::graph::{Direction, GraphIssue, NodeKey, RelationClass};
use crate::knowledge::model::{Entry, SHAPES, Shape};
use crate::knowledge::{Id, KnowledgeSnapshot, fsl_ids, load};
use crate::repo::RepoView;

const REQUIREMENTS: &str = "schema = 'requirement-set'
domain = 'fix'
prefix = 'TR-fix'

[[requirement]]
id = 'TR-fix-01'
title = '1件目'
confidence = 'Fact'
statement = '本文'
formalized_as = ['REQ-fix-001', 'FB-fix-001']

[[requirement]]
id = 'TR-fix-02'
title = '2件目'
confidence = 'Assumption'
needs_component = false
statement = '本文2'
depends_on = ['TR-fix-01']
notes = ['注']
";

const DECISION_1: &str = "schema = 'decision'
id = 'DEC-fix-001'
title = '1つ目'
status = 'superseded'
owner = 'x'
options = ['a', 'b']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
superseded_by = 'DEC-fix-002'
evidence = ['EVID-fix-001']
affects_requirements = ['TR-fix-01']
affects_targets = ['TGT-fix-001']
affects_components = ['CMP-fix-001']
constraint_label = '名'
constraint = '本文'
";

const DECISION_2: &str = "schema = 'decision'
id = 'DEC-fix-002'
title = '2つ目'
status = 'accepted'
owner = 'x'
options = ['a']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
supersedes = ['DEC-fix-001']
";

const QUESTION: &str = "schema = 'question'
id = 'Q-fix-001'
title = '問い'
status = 'closed'
owner = 'x'
why_it_matters = 'なぜ'
how_to_close = 'どう'
closed_by = 'DEC-fix-001'
blocks_profiles = ['PROFILE-fix-1']
";

const EVIDENCE: &str = "schema = 'evidence'
id = 'EVID-fix-001'
title = '調査'
kind = 'measurement'
source = '出典'
provenance = '出どころの説明'
confidence = 'Fact'
supports_decisions = ['DEC-fix-001']
supports_requirements = ['TR-fix-01']
finding = '調べたことごとに違う自由記述'
";

const EVIDENCE_2: &str = "schema = 'evidence'
id = 'EVID-fix-002'
title = '2件目の調査'
kind = 'measurement'
source = '出典'
provenance = '出どころ'
confidence = 'Fact'
supports_decisions = ['DEC-fix-002', 'DEC-fix-001']
";

const COMPONENTS: &str = "schema = 'component-ledger'

[[component]]
id = 'CMP-fix-001'
name = '部品'
purpose = '用途'
license = 'MIT'
status = '採用'
decided_by = 'DEC-fix-001'
domains = ['fix']
supports_requirements = ['TR-fix-01']

[[component]]
id = 'CMP-fix-002'
name = '別の部品'
purpose = '用途'
license = 'GPL'
status = '不適'
supports_requirements = ['TR-fix-02']
";

const BUDGET: &str = "schema = 'budget'
id = 'BUDGET-fix-001'
title = '予算'
limit = 1500
unit = 'MB'
scope = '条件'
derives_from = ['SCALE-fix-001']
source_requirements = ['TR-fix-01']

[[allocations]]
name = '共通'
kind = 'step'
value = 400
measured = false

[[allocations]]
item = '収録中'
kind = 'step'
mode = '収録中'
value = 512
measured = true
stated = '512MB'
";

const SCALE: &str = "schema = 'scale-reference'
id = 'SCALE-fix-001'
title = '代表'
basis = '定義'
scope = '範囲'
rationale = '選んだ理由'
source_targets = ['TGT-fix-001']

[[derived]]
name = '総行数'
value = 444
unit = '行'
formula = '148 × 3'
measured = true

[[derived]]
name = '総容量'
unit = 'GB'
formula = '式'
measured = false
";

const TARGETS: &str = "schema = 'target-set'
domain = 'fix'

[[target]]
id = 'TGT-fix-001'
item = '押下から最初の音'
goal = '6秒'
decided_by = 'DEC-fix-001'
derives_from = ['SCALE-fix-001']
";

const PROFILE: &str = "schema = 'profile'
id = 'PROFILE-fix-1'
title = '対象'
status = 'planned'
includes_requirements = ['TR-fix-01']
includes_fsl = ['REQ-fix-001']
decisions = ['DEC-fix-001']
budgets = ['BUDGET-fix-001']
excludes = ['x']
";

const SUITES: &str = "schema = 'test-portfolio'

[[suite]]
id = 'SUITE-fix-001'
title = '試験'
runner = 'cargo'
package = 'xtask'
target = 'lib'
platforms = ['macos']
min_cases = 3
manual = 1
contracts = ['TR-fix-01', 'REQ-fix-001', 'DEC-fix-001']
";

const FSL: &str = "model Fix {
  @requirement(\"REQ-fix-001\", \"本文\")
  forbidden FB-fix-001 \"禁止\"
  acceptance AC-fix-001 \"受入\"
}
";

fn files() -> BTreeMap<String, String> {
    [
        ("meta/requirements/fix.toml", REQUIREMENTS),
        ("meta/decisions/DEC-fix-001.toml", DECISION_1),
        ("meta/decisions/DEC-fix-002.toml", DECISION_2),
        ("meta/questions/Q-fix-001.toml", QUESTION),
        ("meta/evidence/EVID-fix-001.toml", EVIDENCE),
        ("meta/evidence/EVID-fix-002.toml", EVIDENCE_2),
        ("meta/evidence/components.toml", COMPONENTS),
        ("meta/budgets/BUDGET-fix-001.toml", BUDGET),
        ("meta/budgets/SCALE-fix-001.toml", SCALE),
        ("meta/budgets/fix.toml", TARGETS),
        ("meta/profiles/PROFILE-fix-1.toml", PROFILE),
        ("meta/suites/cargo.toml", SUITES),
        ("specs/fix.fsl", FSL),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_owned(), b.to_owned()))
    .collect()
}

fn design_of(files: &BTreeMap<String, String>) -> Design {
    Design::load(&RepoView::in_memory(Path::new("/fixture"), files.clone()))
}

fn base() -> Design {
    design_of(&files())
}

fn key(raw: &str) -> NodeKey {
    NodeKey::Object(raw.parse().expect("ID の形"))
}

fn id(raw: &str) -> Id {
    raw.parse().expect("ID の形")
}

/// fixture の全 object が、target の種類を持って graph に載る。 問題は1件も出ない。
#[test]
fn fixture_の全_object_が_target_の種類で_graph_に載る() {
    let design = base();
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    let kind_of = |raw: &str| graph.node(&key(raw)).map(|n| n.kind);
    for (raw, kind) in [
        ("TR-fix-01", "requirement"),
        ("BUDGET-fix-001", "requirement"),
        ("DEC-fix-001", "decision"),
        ("Q-fix-001", "question"),
        ("EVID-fix-001", "evidence"),
        ("SCALE-fix-001", "workload"),
        ("CMP-fix-001", "dependency"),
        ("SUITE-fix-001", "probe"),
        ("TGT-fix-001", "unclassified-target"),
        ("PROFILE-fix-1", "release-profile"),
        ("REQ-fix-001", "formal-contract"),
        ("FB-fix-001", "formal-contract"),
        ("AC-fix-001", "formal-contract"),
    ] {
        assert_eq!(kind_of(raw), Some(kind), "{raw}");
    }
}

fn edges(graph: &crate::graph::SemanticGraph, from: &str) -> Vec<(&'static str, String)> {
    graph
        .outgoing(&key(from))
        .iter()
        .map(|r| {
            let NodeKey::Object(to) = &r.to else {
                panic!("locator は無い")
            };
            (r.name, to.to_string())
        })
        .collect()
}

/// 欄は正規の名前の関係になる。 向きと名前が target の語彙のとおり。
#[test]
fn legacy_の欄は正規の関係として引ける() {
    let graph = base().graph();
    // formalizes は contract から REQ へ。 TR の `formalized_as` の向きではない。
    assert_eq!(
        edges(&graph, "REQ-fix-001"),
        [("formalizes", "TR-fix-01".to_owned())]
    );
    assert_eq!(
        edges(&graph, "FB-fix-001"),
        [("formalizes", "TR-fix-01".to_owned())]
    );
    assert!(edges(&graph, "AC-fix-001").is_empty());
    assert!(
        edges(&graph, "TR-fix-01").is_empty(),
        "REQ 側に逆向きの edge を持たない"
    );
    // 古い名前（closed_by）も resolved_by。
    assert_eq!(
        edges(&graph, "Q-fix-001"),
        [
            ("resolved_by", "DEC-fix-001".to_owned()),
            ("blocks_profiles", "PROFILE-fix-1".to_owned())
        ]
    );
    // `evidence` は considers、`affects_*` の族は affects にまとまる。
    let dec1 = edges(&graph, "DEC-fix-001");
    assert!(dec1.contains(&("considers", "EVID-fix-001".to_owned())));
    for to in ["TR-fix-01", "TGT-fix-001", "CMP-fix-001"] {
        assert!(dec1.contains(&("affects", to.to_owned())), "{to}");
    }
    // 置き換えは新しい側だけが edge を持つ。 `superseded_by` の写しは二重にしない。
    assert_eq!(dec1.iter().filter(|(n, _)| *n == "supersedes").count(), 0);
    assert!(edges(&graph, "DEC-fix-002").contains(&("supersedes", "DEC-fix-001".to_owned())));
    assert_eq!(
        graph
            .supersession_chain(&key("DEC-fix-002"))
            .expect("循環しない"),
        [key("DEC-fix-002"), key("DEC-fix-001")]
    );
    assert_eq!(
        graph.outgoing(&key("DEC-fix-002"))[0].class,
        RelationClass::Supersession
    );
    // 採用の部品だけが REQ を実現する。 採否を決めた判断は仮の名前のまま。
    let cmp = edges(&graph, "CMP-fix-001");
    assert!(cmp.contains(&("implements", "TR-fix-01".to_owned())));
    assert!(cmp.contains(&("decided_by", "DEC-fix-001".to_owned())));
    assert!(!cmp.iter().any(|(n, _)| *n == "supports_requirements"));
    // 不適の部品からは implements が出ない。 結びつきは仮の名前で残る。
    assert_eq!(
        edges(&graph, "CMP-fix-002"),
        [("supports_requirements", "TR-fix-02".to_owned())]
    );
    // 調査が支える判断は、判断から調査への considers になる。 判断が `evidence` で既に挙げている
    // 調査は二重にならない。
    assert_eq!(
        dec1.iter()
            .filter(|(n, t)| *n == "considers" && t == "EVID-fix-001")
            .count(),
        1
    );
    let dec2 = edges(&graph, "DEC-fix-002");
    assert!(dec2.contains(&("considers", "EVID-fix-002".to_owned())));
    assert!(dec1.contains(&("considers", "EVID-fix-002".to_owned())));
    // 調査側には逆向きの edge を持たない。 要件への結びつきだけが仮の名前で残る。
    assert_eq!(
        edges(&graph, "EVID-fix-001"),
        [("supports_requirements", "TR-fix-01".to_owned())]
    );
    assert!(edges(&graph, "EVID-fix-002").is_empty());
    // 要件を指す contracts だけが targets。 判断・FSL の contract は仮の名前のまま。
    let probe = edges(&graph, "SUITE-fix-001");
    assert!(probe.contains(&("targets", "TR-fix-01".to_owned())));
    assert!(probe.contains(&("contracts", "REQ-fix-001".to_owned())));
    assert!(probe.contains(&("contracts", "DEC-fix-001".to_owned())));
    assert_eq!(probe.len(), 3);
    // TR の depends_on は depends_on。
    assert_eq!(
        edges(&graph, "TR-fix-02"),
        [("depends_on", "TR-fix-01".to_owned())]
    );
}

/// 形式 contract との関係は、完全な被覆を意味しない。
#[test]
fn formalizes_は部分的な被覆として読まれる() {
    let rule = relation("formalizes").expect("登録");
    assert!(rule.partial);
    let design = base();
    let contract = design
        .object(&id("REQ-fix-001"))
        .and_then(|o| o.as_contract())
        .expect("contract");
    assert_eq!(contract.kind(), Some(ContractKind::Obligation));
    assert_eq!(contract.formalizes(), ["TR-fix-01"]);
    assert_eq!(contract.site(), Some("specs/fix.fsl:2"));
    let fb = design
        .object(&id("FB-fix-001"))
        .and_then(|o| o.as_contract());
    assert_eq!(
        fb.and_then(|c| c.kind()),
        Some(ContractKind::ForbiddenBehavior)
    );
    let ac = design
        .object(&id("AC-fix-001"))
        .and_then(|o| o.as_contract());
    assert_eq!(ac.and_then(|c| c.kind()), Some(ContractKind::Acceptance));
    // contract の出どころが FSL の宣言の行まで引ける。
    let origin = design.object(&id("REQ-fix-001")).expect("在る").origin();
    assert_eq!(origin.path, Path::new("/fixture/specs/fix.fsl"));
    assert_eq!(origin.line, Some(2));
}

/// 下流は legacy の接頭辞を知らずに、意味で引ける。
#[test]
fn 意味で引ける_接頭辞を知らなくてよい() {
    let design = base();
    let by_kind = |k: Kind| design.objects().filter(|o| o.kind() == k).count();
    assert_eq!(by_kind(Kind::Requirement), 3, "技術要件2 + 上限1");
    assert_eq!(by_kind(Kind::Decision), 2);
    assert_eq!(by_kind(Kind::Question), 1);
    assert_eq!(by_kind(Kind::Evidence), 2);
    assert_eq!(by_kind(Kind::Probe), 1);
    assert_eq!(by_kind(Kind::Workload), 1);
    assert_eq!(by_kind(Kind::Dependency), 2);
    assert_eq!(by_kind(Kind::FormalContract), 3);
    assert_eq!(by_kind(Kind::Hypothesis), 0, "legacy に HYP は無い");

    // 上限は定量の REQ。 配分は読めるが、合計・余白は持たない。
    let budget = design.object(&id("BUDGET-fix-001")).expect("在る");
    assert_eq!(budget.kind(), Kind::Requirement);
    assert_eq!(budget.semantic_kind(), Some(Kind::Requirement));
    assert!(
        matches!(budget.meaning(), Meaning::Decomposed(parts) if parts.contains(&Part::View("budget")))
    );
    let q = budget.as_quantity().expect("定量");
    assert_eq!(
        (q.limit(), q.unit(), q.scope()),
        (Some(1500), Some("MB"), Some("条件"))
    );
    let allocations = q.allocations();
    assert_eq!(allocations.len(), 2);
    assert_eq!(allocations[0].label(), Some("共通"));
    assert_eq!(allocations[0].measured(), Some(false));
    assert_eq!(allocations[0].mode(), None, "モードが無ければ共通分");
    assert_eq!(allocations[1].label(), Some("収録中"));
    assert_eq!(allocations[1].mode(), Some("収録中"));
    assert_eq!(allocations[1].value(), Some(512));
    assert_eq!(allocations[1].stated(), Some("512MB"));

    // 技術要件。
    let tr = design
        .object(&id("TR-fix-02"))
        .and_then(|o| o.as_requirement())
        .expect("要件");
    assert_eq!(tr.statement(), Some("本文2"));
    assert_eq!(tr.confidence(), Some("Assumption"));
    assert_eq!(tr.depends_on(), ["TR-fix-01"]);
    assert_eq!(tr.legacy_needs_component(), Some(false));
    assert_eq!(tr.notes(), ["注"]);

    // 判断・問い・調査。
    let d = design
        .object(&id("DEC-fix-001"))
        .and_then(|o| o.as_decision())
        .expect("判断");
    assert_eq!(d.status(), Some("superseded"));
    assert_eq!(d.superseded_by(), Some("DEC-fix-002"));
    assert_eq!(d.constraint(), Some(("名", "本文")));
    assert_eq!(d.options(), ["a", "b"]);
    let q = design
        .object(&id("Q-fix-001"))
        .and_then(|o| o.as_question())
        .expect("問い");
    assert_eq!(q.resolved_by(), Some("DEC-fix-001"));
    let e = design
        .object(&id("EVID-fix-001"))
        .and_then(|o| o.as_evidence())
        .expect("調査");
    assert_eq!(e.kind(), Some("measurement"));
    assert_eq!(e.provenance_note(), Some("出どころの説明"));
    assert_eq!(e.narrative("finding"), Some("調べたことごとに違う自由記述"));

    // 代表 workload。 実測と式の結果は、`measured` で区別できる。
    let w = design.object(&id("SCALE-fix-001")).expect("在る");
    assert_eq!(w.kind(), Kind::Workload);
    assert_eq!(
        w.semantic_kind(),
        None,
        "workload は semantic object ではない"
    );
    let calcs = w.as_workload().expect("workload").calculations();
    assert_eq!(calcs.len(), 2);
    assert_eq!(
        (calcs[0].value(), calcs[0].measured()),
        (Some(444), Some(true))
    );
    assert_eq!((calcs[1].value(), calcs[1].measured()), (None, Some(false)));
    assert_eq!(
        w.as_workload().and_then(|w| w.selection_rationale()),
        Some("選んだ理由")
    );

    // 部品。 採否の語は legacy の語彙のまま1対1。
    let c1 = design
        .object(&id("CMP-fix-001"))
        .and_then(|o| o.as_component())
        .expect("部品");
    assert_eq!(c1.adoption(), Some(Adoption::Adopted));
    assert_eq!(c1.decided_by(), Some("DEC-fix-001"));
    let c2 = design
        .object(&id("CMP-fix-002"))
        .and_then(|o| o.as_component())
        .expect("部品");
    assert_eq!(c2.adoption(), Some(Adoption::Unsuitable));
    assert_ne!(Adoption::Unsuitable, Adoption::Rejected);

    // 試験の登録。
    let p = design
        .object(&id("SUITE-fix-001"))
        .and_then(|o| o.as_probe())
        .expect("probe");
    assert_eq!(
        (p.runner(), p.package(), p.target()),
        (Some("cargo"), Some("xtask"), Some("lib"))
    );
    assert_eq!((p.min_cases(), p.manual()), (Some(3), Some(1)));
    assert_eq!(p.contracts(), ["TR-fix-01", "REQ-fix-001", "DEC-fix-001"]);
}

/// 目標値は機械で分類しない。 どの種類にもせず、候補を示す。
#[test]
fn tgt_は分類されないまま引ける() {
    let design = base();
    let t = design.object(&id("TGT-fix-001")).expect("在る");
    assert!(t.is_unclassified());
    assert_eq!(t.semantic_kind(), None);
    assert_eq!(t.kind(), Kind::UnclassifiedTarget);
    assert_eq!(t.layer(), Layer::Transitional);
    let target = t.as_target().expect("target");
    assert_eq!(
        target.candidates(),
        [
            Kind::Requirement,
            Kind::Hypothesis,
            Kind::Evidence,
            Kind::Decision
        ]
    );
    assert_eq!(target.goal(), Some("6秒"));
    assert_eq!(target.decided_by(), Some("DEC-fix-001"));
    // どの意味の object にも数えない。
    let semantic: BTreeSet<Id> = design
        .objects()
        .filter(|o| o.semantic_kind().is_some())
        .map(|o| o.id().clone())
        .collect();
    assert!(!semantic.contains(&id("TGT-fix-001")));
    // リリース対象は運用の定義。 semantic object ではない。
    let profile = design.object(&id("PROFILE-fix-1")).expect("在る");
    assert!(matches!(profile.meaning(), Meaning::Direct));
    assert_eq!(profile.layer(), Layer::Operational);
    assert_eq!(profile.semantic_kind(), None);
}

/// 元の記録の出どころが残る。 cutover が旧 ID と新 ID を対応づけるのに引く。
#[test]
fn 出どころは_legacy_の_schema_と場所を持つ() {
    let design = base();
    let o = design.object(&id("TR-fix-02")).expect("在る").origin();
    assert_eq!(o.schema, "requirement-set");
    assert_eq!(o.path, Path::new("/fixture/meta/requirements/fix.toml"));
    assert_eq!(o.line, Some(13));
}

/// 表の全 schema が、読み込みの `SHAPES` に対応している。 対応の無い schema が増えたら落ちる。
#[test]
fn 読み込みの全_schema_に写し方がある() {
    for shape in SHAPES {
        assert!(
            spec(shape.schema).is_some(),
            "{} に写し方が無い",
            shape.schema
        );
    }
    let node_kinds: BTreeSet<&str> = registry().node_kinds.iter().map(|k| k.schema).collect();
    for shape in SHAPES {
        assert!(node_kinds.contains(shape.schema), "{}", shape.schema);
    }
    assert!(node_kinds.contains(CONTRACT_SCHEMA));
    // 目標で加わる schema も node の種類として登録する。
    assert!(node_kinds.contains("design-hypothesis"));
}

/// 欄に割り当てた関係が、全部語彙にある。 registry に載らない欄を作らない。
#[test]
fn 欄の関係は全て語彙にあり_registry_に載る() {
    let mut expected = 0;
    for s in SCHEMAS {
        for f in s.fields {
            match f.role {
                Role::Edge(name) | Role::Inverse(name) => {
                    assert!(
                        relation(name).is_some(),
                        "{}.{} の {name}",
                        s.schema,
                        f.name
                    );
                    expected += 1;
                }
                Role::Lifted(name) | Role::Mirror(name) => {
                    assert!(
                        relation(name).is_some(),
                        "{}.{} の {name}",
                        s.schema,
                        f.name
                    );
                }
                Role::Data => {}
            }
        }
    }
    let reg = registry();
    assert_eq!(reg.relations.len(), expected);
    assert!(reg.locators.is_empty());
    for rule in &reg.relations {
        let spec = relation(rule.relation).expect("語彙にある");
        assert_eq!(rule.class, spec.class);
    }
    // 値の行き先で relation を分ける欄は、組み立てた欄にだけ rule がある。 元の欄から edge を作らない。
    for (schema, field) in [
        ("component-ledger", "supports_requirements"),
        ("test-portfolio", "contracts"),
    ] {
        assert!(
            !reg.relations
                .iter()
                .any(|r| r.from_schema == schema && r.field == field),
            "{schema}.{field}"
        );
    }
    // 関係の行き先の側に書かれた欄は、逆向きの規則で読む。 欄を持たない記録は組み立てない。
    for (schema, field, relation) in [
        ("requirement-set", "formalized_as", "formalizes"),
        ("evidence", "supports_decisions", "considers"),
    ] {
        let rules: Vec<_> = reg
            .relations
            .iter()
            .filter(|r| r.from_schema == schema && r.field == field)
            .collect();
        assert_eq!(rules.len(), 1, "{schema}.{field}");
        assert_eq!(rules[0].relation, relation);
        assert_eq!(rules[0].direction, Direction::Reverse, "{schema}.{field}");
    }
    // 逆向きの規則は、この2つだけ。
    assert_eq!(
        reg.relations
            .iter()
            .filter(|r| r.direction == Direction::Reverse)
            .count(),
        2
    );
}

// ## 壊れた入力を黙って通さない

fn with(path: &str, body: &str) -> BTreeMap<String, String> {
    let mut f = files();
    f.insert(path.to_owned(), body.to_owned());
    f
}

fn has(issues: &[DesignIssue], pred: impl Fn(&DesignIssue) -> bool) -> bool {
    issues.iter().any(pred)
}

#[test]
fn 知らない_schema_は読み込みの問題として出る() {
    let f = with(
        "meta/decisions/DEC-fix-009.toml",
        "schema = 'mystery'\nid = 'DEC-fix-009'\n",
    );
    let issues = design_of(&f).audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::Load(m) if m.contains("知らない schema"))
    ));
}

#[test]
fn 必須の欄が欠けた記録は読み込みの問題として出る() {
    let body = DECISION_2.replace("selected = 'a'\n", "");
    let f = with("meta/decisions/DEC-fix-002.toml", &body);
    let issues = design_of(&f).audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::Load(m) if m.contains("`selected`"))
    ));
}

#[test]
fn ファイル名と_id_が食い違う記録は落ちる() {
    let f = with("meta/decisions/DEC-fix-003.toml", DECISION_2);
    let issues = design_of(&f).audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::Load(m) if m.contains("ファイル名"))
    ));
}

#[test]
fn 重複した_id_は読み込みの問題として出る() {
    let f = with(
        "meta/decisions/DEC-fix-003.toml",
        &DECISION_1.replace("DEC-fix-001", "DEC-fix-002"),
    );
    let issues = design_of(&f).audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::Load(m) if m.contains("重複"))
    ));
}

/// 目標の opaque な ID は今の `Id` に読めず、node が黙って消える。 消えたことを問題として出す。
#[test]
fn id_の形に読めない記録は_node_にならず問題として出る() {
    let body = DECISION_2.replace("DEC-fix-002", "DEC-6CQ3NA");
    let f = with("meta/decisions/DEC-6CQ3NA.toml", &body);
    let design = design_of(&f);
    let issues = design.audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::UnparsedId { raw, .. } if raw == "DEC-6CQ3NA")
    ));
    assert_eq!(
        design
            .objects()
            .filter(|o| o.kind() == Kind::Decision)
            .count(),
        2
    );
}

#[test]
fn id_の形に読めない参照は_edge_にならず問題として出る() {
    let body = DECISION_2.replace(
        "supersedes = ['DEC-fix-001']",
        "supersedes = ['DEC-fix-001', 'DEC-6CQ3NA']",
    );
    let issues = design_of(&with("meta/decisions/DEC-fix-002.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::UnparsedReference { field: "supersedes", raw, .. } if raw == "DEC-6CQ3NA"
    )));
}

#[test]
fn 存在しない相手を指す参照は宙に浮いた_edge_として出る() {
    let body = DECISION_2.replace(
        "supersedes = ['DEC-fix-001']",
        "supersedes = ['DEC-fix-001']\nevidence = ['EVID-fix-404']",
    );
    let issues = design_of(&with("meta/decisions/DEC-fix-002.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::Graph(GraphIssue::Dangling { relation: "considers", to, .. }) if *to == key("EVID-fix-404")
    )));
}

/// 登録した組に入らない種類へ向かう edge は、存在しても通さない。
#[test]
fn 関係の両端の種類が合わなければ問題として出る() {
    // 判断が considers で指せるのは調査だけ。 技術要件を指している。
    let body = DECISION_2.replace(
        "supersedes = ['DEC-fix-001']",
        "supersedes = ['DEC-fix-001']\nevidence = ['TR-fix-01']",
    );
    let issues = design_of(&with("meta/decisions/DEC-fix-002.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::EndpointKind {
            relation: "considers",
            from_kind: Kind::Decision,
            to_kind: Kind::Requirement,
            ..
        }
    )));
}

/// 逆向きに読む欄も、両端の種類を見る。 `formalizes` の起点は形式 contract だけ。
#[test]
fn 逆向きの欄が指す相手の種類が合わなければ問題として出る() {
    // `formalized_as` が、contract ではなく判断を指している。
    let body = REQUIREMENTS.replace("['REQ-fix-001', 'FB-fix-001']", "['DEC-fix-001']");
    let issues = design_of(&with("meta/requirements/fix.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::EndpointKind {
            relation: "formalizes",
            from_kind: Kind::Decision,
            to_kind: Kind::Requirement,
            ..
        }
    )));
    // 調査が `supports_decisions` で、判断ではなく技術要件を指している。
    let body = EVIDENCE_2.replace(
        "['DEC-fix-002', 'DEC-fix-001']",
        "['DEC-fix-002', 'TR-fix-01']",
    );
    let issues = design_of(&with("meta/evidence/EVID-fix-002.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::EndpointKind {
            relation: "considers",
            from_kind: Kind::Requirement,
            to_kind: Kind::Evidence,
            ..
        }
    )));
}

#[test]
fn 存在しない_contract_を指す_formalized_as_は_edge_を作らず問題として出る() {
    let body = REQUIREMENTS.replace(
        "['REQ-fix-001', 'FB-fix-001']",
        "['REQ-fix-001', 'REQ-fix-404']",
    );
    let design = design_of(&with("meta/requirements/fix.toml", &body));
    let issues = design.audit();
    // contract の node が無いので、起点の無い edge として出る。
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::Graph(GraphIssue::Dangling { from, relation: "formalizes", to })
            if *from == key("REQ-fix-404") && *to == key("TR-fix-01")
    )));
    assert!(edges(&design.graph(), "FB-fix-001").is_empty());
}

#[test]
fn 後継の循環は問題として出る() {
    let body = DECISION_1.replace(
        "superseded_by = 'DEC-fix-002'",
        "supersedes = ['DEC-fix-002']",
    );
    let issues = design_of(&with("meta/decisions/DEC-fix-001.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::Graph(GraphIssue::SupersessionCycle(_))
    )));
}

#[test]
fn 知らない採否の語は問題として出る() {
    let body = COMPONENTS.replace("status = '不適'", "status = '保留'");
    let issues = design_of(&with("meta/evidence/components.toml", &body)).audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::UnknownStatus { raw, .. } if raw == "保留")
    ));
}

/// 写し方の無い schema の記録は、読み込みを通っても adapter から引けない。 引けないことを出す。
#[test]
fn 写し方の無い_schema_は問題として出る() {
    static SHAPE: Shape = Shape {
        schema: "unmapped-fix",
        dir: "decisions",
        entity: Some(("DEC-", &["id"])),
        collection: None,
        entity_arrays: &[],
    };
    let mut table = toml::Table::new();
    table.insert(
        "id".to_owned(),
        toml::Value::String("DEC-fix-001".to_owned()),
    );
    let entry = Entry {
        path: "/fixture/meta/decisions/DEC-fix-001.toml".into(),
        shape: &SHAPE,
        table,
        text: "id = 'DEC-fix-001'\n".to_owned(),
    };
    let design = Design::from_entries(Path::new("/fixture"), &[entry], &BTreeMap::new());
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::UnmappedSchema {
            schema: "unmapped-fix"
        }
    )));
    assert_eq!(design.objects().count(), 0);
}

/// 採用済みの判断の `selected` を同じ ID のまま変えると、新しい判断が要ると言う。
#[test]
fn 採用済みの判断を上書きすると問題として出る() {
    let old = base();
    let body = DECISION_2.replace("selected = 'a'", "selected = 'b'");
    let new = design_of(&with("meta/decisions/DEC-fix-002.toml", &body));
    assert_eq!(
        old.overwritten_decisions(&new),
        [DesignIssue::DecisionOverwritten {
            id: id("DEC-fix-002")
        }]
    );
    assert!(old.overwritten_decisions(&base()).is_empty());
    // 置き換えられた判断は採用済みではないので、対象にしない。
    let body = DECISION_1.replace("selected = 'a'", "selected = 'b'");
    let new = design_of(&with("meta/decisions/DEC-fix-001.toml", &body));
    assert!(old.overwritten_decisions(&new).is_empty());
}

// ## 無損失

/// 記録の欄が、表のどれかに割り当たっている。 割り当たらない欄は、型つきの口からも
/// graph からも見えなくなる。 実在する meta の欄で確かめる。
fn assert_every_field_is_accounted(entries: &[Entry]) {
    for e in entries {
        let s = spec(e.shape.schema).unwrap_or_else(|| panic!("{} に写し方が無い", e.shape.schema));
        let collection = e.shape.collection.map(|(k, _, _)| k);
        for (k, v) in &e.table {
            if Some(k.as_str()) == collection {
                continue;
            }
            let is_entity_field = e.shape.entity.is_some();
            let ok = if is_entity_field {
                s.knows(k)
            } else {
                s.knows_file_field(k)
            };
            assert!(
                ok,
                "{}: {} の欄 `{k}` が表に無い（{v:?}）",
                e.path.display(),
                e.shape.schema
            );
        }
        for item in e.items() {
            for k in item.keys() {
                assert!(
                    s.knows(k),
                    "{}: {} の項目の欄 `{k}` が表に無い",
                    e.path.display(),
                    e.shape.schema
                );
            }
        }
    }
}

/// 欄に書かれた参照の数だけ edge ができている。 組み立てる欄（`Role::Lifted`）と逆向きに読む欄
/// （`Role::Inverse`）の値も1つにつき1本。 ただし調査の `supports_decisions` は、判断が同じ
/// 調査を挙げていれば二重にしない。
fn expected_edges(entries: &[Entry]) -> usize {
    let mut n = 0;
    let mut considered: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for e in entries.iter().filter(|e| e.shape.schema == "decision") {
        let id = crate::knowledge::str_of(&e.table, "id")
            .expect("id")
            .to_owned();
        let set = considered.entry(id).or_default();
        set.extend(crate::knowledge::list_of(&e.table, "evidence"));
        set.extend(crate::knowledge::list_of(&e.table, "considers_evidence"));
    }
    let mut tables: Vec<(&str, toml::Table)> = Vec::new();
    for e in entries {
        if e.shape.entity.is_some() {
            tables.push((e.shape.schema, e.table.clone()));
        }
        for item in e.items() {
            tables.push((e.shape.schema, item));
        }
    }
    for (schema, t) in tables {
        let s = spec(schema).expect("写し方がある");
        for f in s.fields {
            if !matches!(f.role, Role::Edge(_) | Role::Lifted(_) | Role::Inverse(_)) {
                continue;
            }
            if f.name == "supports_decisions" {
                let evid = crate::knowledge::str_of(&t, "id").expect("id");
                n += crate::knowledge::list_of(&t, "supports_decisions")
                    .iter()
                    .filter(|d| !considered.get(*d).is_some_and(|set| set.contains(evid)))
                    .count();
                continue;
            }
            match t.get(f.name) {
                Some(toml::Value::Array(a)) => n += a.len(),
                Some(toml::Value::String(_)) => n += 1,
                _ => {}
            }
        }
    }
    n
}

fn load_entries(files: &BTreeMap<String, String>) -> Vec<Entry> {
    let view = RepoView::in_memory(Path::new("/fixture"), files.clone());
    let mut rep = Report::default();
    let entries = crate::knowledge::load_view(&view, &mut rep);
    assert!(!rep.has_errors());
    entries
}

#[test]
fn fixture_の欄は全て表に割り当たり_参照は全て_edge_になる() {
    let entries = load_entries(&files());
    assert_every_field_is_accounted(&entries);
    let graph = base().graph();
    assert_eq!(graph.all_relations().count(), expected_edges(&entries));
}

/// 実在する meta を全部読ませる。 件数は legacy の読み方（`check-meta` が使うのと同じ
/// `load` と snapshot）で数えたものと一致し、問題は1件も出ない。
#[test]
fn 実在する_meta_の全_object_が_adapter_を通って_graph_に載る() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let mut rep = Report::default();
    let entries = load(root, &mut rep);
    assert!(!rep.has_errors(), "meta がそもそも読めない");
    let (legacy, dups) = KnowledgeSnapshot::from_entries(&entries);
    assert!(dups.is_empty());
    let fsl = fsl_ids(root);

    let design = Design::load(&RepoView::working_tree(root));
    assert_eq!(design.audit(), Vec::new());
    assert_every_field_is_accounted(&entries);

    let graph = design.graph();
    let count = |kind: Kind| graph.nodes().filter(|n| n.kind == kind.name()).count();
    let schema = |s: &str| legacy.by_schema(s).count();
    // `check-meta` の件数の言い方: FSL の要求 / 技術要件 / 部品台帳 / 性能目標。
    assert_eq!(count(Kind::FormalContract), fsl.len());
    assert_eq!(
        count(Kind::Requirement),
        schema("requirement-set") + schema("budget")
    );
    assert_eq!(count(Kind::Dependency), schema("component-ledger"));
    assert_eq!(count(Kind::UnclassifiedTarget), schema("target-set"));
    assert_eq!(count(Kind::Decision), schema("decision"));
    assert_eq!(count(Kind::Question), schema("question"));
    assert_eq!(count(Kind::Evidence), schema("evidence"));
    assert_eq!(count(Kind::Workload), schema("scale-reference"));
    assert_eq!(count(Kind::Probe), schema("test-portfolio"));
    assert_eq!(count(Kind::ReleaseProfile), schema("profile"));
    // 全 node が語彙にある種類で、記録を1件も取りこぼしていない。
    assert_eq!(graph.nodes().count(), legacy.records().count() + fsl.len());
    assert!(graph.nodes().all(|n| Kind::of(n).is_some()));
    // 欄に書かれた参照の数 + `formalizes` の数だけ edge がある。
    let formalizes = graph
        .all_relations()
        .filter(|r| r.name == "formalizes")
        .count();
    assert_eq!(formalizes, formalized_as_values(&entries));
    assert_eq!(graph.all_relations().count(), expected_edges(&entries));
    // 実在する要件の中に、FSL を持つものがある。
    assert!(formalizes > 0);
}

/// `formalized_as` の値の数。 REQ 側の欄は edge にならず、contract 側に持ち替える。
fn formalized_as_values(entries: &[Entry]) -> usize {
    entries
        .iter()
        .filter(|e| e.shape.schema == "requirement-set")
        .flat_map(Entry::items)
        .map(|t| crate::knowledge::list_of(&t, "formalized_as").len())
        .sum()
}

// ## 決まった写し方（`DEC-PLT-048`）

/// 目標の schema を node の種類として登録し、目標の欄を relation の規則にする。
#[test]
fn 目標の_schema_と欄が登録される() {
    let reg = registry();
    for (schema, kind) in [
        ("design-hypothesis", "hypothesis"),
        ("requirement", "requirement"),
        ("evidence", "evidence"),
        ("probe", "probe"),
        ("workload", "workload"),
        ("decision", "decision"),
    ] {
        assert_eq!(reg.kind_for_schema(schema), Some(kind), "{schema} の種類");
    }
    let rule = |field: &str| {
        reg.relations
            .iter()
            .find(|r| r.from_schema == "decision" && r.field == field)
            .map(|r| r.relation)
    };
    for (field, relation) in [
        ("relies_on_hypotheses", "relies_on_hypotheses"),
        ("considers_evidence", "considers"),
        ("establishes_requirements", "establishes"),
        ("revises_requirements", "revises"),
        ("removes_requirements", "removes"),
    ] {
        assert_eq!(rule(field), Some(relation), "{field}");
    }
    // 逆向きに持つ欄は edge にしない。
    assert_eq!(rule("question"), None);
    assert_eq!(rule("provenance"), None);
}

/// 採否の語が採用でないもの（知らない語を含む）は、`implements` にならない。
#[test]
fn 採用でない部品は_implements_にならない() {
    for status in ["不適", "不採用", "採用候補", "条件付き", "要調査", "保留"] {
        let body = COMPONENTS.replace("status = '採用'", &format!("status = '{status}'"));
        let design = design_of(&with("meta/evidence/components.toml", &body));
        let graph = design.graph();
        let cmp = edges(&graph, "CMP-fix-001");
        assert!(!cmp.iter().any(|(n, _)| *n == "implements"), "{status}");
        assert!(
            cmp.contains(&("supports_requirements", "TR-fix-01".to_owned())),
            "{status}"
        );
    }
}

/// 要件を指す contracts だけが targets。 定量の要件（上限）も要件。
#[test]
fn 要件を指す_contracts_だけが_targets_になる() {
    let body = SUITES.replace(
        "contracts = ['TR-fix-01', 'REQ-fix-001', 'DEC-fix-001']",
        "contracts = ['BUDGET-fix-001', 'EVID-fix-001', 'FB-fix-001']",
    );
    let graph = design_of(&with("meta/suites/cargo.toml", &body)).graph();
    let probe = edges(&graph, "SUITE-fix-001");
    assert!(probe.contains(&("targets", "BUDGET-fix-001".to_owned())));
    assert!(probe.contains(&("contracts", "EVID-fix-001".to_owned())));
    assert!(probe.contains(&("contracts", "FB-fix-001".to_owned())));
    assert_eq!(probe.len(), 3);
    // 型つきの口は、元の並びのまま読める。
    let original = design_of(&files());
    let contracts = original
        .object(&id("SUITE-fix-001"))
        .and_then(|o| o.as_probe())
        .map(|p| p.contracts());
    assert_eq!(
        contracts,
        Some(vec![
            "TR-fix-01".to_owned(),
            "REQ-fix-001".to_owned(),
            "DEC-fix-001".to_owned()
        ])
    );
}

/// 指す判断が無い `supports_decisions` は、起点の無い edge として出る。
#[test]
fn 存在しない判断を指す_supports_decisions_は宙に浮いた_edge_として出る() {
    let body = EVIDENCE_2.replace(
        "['DEC-fix-002', 'DEC-fix-001']",
        "['DEC-fix-002', 'DEC-fix-404']",
    );
    let design = design_of(&with("meta/evidence/EVID-fix-002.toml", &body));
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::Graph(GraphIssue::Dangling { from, relation: "considers", to })
            if *from == key("DEC-fix-404") && *to == key("EVID-fix-002")
    )));
    assert!(
        edges(&design.graph(), "DEC-fix-002").contains(&("considers", "EVID-fix-002".to_owned()))
    );
}

/// 調査が支える先を変えても、判断の記録そのものは変わらない。 以前は判断の記録に
/// `considers_evidence` を足していたので、調査の側を直すと判断の digest が変わり、判断が
/// 変わったように見えた。
#[test]
fn 調査の支える先を変えても判断の_digest_は変わらない() {
    let before = base().graph();
    let body = EVIDENCE_2.replace(
        "['DEC-fix-002', 'DEC-fix-001']",
        "['DEC-fix-002']", // DEC-fix-001 を外す
    );
    let after = design_of(&with("meta/evidence/EVID-fix-002.toml", &body)).graph();
    for raw in ["DEC-fix-001", "DEC-fix-002"] {
        assert_eq!(
            before.node(&key(raw)).map(|n| n.fingerprint),
            after.node(&key(raw)).map(|n| n.fingerprint),
            "{raw} の digest が変わった"
        );
    }
    // 書き換えた調査自身は、内容が変わったと出る。
    assert_ne!(
        before.node(&key("EVID-fix-002")).map(|n| n.fingerprint),
        after.node(&key("EVID-fix-002")).map(|n| n.fingerprint)
    );
    // 変わったのは edge だけ。 判断は内容ではなく edge が変わったと出る。
    let d = crate::graph::diff(&before, &after);
    assert_eq!(
        d.changed.get(&key("DEC-fix-001")),
        Some(&crate::graph::ChangeKind::Edges)
    );
    assert_eq!(
        d.changed.get(&key("EVID-fix-002")),
        Some(&crate::graph::ChangeKind::Content)
    );
    assert!(!d.changed.contains_key(&key("DEC-fix-002")));
    assert!(d.added.is_empty() && d.removed.is_empty());
}

/// REQ の `formalized_as` を直しても、contract の記録そのものは変わらない。
#[test]
fn 要件の_formalized_as_を直しても_contract_の_digest_は変わらない() {
    let before = base().graph();
    let body = REQUIREMENTS.replace("['REQ-fix-001', 'FB-fix-001']", "['REQ-fix-001']");
    let after = design_of(&with("meta/requirements/fix.toml", &body)).graph();
    for raw in ["REQ-fix-001", "FB-fix-001"] {
        assert_eq!(
            before.node(&key(raw)).map(|n| n.fingerprint),
            after.node(&key(raw)).map(|n| n.fingerprint),
            "{raw}"
        );
    }
    assert!(edges(&after, "FB-fix-001").is_empty());
}

/// 受入は形式 contract の1種類で、目標の接頭辞は `ACC`。
#[test]
fn 受入の目標の接頭辞は_acc() {
    assert_eq!(ContractKind::Acceptance.target_prefix(), "ACC");
    assert_eq!(ContractKind::Obligation.target_prefix(), "OBL");
    assert_eq!(ContractKind::Invariant.target_prefix(), "INV");
    assert_eq!(ContractKind::ForbiddenBehavior.target_prefix(), "FB");
    for kind in [
        ContractKind::Obligation,
        ContractKind::Invariant,
        ContractKind::ForbiddenBehavior,
        ContractKind::Acceptance,
    ] {
        let id = format!("{}-4K7DP2", kind.target_prefix());
        assert_eq!(
            id.parse::<TargetId>().map(|t| t.kind()),
            Ok(Kind::FormalContract),
            "{id}"
        );
    }
}
