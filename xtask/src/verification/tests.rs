//! D02 の試験。 fixture は `RepoView::in_memory` で組む。 ID は目標の形（`<接頭辞>-<opaque>`）で、
//! `check-references` が拾わない綴りにしている。 実在する資格の表を読ませる試験が最後にある。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use super::*;
use crate::diagnostic::Report;
use crate::graph::NodeKey;
use crate::knowledge::design::{Design, TARGET_SHAPES};
use crate::knowledge::{Id, KnowledgeSnapshot, load, load_view_with};
use crate::probe::{Counts, ExecutionContext, NotRunReason, Receipt, Status};
use crate::repo::RepoView;

/// 実在する資格の表。 試験の資格の判定は、ここに書いてあるとおりに掛ける。
const REGISTRY: &str = include_str!("../../../meta/verification/capabilities.toml");

const H_COMPREHENSION: &str = "HYP-S00001";
const H_FORMAL: &str = "HYP-F00002";
const H_AUDIO: &str = "HYP-A00003";
const H_EMOTION: &str = "HYP-E00004";
const H_WORKFLOW: &str = "HYP-W00005";

fn arr(items: &[&str]) -> String {
    let quoted: Vec<String> = items.iter().map(|s| format!("'{s}'")).collect();
    format!("[{}]", quoted.join(", "))
}

/// メモリ上のリポジトリ。 資格の表は実物を入れる。
struct Fx {
    files: BTreeMap<String, String>,
}

impl Fx {
    fn new() -> Self {
        Self::bare().put("meta/verification/capabilities.toml", REGISTRY)
    }

    fn bare() -> Self {
        Self {
            files: BTreeMap::new(),
        }
    }

    fn put(mut self, path: &str, body: &str) -> Self {
        self.files.insert(path.to_owned(), body.to_owned());
        self
    }

    fn hyp(self, id: &str, kind: &str, supporting: &[&str], contradicting: &[&str]) -> Self {
        let body = format!(
            "schema = 'design-hypothesis'\nid = '{id}'\nhypothesis_kind = '{kind}'\n\
             statement = '予測'\nassessment = 'untested'\n\
             supporting_evidence = {}\ncontradicting_evidence = {}\n",
            arr(supporting),
            arr(contradicting),
        );
        self.put(&format!("meta/hypotheses/{id}.toml"), &body)
    }

    /// `extra` は、観測の欄（`method` など）をそのまま書く。
    fn evid(self, id: &str, extra: &str) -> Self {
        let body = format!(
            "schema = 'evidence'\nid = '{id}'\ntitle = '調査'\nkind = 'observation'\n\
             source = '出典'\nprovenance = '出どころ'\nconfidence = 'Fact'\n{extra}\n"
        );
        self.put(&format!("meta/evidence/{id}.toml"), &body)
    }

    fn dec(self, id: &str, status: &str, relies: &[&str], requirements: &[&str]) -> Self {
        let body = format!(
            "schema = 'decision'\nid = '{id}'\ntitle = '判断'\nstatus = '{status}'\nowner = 'x'\n\
             options = ['a']\nselected = 'a'\nrationale = '理由'\nreview_triggers = ['t']\n\
             relies_on_hypotheses = {}\nestablishes_requirements = {}\n",
            arr(relies),
            arr(requirements),
        );
        self.put(&format!("meta/decisions/{id}.toml"), &body)
    }

    fn probe(self, id: &str, extra: &str) -> Self {
        let body = format!("schema = 'probe'\nid = '{id}'\n{extra}\n");
        self.put(&format!("meta/verification/probes/{id}.toml"), &body)
    }

    fn view(&self) -> RepoView {
        RepoView::in_memory(Path::new("/fixture"), self.files.clone())
    }

    fn snapshot(&self) -> KnowledgeSnapshot {
        let mut rep = Report::default();
        let entries = load_view_with(&self.view(), TARGET_SHAPES, &mut rep);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        KnowledgeSnapshot::from_entries(&entries).0
    }

    fn facts(&self) -> Facts {
        Facts::from_snapshot(&self.snapshot())
    }

    fn registry(&self) -> CapabilityRegistry {
        CapabilityRegistry::from_snapshot(&self.snapshot()).0
    }

    fn findings(&self) -> Vec<Finding> {
        findings(&self.snapshot())
    }

    fn codes(&self) -> Vec<&'static str> {
        self.findings().iter().map(Finding::code).collect()
    }
}

// ## 資格の表

#[test]
fn 実在する資格の表は読めて_問題が無い() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let mut rep = Report::default();
    let entries = load(root, &mut rep);
    assert!(
        !rep.has_errors(),
        "実在する meta が読めない: {:?}",
        rep.diagnostics()
    );
    let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
    let (registry, issues) = CapabilityRegistry::from_snapshot(&snapshot);
    assert_eq!(issues, Vec::new());
    // 提案書 10.1 の表の9行。
    let kinds: BTreeSet<&str> = [
        "mechanical-formal-property",
        "visibility-perception",
        "semantic-comprehension",
        "workflow-fit",
        "audio-processing-validity",
        "aesthetic-identity",
        "emotional-response",
        "accessibility-conformance",
        "long-term-appropriation",
    ]
    .into();
    assert_eq!(registry.kinds(), kinds);
    // どの種類にも、支持できる方法が1つはある。 無ければ、その種類の仮説は誰も支持できない。
    for kind in &kinds {
        assert!(
            !registry.supporting_methods(kind).is_empty(),
            "{kind} を支持できる観測方法が表に無い"
        );
    }
    // 4つの語のうち、実際の表が使っているもの。 `context-dependent` は表が名指しした組が無い。
    let used: BTreeSet<Support> = registry.rows().iter().map(|r| r.support).collect();
    assert!(used.contains(&Support::CanSupport));
    assert!(used.contains(&Support::CannotSupport));
    assert!(used.contains(&Support::HypothesisGenerationOnly));
    assert!(!used.contains(&Support::ContextDependent));
    // どの行も、提案書のどこから写したかを持つ。
    assert!(registry.rows().iter().all(|r| r.source.contains("10.")));
}

