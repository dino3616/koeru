//! closure の規則の表。
//!
//! どの relation を、どの向きに、どの種類の node から辿るかを1か所に持つ。 relation の名前は
//! この表の外に書かない（名前の正本は D00 の `RELATIONS`）。 規則を足す・外すときは、ここだけを
//! 触る。
//!
//! 出典は提案書 7.2。 必須（[`Tier::Required`]）は「必ず取る」closure で、予算で削らない。
//! 周辺（[`Tier::Peripheral`]）は必須を集めた後で足す、予算で絞れる項目。 graph の距離は
//! relevance の代わりにしない——周辺は seed からの1歩だけで、遠くの項目を距離で拾わない。
//! `mentions` は辿らない（引用であって論理的な依存ではない）。 `NOT_TRAVERSED` が、辿らない
//! relation とその理由を持つ。

use crate::knowledge::design::Kind;

/// 辿る向き。 `Out` は focus の記録が起点の edge、`In` は focus が行き先の edge。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Dir {
    Out,
    In,
}

/// 項目の重さ。 並びは `Required` が先で、周辺は数が小さいほど先（予算で最後に削られる）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Tier {
    Required,
    Peripheral(u8),
}

/// 規則で入った項目を、さらに辿るか。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Next {
    /// 辿らない。
    Stop,
    /// 同じ規則（同じ `code`）だけで辿り続ける。 `depends_on` の連なり。
    SameRule,
    /// seed と同じ扱いで、全部の規則で辿る。 後継の判断は、現在有効な判断として読む。
    Full,
}

/// 行き先が満たす条件。 記録の状態を引く必要があるものだけ。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum When {
    Always,
    /// 未解決の問い。
    OpenQuestion,
    /// 未解決の問いではない（閉じた問い、問いではない記録）。
    NotOpenQuestion,
}

/// 1本の規則。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Rule {
    /// 取得理由。 出力に出る、規則ごとの安定した名前。
    pub(crate) code: &'static str,
    /// 取得理由の人向けの言い方。
    pub(crate) label: &'static str,
    /// 辿り始める側の種類。 空なら問わない。
    pub(crate) from: &'static [Kind],
    pub(crate) relation: &'static str,
    pub(crate) dir: Dir,
    /// 行き先の種類。 空なら問わない。
    pub(crate) to: &'static [Kind],
    pub(crate) tier: Tier,
    pub(crate) next: Next,
    pub(crate) when: When,
}

const DEC: &[Kind] = &[Kind::Decision];
const REQ: &[Kind] = &[Kind::Requirement];
const DEC_REQ: &[Kind] = &[Kind::Decision, Kind::Requirement];
const HYP: &[Kind] = &[Kind::Hypothesis];
const QUESTION: &[Kind] = &[Kind::Question];
const EVIDENCE: &[Kind] = &[Kind::Evidence];
const CONTRACT: &[Kind] = &[Kind::FormalContract];
const ANY: &[Kind] = &[];

const fn rule(
    code: &'static str,
    label: &'static str,
    from: &'static [Kind],
    relation: &'static str,
    dir: Dir,
    to: &'static [Kind],
    tier: Tier,
) -> Rule {
    Rule {
        code,
        label,
        from,
        relation,
        dir,
        to,
        tier,
        next: Next::Stop,
        when: When::Always,
    }
}

/// normative ancestor の規則。 入った要件は、`depends_on` の連なりも辿る。
const fn ancestor(
    label: &'static str,
    from: &'static [Kind],
    relation: &'static str,
    dir: Dir,
) -> Rule {
    Rule {
        next: Next::SameRule,
        ..rule(
            NORMATIVE_ANCESTOR,
            label,
            from,
            relation,
            dir,
            REQ,
            Tier::Required,
        )
    }
}

const NORMATIVE_ANCESTOR: &str = "normative-ancestor";
const NORMATIVE_ORIGIN: &str = "normative-origin";
const SUPERSESSION: &str = "supersession";
const EMPIRICAL_DEPENDENCY: &str = "empirical-dependency";
const OPEN_UNCERTAINTY: &str = "open-uncertainty";
pub(crate) const ARTIFACT_CLOSURE: &str = "artifact-closure";
pub(crate) const CONSIDERED_EVIDENCE: &str = "considered-evidence";
pub(crate) const PRIOR_DECISION: &str = "prior-decision";

/// 表の外で読む relation。 名前はここにしか書かない。 項目として数えず付ける出どころと、
/// 問いの状態が要るのでコードが足すリリース対象の edge。
pub(crate) const PROVENANCE: &str = "provenance";
pub(crate) const BLOCKS_PROFILES: &str = "blocks_profiles";

