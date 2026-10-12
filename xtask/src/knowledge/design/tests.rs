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
use crate::repo::{RepoView, RevisionRef};

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
                Role::Lifted(name) | Role::Mirror(name) | Role::Locator(name) => {
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
    // locator の欄は、判断の `provenance` だけ。 値から node の種類を決める。
    assert_eq!(reg.locators.len(), 1);
    assert_eq!(
        (reg.locators[0].from_schema, reg.locators[0].field),
        ("decision", "provenance")
    );
    assert!(reg.locators[0].kind_of.is_some());
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

/// `Id` に読めない ID の記録は、node が黙って消える。 消えたことを問題として出す。
/// opaque の小文字は目標の形ではない（今の形としても読めない）。
#[test]
fn id_の形に読めない記録は_node_にならず問題として出る() {
    let body = DECISION_2.replace("DEC-fix-002", "DEC-6cq3na");
    let f = with("meta/decisions/DEC-6cq3na.toml", &body);
    let design = design_of(&f);
    let issues = design.audit();
    assert!(has(
        &issues,
        |i| matches!(i, DesignIssue::UnparsedId { raw, .. } if raw == "DEC-6cq3na")
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
        "supersedes = ['DEC-fix-001', 'DEC-6cq3na']",
    );
    let issues = design_of(&with("meta/decisions/DEC-fix-002.toml", &body)).audit();
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::UnparsedReference { field: "supersedes", raw, .. } if raw == "DEC-6cq3na"
    )));
}

