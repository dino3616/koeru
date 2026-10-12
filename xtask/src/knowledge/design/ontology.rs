//! Design System の語彙（D00）。 node の種類と、relation の取りうる種類の組。
//!
//! 語彙の出典は `docs/reports/design-system/koeru_design_system_proposal_full.md` の
//! 5.1（object の種類）、5.1.4（三層）、7.1（edge の表）。 ここは表を写して持つだけで、
//! 規則そのものの正本は提案書にある。 表と食い違ったら、直すのはここ。
//!
//! graph engine（`crate::graph`）は具体語を持たない。 この語彙を [`super::registry`] が
//! `Registry` に載せて渡す。

use crate::graph::{GraphNode, RelationClass};

/// 何の層に属するか。 三層のうち derived view は node にならないので、ここには無い。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Layer {
    /// 元の PR や出力が消えても、未来の判断から address する意味を持つ。
    Semantic,
    /// FSL の contract。 REQ そのものではなく、その一部を形式にしたもの。
    Formal,
    /// 観測方法・workload・実装・provenance など、semantic object と結ぶ実体。
    Operational,
    /// 意味が決まっていない legacy の object。 D08 が行き先を決めるまでの置き場で、
    /// 新しい知識の追加先にしない（`DEC-PLT-048`）。
    Transitional,
}

/// node の種類。 graph の `GraphNode::kind` は [`Kind::name`] の文字列で持つ。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub(crate) enum Kind {
    Question,
    Hypothesis,
    Evidence,
    Decision,
    Requirement,
    /// FSL の contract（目標の命名では OBL / INV / FB）。 どれかは [`super::ContractKind`] が言う。
    FormalContract,
    Probe,
    Workload,
    Source,
    Story,
    Dependency,
    Artifact,
    Issue,
    PullRequest,
    Commit,
    Receipt,
    /// legacy の `TGT`。 意味で分類するまで、REQ / HYP / EVID / DEC のどれにも決めない。
    /// D08 で1件ずつ分類する（`DEC-PLT-048`）。
    UnclassifiedTarget,
    /// legacy の `PROFILE`。 リリース対象の定義で、運用の定義（operational resource）として残す
    /// （`DEC-PLT-048`）。 semantic object ではない。
    ReleaseProfile,
    /// 観測方法が、どの種類の仮説について証言できるかの表の1行（D02）。 行は ID を持たず、
    /// 鍵は（仮説の種類、観測方法）の組なので、node にならない。 `SchemaSpec` が種類を要求する
    /// ので名前を持つ。 運用の定義（operational resource）。
    Capability,
}

impl Kind {
    pub(crate) const ALL: [Kind; 19] = [
        Kind::Question,
        Kind::Hypothesis,
        Kind::Evidence,
        Kind::Decision,
        Kind::Requirement,
        Kind::FormalContract,
        Kind::Probe,
        Kind::Workload,
        Kind::Source,
        Kind::Story,
        Kind::Dependency,
        Kind::Artifact,
        Kind::Issue,
        Kind::PullRequest,
        Kind::Commit,
        Kind::Receipt,
        Kind::UnclassifiedTarget,
        Kind::ReleaseProfile,
        Kind::Capability,
    ];

    /// graph の node が持つ種類の文字列。
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Kind::Question => "question",
            Kind::Hypothesis => "hypothesis",
            Kind::Evidence => "evidence",
            Kind::Decision => "decision",
            Kind::Requirement => "requirement",
            Kind::FormalContract => "formal-contract",
            Kind::Probe => "probe",
            Kind::Workload => "workload",
            Kind::Source => "source",
            Kind::Story => "story",
            Kind::Dependency => "dependency",
            Kind::Artifact => "artifact",
            Kind::Issue => "issue",
            Kind::PullRequest => "pull-request",
            Kind::Commit => "commit",
            Kind::Receipt => "receipt",
            Kind::UnclassifiedTarget => "unclassified-target",
            Kind::ReleaseProfile => "release-profile",
            Kind::Capability => "capability",
        }
    }

    pub(crate) fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|k| k.name() == name)
    }

    /// node の種類。 graph が持つ文字列が語彙に無ければ `None`。
    pub(crate) fn of(node: &GraphNode) -> Option<Kind> {
        Kind::from_name(node.kind)
    }

    pub(crate) const fn layer(self) -> Layer {
        match self {
            Kind::Question
            | Kind::Hypothesis
            | Kind::Evidence
            | Kind::Decision
            | Kind::Requirement => Layer::Semantic,
            Kind::FormalContract => Layer::Formal,
            Kind::Probe
            | Kind::Workload
            | Kind::Source
            | Kind::Story
            | Kind::Dependency
            | Kind::Artifact
            | Kind::Issue
            | Kind::PullRequest
            | Kind::Commit
            | Kind::Receipt
            | Kind::ReleaseProfile
            | Kind::Capability => Layer::Operational,
            Kind::UnclassifiedTarget => Layer::Transitional,
        }
    }
}

