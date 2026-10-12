//! D06 の Pattern View の試験。 fixture の旧形の ID は `fix` を領域にする（`check-references` が
//! 拾わない綴り）。 目標の形の ID は opaque の6文字。 実在する meta を読ませる試験が最後に1本ある。

use std::collections::BTreeMap;
use std::path::Path;

use super::*;
use crate::diagnostic::Report;
use crate::graph::{Direction, RelationClass, RelationRule};
use crate::knowledge::design::{Standing, TARGET_SHAPES, registry};
use crate::knowledge::{KnowledgeSnapshot, load_view_with};
use crate::repo::RepoView;

/// fixture の meta を組み立てる。 各記録を1件1ファイルで足す。
#[derive(Default)]
struct Fx {
    files: BTreeMap<String, String>,
    legacy_requirements: Vec<String>,
}

impl Fx {
    /// 判断。 `lines` は TOML の行そのもの。 `status` と `review_triggers` を渡さなければ、
    /// 受理済みで、ほかと一致しない見直し条件を持つ。
    fn dec(mut self, id: &str, lines: &[&str]) -> Self {
        let has = |key: &str| lines.iter().any(|l| l.starts_with(key));
        let mut body = format!(
            "schema = 'decision'\nid = '{id}'\ntitle = '{id}'\nowner = 'x'\n\
             options = ['a']\nselected = 'a'\nrationale = '理由'\n"
        );
        if !has("status") {
            body.push_str("status = 'accepted'\n");
        }
        if !has("review_triggers") {
            body.push_str(&format!("review_triggers = ['{id} だけの条件']\n"));
        }
        for line in lines {
            body.push_str(line);
            body.push('\n');
        }
        self.files.insert(format!("meta/decisions/{id}.toml"), body);
        self
    }

    /// 旧形の問い。 閉じた判断を `closed_by` で指す。
    fn question(mut self, id: &str, closed_by: &str) -> Self {
        let body = format!(
            "schema = 'question'\nid = '{id}'\ntitle = '{id}'\nstatus = 'closed'\nowner = 'x'\n\
             why_it_matters = 'なぜ'\nhow_to_close = 'どう'\nclosed_by = '{closed_by}'\n"
        );
        self.files.insert(format!("meta/questions/{id}.toml"), body);
        self
    }

    fn evidence(mut self, id: &str) -> Self {
        let body = format!(
            "schema = 'evidence'\nid = '{id}'\ntitle = '{id}'\nkind = 'measurement'\n\
             source = '出典'\nprovenance = '出どころ'\nconfidence = 'Fact'\n"
        );
        self.files.insert(format!("meta/evidence/{id}.toml"), body);
        self
    }

    fn hypothesis(mut self, id: &str) -> Self {
        let body = format!(
            "schema = 'design-hypothesis'\nid = '{id}'\n\
             hypothesis_kind = 'mechanical-formal-property'\nstatement = '予測'\n"
        );
        self.files
            .insert(format!("meta/hypotheses/{id}.toml"), body);
        self
    }

    /// 目標の形の要件。
    fn requirement(mut self, id: &str) -> Self {
        let body = format!("schema = 'requirement'\nid = '{id}'\nstatement = '義務'\n");
        self.files
            .insert(format!("meta/requirements/{id}.toml"), body);
        self
    }

    /// 旧形の技術要件。 1つの収集ファイルにまとまる。
    fn legacy_requirement(mut self, id: &str) -> Self {
        self.legacy_requirements.push(format!(
            "\n[[requirement]]\nid = '{id}'\ntitle = '{id}'\nconfidence = 'Fact'\nstatement = '本文'\n"
        ));
        self
    }

    fn view(&self) -> RepoView {
        let mut files = self.files.clone();
        if !self.legacy_requirements.is_empty() {
            let mut body =
                String::from("schema = 'requirement-set'\ndomain = 'fix'\nprefix = 'TR-fix'\n");
            body.extend(self.legacy_requirements.iter().map(String::as_str));
            files.insert("meta/requirements/fix.toml".to_owned(), body);
        }
        RepoView::in_memory(Path::new("/fixture"), files)
    }