/// 目標の opaque な ID の記録は node になり、今の ID と同じ graph に載る。 参照も edge になる。
#[test]
fn 目標の_opaque_id_の記録は_node_になる() {
    let body = DECISION_2.replace("DEC-fix-002", "DEC-6CQ3NA");
    let f = with("meta/decisions/DEC-6CQ3NA.toml", &body);
    let design = design_of(&f);
    // 読めない ID は1件も無い。 参照先は今の ID の DEC-fix-001。
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    assert_eq!(
        graph.node(&key("DEC-6CQ3NA")).map(|n| n.kind),
        Some("decision")
    );
    // 今の ID と別の node。
    assert_ne!(key("DEC-6CQ3NA"), key("DEC-ALL-001"));
    assert!(edges(&graph, "DEC-6CQ3NA").contains(&("supersedes", "DEC-fix-001".to_owned())));
    assert_eq!(
        design
            .objects()
            .filter(|o| o.kind() == Kind::Decision)
            .count(),
        3
    );
    let id = id("DEC-6CQ3NA");
    assert!(id.is_target());
    assert_eq!(id.opaque(), Some("6CQ3NA"));
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

    // 実在する ID は1つも目標の形に読み替わらない（今の読みを変えていない）。
    assert!(
        legacy
            .records()
            .filter_map(|r| r.id())
            .all(|id| !id.is_target()),
        "実在する meta の ID が目標の形に読まれた"
    );
    assert!(
        fsl.iter()
            .filter_map(|raw| raw.parse::<Id>().ok())
            .all(|id| !id.is_target()),
        "実在する FSL の ID が目標の形に読まれた"
    );

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
    // `provenance` は locator の欄で、relation の規則ではなく locator の規則。
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

// ## 目標の schema を読み込みへ流し込む（X08）

const HYPOTHESIS: &str = "schema = 'design-hypothesis'
id = 'HYP-91MTWX'
question = 'Q-fix-001'
hypothesis_kind = 'semantic-comprehension'
statement = '初めて使う人が説明できる'
scope = '音源一覧'
assessment = 'untested'
supporting_evidence = []
contradicting_evidence = []
";

const TARGET_DECISION: &str = "schema = 'decision'
id = 'DEC-6CQ3NA'
title = '目標の形の判断'
status = 'accepted'
owner = 'x'
options = ['a']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
question = 'Q-fix-001'
relies_on_hypotheses = ['HYP-91MTWX']
considers_evidence = ['EVID-fix-001']
establishes_requirements = ['REQ-5H8NQ2']
";

const TARGET_REQUIREMENT: &str = "schema = 'requirement'
id = 'REQ-5H8NQ2'
statement = '現在可能な創作行為を利用者が認識できなければならない'
established_by = 'DEC-6CQ3NA'
";

fn target_files() -> BTreeMap<String, String> {
    let mut f = files();
    for (path, body) in [
        ("meta/hypotheses/HYP-91MTWX.toml", HYPOTHESIS),
        ("meta/decisions/DEC-6CQ3NA.toml", TARGET_DECISION),
        ("meta/requirements/REQ-5H8NQ2.toml", TARGET_REQUIREMENT),
    ] {
        f.insert(path.to_owned(), body.to_owned());
    }
    f
}

/// 目標の形の fixture（`meta/hypotheses/HYP-<opaque>.toml` など）が読まれ、graph に載る。
#[test]
fn 目標の形の_fixture_が読まれ_graph_に載る() {
    let design = design_of(&target_files());
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    for (raw, kind) in [
        ("HYP-91MTWX", "hypothesis"),
        ("DEC-6CQ3NA", "decision"),
        ("REQ-5H8NQ2", "requirement"),
    ] {
        assert_eq!(graph.node(&key(raw)).map(|n| n.kind), Some(kind), "{raw}");
        assert!(id(raw).is_target(), "{raw}");
    }
    let dec = edges(&graph, "DEC-6CQ3NA");
    for (relation, to) in [
        ("relies_on_hypotheses", "HYP-91MTWX"),
        ("considers", "EVID-fix-001"),
        ("establishes", "REQ-5H8NQ2"),
    ] {
        assert!(dec.contains(&(relation, to.to_owned())), "{relation}");
    }
    // 目標の記録は、型つきの口からも種類で引ける。
    let kinds: Vec<Kind> = design
        .objects()
        .filter(|o| o.id().is_target())
        .map(|o| o.kind())
        .collect();
    assert_eq!(kinds.len(), 3);
    assert!(kinds.contains(&Kind::Hypothesis));
    assert_eq!(
        design
            .objects()
            .filter(|o| o.kind() == Kind::Requirement)
            .count(),
        3 + 1,
        "技術要件2 + 上限1 + 目標の REQ 1"
    );
}

/// 目標の schema を渡さずに読むと、今までどおり「知らない schema」になる。
#[test]
fn 目標の_schema_を渡さない読み込みは今のまま知らない_schema_で落ちる() {
    let view = RepoView::in_memory(Path::new("/fixture"), target_files());
    let mut rep = Report::default();
    let entries = crate::knowledge::load_view(&view, &mut rep);
    assert!(rep.has_errors());
    assert!(
        rep.diagnostics()
            .iter()
            .any(|d| d.message.contains("知らない schema `design-hypothesis`"))
    );
    assert!(
        entries
            .iter()
            .all(|e| !matches!(e.shape.schema, "design-hypothesis" | "requirement"))
    );
}

/// 置き場所が schema と合わなければ落ちる。 node にもならない。
#[test]
fn 目標の_schema_は置き場所が違えば落ちる() {
    let mut f = target_files();
    let body = f.remove("meta/hypotheses/HYP-91MTWX.toml").expect("在る");
    f.insert("meta/decisions/HYP-91MTWX.toml".to_owned(), body);
    let design = design_of(&f);
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::Load(m) if m.contains("meta/hypotheses/")
    )));
    assert!(design.object(&id("HYP-91MTWX")).is_none());
}