/// 提案書 10.1.1 と 10.2 が名指しした組は、そのとおりに入っている。
#[test]
fn 提案書が名指しした組は表のとおり() {
    let registry = Fx::new().registry();
    for (kind, method, expected) in [
        (
            "semantic-comprehension",
            "agent-review",
            Capability::CannotSupport,
        ),
        (
            "accessibility-conformance",
            "browser-measurement",
            Capability::CanSupport,
        ),
        (
            "emotional-response",
            "competitive-corpus",
            Capability::HypothesisGenerationOnly,
        ),
        (
            "emotional-response",
            "agent-review",
            Capability::CannotSupport,
        ),
        (
            "semantic-comprehension",
            "synthetic-novice",
            Capability::CannotSupport,
        ),
        (
            "audio-processing-validity",
            "mock",
            Capability::CannotSupport,
        ),
        (
            "mechanical-formal-property",
            "executed-test",
            Capability::CanSupport,
        ),
    ] {
        assert_eq!(registry.assess(kind, method), expected, "{kind} × {method}");
    }
}

/// 表に書いていない組は、昇格しない。 「だめ」と決めたのではなく「決めていない」ので、
/// 支持にならない側に倒す。
#[test]
fn 表に無い組は自動では支持にならない() {
    let registry = Fx::new().registry();
    // どちらの語も表にあるが、この組は表に行が無い。
    for (kind, method) in [
        ("workflow-fit", "self-observation"),
        ("mechanical-formal-property", "human-observation"),
        ("audio-processing-validity", "executed-test"),
    ] {
        let capability = registry.assess(kind, method);
        assert_eq!(capability, Capability::Unregistered, "{kind} × {method}");
        assert!(!capability.supports());
    }
}

#[test]
fn 表に無い語は区別して返る() {
    let registry = Fx::new().registry();
    assert_eq!(
        registry.assess("vibes", "executed-test"),
        Capability::UnknownKind
    );
    assert_eq!(
        registry.assess("semantic-comprehension", "astrology"),
        Capability::UnknownMethod
    );
}

/// `context-dependent` は、必要条件を機械が読めない。 昇格させない。
#[test]
fn context_dependent_は昇格しない() {
    let table = "schema = 'evidence-capability'\n\n[[capability]]\nhypothesis_kind = 'k'\n\
                 method = 'm'\nsupport = 'context-dependent'\nsource = '出典'\n";
    let registry = Fx::bare()
        .put("meta/verification/capabilities.toml", table)
        .registry();
    assert_eq!(registry.assess("k", "m"), Capability::ContextDependent);
    assert!(!registry.assess("k", "m").supports());
}

#[test]
fn 表の誤りは問題として出る() {
    let table = "schema = 'evidence-capability'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'm'\nsupport = 'maybe'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'a'\nsupport = 'can-support'\n\
        observer = 'robot'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'a'\nsupport = 'cannot-support'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k2'\nmethod = 'h'\nsupport = 'can-support'\n\
        observer = 'human'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k3'\nmethod = 'h'\nsupport = 'can-support'\n\
        observer = 'automated'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'n'\nsupport = 5\nsource = 's'\n";
    let fx = Fx::bare().put("meta/verification/capabilities.toml", table);
    let (_, issues) = CapabilityRegistry::from_snapshot(&fx.snapshot());
    let codes: Vec<&'static str> = issues
        .into_iter()
        .map(|i| Finding::Registry(i).code())
        .collect();
    assert_eq!(
        codes,
        [
            "verification.registry-unknown-support",
            "verification.registry-unknown-observer",
            "verification.registry-duplicate-pair",
            "verification.registry-missing-field",
            "verification.registry-observer-conflict",
        ]
    );
}

/// 同じ組が食い違って2行あれば、支持にならない側に倒す。
#[test]
fn 食い違う重複は支持にならない側に倒す() {
    let table = "schema = 'evidence-capability'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'm'\nsupport = 'can-support'\nsource = 's'\n\n\
        [[capability]]\nhypothesis_kind = 'k'\nmethod = 'm'\nsupport = 'cannot-support'\nsource = 's'\n";
    let registry = Fx::bare()
        .put("meta/verification/capabilities.toml", table)
        .registry();
    assert_eq!(registry.assess("k", "m"), Capability::CannotSupport);
}

/// 表は `meta/verification/` に置く。
#[test]
fn 資格の表を別の場所に置くと落ちる() {
    let fx = Fx::bare().put("meta/capabilities.toml", REGISTRY);
    let mut rep = Report::default();
    let entries = load_view_with(&fx.view(), TARGET_SHAPES, &mut rep);
    assert!(entries.is_empty());
    assert!(
        rep.diagnostics()
            .iter()
            .any(|d| d.message.contains("meta/verification/ に置く")),
        "{:?}",
        rep.diagnostics()
    );
}