/// 五つの semantic object。
pub(crate) const SEMANTIC: &[Kind] = &[
    Kind::Question,
    Kind::Hypothesis,
    Kind::Evidence,
    Kind::Decision,
    Kind::Requirement,
];

/// relation の片側が取りうる種類。
#[derive(Debug, Clone, Copy)]
pub(crate) enum Ends {
    /// 種類を問わない（`depends_on` と `affects`）。
    Any,
    Of(&'static [Kind]),
}

impl Ends {
    pub(crate) fn admits(self, kind: Kind) -> bool {
        match self {
            Ends::Any => true,
            Ends::Of(kinds) => kinds.contains(&kind),
        }
    }
}

/// relation が目標の語彙に載っているか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Standing {
    /// 7.1 の表にある。
    Canonical,
    /// legacy の欄を写すために、仮に名前を付けた。 目標の語彙に対応する関係が書かれていない。
    /// D08 が、正規の関係に置き換えるか、関係ごと無くす。
    Transitional,
}

/// 1つの relation の規則。 向きは `from` から `to`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct RelationSpec {
    pub(crate) name: &'static str,
    pub(crate) from: Ends,
    pub(crate) to: Ends,
    pub(crate) class: RelationClass,
    pub(crate) standing: Standing,
    /// 相手の一部だけを表す。 完全同値・完全被覆を意味しない（`formalizes`）。
    pub(crate) partial: bool,
}

const fn canon(name: &'static str, from: Ends, to: Ends) -> RelationSpec {
    RelationSpec {
        name,
        from,
        to,
        class: RelationClass::Reference,
        standing: Standing::Canonical,
        partial: false,
    }
}

const fn transitional(name: &'static str, from: Ends, to: Ends) -> RelationSpec {
    RelationSpec {
        name,
        from,
        to,
        class: RelationClass::Reference,
        standing: Standing::Transitional,
        partial: false,
    }
}

const DECISION_OR_REQUIREMENT: &[Kind] = &[Kind::Decision, Kind::Requirement];