/// ファイル名と ID の一致、必須項目、接頭辞は、目標の形にも同じように掛かる。
#[test]
fn 目標の_schema_にも読み込みの検査が掛かる() {
    // ファイル名が ID と違う。
    let mut f = target_files();
    let body = f.remove("meta/hypotheses/HYP-91MTWX.toml").expect("在る");
    f.insert("meta/hypotheses/HYP-91MTWY.toml".to_owned(), body);
    assert!(has(&design_of(&f).audit(), |i| matches!(
        i,
        DesignIssue::Load(m) if m.contains("ファイル名")
    )));
    // 必須の `statement` が無い。
    let body = HYPOTHESIS.replace("statement = '初めて使う人が説明できる'\n", "");
    assert!(has(
        &design_of(&with("meta/hypotheses/HYP-91MTWX.toml", &body)).audit(),
        |i| matches!(i, DesignIssue::Load(m) if m.contains("`statement`"))
    ));
    // 接頭辞が schema と違う。
    let body = HYPOTHESIS.replace("HYP-91MTWX", "DEC-91MTWX");
    assert!(has(
        &design_of(&with("meta/hypotheses/DEC-91MTWX.toml", &body)).audit(),
        |i| matches!(i, DesignIssue::Load(m) if m.contains("`HYP-` で始まる"))
    ));
}

/// 目標の形でだけ書く schema に、今の形の ID は置けない。
#[test]
fn 目標の_schema_の_id_は目標の形でなければならない() {
    let body = HYPOTHESIS.replace("HYP-91MTWX", "HYP-fix-001");
    let design = design_of(&with("meta/hypotheses/HYP-fix-001.toml", &body));
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::TargetIdentity { error: IdentityError::NotTargetForm, id }
            if id.as_str() == "HYP-fix-001"
    )));
    // 小文字の opaque は ID の形に読めず、node にならない。
    let body = HYPOTHESIS.replace("HYP-91MTWX", "HYP-91mtwx");
    let design = design_of(&with("meta/hypotheses/HYP-91mtwx.toml", &body));
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::UnparsedId { raw, .. } if raw == "HYP-91mtwx"
    )));
    // 記録の種類と接頭辞が合わない（読み込みの検査を通らない経路。 組み立て済みの記録で試す）。
    let mut table = toml::Table::new();
    for (k, v) in [
        ("id", "REQ-91MTWX"),
        ("statement", "本文"),
        ("schema", "design-hypothesis"),
    ] {
        table.insert(k.to_owned(), toml::Value::String(v.to_owned()));
    }
    let entry = Entry {
        path: "/fixture/meta/hypotheses/REQ-91MTWX.toml".into(),
        shape: &TARGET_SHAPES[0],
        table,
        text: "id = 'REQ-91MTWX'\n".to_owned(),
    };
    let design = Design::from_entries(Path::new("/fixture"), &[entry], &BTreeMap::new());
    assert!(has(&design.audit(), |i| matches!(
        i,
        DesignIssue::TargetIdentity {
            error: IdentityError::PrefixMismatch,
            ..
        }
    )));
}

/// 追加の形は足すだけ。 今の schema の必須項目を、追加の形で緩められない。
#[test]
fn 追加の形は今の_schema_を置き換えない() {
    static LOOSE: [Shape; 1] = [Shape {
        schema: "decision",
        dir: "decisions",
        entity: Some(("DEC-", &["id"])),
        collection: None,
        entity_arrays: &[],
    }];
    let body = "schema = 'decision'\nid = 'DEC-fix-009'\n";
    let view = RepoView::in_memory(
        Path::new("/fixture"),
        with("meta/decisions/DEC-fix-009.toml", body),
    );
    let mut rep = Report::default();
    let _ = crate::knowledge::load_view_with(&view, &LOOSE, &mut rep);
    assert!(
        rep.diagnostics()
            .iter()
            .any(|d| d.message.contains("必須項目 `title` が無い")),
        "今の schema の必須項目が緩んだ"
    );
    // 追加が空なら、`load_view` と同じ。
    let mut a = Report::default();
    let mut b = Report::default();
    let ea = crate::knowledge::load_view_with(&view, &[], &mut a);
    let eb = crate::knowledge::load_view(&view, &mut b);
    assert_eq!(a.diagnostics(), b.diagnostics());
    assert_eq!(ea.len(), eb.len());
}