    fn pattern(&self) -> Pattern {
        let design = Design::load(&self.view());
        Pattern::build(&design, &design.graph())
    }
}

fn id(raw: &str) -> Id {
    raw.parse().expect("ID の形")
}

/// source は昇順で並ぶので、期待値も並べ直して渡す。
fn ids(raws: &[&str]) -> Vec<Id> {
    let mut out: Vec<Id> = raws.iter().map(|r| id(r)).collect();
    out.sort();
    out
}

/// どの候補も、source を持ち、重複なく昇順で、種類と規則が揃っている。
fn assert_well_formed(pattern: &Pattern) {
    for c in &pattern.candidates {
        assert!(!c.sources.is_empty(), "{c:?}");
        let mut sorted = c.sources.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(c.sources, sorted, "{c:?}");
        let shared_matches = matches!(
            (&c.kind, &c.shared),
            (
                PatternKind::ConvergentRequirement | PatternKind::SharedPremise,
                Shared::Object(_),
            ) | (PatternKind::RepeatedShape, Shared::Shape(_))
                | (PatternKind::RepeatedCondition, Shared::Condition { .. })
        );
        assert!(shared_matches, "{c:?}");
    }
    let mut sorted = pattern.candidates.clone();
    sorted.sort();
    assert_eq!(pattern.candidates, sorted, "候補は決まった順で並ぶ");
}

fn only(pattern: &Pattern, kind: PatternKind) -> &Candidate {
    let found: Vec<&Candidate> = pattern.of_kind(kind).collect();
    assert_eq!(found.len(), 1, "{kind:?}: {found:?}");
    found[0]
}

// ## 規則1 同じ要件への収束

#[test]
fn 複数の判断が同じ要件を変えていれば収束の候補になる() {
    let pattern = Fx::default()
        .legacy_requirement("TR-fix-01")
        .legacy_requirement("TR-fix-02")
        .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
        .dec("DEC-fix-002", &["revises_requirements = ['TR-fix-01']"])
        .dec("DEC-fix-003", &["removes_requirements = ['TR-fix-01']"])
        // 別の要件を1件だけ変えている。 収束していない。
        .dec("DEC-fix-004", &["establishes_requirements = ['TR-fix-02']"])
        .pattern();
    assert_well_formed(&pattern);
    let c = only(&pattern, PatternKind::ConvergentRequirement);
    assert_eq!(c.rule, RULE_REQUIREMENT);
    assert_eq!(c.shared, Shared::Object(id("TR-fix-01")));
    assert_eq!(
        c.sources,
        ids(&["DEC-fix-001", "DEC-fix-002", "DEC-fix-003", "TR-fix-01"])
    );
    assert_eq!(pattern.candidates.len(), 1);
}