/// 表の外で組み立てる取得理由（入口、HYP の欄から読む証拠、問いが塞ぐリリース対象）。
pub(crate) const ROOT: (&str, &str) = ("root", "指定した入口");
pub(crate) const CONTRADICTION_CLOSURE: (&str, &str) =
    ("contradiction-closure", "仮説を反証する調査");
pub(crate) const EMPIRICAL_SUPPORT: (&str, &str) = ("empirical-support", "仮説を支持する調査");
pub(crate) const BLOCKED_RELEASE: (&str, &str) =
    ("blocked-release", "未解決の問いが塞ぐリリース対象");

const REQUIRED: Tier = Tier::Required;

/// closure の規則の全部。
pub(crate) const RULES: &[Rule] = &[
    // ## 必須 closure（提案書 7.2）
    //
    // normative ancestor。 判断が置く・変える・消す要件と、判断を縛る要件。
    ancestor("判断が置く要件", DEC, "establishes", Dir::Out),
    ancestor("判断が変える要件", DEC, "revises", Dir::Out),
    // 消された要件は現在の義務ではない。 それでも、判断が何を消したかは読む人が要る。
    ancestor("判断が消す要件", DEC, "removes", Dir::Out),
    ancestor("判断を縛る要件", DEC, "constrains", Dir::In),
    ancestor("要件が依って立つ要件", REQ, "depends_on", Dir::Out),
    // 形式 contract から、一部を形式にしている要件へ。 contract 側から読み始めたとき。
    ancestor(
        "形式 contract が一部を形式にしている要件",
        CONTRACT,
        "formalizes",
        Dir::Out,
    ),
    // 要件を置いた判断。 要件から読み始めたときの「なぜそうなったか」。
    rule(
        NORMATIVE_ORIGIN,
        "要件を置いた判断",
        REQ,
        "establishes",
        Dir::In,
        DEC,
        REQUIRED,
    ),
    rule(
        NORMATIVE_ORIGIN,
        "要件を変えた判断",
        REQ,
        "revises",
        Dir::In,
        DEC,
        REQUIRED,
    ),
    rule(
        NORMATIVE_ORIGIN,
        "要件を消した判断",
        REQ,
        "removes",
        Dir::In,
        DEC,
        REQUIRED,
    ),
    // supersession chain。 後継は、現在有効な判断として seed と同じに読む。
    Rule {
        next: Next::Full,
        ..rule(
            SUPERSESSION,
            "現在有効な後継",
            DEC_REQ,
            "supersedes",
            Dir::In,
            DEC_REQ,
            REQUIRED,
        )
    },
    // empirical dependency。
    rule(
        EMPIRICAL_DEPENDENCY,
        "判断が前提にする仮説",
        DEC,
        "relies_on_hypotheses",
        Dir::Out,
        HYP,
        REQUIRED,
    ),
    // open uncertainty。 関連する未解決の問いは両方向の `affects` で拾う。
    Rule {
        when: When::OpenQuestion,
        ..rule(
            OPEN_UNCERTAINTY,
            "関係する未解決の問い（この記録が影響する）",
            ANY,
            "affects",
            Dir::Out,
            QUESTION,
            REQUIRED,
        )
    },
    Rule {
        when: When::OpenQuestion,
        ..rule(
            OPEN_UNCERTAINTY,
            "関係する未解決の問い（この記録へ影響する）",
            ANY,
            "affects",
            Dir::In,
            QUESTION,
            REQUIRED,
        )
    },
    // 問いから読み始めたとき、それを閉じた判断。
    rule(
        OPEN_UNCERTAINTY,
        "問いを閉じた判断",
        QUESTION,
        "resolved_by",
        Dir::Out,
        DEC,
        REQUIRED,
    ),
    // artifact closure。 読むべき形式 contract・実装・入口。 満足の証明ではない。
    rule(
        ARTIFACT_CLOSURE,
        "要件の一部を形式にしている contract",
        REQ,
        "formalizes",
        Dir::In,
        CONTRACT,
        REQUIRED,
    ),
    rule(
        ARTIFACT_CLOSURE,
        "要件を実現する資源",
        REQ,
        "implements",
        Dir::In,
        ANY,
        REQUIRED,
    ),
    rule(
        ARTIFACT_CLOSURE,
        "読む入口（source / story）",
        &[
            Kind::Question,
            Kind::Hypothesis,
            Kind::Evidence,
            Kind::Decision,
            Kind::Requirement,
        ],
        "context_for",
        Dir::In,
        ANY,
        REQUIRED,
    ),
    // ## 周辺（予算で絞れる）。 seed からの1歩だけ。 数が小さいほど削られにくい。
    rule(
        CONSIDERED_EVIDENCE,
        "判断が直接参照した調査",
        DEC,
        "considers",
        Dir::Out,
        EVIDENCE,
        Tier::Peripheral(1),
    ),
    Rule {
        when: When::NotOpenQuestion,
        ..rule(
            "resolved-question",
            "判断が閉じた問い",
            DEC,
            "resolved_by",
            Dir::In,
            QUESTION,
            Tier::Peripheral(1),
        )
    },
    rule(
        PRIOR_DECISION,
        "置き換えた以前の判断",
        DEC_REQ,
        "supersedes",
        Dir::Out,
        DEC_REQ,
        Tier::Peripheral(2),
    ),
    rule(
        "hypothesis-dependents",
        "この仮説を前提にする判断",
        HYP,
        "relies_on_hypotheses",
        Dir::In,
        DEC,
        Tier::Peripheral(2),
    ),
    rule(
        "evidence-consumers",
        "この調査を参照する判断",
        EVIDENCE,
        "considers",
        Dir::In,
        DEC,
        Tier::Peripheral(2),
    ),
    // `assesses` は向きが「評価した」ことしか言わない。 支持か反証かは HYP の欄が持つ。
    rule(
        "assessed-hypothesis",
        "この調査が評価した仮説",
        EVIDENCE,
        "assesses",
        Dir::Out,
        HYP,
        Tier::Peripheral(2),
    ),
    rule(
        "assessing-evidence",
        "この仮説を評価した調査（支持か反証かは仮説の欄が言う）",
        HYP,
        "assesses",
        Dir::In,
        EVIDENCE,
        Tier::Peripheral(2),
    ),
    rule(
        "verification-probe",
        "確かめるための Probe（実行したことは意味しない）",
        &[Kind::Requirement, Kind::Hypothesis],
        "targets",
        Dir::In,
        ANY,
        Tier::Peripheral(3),
    ),
    rule(
        "constrained-decision",
        "この要件が縛る判断",
        REQ,
        "constrains",
        Dir::Out,
        DEC,
        Tier::Peripheral(3),
    ),
    // legacy の仮の名前。 要件を支える調査と、採用でない部品（`DEC-PLT-048`）。
    rule(
        "supporting-resource",
        "要件に結びつく調査・部品（legacy の仮の名前）",
        REQ,
        "supports_requirements",
        Dir::In,
        ANY,
        Tier::Peripheral(3),
    ),
    // 変更時に確かめる対象。 論理的含意ではない。 未解決の問いは必須の側に入っている。
    Rule {
        when: When::NotOpenQuestion,
        ..rule(
            "affected-context",
            "変更時に確かめる対象（影響する側）",
            ANY,
            "affects",
            Dir::Out,
            ANY,
            Tier::Peripheral(4),
        )
    },
    Rule {
        when: When::NotOpenQuestion,
        ..rule(
            "affected-context",
            "変更時に確かめる対象（影響される側）",
            ANY,
            "affects",
            Dir::In,
            ANY,
            Tier::Peripheral(4),
        )
    },
    rule(
        "explicit-dependency",
        "明示された依存",
        &[
            Kind::Decision,
            Kind::Hypothesis,
            Kind::Evidence,
            Kind::Question,
        ],
        "depends_on",
        Dir::Out,
        ANY,
        Tier::Peripheral(4),
    ),
];