/// 目標の schema の形は、写し方の表（`SCHEMAS`）にあり、今の schema と名前が重ならず、
/// 接頭辞が語彙の種類と一致する。
#[test]
fn 目標の形は写し方の表と読み込みの形に矛盾しない() {
    for shape in TARGET_SHAPES {
        assert!(is_target_only(shape.schema));
        assert!(
            SHAPES.iter().all(|s| s.schema != shape.schema),
            "{} は今の schema と名前が重なる",
            shape.schema
        );
        let kind = spec(shape.schema).expect("写し方がある").kind;
        let (prefix, required) = shape.entity.expect("1件1ファイル");
        assert!(required.contains(&"id"));
        let sample = format!("{prefix}4K7DP2");
        assert_eq!(
            sample.parse::<TargetId>().map(|t| t.kind()),
            Ok(kind),
            "{sample}"
        );
    }
}

// ## locator の種類を値から決める（X08）

/// 判断の `provenance` の locator は、値の書式から node の種類が決まる。 書式が決まっていない値
/// （commit・issue・artifact の書式は提案書に無い）と崩れた値は、node にならず読めないと言う。
#[test]
fn provenance_の_locator_は値の書式で_node_の種類が決まる() {
    let body = TARGET_DECISION.replace(
        "question = 'Q-fix-001'\n",
        "question = 'Q-fix-001'\nprovenance = ['github:pr/123@abc123', 'github:commit/abc', 'github:pr/x', 'crates/a/src/lib.rs']\n",
    );
    let design = design_of(&with("meta/decisions/DEC-6CQ3NA.toml", &body));
    let graph = design.graph();
    let node = |kind: &'static str, value: &str| {
        graph.node(&NodeKey::Locator {
            kind,
            value: value.to_owned(),
        })
    };
    assert_eq!(
        node("pull-request", "github:pr/123@abc123").map(|n| n.kind),
        Some("pull-request")
    );
    assert_eq!(
        node("source", "crates/a/src/lib.rs").map(|n| n.kind),
        Some("source")
    );
    // 書式が決まっていない・崩れた値は node にならない。
    assert_eq!(
        graph
            .nodes()
            .filter(|n| matches!(n.key, NodeKey::Locator { .. }))
            .count(),
        2
    );
    let issues = design.audit();
    for value in ["github:commit/abc", "github:pr/x"] {
        assert!(
            has(&issues, |i| matches!(
                i,
                DesignIssue::Graph(GraphIssue::UnreadableLocator { value: v, .. }) if v == value
            )),
            "{value}"
        );
    }
    // provenance の行き先に取れるのは Issue / PR / commit / artifact。 ソースのパスは取れない。
    assert!(has(&issues, |i| matches!(
        i,
        DesignIssue::EndpointKind {
            relation: "provenance",
            to_kind: Kind::Source,
            ..
        }
    )));
    // PR への edge は問題にならない。
    assert!(!has(&issues, |i| matches!(
        i,
        DesignIssue::EndpointKind {
            to_kind: Kind::PullRequest,
            ..
        }
    )));
    // 同じ PR を2つの判断が挙げれば、1つの node に edge が2本。
    let other = body.replace("DEC-6CQ3NA", "DEC-6CQ3NB");
    let mut f = with("meta/decisions/DEC-6CQ3NA.toml", &body);
    f.insert("meta/decisions/DEC-6CQ3NB.toml".to_owned(), other);
    let graph = design_of(&f).graph();
    let pr = NodeKey::Locator {
        kind: "pull-request",
        value: "github:pr/123@abc123".to_owned(),
    };
    assert_eq!(graph.incoming(&pr).len(), 2);
}

// ## 版を持つ graph / 2つの版の比較（X08）

/// 試験ごとに使い捨てる git リポジトリ。 `tempfile` は引かない（`repo::view` の試験と同じ形）。
struct TempRepo(std::path::PathBuf);