/// 種類は ID を持たず、node にならない。 語彙には種類として登録されている。
#[test]
fn 資格の表の行は_node_にならない() {
    let registry = crate::knowledge::design::registry();
    assert_eq!(
        registry.kind_for_schema("evidence-capability"),
        Some("capability")
    );
    let design = Design::load(&Fx::new().view());
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    assert_eq!(graph.nodes().count(), 0);
}

// ## 検査

#[test]
fn synthetic_novice_は意味理解を支持できない() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &["EVID-E00001"],
            &[],
        )
        .evid("EVID-E00001", "method = 'synthetic-novice'");
    let found = fx.findings();
    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].code(), "verification.cannot-support");
    assert!(
        matches!(
            &found[0],
            Finding::BeyondCapability {
                capability: Capability::CannotSupport,
                ..
            }
        ),
        "{:?}",
        found[0]
    );
    let message = found[0].message();
    assert!(message.contains("synthetic-novice") && message.contains(H_COMPREHENSION));
    // 診断として積まれ、`check-meta` を落とす。
    let mut rep = Report::default();
    check_verification(&fx.snapshot(), &mut rep);
    assert!(rep.has_errors());
    assert_eq!(rep.diagnostics()[0].code, "verification.cannot-support");
    assert!(rep.diagnostics()[0].location.is_some());
}

/// 支持の言い方は2通りある。 仮説の側が `supporting_evidence` に挙げるか、調査の側が
/// `assesses` で仮説を指す。 どちらも同じ規則で見る。
#[test]
fn 調査の側から評価すると言っても同じ規則が掛かる() {
    let fx = Fx::new()
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .evid(
            "EVID-E00001",
            &format!("method = 'synthetic-novice'\nassesses = ['{H_COMPREHENSION}']"),
        );
    assert_eq!(fx.codes(), ["verification.cannot-support"]);
}

#[test]
fn 反証の側に挙げた調査は支持とみなさない() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &[],
            &["EVID-E00001"],
        )
        .evid(
            "EVID-E00001",
            &format!("method = 'synthetic-novice'\nassesses = ['{H_COMPREHENSION}']"),
        );
    assert_eq!(fx.codes(), Vec::<&str>::new());
}

#[test]
fn 資格のある方法の支持は通る() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &["EVID-E00001"],
            &[],
        )
        .evid("EVID-E00001", "method = 'first-contact-comprehension'")
        .hyp(H_FORMAL, "mechanical-formal-property", &[], &[])
        .evid(
            "EVID-E00002",
            &format!("method = 'executed-test'\nassesses = ['{H_FORMAL}']"),
        );
    assert_eq!(fx.codes(), Vec::<&str>::new());
}

/// 感情には `agent-review` も競合の収集も使えない。 後者は、仮説を作る材料にはなる。
#[test]
fn 仮説を作る材料と推定は感情の支持にならない() {
    let fx = Fx::new()
        .hyp(
            H_EMOTION,
            "emotional-response",
            &["EVID-E00001", "EVID-E00002"],
            &[],
        )
        .evid("EVID-E00001", "method = 'agent-review'")
        .evid("EVID-E00002", "method = 'competitive-corpus'");
    assert_eq!(
        fx.codes(),
        [
            "verification.cannot-support",
            "verification.generation-only"
        ]
    );
}

/// mock は、音声処理の妥当性を支持しない。
#[test]
fn mock_は音声処理の支持にならない() {
    let fx = Fx::new()
        .hyp(H_AUDIO, "audio-processing-validity", &["EVID-E00001"], &[])
        .evid("EVID-E00001", "method = 'mock'");
    assert_eq!(fx.codes(), ["verification.cannot-support"]);
}

#[test]
fn 表に行が無い組は未登録として落ちる() {
    // 競合の収集は、workflow fit について `cannot-support` と書いてある。
    let fx = Fx::new()
        .hyp(H_WORKFLOW, "workflow-fit", &["EVID-E00001"], &[])
        .evid("EVID-E00001", "method = 'competitive-corpus'");
    assert_eq!(fx.codes(), ["verification.cannot-support"]);
    // 行が無い組は、`cannot-support` の行とは別の名前で出る。 maintainer 自身の使用も、
    // 実行した test も、workflow fit については表に何も書いていない。
    for method in ["self-observation", "executed-test"] {
        let fx = Fx::new()
            .hyp(H_WORKFLOW, "workflow-fit", &["EVID-E00001"], &[])
            .evid("EVID-E00001", &format!("method = '{method}'"));
        assert_eq!(
            fx.codes(),
            ["verification.unregistered-capability"],
            "{method}"
        );
    }
}

#[test]
fn context_dependent_の組は昇格せず落ちる() {
    let table = "schema = 'evidence-capability'\n\n[[capability]]\nhypothesis_kind = 'k'\n\
                 method = 'm'\nsupport = 'context-dependent'\nsource = '出典'\n";
    let fx = Fx::bare()
        .put("meta/verification/capabilities.toml", table)
        .hyp(H_COMPREHENSION, "k", &["EVID-E00001"], &[])
        .evid("EVID-E00001", "method = 'm'");
    assert_eq!(fx.codes(), ["verification.context-dependent"]);
}