/// 登録する relation の全部。 名前は一意。
pub(crate) const RELATIONS: &[RelationSpec] = &[
    // 形式 contract は Requirement の一部だけを表す。 完全同値として扱わない。
    RelationSpec {
        partial: true,
        ..canon(
            "formalizes",
            Ends::Of(&[Kind::FormalContract]),
            Ends::Of(&[Kind::Requirement]),
        )
    },
    canon(
        "resolved_by",
        Ends::Of(&[Kind::Question]),
        Ends::Of(&[Kind::Decision]),
    ),
    canon(
        "relies_on_hypotheses",
        Ends::Of(&[Kind::Decision]),
        Ends::Of(&[Kind::Hypothesis]),
    ),
    // 実行済みを意味しない。 Probe が何を観測するためのものか、だけ。
    // 契約（FSL の contract）への `targets` は、提案書 7.1 の表（HYP / REQ だけ）に足した。
    // `DEC-PLT-048` の5が残した、試験が支える契約の行き先を D02 で決めた。
    canon(
        "targets",
        Ends::Of(&[Kind::Probe]),
        Ends::Of(&[Kind::Hypothesis, Kind::Requirement, Kind::FormalContract]),
    ),
    canon(
        "runs_with",
        Ends::Of(&[Kind::Probe]),
        Ends::Of(&[Kind::Workload]),
    ),
    canon(
        "produces",
        Ends::Of(&[Kind::Probe]),
        Ends::Of(&[Kind::Receipt]),
    ),
    canon(
        "derived_from",
        Ends::Of(&[Kind::Evidence]),
        Ends::Of(&[Kind::Receipt, Kind::Artifact, Kind::Source]),
    ),
    canon(
        "assesses",
        Ends::Of(&[Kind::Evidence]),
        Ends::Of(&[Kind::Hypothesis]),
    ),
    canon(
        "considers",
        Ends::Of(&[Kind::Decision]),
        Ends::Of(&[Kind::Evidence]),
    ),
    canon(
        "establishes",
        Ends::Of(&[Kind::Decision]),
        Ends::Of(&[Kind::Requirement]),
    ),
    canon(
        "revises",
        Ends::Of(&[Kind::Decision]),
        Ends::Of(&[Kind::Requirement]),
    ),
    canon(
        "removes",
        Ends::Of(&[Kind::Decision]),
        Ends::Of(&[Kind::Requirement]),
    ),
    canon(
        "constrains",
        Ends::Of(&[Kind::Requirement]),
        Ends::Of(&[Kind::Decision]),
    ),
    // 意味を上書きせず後継を作る。 循環は不正（`RelationClass::Supersession`）。
    RelationSpec {
        class: RelationClass::Supersession,
        ..canon(
            "supersedes",
            Ends::Of(DECISION_OR_REQUIREMENT),
            Ends::Of(DECISION_OR_REQUIREMENT),
        )
    },
    canon(
        "provenance",
        Ends::Of(SEMANTIC),
        Ends::Of(&[Kind::Issue, Kind::PullRequest, Kind::Commit, Kind::Artifact]),
    ),
    canon("depends_on", Ends::Any, Ends::Any),
    // `affects_*` の族をまとめた名前。 論理的含意ではない。 行き先の種類が、どの族かを言う。
    canon("affects", Ends::Any, Ends::Any),
    canon("mentions", Ends::Of(&[Kind::Source]), Ends::Any),
    canon(
        "context_for",
        Ends::Of(&[Kind::Source, Kind::Story]),
        Ends::Of(SEMANTIC),
    ),
    // 満足の証明ではない。
    canon(
        "implements",
        Ends::Of(&[Kind::Source, Kind::Dependency, Kind::Artifact]),
        Ends::Of(&[Kind::Requirement]),
    ),
    // ここから下は legacy の欄を写すための仮の名前。 `EVID.supports_decisions` は
    // `considers` の逆向きとして正規にしたので、ここには無い（`DEC-PLT-048`）。
    // 調査が支える要件と、採用以外の部品が結びつく要件。 前者は D08 が1件ずつ見る。
    // 後者は、採用の部品だけが `implements` になる（`DEC-PLT-048`）。
    transitional(
        "supports_requirements",
        Ends::Of(&[Kind::Evidence, Kind::Dependency]),
        Ends::Of(&[Kind::Requirement]),
    ),
    transitional(
        "source_requirements",
        Ends::Of(&[Kind::Requirement, Kind::Workload]),
        Ends::Of(&[Kind::Requirement]),
    ),
    transitional(
        "source_targets",
        Ends::Of(&[Kind::Workload]),
        Ends::Of(&[Kind::UnclassifiedTarget]),
    ),
    transitional(
        "derives_from",
        Ends::Of(&[Kind::Requirement, Kind::UnclassifiedTarget]),
        // 目標値は SCALE のほか、上限（BUDGET）からも導かれている。
        Ends::Of(&[Kind::Workload, Kind::Requirement]),
    ),
    transitional(
        "decided_by",
        Ends::Of(&[Kind::Dependency, Kind::UnclassifiedTarget]),
        Ends::Of(&[Kind::Decision]),
    ),
    // 試験が支える契約のうち、要件ではないもの。 判断・調査・FSL の contract が混ざる
    // （`check-meta` も、どの meta でもよいとして実在だけを見ている）ので、行き先の種類は縛らない。
    // 要件と FSL の contract を指すものは `targets` になる（`DEC-PLT-048` の5の残りを D02 で決めた）。
    // ここに残るのは、判断・調査を指すもの。
    transitional("contracts", Ends::Of(&[Kind::Probe]), Ends::Any),
    transitional(
        "blocks_profiles",
        Ends::Of(&[Kind::Question]),
        Ends::Of(&[Kind::ReleaseProfile]),
    ),
    transitional(
        "includes_requirements",
        Ends::Of(&[Kind::ReleaseProfile]),
        Ends::Of(&[Kind::Requirement]),
    ),
    transitional(
        "includes_contracts",
        Ends::Of(&[Kind::ReleaseProfile]),
        Ends::Of(&[Kind::FormalContract]),
    ),
    transitional(
        "includes_decisions",
        Ends::Of(&[Kind::ReleaseProfile]),
        Ends::Of(&[Kind::Decision]),
    ),
    transitional(
        "includes_budgets",
        Ends::Of(&[Kind::ReleaseProfile]),
        Ends::Of(&[Kind::Requirement]),
    ),
];