impl Drop for TempRepo {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

impl TempRepo {
    fn new(name: &str) -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "koeru-xtask-design-{name}-{}-{n}",
            std::process::id()
        ));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).expect("作れる");
        let repo = Self(dir);
        repo.git(&["init", "-q"]);
        for (k, v) in [
            ("user.name", "design-test"),
            ("user.email", "design-test@example.invalid"),
            ("commit.gpgsign", "false"),
            ("core.autocrlf", "false"),
        ] {
            repo.git(&["config", k, v]);
        }
        repo
    }

    fn root(&self) -> &Path {
        &self.0
    }

    fn git(&self, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .current_dir(&self.0)
            .args(args)
            .output()
            .expect("git を起動できる");
        assert!(
            out.status.success(),
            "git {args:?} が失敗した: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout)
            .expect("UTF-8")
            .trim()
            .to_owned()
    }

    fn write_all(&self, files: &BTreeMap<String, String>) {
        for (rel, body) in files {
            let p = self.0.join(rel);
            std::fs::create_dir_all(p.parent().expect("親がある")).expect("作れる");
            std::fs::write(p, body).expect("書ける");
        }
    }

    fn remove(&self, rel: &str) {
        std::fs::remove_file(self.0.join(rel)).expect("消せる");
    }

    fn commit(&self, message: &str) -> String {
        self.git(&["add", "-A"]);
        self.git(&["commit", "-q", "-m", message]);
        self.git(&["rev-parse", "HEAD"])
    }
}

/// `Design` の graph が、読んだ版を持つ。 版を持たない view は `None` のままで、呼び出し側が
/// 分かっている版を足せる。
#[test]
fn design_の_graph_は読んだ版を持つ() {
    let repo = TempRepo::new("revision");
    repo.write_all(&files());
    let sha = repo.commit("base");

    let working = Design::load(&RepoView::working_tree(repo.root()));
    assert_eq!(
        working.graph().revision(),
        Some(&RevisionRef::WorkingTree {
            head: sha.clone(),
            dirty: false
        })
    );
    let at = Design::load(&RepoView::revision(repo.root(), &sha).expect("解決できる"));
    assert_eq!(
        at.graph().revision(),
        Some(&RevisionRef::Revision { sha: sha.clone() })
    );
    // 未コミットの変更は dirty。
    repo.write_all(&with(
        "meta/decisions/DEC-fix-002.toml",
        &format!("{DECISION_2}\n"),
    ));
    let dirty = Design::load(&RepoView::working_tree(repo.root()));
    assert!(matches!(
        dirty.graph().revision(),
        Some(RevisionRef::WorkingTree { dirty: true, .. })
    ));

    // メモリ上の view は版を持たない。 足せば持つ。
    let memory = base();
    assert_eq!(memory.revision(), None);
    assert_eq!(memory.graph().revision(), None);
    let rev = RevisionRef::Revision {
        sha: "0123abc".to_owned(),
    };
    assert_eq!(
        base().with_revision(rev.clone()).graph().revision(),
        Some(&rev)
    );
    // 版を足しても、graph の中身（node・edge）は変わらない。
    let a = base().graph();
    let b = base().with_revision(rev).graph();
    assert_eq!(
        a.nodes()
            .map(|n| (n.key.clone(), n.fingerprint))
            .collect::<Vec<_>>(),
        b.nodes()
            .map(|n| (n.key.clone(), n.fingerprint))
            .collect::<Vec<_>>()
    );
    assert_eq!(a.all_relations().count(), b.all_relations().count());
}

// ## 収集ファイル直下の欄を型つきで読む（X08）