#[test]
fn 表に無い語は落ちる() {
    let fx = Fx::new()
        .hyp(H_COMPREHENSION, "vibes", &["EVID-E00001"], &[])
        .evid("EVID-E00001", "method = 'astrology'")
        .probe(
            "PROBE-P00001",
            &format!("targets = ['{H_COMPREHENSION}']\nmethod = 'tarot'"),
        );
    let found = fx.findings();
    let codes: Vec<&str> = found.iter().map(Finding::code).collect();
    // 語の誤りだけを出す。 資格の判定は、語が表にあるときに掛ける。
    assert_eq!(
        codes,
        [
            "verification.unknown-hypothesis-kind",
            "verification.unknown-method",
            "verification.unknown-method",
        ]
    );
    let owners: Vec<String> = found.iter().map(Finding::message).collect();
    assert!(owners[1].contains("EVID-E00001") && owners[1].contains("astrology"));
    assert!(owners[2].contains("PROBE-P00001") && owners[2].contains("tarot"));
}

/// 観測の欄が無い調査は、判定しない。 今の `EVID-*` は全部これ。
#[test]
fn 観測の欄が無い_legacy_の調査は判定しない() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &["EVID-fix-001"],
            &[],
        )
        .evid(
            "EVID-fix-001",
            &format!("supports_decisions = []\nassesses = ['{H_COMPREHENSION}']"),
        );
    assert_eq!(fx.codes(), Vec::<&str>::new());
}

#[test]
fn 仮説に種類が無ければ_観測を書いた支持は決められない() {
    let body = "schema = 'design-hypothesis'\nid = 'HYP-S00009'\nstatement = '予測'\n\
                supporting_evidence = ['EVID-E00001']\n";
    let fx = Fx::new()
        .put("meta/hypotheses/HYP-S00009.toml", body)
        .evid("EVID-E00001", "method = 'executed-test'");
    assert_eq!(fx.codes(), ["verification.missing-hypothesis-kind"]);
    // 欄の無い調査なら、同じ仮説でも判定しない。
    let fx = Fx::new()
        .put("meta/hypotheses/HYP-S00009.toml", body)
        .evid("EVID-E00001", "");
    assert_eq!(fx.codes(), Vec::<&str>::new());
}

#[test]
fn 存在しない調査と仮説を指す参照は落ちる() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &["EVID-E00099"],
            &[],
        )
        .evid(
            "EVID-E00001",
            "method = 'executed-test'\nassesses = ['HYP-S00099']",
        );
    assert_eq!(
        fx.codes(),
        [
            "verification.unknown-evidence",
            "verification.unknown-hypothesis"
        ]
    );
}

/// 検査は仮説の真偽（`assessment`）を決めない。 支持の資格が無い調査が挙がっても、
/// 書き換えず、判定もしない。
#[test]
fn 検査は仮説の_assessment_を書き換えず判定しない() {
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &["EVID-E00001"],
            &[],
        )
        .evid("EVID-E00001", "method = 'synthetic-novice'");
    let snapshot = fx.snapshot();
    let before = Facts::from_snapshot(&snapshot);
    let _ = findings(&snapshot);
    let (registry, _) = CapabilityRegistry::from_snapshot(&snapshot);
    let _ = human_verification_debt(&before, &registry);
    // 読んだ記録は変わらず、`assessment` は人が書いた値のまま。
    assert_eq!(Facts::from_snapshot(&snapshot), before);
    assert_eq!(
        before
            .hypothesis(H_COMPREHENSION)
            .and_then(|h| h.assessment.as_deref()),
        Some("untested")
    );
}

/// 実在する meta に、検査は1件も出さない。 `check-meta` に載せてよい。
#[test]
fn 実在する_meta_で検査は0件() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let mut rep = Report::default();
    let entries = load(root, &mut rep);
    assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
    let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
    assert_eq!(findings(&snapshot), Vec::new());
    let mut rep = Report::default();
    check_verification(&snapshot, &mut rep);
    assert!(rep.diagnostics().is_empty());
}

// ## Probe の形

/// `targets` が edge になり、調査の `assesses` が edge になる。 実行の契約は任意。
#[test]
fn probe_の_targets_と調査の_assesses_は_edge_になる() {
    let fx = Fx::new()
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .probe(
            "PROBE-P00001",
            &format!(
                "kind = 'human-first-contact'\ntargets = ['{H_COMPREHENSION}']\n\
                 method = 'first-contact-comprehension'\nrequires_human = true\n\
                 workloads = ['workload:voice-list-uneven-coverage']\n\
                 prediction = '説明する'\ndisconfirm_if = ['優劣として説明する']"
            ),
        )
        .evid(
            "EVID-E00001",
            &format!("method = 'first-contact-comprehension'\nassesses = ['{H_COMPREHENSION}']"),
        );
    let design = Design::load(&fx.view());
    assert_eq!(design.audit(), Vec::new());
    let graph = design.graph();
    let hyp = NodeKey::Object(H_COMPREHENSION.parse::<Id>().expect("ID"));
    for (name, from) in [("targets", "PROBE-P00001"), ("assesses", "EVID-E00001")] {
        let from = NodeKey::Object(from.parse::<Id>().expect("ID"));
        assert!(
            graph
                .all_relations()
                .any(|r| r.name == name && r.from == from && r.to == hyp),
            "{name}"
        );
    }
    assert_eq!(graph.node(&hyp).map(|n| n.kind), Some("hypothesis"));
}