/// 名前から relation の規則を引く。
pub(crate) fn relation(name: &str) -> Option<&'static RelationSpec> {
    RELATIONS.iter().find(|r| r.name == name)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    /// 実装計画の D00 が挙げた関係と、提案書 7.1 の表の関係が、1つも欠けず載っている。
    /// 載せ忘れた関係は、edge にならないだけで検査は通ってしまう。
    #[test]
    fn 実装計画と提案書の関係が漏れなく登録されている() {
        let plan = [
            "resolved_by",
            "relies_on_hypotheses",
            "targets",
            "derived_from",
            "assesses",
            "considers",
            "establishes",
            "revises",
            "removes",
            "constrains",
            "formalizes",
            "provenance",
            "implements",
            "context_for",
        ];
        // 計画の列挙に無く、7.1 の表にだけある。
        let table = [
            "supersedes",
            "runs_with",
            "produces",
            "depends_on",
            "affects",
            "mentions",
        ];
        for name in plan.iter().chain(&table) {
            let spec = relation(name).unwrap_or_else(|| panic!("{name} が登録されていない"));
            assert_eq!(spec.standing, Standing::Canonical, "{name}");
        }
        let canonical: BTreeSet<&str> = RELATIONS
            .iter()
            .filter(|r| r.standing == Standing::Canonical)
            .map(|r| r.name)
            .collect();
        let expected: BTreeSet<&str> = plan.iter().chain(&table).copied().collect();
        assert_eq!(canonical, expected, "正規の関係はこの20個だけ");
    }

    #[test]
    fn 関係の名前は一意() {
        let names: BTreeSet<&str> = RELATIONS.iter().map(|r| r.name).collect();
        assert_eq!(names.len(), RELATIONS.len());
    }

    /// 向きと種類の組が提案書 7.1 のとおり。
    #[test]
    fn 関係の向きと種類の組が表のとおり() {
        let r = |n| relation(n).expect("登録されている");
        // formalizes は contract から Requirement へ。 逆向きの `formalized_as` は使わない。
        assert!(r("formalizes").from.admits(Kind::FormalContract));
        assert!(!r("formalizes").from.admits(Kind::Requirement));
        assert!(r("formalizes").to.admits(Kind::Requirement));
        assert!(relation("formalized_as").is_none());
        // Decision が Requirement を変える。 逆ではない。
        for n in ["establishes", "revises", "removes"] {
            assert!(r(n).from.admits(Kind::Decision) && r(n).to.admits(Kind::Requirement));
            assert!(!r(n).from.admits(Kind::Requirement));
        }
        assert!(r("constrains").from.admits(Kind::Requirement));
        assert!(r("constrains").to.admits(Kind::Decision));
        assert!(
            r("considers").from.admits(Kind::Decision) && r("considers").to.admits(Kind::Evidence)
        );
        assert!(
            r("assesses").from.admits(Kind::Evidence) && r("assesses").to.admits(Kind::Hypothesis)
        );
        // Probe は HYP / REQ を対象にする。 Evidence は対象にならない。
        assert!(
            r("targets").to.admits(Kind::Hypothesis) && r("targets").to.admits(Kind::Requirement)
        );
        assert!(!r("targets").to.admits(Kind::Evidence));
        assert!(r("implements").to.admits(Kind::Requirement));
        assert!(!r("implements").to.admits(Kind::Decision));
        // PR / commit は provenance の行き先で、DEC の代わりではない。
        assert!(r("provenance").to.admits(Kind::PullRequest));
        assert!(!r("provenance").to.admits(Kind::Decision));
    }

    /// 後継の連なりだけが `Supersession`。 ほかは循環があってよい参照。
    #[test]
    fn 後継の関係だけが_supersession() {
        for spec in RELATIONS {
            assert_eq!(
                spec.class == RelationClass::Supersession,
                spec.name == "supersedes",
                "{}",
                spec.name
            );
        }
    }

    /// `formalizes` だけが「一部」。 完全被覆を意味する関係は作らない。
    #[test]
    fn formalizes_だけが部分的な被覆() {
        for spec in RELATIONS {
            assert_eq!(spec.partial, spec.name == "formalizes", "{}", spec.name);
        }
    }

    #[test]
    fn 種類は三層のどれかに属し名前から引ける() {
        let names: BTreeSet<&str> = Kind::ALL.iter().map(|k| k.name()).collect();
        assert_eq!(names.len(), Kind::ALL.len());
        for k in Kind::ALL {
            assert_eq!(Kind::from_name(k.name()), Some(k));
        }
        assert_eq!(Kind::from_name("decision"), Some(Kind::Decision));
        assert_eq!(Kind::from_name("PAT"), None, "Pattern は node にならない");
        for k in SEMANTIC {
            assert_eq!(k.layer(), Layer::Semantic);
        }
        assert_eq!(Kind::Probe.layer(), Layer::Operational);
        assert_eq!(Kind::Workload.layer(), Layer::Operational);
        assert_eq!(Kind::FormalContract.layer(), Layer::Formal);
        assert_eq!(Kind::UnclassifiedTarget.layer(), Layer::Transitional);
        // リリース対象は運用の定義。 semantic object ではない。
        assert_eq!(Kind::ReleaseProfile.layer(), Layer::Operational);
        assert!(!SEMANTIC.contains(&Kind::ReleaseProfile));
    }
}