/// 収集ファイル直下の文字列の欄を、型つきで読める。 持たない schema の欄・文字列でない値は読まない。
#[test]
fn 収集ファイル直下の欄を型つきで読める() {
    let design = base();
    let mut files: Vec<_> = design
        .source_files()
        .map(|f| (f.domain(), f.domain_title(), f.prefix()))
        .collect();
    files.sort();
    // 技術要件は領域と接頭辞を持つ。 目標値は領域だけ。 1件1ファイルの schema は含まれない。
    assert_eq!(
        files,
        [
            (Some("fix"), None, None),
            (Some("fix"), None, Some("TR-fix"))
        ]
    );

    // 目標値のファイルに `prefix` を書いても、その schema は持たない欄なので読まない。
    // 文字列でない値も読まない。
    // 直下の欄は `[[target]]` より前に置く。
    let targets = TARGETS
        .replace("domain = 'fix'", "domain = 3")
        .replace("\n[[target]]", "prefix = 'TGT-fix'\n\n[[target]]");
    let design = design_of(&with("meta/budgets/fix.toml", &targets));
    let t = design
        .source_files()
        .find(|f| f.path().ends_with("meta/budgets/fix.toml"))
        .expect("目標値のファイル");
    assert_eq!(t.domain(), None, "文字列でない値");
    assert_eq!(t.prefix(), None, "その schema が持たない欄");

    // 欄の一覧と、型つきの口が読む欄が一致する。
    for schema in ["requirement-set", "target-set"] {
        let spec = spec(schema).expect("写し方がある");
        for name in ["domain", "domain_title", "prefix"] {
            let readable = match schema {
                "requirement-set" => true,
                _ => name == "domain",
            };
            assert_eq!(spec.knows_file_field(name), readable, "{schema}.{name}");
        }
    }
}

// ## 「内容を直したら ID が変わった」の検出（X08）

fn rename(from: &str, to: &str) -> crate::repo::Rename {
    crate::repo::Rename {
        from: from.to_owned(),
        to: to.to_owned(),
        similarity: 100,
    }
}

/// 同じ種類のファイルが改名され、ID が変わっていれば出す。 git の改名検出を通す。
#[test]
fn 同じ種類のファイルの改名で_id_が変われば出る() {
    let repo = TempRepo::new("identity-changed");
    repo.write_all(&target_files());
    let base = repo.commit("base");

    // 仮説を直したついでに ID を付け直した（ファイル名も ID に合わせた）。
    repo.remove("meta/hypotheses/HYP-91MTWX.toml");
    repo.write_all(&BTreeMap::from([(
        "meta/hypotheses/HYP-91MTWY.toml".to_owned(),
        HYPOTHESIS.replace("HYP-91MTWX", "HYP-91MTWY"),
    )]));
    // 技術要件の収集ファイルは、名前を変えても ID が変わらない。
    repo.remove("meta/requirements/fix.toml");
    repo.write_all(&BTreeMap::from([(
        "meta/requirements/fix-renamed.toml".to_owned(),
        REQUIREMENTS.to_owned(),
    )]));
    let head = repo.commit("rename");

    let old = RepoView::revision(repo.root(), &base).expect("解決できる");
    let new = RepoView::revision(repo.root(), &head).expect("解決できる");
    assert_eq!(
        Design::compare_identities(&old, &new).expect("比べられる"),
        [DesignIssue::IdentityChanged {
            from: id("HYP-91MTWX"),
            to: id("HYP-91MTWY"),
            schema: "design-hypothesis",
        }]
    );
}