#[test]
fn probe_は_targets_と_method_を欠くと落ちる() {
    let fx = Fx::new().put(
        "meta/verification/probes/PROBE-P00001.toml",
        "schema = 'probe'\nid = 'PROBE-P00001'\n",
    );
    let mut rep = Report::default();
    load_view_with(&fx.view(), TARGET_SHAPES, &mut rep);
    let messages: Vec<&str> = rep
        .diagnostics()
        .iter()
        .map(|d| d.message.as_str())
        .collect();
    assert!(
        messages.iter().any(|m| m.contains("`targets`")),
        "{messages:?}"
    );
    assert!(
        messages.iter().any(|m| m.contains("`method`")),
        "{messages:?}"
    );
}

#[test]
fn probe_の実行の契約は_probe_definition_として渡せる() {
    let fx = Fx::new()
        .probe(
            "PROBE-P00001",
            &format!(
                "targets = ['{H_FORMAL}']\nmethod = 'executed-test'\nrequires_native = true\n\
                 runner = 'cargo'\npackage = 'xtask'\ntarget = 'lib'\nplatforms = ['macos']\n\
                 min_cases = 3\nfixtures = ['fixtures/voice.wav']"
            ),
        )
        .probe(
            "PROBE-P00002",
            &format!(
                "targets = ['{H_FORMAL}']\nmethod = 'human-observation'\nrequires_human = true"
            ),
        );
    let facts = fx.facts();
    let with = facts
        .probes
        .iter()
        .find(|p| p.id == "PROBE-P00001")
        .expect("ある");
    assert!(with.requires_native && !with.requires_human);
    let definition = with.definition().expect("契約がある");
    assert_eq!(definition.id, "PROBE-P00001");
    assert_eq!(definition.min_cases, 3);
    assert_eq!(definition.backends, ["native", "unsupported"], "既定");
    assert_eq!(definition.prerequisites.fixtures, ["fixtures/voice.wav"]);
    // 契約を書かない Probe（人の観察の手順）は、走らせる runner が無い。
    let without = facts
        .probes
        .iter()
        .find(|p| p.id == "PROBE-P00002")
        .expect("ある");
    assert!(without.requires_human);
    assert!(without.definition().is_none());
}

// ## Receipt から EVID への境界

fn context(backend: &'static str) -> ExecutionContext {
    ExecutionContext {
        platform: if backend == "native" {
            "macos"
        } else {
            "linux"
        },
        backend,
        git_sha: "0123abc".into(),
        dirty: false,
        runner_version: None,
    }
}

fn receipt(probe: &str, status: Status, actual_work: u64) -> Receipt {
    Receipt {
        probe: probe.into(),
        package: "xtask".into(),
        target: "lib".into(),
        applies: true,
        status,
        counts: Counts::default(),
        actual_work,
        problems: Vec::new(),
    }
}

/// 契約つきの Probe 1件と、それが対象にする仮説を持つ記録。
fn canonize_fx(kind: &str, method: &str, extra: &str) -> Fx {
    Fx::new().hyp(H_FORMAL, kind, &[], &[]).probe(
        "PROBE-P00001",
        &format!(
            "targets = ['{H_FORMAL}']\nmethod = '{method}'\nrunner = 'cargo'\n\
                 package = 'xtask'\ntarget = 'lib'\nplatforms = ['macos', 'linux']\n\
                 min_cases = 1\n{extra}"
        ),
    )
}

/// `canonize_fx` の記録に、`receipt` と `ctx` で canonize を掛ける。
fn run(fx: &Fx, receipt: &Receipt, ctx: &ExecutionContext) -> Result<EvidenceCandidate, Refusal> {
    let facts = fx.facts();
    let hypothesis = facts.hypothesis(H_FORMAL).expect("仮説");
    let probe = &facts.probes[0];
    let definition = probe.definition().expect("契約");
    canonize(&fx.registry(), hypothesis, probe, &definition, receipt, ctx)
}

#[test]
fn 走って通った_receipt_は支持の候補になる() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    let candidate = run(
        &fx,
        &receipt("PROBE-P00001", Status::Passed, 5),
        &context("native"),
    )
    .expect("通る");
    assert_eq!(candidate.stance, Stance::Supporting);
    assert_eq!(candidate.hypothesis, H_FORMAL);
    assert_eq!(candidate.method, "executed-test");
    assert_eq!(candidate.tested_sha, "0123abc");
    assert_eq!(candidate.backend, "native");
    assert_eq!(candidate.actual_work, 5);
}

/// native が要る Probe を unsupported の backend で走らせた Receipt は、canonize できない。
#[test]
fn native_が要る_probe_を_native_でない_backend_で走らせた_receipt_は断る() {
    let fx = canonize_fx(
        "mechanical-formal-property",
        "executed-test",
        "requires_native = true",
    );
    // X04 は対象外の環境を `NotApplicable` にする。
    let mut skipped = receipt("PROBE-P00001", Status::NotApplicable, 0);
    skipped.applies = false;
    assert_eq!(
        run(&fx, &skipped, &context("unsupported")),
        Err(Refusal::NotApplicable)
    );
    // 対象の環境だと名乗って通ったことにしても、backend が native でなければ断る。
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 9),
            &context("unsupported")
        ),
        Err(Refusal::NativeRequired)
    );
    // native で走らせた通った Receipt は通る。
    assert!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 9),
            &context("native")
        )
        .is_ok()
    );
}