/// 表の外で扱う relation（辿らない・別の形で読む）と、その理由。 [`RULES`] にも
/// ここにも載らない canonical な relation があれば、試験が落とす
/// ——relation を足したときに、辿るかどうかを決め忘れない。
#[cfg(test)]
pub(crate) const NOT_TRAVERSED: &[(&str, &str)] = &[
    (
        "mentions",
        "ID に言及しているだけで、論理的な依存ではない。 closure を広げない。 path の root が引いている ID を seed として拾うときだけ、引用として読む",
    ),
    (
        PROVENANCE,
        "元の議論・実装 event への locator。 項目として数えず、出どころとして各項目に付ける",
    ),
    (
        "runs_with",
        "Probe が使う workload。 検証の面（D02・D07c）が読むもので、判断の文脈には広げない",
    ),
    (
        "produces",
        "Probe が出す Receipt。 実行の記録は生成物で、判断の文脈には広げない",
    ),
    (
        "derived_from",
        "調査が由来する観測・資源。 Level 2 の出どころで、判断の文脈には広げない",
    ),
    (
        "implements",
        "要件を実現する資源は、要件から読む向き（`RULES` の artifact closure）だけ辿る",
    ),
    (
        "context_for",
        "読む入口は、semantic object から読む向き（`RULES` の artifact closure）だけ辿る",
    ),
    (
        BLOCKS_PROFILES,
        "未解決の問いが塞ぐリリース対象。 問いの状態が要るので、表ではなく `compile` が足す",
    ),
];

/// 表で `relation` を使っているか。
#[cfg(test)]
pub(crate) fn is_ruled(relation: &str) -> bool {
    RULES.iter().any(|r| r.relation == relation)
}