/// 内容を直しただけ・改名していない・ID が同じ・種類が違う改名は出さない。
#[test]
fn id_が変わっていない_または種類が違う改名は出ない() {
    // 内容を直しただけ（改名なし）。
    let repo = TempRepo::new("identity-unchanged");
    repo.write_all(&target_files());
    let base = repo.commit("base");
    repo.write_all(&BTreeMap::from([(
        "meta/hypotheses/HYP-91MTWX.toml".to_owned(),
        HYPOTHESIS.replace("untested", "supported"),
    )]));
    let head = repo.commit("edit");
    let old = RepoView::revision(repo.root(), &base).expect("解決できる");
    let new = RepoView::revision(repo.root(), &head).expect("解決できる");
    assert_eq!(
        Design::compare_identities(&old, &new).expect("比べられる"),
        Vec::new()
    );

    // 種類が違う（仮説の記録が、判断の記録に作り替わった）。 取り違えとは言わない。
    let old = design_of(&target_files());
    let body = TARGET_DECISION.replace("DEC-6CQ3NA", "DEC-6CQ3NB");
    let new = design_of(&with("meta/decisions/DEC-6CQ3NB.toml", &body));
    let renames = [rename(
        "meta/hypotheses/HYP-91MTWX.toml",
        "meta/decisions/DEC-6CQ3NB.toml",
    )];
    assert_eq!(old.changed_identities(&new, &renames), Vec::new());
    // 同じ種類なら出る。
    let renames = [rename(
        "meta/decisions/DEC-6CQ3NA.toml",
        "meta/decisions/DEC-6CQ3NB.toml",
    )];
    assert_eq!(
        old.changed_identities(&new, &renames),
        [DesignIssue::IdentityChanged {
            from: id("DEC-6CQ3NA"),
            to: id("DEC-6CQ3NB"),
            schema: "decision",
        }]
    );
    // 収集ファイルの改名は、ファイル名が ID を決めないので出さない。 meta の外のファイルも。
    let renames = [
        rename("meta/requirements/fix.toml", "meta/requirements/other.toml"),
        rename("docs/a.md", "docs/b.md"),
        rename("specs/fix.fsl", "specs/other.fsl"),
    ];
    assert_eq!(old.changed_identities(&old, &renames), Vec::new());
    // 改名が無ければ何も出ない。
    assert_eq!(old.changed_identities(&new, &[]), Vec::new());
}

/// 改名を引けない組（メモリ上の view）は、空ではなく失敗で返す。
#[test]
fn 改名を引けない組は失敗する() {
    let memory = RepoView::in_memory(Path::new("/fixture"), files());
    assert!(Design::compare_identities(&memory, &memory).is_err());
}

// ## 採用済みの判断の上書きと、意味を変えない修正の明示（X08）

/// `DEC-fix-002`（採用済み）を書き換えた新しい版。 `edits` は `meaning_preserving_edits` の本文。
fn decision_2_changed(replacements: &[(&str, &str)], edits: Option<&str>) -> Design {
    let mut body = DECISION_2.to_owned();
    for (from, to) in replacements {
        assert!(body.contains(from), "{from}");
        body = body.replace(from, to);
    }
    if let Some(edits) = edits {
        body.push_str(&format!("meaning_preserving_edits = {edits}\n"));
    }
    design_of(&with("meta/decisions/DEC-fix-002.toml", &body))
}

fn overwritten(old: &Design, new: &Design) -> Vec<String> {
    old.overwritten_decisions(new)
        .into_iter()
        .map(|i| match i {
            DesignIssue::DecisionOverwritten { id } => id.to_string(),
            other => panic!("{other:?}"),
        })
        .collect()
}

/// 同じ変更の中で「意味を変えない修正」の記録が1件増えていれば通す。 増えていなければ落とす。
#[test]
fn 意味を変えない修正の記録が増えていれば上書きとして出ない() {
    let old = base();
    let selected = [("selected = 'a'", "selected = 'b'")];

    // 記録なし → 出る。
    assert_eq!(
        overwritten(&old, &decision_2_changed(&selected, None)),
        ["DEC-fix-002"]
    );
    // 記録が1件増えた → 通る。
    assert!(overwritten(&old, &decision_2_changed(&selected, Some("['誤字の修正']"))).is_empty());
    // 中身ごとに同じ（理由・案・制約・帰結）。
    for change in [
        [("rationale = '理由'", "rationale = '直した理由'")],
        [("options = ['a']", "options = ['a', 'c']")],
    ] {
        assert_eq!(
            overwritten(&old, &decision_2_changed(&change, None)),
            ["DEC-fix-002"],
            "{change:?}"
        );
        assert!(
            overwritten(&old, &decision_2_changed(&change, Some("['直した']"))).is_empty(),
            "{change:?}"
        );
    }
    let with_constraint = |c: &str| {
        design_of(&with(
            "meta/decisions/DEC-fix-002.toml",
            &format!("{DECISION_2}constraint_label = '名'\nconstraint = '{c}'\n"),
        ))
    };
    assert_eq!(
        overwritten(&with_constraint("A"), &with_constraint("B")),
        ["DEC-fix-002"]
    );
    let with_consequences = |c: &str| {
        design_of(&with(
            "meta/decisions/DEC-fix-002.toml",
            &format!("{DECISION_2}consequences = ['{c}']\n"),
        ))
    };
    assert_eq!(
        overwritten(&with_consequences("A"), &with_consequences("B")),
        ["DEC-fix-002"]
    );
}