/// 落ちた Receipt も、native でなければ反証にならない。 native が要る主張について何も言わない。
#[test]
fn native_でない_backend_の失敗は反証にもならない() {
    let fx = canonize_fx(
        "mechanical-formal-property",
        "executed-test",
        "requires_native = true",
    );
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Failed, 3),
            &context("unsupported")
        ),
        Err(Refusal::NativeRequired)
    );
}

#[test]
fn 走っていない_receipt_は断る() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    let definition = fx.facts().probes[0].definition().expect("契約");
    for reason in [
        NotRunReason::BuildFailed,
        NotRunReason::MissingPrerequisite,
        NotRunReason::UnknownRunner,
    ] {
        let not_run = Receipt::not_run(&definition, reason);
        assert!(!not_run.countable());
        assert_eq!(
            run(&fx, &not_run, &context("native")),
            Err(Refusal::NotRun(reason))
        );
    }
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Skipped, 0),
            &context("native")
        ),
        Err(Refusal::Skipped)
    );
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::NotApplicable, 0),
            &context("native")
        ),
        Err(Refusal::NotApplicable)
    );
    // `applies` が偽のまま通ったことにしても、断る。
    let mut mislabeled = receipt("PROBE-P00001", Status::Passed, 4);
    mislabeled.applies = false;
    assert_eq!(
        run(&fx, &mislabeled, &context("native")),
        Err(Refusal::NotApplicable)
    );
}

/// 0 件を開いて通った、前提を欠いて早期 return した。 通ったことにしない。
#[test]
fn 実際の仕事が0件の_receipt_は通っていても断る() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 0),
            &context("native")
        ),
        Err(Refusal::NoActualWork)
    );
    // 落ちた Receipt も、仕事をしていなければ反証にならない。
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Failed, 0),
            &context("native")
        ),
        Err(Refusal::NoActualWork)
    );
}

/// Probe の定義と Receipt が揃っていても、観測方法の資格を超える支持は断る。
#[test]
fn probe_の定義と_receipt_があるだけでは支持にならない() {
    // 意味理解を synthetic novice で支持しようとする。
    let fx = canonize_fx("semantic-comprehension", "synthetic-novice", "");
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 12),
            &context("native")
        ),
        Err(Refusal::BeyondCapability(Capability::CannotSupport))
    );
    // 音声処理を mock で支持しようとする。 ブラウザの mock を native の主張の支持にしない。
    let fx = canonize_fx("audio-processing-validity", "mock", "");
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 12),
            &context("native")
        ),
        Err(Refusal::BeyondCapability(Capability::CannotSupport))
    );
    // 表に行が無い組。
    let fx = canonize_fx("workflow-fit", "executed-test", "");
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 12),
            &context("native")
        ),
        Err(Refusal::BeyondCapability(Capability::Unregistered))
    );
}

/// 落ちた Receipt は反証の候補。 支持する資格を問わない。
#[test]
fn 落ちた_receipt_は反証の候補になる() {
    let fx = canonize_fx("audio-processing-validity", "mock", "");
    let candidate = run(
        &fx,
        &receipt("PROBE-P00001", Status::Failed, 2),
        &context("native"),
    )
    .expect("反証は残せる");
    assert_eq!(candidate.stance, Stance::Contradicting);
}

#[test]
fn 人の観察が要る_probe_は機械の_receipt_から_evid_にしない() {
    let fx = canonize_fx(
        "semantic-comprehension",
        "first-contact-comprehension",
        "requires_human = true",
    );
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00001", Status::Passed, 1),
            &context("native")
        ),
        Err(Refusal::HumanObservationRequired)
    );
}

#[test]
fn 別の_probe_の_receipt_と対象外の仮説は断る() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    assert_eq!(
        run(
            &fx,
            &receipt("PROBE-P00002", Status::Passed, 5),
            &context("native")
        ),
        Err(Refusal::ReceiptOfAnotherProbe)
    );
    // Probe が対象にしていない仮説は支持できない。
    let facts = fx.facts();
    let other = Hypothesis {
        id: H_EMOTION.to_owned(),
        place: None,
        kind: Some("mechanical-formal-property".to_owned()),
        supporting: Vec::new(),
        contradicting: Vec::new(),
        assessment: None,
    };
    let probe = &facts.probes[0];
    let definition = probe.definition().expect("契約");
    assert_eq!(
        canonize(
            &fx.registry(),
            &other,
            probe,
            &definition,
            &receipt("PROBE-P00001", Status::Passed, 5),
            &context("native"),
        ),
        Err(Refusal::NotTargeted)
    );
}

#[test]
fn 資格を決める欄が無ければ断る() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    let facts = fx.facts();
    let mut hypothesis = facts.hypothesis(H_FORMAL).expect("仮説").clone();
    let mut probe = facts.probes[0].clone();
    let definition = probe.definition().expect("契約");
    let passed = receipt("PROBE-P00001", Status::Passed, 5);
    hypothesis.kind = None;
    assert_eq!(
        canonize(
            &fx.registry(),
            &hypothesis,
            &probe,
            &definition,
            &passed,
            &context("native")
        ),
        Err(Refusal::HypothesisWithoutKind)
    );
    probe.method = None;
    assert_eq!(
        canonize(
            &fx.registry(),
            &facts.hypotheses[0],
            &probe,
            &definition,
            &passed,
            &context("native")
        ),
        Err(Refusal::ProbeWithoutMethod)
    );
}