#[test]
fn 判断が1件だけなら収束の候補にならない() {
    let pattern = Fx::default()
        .legacy_requirement("TR-fix-01")
        .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

#[test]
fn 同じ判断が同じ要件を2通りに変えても1件として数える() {
    let pattern = Fx::default()
        .legacy_requirement("TR-fix-01")
        .dec(
            "DEC-fix-001",
            &[
                "establishes_requirements = ['TR-fix-01']",
                "revises_requirements = ['TR-fix-01']",
            ],
        )
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

/// `affects` は論理的含意ではない（D00 の語彙）。 同じ要件に影響すると書いてあるだけでは、
/// 収束とは言えない。
#[test]
fn affects_だけでは候補にならない() {
    let pattern = Fx::default()
        .legacy_requirement("TR-fix-01")
        .dec("DEC-fix-001", &["affects_requirements = ['TR-fix-01']"])
        .dec("DEC-fix-002", &["affects_requirements = ['TR-fix-01']"])
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

// ## 規則2 同じ経験的前提への依存

#[test]
fn 同じ仮説に依る判断が複数あれば候補になる() {
    let pattern = Fx::default()
        .hypothesis("HYP-3A1B2C")
        .hypothesis("HYP-4D5E6F")
        .dec("DEC-fix-001", &["relies_on_hypotheses = ['HYP-3A1B2C']"])
        .dec("DEC-fix-002", &["relies_on_hypotheses = ['HYP-3A1B2C']"])
        // 別の仮説に1件だけ依る。
        .dec("DEC-fix-003", &["relies_on_hypotheses = ['HYP-4D5E6F']"])
        .pattern();
    assert_well_formed(&pattern);
    let c = only(&pattern, PatternKind::SharedPremise);
    assert_eq!(c.rule, RULE_HYPOTHESIS);
    assert_eq!(c.shared, Shared::Object(id("HYP-3A1B2C")));
    assert_eq!(
        c.sources,
        ids(&["DEC-fix-001", "DEC-fix-002", "HYP-3A1B2C"])
    );
    assert_eq!(pattern.candidates.len(), 1);
}

#[test]
fn 同じ調査を考慮した判断が複数あれば候補になる() {
    let pattern = Fx::default()
        .evidence("EVID-fix-001")
        .evidence("EVID-fix-002")
        .dec("DEC-fix-001", &["evidence = ['EVID-fix-001']"])
        .dec("DEC-fix-002", &["considers_evidence = ['EVID-fix-001']"])
        .dec("DEC-fix-003", &["evidence = ['EVID-fix-002']"])
        .pattern();
    assert_well_formed(&pattern);
    let c = only(&pattern, PatternKind::SharedPremise);
    assert_eq!(c.rule, RULE_EVIDENCE);
    assert_eq!(c.shared, Shared::Object(id("EVID-fix-001")));
    assert_eq!(
        c.sources,
        ids(&["DEC-fix-001", "DEC-fix-002", "EVID-fix-001"])
    );
}

/// 調査の側が `supports_decisions` で書いても、`considers` と同じ edge になる。
#[test]
fn 調査の側から書いた考慮も同じ前提として数える() {
    let mut fx = Fx::default()
        .dec("DEC-fix-001", &[])
        .dec("DEC-fix-002", &[]);
    fx.files.insert(
        "meta/evidence/EVID-fix-001.toml".to_owned(),
        "schema = 'evidence'\nid = 'EVID-fix-001'\ntitle = '調査'\nkind = 'measurement'\n\
         source = '出典'\nprovenance = '出どころ'\nconfidence = 'Fact'\n\
         supports_decisions = ['DEC-fix-001', 'DEC-fix-002']\n"
            .to_owned(),
    );
    let pattern = fx.pattern();
    let c = only(&pattern, PatternKind::SharedPremise);
    assert_eq!(c.rule, RULE_EVIDENCE);
}

#[test]
fn 判断が1件だけ依る前提は候補にならない() {
    let pattern = Fx::default()
        .hypothesis("HYP-3A1B2C")
        .evidence("EVID-fix-001")
        .dec(
            "DEC-fix-001",
            &[
                "relies_on_hypotheses = ['HYP-3A1B2C']",
                "evidence = ['EVID-fix-001']",
            ],
        )
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

// ## 規則3 同じ形の推論の繰り返し

/// Q → DEC → EVID の連なり。 ID は引数で変える。
fn chain(fx: Fx, q: &str, dec: &str, evid: &str) -> Fx {
    fx.question(q, dec)
        .dec(dec, &[&format!("evidence = ['{evid}']")])
        .evidence(evid)
}

#[test]
fn 異なる_q_から同じ形の推論が辿れれば候補になる() {
    let fx = Fx::default();
    let fx = chain(fx, "Q-fix-001", "DEC-fix-001", "EVID-fix-001");
    let fx = chain(fx, "Q-fix-002", "DEC-fix-002", "EVID-fix-002");
    // 判断が調査を2件引く。 形が違う。
    let fx = fx
        .question("Q-fix-003", "DEC-fix-003")
        .dec(
            "DEC-fix-003",
            &["evidence = ['EVID-fix-003', 'EVID-fix-004']"],
        )
        .evidence("EVID-fix-003")
        .evidence("EVID-fix-004");
    let pattern = fx.pattern();
    assert_well_formed(&pattern);
    let c = only(&pattern, PatternKind::RepeatedShape);
    assert_eq!(c.rule, RULE_SHAPE);
    assert_eq!(
        c.sources,
        ids(&[
            "Q-fix-001",
            "Q-fix-002",
            "DEC-fix-001",
            "DEC-fix-002",
            "EVID-fix-001",
            "EVID-fix-002"
        ]),
        "形の違う Q-fix-003 は含まれない"
    );
    let Shared::Shape(shape) = &c.shared else {
        panic!("形が共有されている: {c:?}");
    };
    assert_eq!(shape.edge_count(), 2);
    assert_eq!(shape.nodes.get(&Kind::Question), Some(&1));
    assert_eq!(shape.nodes.get(&Kind::Decision), Some(&1));
    assert_eq!(shape.nodes.get(&Kind::Evidence), Some(&1));
    assert_eq!(
        shape
            .edges
            .get(&(Kind::Question, "resolved_by", Kind::Decision)),
        Some(&1)
    );
    assert_eq!(
        shape
            .edges
            .get(&(Kind::Decision, "considers", Kind::Evidence)),
        Some(&1)
    );
}

#[test]
fn id_だけ違って形も違えば候補にならない() {
    // 片方は調査を引き、もう片方は仮説に依る。 relation の名前が違う。
    let fx = Fx::default().hypothesis("HYP-3A1B2C");
    let fx = chain(fx, "Q-fix-001", "DEC-fix-001", "EVID-fix-001");
    let fx = fx
        .question("Q-fix-002", "DEC-fix-002")
        .dec(
            "DEC-fix-002",
            &[
                "relies_on_hypotheses = ['HYP-3A1B2C']",
                "establishes_requirements = ['TR-fix-01']",
            ],
        )
        .legacy_requirement("TR-fix-01");
    let pattern = fx.pattern();
    assert_eq!(pattern.of_kind(PatternKind::RepeatedShape).count(), 0);
}

#[test]
fn 形は_id_も本文も含めない() {
    let fx = Fx::default();
    let fx = chain(fx, "Q-fix-001", "DEC-fix-001", "EVID-fix-001");
    let fx = chain(fx, "Q-fix-002", "DEC-fix-002", "EVID-fix-002");
    let pattern = fx.pattern();
    let Shared::Shape(shape) = &only(&pattern, PatternKind::RepeatedShape).shared else {
        panic!("形");
    };
    // 形は種類と relation の名前の多重集合だけ。 型の上でも ID を持てない。
    let _: &BTreeMap<Kind, usize> = &shape.nodes;
    let _: &BTreeMap<(Kind, &'static str, Kind), usize> = &shape.edges;
}

/// Q から判断へ1本引いただけの形は、すべての Q が持つ。 候補にしても何も言っていない。
#[test]
fn 辺が1本だけの形は候補にならない() {
    let pattern = Fx::default()
        .question("Q-fix-001", "DEC-fix-001")
        .dec("DEC-fix-001", &[])
        .question("Q-fix-002", "DEC-fix-002")
        .dec("DEC-fix-002", &[])
        .pattern();
    assert_eq!(pattern.of_kind(PatternKind::RepeatedShape).count(), 0);
}

#[test]
fn 一つの_q_だけの形は候補にならない() {
    let pattern = chain(Fx::default(), "Q-fix-001", "DEC-fix-001", "EVID-fix-001").pattern();
    assert_eq!(pattern.of_kind(PatternKind::RepeatedShape).count(), 0);
}

// ## 規則4 同じ見直し条件の反復

#[test]
fn 空白を畳んで一致する見直し条件は候補になる() {
    let pattern = Fx::default()
        .dec("DEC-fix-001", &["review_triggers = ['  同じ   条件 ']"])
        .dec(
            "DEC-fix-002",
            &["review_triggers = ['別の条件', '同じ\t条件']"],
        )
        .dec("DEC-fix-003", &["review_triggers = ['同じではない条件']"])
        .pattern();
    assert_well_formed(&pattern);
    let c = only(&pattern, PatternKind::RepeatedCondition);
    assert_eq!(c.rule, RULE_CONDITION);
    assert_eq!(
        c.shared,
        Shared::Condition {
            field: "review_triggers",
            text: "同じ 条件".to_owned()
        }
    );
    assert_eq!(c.sources, ids(&["DEC-fix-001", "DEC-fix-002"]));
}

#[test]
fn 一致しない条件と1件の判断の中の重複は候補にならない() {
    let pattern = Fx::default()
        .dec("DEC-fix-001", &["review_triggers = ['条件A', '条件A']"])
        .dec("DEC-fix-002", &["review_triggers = ['条件B', '']"])
        .dec(
            "DEC-fix-003",
            &["review_triggers = ['条件 B ですらない', ' ']"],
        )
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

/// 空白だけの条件が一致しても、条件ではない。
#[test]
fn 空の条件は数えない() {
    let pattern = Fx::default()
        .dec("DEC-fix-001", &["review_triggers = [' ']"])
        .dec("DEC-fix-002", &["review_triggers = ['  ']"])
        .pattern();
    assert_eq!(pattern.candidates, Vec::new());
}

// ## 現在有効でない object

/// 収束の候補から、後継を持つ判断が外れる。 後継が同じ要件を変えていれば組み直される。
#[test]
fn 判断が_superseded_になると収束の候補から外れ_後継で組み直される() {
    let base = || {
        Fx::default()
            .legacy_requirement("TR-fix-01")
            .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
    };
    // 置き換えられる前。
    let before = base()
        .dec("DEC-fix-002", &["revises_requirements = ['TR-fix-01']"])
        .pattern();
    assert_eq!(
        only(&before, PatternKind::ConvergentRequirement).sources,
        ids(&["DEC-fix-001", "DEC-fix-002", "TR-fix-01"])
    );

    // 置き換えた。 後継は要件を変えていない。 候補は消える。
    let gone = base()
        .dec(
            "DEC-fix-002",
            &[
                "status = 'superseded'",
                "superseded_by = 'DEC-fix-003'",
                "revises_requirements = ['TR-fix-01']",
            ],
        )
        .dec("DEC-fix-003", &["supersedes = ['DEC-fix-002']"])
        .pattern();
    assert_eq!(gone.candidates, Vec::new());

    // 後継が同じ要件を変えている。 古い判断は source から外れ、後継が入る。
    let rebuilt = base()
        .dec(
            "DEC-fix-002",
            &[
                "status = 'superseded'",
                "superseded_by = 'DEC-fix-003'",
                "revises_requirements = ['TR-fix-01']",
            ],
        )
        .dec(
            "DEC-fix-003",
            &[
                "supersedes = ['DEC-fix-002']",
                "revises_requirements = ['TR-fix-01']",
            ],
        )
        .pattern();
    assert_eq!(
        only(&rebuilt, PatternKind::ConvergentRequirement).sources,
        ids(&["DEC-fix-001", "DEC-fix-003", "TR-fix-01"])
    );
}

/// 後継の edge だけ、状態の欄だけ、のどちらでも現在有効として数えない。
#[test]
fn 後継の関係と状態の欄は片方だけでも効く() {
    let with = |old_lines: &[&str], new_lines: &[&str]| {
        Fx::default()
            .legacy_requirement("TR-fix-01")
            .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
            .dec(
                "DEC-fix-002",
                &[&["establishes_requirements = ['TR-fix-01']"], old_lines].concat(),
            )
            .dec("DEC-fix-003", new_lines)
            .pattern()
    };
    // 後継が `supersedes` を書いただけで、古い判断はまだ受理済みと名乗っている。
    let edge_only = with(&[], &["supersedes = ['DEC-fix-002']"]);
    assert_eq!(edge_only.candidates, Vec::new());
    // 古い判断が `superseded` と名乗っているだけで、後継は何も書いていない。
    let status_only = with(&["status = 'superseded'"], &[]);
    assert_eq!(status_only.candidates, Vec::new());
    // `superseded_by` だけ。
    let by_only = with(&["superseded_by = 'DEC-fix-003'"], &[]);
    assert_eq!(by_only.candidates, Vec::new());
    // 何も無ければ、候補は立つ。
    let neither = with(&[], &[]);
    assert_eq!(
        neither.of_kind(PatternKind::ConvergentRequirement).count(),
        1
    );
}

#[test]
fn 前提を共有する判断が_superseded_になると候補が消える() {
    let fx = |status_lines: &[&str]| {
        Fx::default()
            .hypothesis("HYP-3A1B2C")
            .dec("DEC-fix-001", &["relies_on_hypotheses = ['HYP-3A1B2C']"])
            .dec(
                "DEC-fix-002",
                &[&["relies_on_hypotheses = ['HYP-3A1B2C']"], status_lines].concat(),
            )
            .pattern()
    };
    assert_eq!(fx(&[]).of_kind(PatternKind::SharedPremise).count(), 1);
    let gone = fx(&["status = 'superseded'"]);
    assert_eq!(gone.candidates, Vec::new());
}

#[test]
fn 形の候補は_q_を閉じた判断が_superseded_になると消え_後継で組み直される() {
    let first = || chain(Fx::default(), "Q-fix-001", "DEC-fix-001", "EVID-fix-001");
    // 2つ目の Q を閉じる判断を置き換える。 `closed_by` はまだ古い判断を指している。
    let superseded = chain(first(), "Q-fix-002", "DEC-fix-002", "EVID-fix-002")
        .dec(
            "DEC-fix-002",
            &[
                "status = 'superseded'",
                "superseded_by = 'DEC-fix-004'",
                "evidence = ['EVID-fix-002']",
            ],
        )
        .dec("DEC-fix-004", &["supersedes = ['DEC-fix-002']"])
        .pattern();
    assert_eq!(superseded.of_kind(PatternKind::RepeatedShape).count(), 0);

    // 問いを後継の判断で閉じ直した。 後継が調査を引いていれば、形は組み直される。
    let rebuilt = first()
        .question("Q-fix-002", "DEC-fix-004")
        .dec(
            "DEC-fix-002",
            &[
                "status = 'superseded'",
                "superseded_by = 'DEC-fix-004'",
                "evidence = ['EVID-fix-002']",
            ],
        )
        .dec(
            "DEC-fix-004",
            &[
                "supersedes = ['DEC-fix-002']",
                "evidence = ['EVID-fix-002']",
            ],
        )
        .evidence("EVID-fix-002")
        .pattern();
    let c = only(&rebuilt, PatternKind::RepeatedShape);
    assert!(c.sources.contains(&id("DEC-fix-004")));
    assert!(!c.sources.contains(&id("DEC-fix-002")));
}

#[test]
fn 条件を共有する判断が_superseded_になると候補が消える() {
    let fx = |lines: &[&str]| {
        Fx::default()
            .dec("DEC-fix-001", &["review_triggers = ['同じ条件']"])
            .dec(
                "DEC-fix-002",
                &[&["review_triggers = ['同じ条件']"], lines].concat(),
            )
            .pattern()
    };
    assert_eq!(fx(&[]).of_kind(PatternKind::RepeatedCondition).count(), 1);
    assert_eq!(fx(&["status = 'superseded'"]).candidates, Vec::new());
}

// ## 旧形と目標の形

/// 旧形（`TR-`・`DEC-<領域>-<番号>`）と目標の形（`REQ-`・`DEC-<opaque>`）が混ざっていても、
/// 同じ規則で候補が出る。
#[test]
fn 旧形と目標の形が混ざっても同じ規則で候補が出る() {
    let pattern = Fx::default()
        .legacy_requirement("TR-fix-01")
        .requirement("REQ-5H8NQ2")
        .hypothesis("HYP-3A1B2C")
        // 旧形の判断と目標の形の判断が、旧形の要件を変える。
        .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
        .dec("DEC-6CQ3NA", &["revises_requirements = ['TR-fix-01']"])
        // 旧形の判断と目標の形の判断が、目標の形の要件を変える。
        .dec(
            "DEC-fix-002",
            &["establishes_requirements = ['REQ-5H8NQ2']"],
        )
        .dec("DEC-7DR4PB", &["establishes_requirements = ['REQ-5H8NQ2']"])
        // 同じ仮説に依る。
        .dec("DEC-fix-003", &["relies_on_hypotheses = ['HYP-3A1B2C']"])
        .dec("DEC-8ES5QC", &["relies_on_hypotheses = ['HYP-3A1B2C']"])
        .pattern();
    assert_well_formed(&pattern);
    let convergent: Vec<&Candidate> = pattern
        .of_kind(PatternKind::ConvergentRequirement)
        .collect();
    assert_eq!(convergent.len(), 2);
    assert_eq!(
        convergent
            .iter()
            .map(|c| c.shared.clone())
            .collect::<Vec<_>>(),
        // 並びは `Id` の順（旧形が先）。
        vec![
            Shared::Object(id("REQ-5H8NQ2")),
            Shared::Object(id("TR-fix-01"))
        ]
    );
    assert_eq!(only(&pattern, PatternKind::SharedPremise).sources.len(), 3);
}

// ## 引用だけの関係

/// `mentions`（引用）は、規範的な構造を言わない。 同じ要件を引用しているだけの判断は、
/// 収束とは言えない。 D00 の組み立てはまだ `mentions` の edge を作らないので、
/// 欄を足した registry で graph を作って確かめる。 対照に、同じ欄を `establishes` にした
/// registry では候補が立つ。
#[test]
fn mentions_だけの関係では候補を作らない() {
    let fx = Fx::default()
        .legacy_requirement("TR-fix-01")
        .dec("DEC-fix-001", &["see_also = ['TR-fix-01']"])
        .dec("DEC-fix-002", &["see_also = ['TR-fix-01']"]);

    let with_relation = |relation: &'static str| {
        let view = fx.view();
        let design = Design::load(&view);
        let mut rep = Report::default();
        let entries = load_view_with(&view, TARGET_SHAPES, &mut rep);
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        let mut registry = registry();
        registry.relations.push(RelationRule {
            from_schema: "decision",
            field: "see_also",
            relation,
            class: RelationClass::Reference,
            direction: Direction::Forward,
        });
        let graph = SemanticGraph::from_snapshot(&snapshot, &registry);
        assert!(
            graph.all_relations().any(|r| r.name == relation),
            "{relation} の edge が張られている"
        );
        Pattern::build(&design, &graph)
    };

    assert_eq!(with_relation("mentions").candidates, Vec::new());
    assert_eq!(
        with_relation("establishes")
            .of_kind(PatternKind::ConvergentRequirement)
            .count(),
        1
    );
}

// ## 表と不変条件

/// 読む relation はすべて D00 の正規の語彙にある。 載せ忘れ・仮の名前・綴りの違いで、
/// 何も言わずに0件になるのを防ぐ。
#[test]
fn 表の_relation_は語彙の正規の関係だけ() {
    assert!(!BASES.is_empty());
    for basis in BASES {
        let spec =
            relation(basis.relation).unwrap_or_else(|| panic!("{} が語彙に無い", basis.relation));
        assert_eq!(spec.standing, Standing::Canonical, "{}", basis.relation);
        assert!(
            ![
                "mentions",
                "affects",
                "provenance",
                "context_for",
                "constrains"
            ]
            .contains(&basis.relation),
            "{}",
            basis.relation
        );
    }
    let names: Vec<&str> = BASES.iter().map(|b| b.relation).collect();
    let unique: std::collections::BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(names.len(), unique.len(), "表の relation は一意");
    // 規則1は DEC から REQ へ、規則2は DEC から HYP・EVID へ。 向きと種類は語彙が持つ。
    for basis in BASES {
        let spec = relation(basis.relation).expect("語彙にある");
        if basis.changes_requirement {
            assert!(spec.from.admits(Kind::Decision) && spec.to.admits(Kind::Requirement));
        }
        if basis.premise.is_some() {
            assert!(spec.from.admits(Kind::Decision));
            assert!(spec.to.admits(Kind::Hypothesis) || spec.to.admits(Kind::Evidence));
        }
    }
}

#[test]
fn 規則の名前は種類ごとに一意で_種類の名前は重ならない() {
    let rules = [
        RULE_REQUIREMENT,
        RULE_HYPOTHESIS,
        RULE_EVIDENCE,
        RULE_SHAPE,
        RULE_CONDITION,
    ];
    let unique: std::collections::BTreeSet<&str> = rules.iter().copied().collect();
    assert_eq!(unique.len(), rules.len());
    let kinds: std::collections::BTreeSet<&str> =
        PatternKind::ALL.iter().map(|k| k.name()).collect();
    assert_eq!(kinds.len(), PatternKind::ALL.len());
}

#[test]
fn 何も無い_repository_では候補も無い() {
    let design = Design::load(&RepoView::in_memory(Path::new("/fixture"), BTreeMap::new()));
    let pattern = Pattern::build(&design, &design.graph());
    assert_eq!(pattern.candidates, Vec::new());
    assert_eq!(pattern.summary(), PatternSummary::default());
}

#[test]
fn 件数は種類ごとに数える() {
    let fx = Fx::default()
        .legacy_requirement("TR-fix-01")
        .hypothesis("HYP-3A1B2C");
    let fx = chain(fx, "Q-fix-001", "DEC-fix-001", "EVID-fix-001");
    let fx = chain(fx, "Q-fix-002", "DEC-fix-002", "EVID-fix-001");
    let fx = fx
        .dec(
            "DEC-fix-010",
            &[
                "establishes_requirements = ['TR-fix-01']",
                "review_triggers = ['同じ']",
            ],
        )
        .dec(
            "DEC-fix-011",
            &[
                "establishes_requirements = ['TR-fix-01']",
                "review_triggers = ['同じ']",
            ],
        );
    let summary = fx.pattern().summary();
    assert_eq!(
        summary,
        PatternSummary {
            convergent_requirement: 1,
            // 2つの判断が同じ EVID-fix-001 を引いている。
            shared_premise: 1,
            repeated_shape: 1,
            repeated_condition: 1,
        }
    );
}

#[test]
fn 同じ入力からは同じ候補が出る() {
    let fx = Fx::default()
        .legacy_requirement("TR-fix-01")
        .dec("DEC-fix-001", &["establishes_requirements = ['TR-fix-01']"])
        .dec("DEC-fix-002", &["establishes_requirements = ['TR-fix-01']"]);
    assert_eq!(fx.pattern(), fx.pattern());
}

/// 実在する meta を読ませる。 件数には依存せず、落ちずに回ること、どの候補も source を持ち、
/// その source が graph に実在することだけを見る。
#[test]
fn 実在する_meta_で_view_が回る() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let design = Design::load(&RepoView::working_tree(root));
    let graph = design.graph();
    let pattern = Pattern::build(&design, &graph);
    assert_well_formed(&pattern);
    for c in &pattern.candidates {
        for source in &c.sources {
            assert!(
                graph.node(&NodeKey::Object(source.clone())).is_some(),
                "{c:?} の source {source} が graph に無い"
            );
            assert!(
                design.is_current(&graph, &NodeKey::Object(source.clone())),
                "{c:?} の source {source} が現在有効でない"
            );
        }
    }
    let summary = pattern.summary();
    assert_eq!(
        summary.convergent_requirement
            + summary.shared_premise
            + summary.repeated_shape
            + summary.repeated_condition,
        pattern.candidates.len()
    );
}