/// 増えていない（同じ件数のまま・減った・空の文字列を足した）なら、記録があっても落とす。
/// 中身を変えていなければ、記録が増えても何も出ない。
#[test]
fn 記録が増えていなければ上書きとして出る() {
    let selected = [("selected = 'a'", "selected = 'b'")];
    // 古い版にすでに1件ある。
    let old = decision_2_changed(&[], Some("['前の修正']"));
    assert_eq!(
        overwritten(&old, &decision_2_changed(&selected, Some("['前の修正']"))),
        ["DEC-fix-002"],
        "件数が同じなら、前の記録は今回の修正を言っていない"
    );
    assert_eq!(
        overwritten(&old, &decision_2_changed(&selected, None)),
        ["DEC-fix-002"],
        "記録を消しながら書き換えても通さない"
    );
    assert!(
        overwritten(
            &old,
            &decision_2_changed(&selected, Some("['前の修正', '今回の修正']"))
        )
        .is_empty()
    );
    // 空の文字列は記録として数えない。
    assert_eq!(
        overwritten(&base(), &decision_2_changed(&selected, Some("['', '  ']"))),
        ["DEC-fix-002"]
    );
    // 中身を変えていなければ、記録が増えても出ない（記録だけを足した変更）。
    assert!(overwritten(&base(), &decision_2_changed(&[], Some("['足しただけ']"))).is_empty());
}

/// 判断の周りの記録（状態・影響先・置き換え）を変えても、上書きではない。
#[test]
fn 判断の周りの記録の変更は上書きではない() {
    let old = base();
    let new = decision_2_changed(&[("status = 'accepted'", "status = 'superseded'")], None);
    assert!(overwritten(&old, &new).is_empty());
    let body = format!("{DECISION_2}affects_requirements = ['TR-fix-02']\n");
    let new = design_of(&with("meta/decisions/DEC-fix-002.toml", &body));
    assert!(overwritten(&old, &new).is_empty());
}

/// 欄は、判断の schema に省略可能な欄として登録してある。 無くても読み込みの検査は通る。
#[test]
fn 意味を変えない修正の欄は省略可能で型つきで読める() {
    let decision = spec("decision").expect("写し方がある");
    assert!(decision.knows("meaning_preserving_edits"));
    assert!(matches!(
        decision.field("meaning_preserving_edits").map(|f| f.role),
        Some(Role::Data)
    ));
    // 他の schema には持たせない。
    assert!(
        !spec("question")
            .expect("写し方がある")
            .knows("meaning_preserving_edits")
    );
    // 欄が無い既存の判断は、読み込みの検査を通り、記録は0件。
    let design = base();
    assert_eq!(design.audit(), Vec::new());
    let edits = |d: &Design| {
        d.object(&id("DEC-fix-002"))
            .and_then(|o| o.as_decision())
            .map(|d| d.meaning_preserving_edits())
    };
    assert_eq!(edits(&design), Some(Vec::new()));
    let new = decision_2_changed(&[], Some("['一件目', '', '二件目']"));
    assert_eq!(new.audit(), Vec::new());
    assert_eq!(
        edits(&new),
        Some(vec!["一件目".to_owned(), "二件目".to_owned()])
    );
}