/// 候補を作っても、仮説の `assessment` は動かない。 候補は EVID ですらない。
#[test]
fn canonize_は仮説の_assessment_を動かさない() {
    let fx = canonize_fx("mechanical-formal-property", "executed-test", "");
    let facts = fx.facts();
    let before = facts.clone();
    let candidate = run(
        &fx,
        &receipt("PROBE-P00001", Status::Passed, 5),
        &context("native"),
    );
    assert!(candidate.is_ok());
    assert_eq!(fx.facts(), before);
    assert_eq!(
        facts
            .hypothesis(H_FORMAL)
            .and_then(|h| h.assessment.as_deref()),
        Some("untested")
    );
}

#[test]
fn 断る理由は文で言える() {
    for refusal in [
        Refusal::ReceiptOfAnotherProbe,
        Refusal::NotRun(NotRunReason::BuildFailed),
        Refusal::Skipped,
        Refusal::NotApplicable,
        Refusal::NotCounted,
        Refusal::NoActualWork,
        Refusal::NativeRequired,
        Refusal::HumanObservationRequired,
        Refusal::NotTargeted,
        Refusal::ProbeWithoutMethod,
        Refusal::HypothesisWithoutKind,
        Refusal::BeyondCapability(Capability::CannotSupport),
        Refusal::BeyondCapability(Capability::HypothesisGenerationOnly),
        Refusal::BeyondCapability(Capability::ContextDependent),
        Refusal::BeyondCapability(Capability::Unregistered),
        Refusal::BeyondCapability(Capability::UnknownKind),
        Refusal::BeyondCapability(Capability::UnknownMethod),
    ] {
        assert!(!refusal.to_string().is_empty(), "{refusal:?}");
    }
}

// ## 同じ起源

fn observation(id: &str, group: Option<&str>, derived: &[&str]) -> Observation {
    Observation {
        id: id.to_owned(),
        place: None,
        method: Some("agent-review".to_owned()),
        assesses: Vec::new(),
        origin_group: group.map(str::to_owned),
        derived_from: derived.iter().map(|s| (*s).to_owned()).collect(),
    }
}

/// 同じ原研究を紹介した3件は、独立した起源として1つ。
#[test]
fn 同じ_origin_group_の調査は1件と数える() {
    let a = observation("EVID-E00001", Some("study-a"), &[]);
    let b = observation("EVID-E00002", Some("study-a"), &[]);
    let c = observation("EVID-E00003", Some("study-a"), &[]);
    assert_eq!(independent_origins(&[&a, &b, &c]), 1);
    assert_eq!(
        origin_groups(&[&a, &b, &c]),
        [["EVID-E00001", "EVID-E00002", "EVID-E00003"]]
    );
}

#[test]
fn 同じ派生元を共有する調査は同じ起源() {
    let a = observation("EVID-E00001", None, &["src/study.md"]);
    let b = observation("EVID-E00002", None, &["src/study.md", "src/other.md"]);
    // c は b と派生元を共有し、a とは共有しない。 共有は推移する。
    let c = observation("EVID-E00003", None, &["src/other.md"]);
    assert_eq!(independent_origins(&[&a, &b, &c]), 1);
}

#[test]
fn 起源の違う調査と欄の無い調査は別々に数える() {
    let a = observation("EVID-E00001", Some("study-a"), &[]);
    let b = observation("EVID-E00002", Some("study-b"), &[]);
    let c = observation("EVID-E00003", None, &[]);
    let d = observation("EVID-E00004", None, &[]);
    assert_eq!(independent_origins(&[&a, &b, &c, &d]), 4);
    // 束の名前と派生元は別の欄。 同じ文字列でも交わらない。
    let e = observation("EVID-E00005", Some("same"), &[]);
    let f = observation("EVID-E00006", None, &["same"]);
    assert_eq!(independent_origins(&[&e, &f]), 2);
    assert_eq!(independent_origins(&[]), 0);
}

// ## Human Verification Debt

fn debt_fx() -> Fx {
    Fx::new()
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .dec(
            "DEC-D00001",
            "accepted",
            &[H_COMPREHENSION],
            &["REQ-R00001"],
        )
}

fn debt(fx: &Fx) -> Vec<HumanDebtItem> {
    human_verification_debt(&fx.facts(), &fx.registry())
}

#[test]
fn 人の観察でしか足りない仮説が_判断に依存されていれば_待ち行列に出る() {
    let fx = debt_fx().probe(
        "PROBE-P00001",
        &format!(
            "targets = ['{H_COMPREHENSION}']\nmethod = 'first-contact-comprehension'\n\
             requires_human = true"
        ),
    );
    let queue = debt(&fx);
    assert_eq!(queue.len(), 1);
    let item = &queue[0];
    assert_eq!(item.hypothesis, H_COMPREHENSION);
    assert_eq!(item.kind, "semantic-comprehension");
    assert_eq!(
        item.why_human,
        ["first-contact-comprehension", "human-observation"]
    );
    assert_eq!(item.decisions, ["DEC-D00001"]);
    assert_eq!(item.requirements, ["REQ-R00001"]);
    assert_eq!(item.evidence, Vec::new());
    assert_eq!(item.shortfall, Shortfall::NoEvidence);
    assert_eq!(item.origins, 0);
    assert!(item.has_valid_probe());
    assert_eq!(item.probes[0].id, "PROBE-P00001");
}

#[test]
fn 人の観察で足りる種類でなければ_待ち行列に出ない() {
    // 機械が検査できる性質。
    let fx = Fx::new()
        .hyp(H_FORMAL, "mechanical-formal-property", &[], &[])
        .dec("DEC-D00001", "accepted", &[H_FORMAL], &[]);
    assert_eq!(debt(&fx), Vec::new());
    // 実ブラウザの計測でも足りる種類。
    let fx = Fx::new()
        .hyp(H_AUDIO, "accessibility-conformance", &[], &[])
        .dec("DEC-D00001", "accepted", &[H_AUDIO], &[]);
    assert_eq!(debt(&fx), Vec::new());
}

/// 表が観察者を言っていない方法を含む種類は、人の観察だけとは言えない。
#[test]
fn 観察者を言っていない方法を含む種類は待ち行列に出ない() {
    let fx = Fx::new().hyp(H_WORKFLOW, "workflow-fit", &[], &[]).dec(
        "DEC-D00001",
        "accepted",
        &[H_WORKFLOW],
        &[],
    );
    let registry = fx.registry();
    assert_eq!(registry.human_only("workflow-fit"), None);
    assert_eq!(debt(&fx), Vec::new());
    // 人の観察だけの種類は、方法の一覧を返す。
    assert_eq!(
        registry.human_only("long-term-appropriation"),
        Some(vec!["longitudinal-observation"])
    );
}

/// 人の観察でない調査が付いても、足りないものは残る。 数が多くても同じ起源は1つ。
#[test]
fn 人の観察でない調査が付いても待ち行列に残る() {
    let fx = debt_fx()
        .evid(
            "EVID-E00001",
            &format!(
                "method = 'synthetic-novice'\nassesses = ['{H_COMPREHENSION}']\n\
                 origin_group = 'persona-run'"
            ),
        )
        .evid(
            "EVID-E00002",
            &format!(
                "method = 'synthetic-novice'\nassesses = ['{H_COMPREHENSION}']\n\
                 origin_group = 'persona-run'"
            ),
        )
        .evid(
            "EVID-E00003",
            &format!("method = 'synthetic-novice'\nassesses = ['{H_COMPREHENSION}']\norigin_group = 'other-run'"),
        );
    let queue = debt(&fx);
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].shortfall, Shortfall::NoHumanObservation);
    assert_eq!(queue[0].evidence.len(), 3);
    assert!(queue[0].evidence.iter().all(|e| !e.human_qualified));
    assert_eq!(queue[0].origins, 2, "同じ起源の2件は1つ");
    assert!(!queue[0].has_valid_probe());
}

#[test]
fn 人の観察の調査が付けば_待ち行列から消える() {
    let fx = debt_fx().evid(
        "EVID-E00001",
        &format!("method = 'first-contact-comprehension'\nassesses = ['{H_COMPREHENSION}']"),
    );
    assert_eq!(debt(&fx), Vec::new());
    // 仮説の側から挙げても同じ。 反証でも、人が観察した事実は残る。
    let fx = Fx::new()
        .hyp(
            H_COMPREHENSION,
            "semantic-comprehension",
            &[],
            &["EVID-E00001"],
        )
        .dec("DEC-D00001", "accepted", &[H_COMPREHENSION], &[])
        .evid("EVID-E00001", "method = 'human-observation'");
    assert_eq!(debt(&fx), Vec::new());
}

#[test]
fn 判断が依存しなくなれば_待ち行列から消える() {
    // 判断が仮説を挙げなくなった。
    let fx = Fx::new()
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .dec("DEC-D00001", "accepted", &[], &["REQ-R00001"]);
    assert_eq!(debt(&fx), Vec::new());
    // 依存していた判断が置き換えられた。
    let fx = Fx::new()
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .dec("DEC-D00001", "superseded", &[H_COMPREHENSION], &[]);
    assert_eq!(debt(&fx), Vec::new());
    // 置き換えた側がまだ依存していれば、残る。
    let fx = fx.dec("DEC-D00002", "accepted", &[H_COMPREHENSION], &[]);
    let queue = debt(&fx);
    assert_eq!(queue.len(), 1);
    assert_eq!(queue[0].decisions, ["DEC-D00002"]);
}

#[test]
fn 仮説自体が置き換えられれば_待ち行列から消える() {
    let body = "schema = 'design-hypothesis'\nid = 'HYP-S00001'\nstatement = '予測'\n\
                hypothesis_kind = 'semantic-comprehension'\nassessment = 'superseded'\n";
    let fx = Fx::new().put("meta/hypotheses/HYP-S00001.toml", body).dec(
        "DEC-D00001",
        "accepted",
        &[H_COMPREHENSION],
        &[],
    );
    assert_eq!(debt(&fx), Vec::new());
}

/// 順序は仮説の ID 順。 優先度を持たない。
#[test]
fn 待ち行列は_id_順で_優先度を持たない() {
    let fx = Fx::new()
        .hyp(H_EMOTION, "emotional-response", &[], &[])
        .hyp(H_COMPREHENSION, "semantic-comprehension", &[], &[])
        .dec("DEC-D00001", "accepted", &[H_EMOTION, H_COMPREHENSION], &[]);
    let ids: Vec<String> = debt(&fx).into_iter().map(|i| i.hypothesis).collect();
    assert_eq!(ids, [H_EMOTION, H_COMPREHENSION]);
}
