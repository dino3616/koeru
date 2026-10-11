//! legacy の meta を、target の意味で読む（D00）。
//!
//! 今の meta の schema ごとに「target では何か」と「どの欄が何の関係か」を [`SCHEMAS`] に
//! 1箇所で持つ。 下流は [`Object`] とその型つきの口を引く。 `TR-` や `[[allocations]]` の
//! ような接頭辞・表の並びは、この表の外へ出さない。 meta のファイルは書き換えない
//! （書き換えるのは D08）。
//!
//! 写し方の出典は提案書 5.1.5 と実装計画 9。 どちらにも書かれていないところは、
//! 推測で決めず、その旨を各項目の doc に書いてある。

use std::path::Path;

use super::ontology::{Kind, Layer};
use crate::knowledge::snapshot::{Fields, Record};
use crate::knowledge::{Id, KnowledgeSnapshot};

/// target では何になるか。
#[derive(Debug, Clone, Copy)]
pub(crate) enum Meaning {
    /// そのまま同じ種類の object になる。
    Direct,
    /// 複数の部品に分かれる。 graph の node は主たる部品の種類で持ち、残りは [`Part`] が言う。
    /// 分けるのは D08。
    Decomposed(&'static [Part]),
    /// 意味で分類しないと決められない。 機械で決めない（実装計画 9.2）。
    Unclassified { candidates: &'static [Kind] },
}

/// 分解された部品。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    /// graph の object / resource になる部品。
    Object(Kind),
    /// object に付くデータのまま残る部品。
    Data(&'static str),
    /// 計算して出す derived view。 手書きの正本にしない。
    View(&'static str),
}

/// 欄の役割。
#[derive(Debug, Clone, Copy)]
pub(crate) enum Role {
    /// 値が ID の一覧で、この名前の relation の edge になる。
    Edge(&'static str),
    /// 型つきの口が読む値。
    Data,
    /// ほかの欄が決める関係の、逆向きの写し。 edge にしない（二重になる）。
    Mirror(&'static str),
    /// 欄の値から、この名前の relation を [`super::lift`] が組み立てる。 値の行き先で relation を
    /// 分けるもの（`implements`・`targets`）。 欄そのものは edge の規則を持たない。
    Lifted(&'static str),
    /// 値の記録から、欄を持つ記録へ向かう、この名前の relation の edge になる（`formalizes`・
    /// `considers`）。 欄は関係の行き先の側に書かれていて、engine が逆向きに読む
    /// （`graph::Direction::Reverse`）。 起点の記録は書き換えない。
    Inverse(&'static str),
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Field {
    pub(crate) name: &'static str,
    pub(crate) role: Role,
}

const fn edge(name: &'static str, relation: &'static str) -> Field {
    Field {
        name,
        role: Role::Edge(relation),
    }
}

const fn data(name: &'static str) -> Field {
    Field {
        name,
        role: Role::Data,
    }
}

/// 1つの schema の、target での意味。
#[derive(Debug)]
pub(crate) struct SchemaSpec {
    pub(crate) schema: &'static str,
    /// graph の node の種類。 分解されるものは主たる部品。
    pub(crate) kind: Kind,
    pub(crate) meaning: Meaning,
    /// 表題として読む欄。
    pub(crate) title: Option<&'static str>,
    pub(crate) fields: &'static [Field],
    /// 一覧に無い欄も許す。 調査の記録（evidence）は、調べたことごとに欄が違う自由記述で、
    /// 欄を閉じると記録が書けなくなる。
    pub(crate) open: bool,
    /// 収集ファイルの直下の欄（項目ではなくファイルそのものの欄）。 `SourceRecord` は
    /// 文字列の配列しか読めないので、型つきの口は持たない。 要件の `domain_risks` が
    /// 配列で、領域全体にかかる [Risk] を持つ。
    pub(crate) file_fields: &'static [&'static str],
}

/// 構造として持つ欄。 どの schema にもある。
pub(crate) const STRUCTURAL: &[&str] = &["schema", "id"];

/// legacy の schema と、target で加わる schema の表。
pub(crate) const SCHEMAS: &[SchemaSpec] = &[
    // 技術要件。 REQ になる。 FSL への `formalized_as` は、正規の向き（contract から REQ。
    // 提案書 5.2.2）の `formalizes` の逆向きの規則で読む。
    SchemaSpec {
        schema: "requirement-set",
        kind: Kind::Requirement,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("confidence"),
            data("statement"),
            data("needs_component"),
            data("notes"),
            edge("depends_on", "depends_on"),
            Field {
                name: "formalized_as",
                role: Role::Inverse("formalizes"),
            },
        ],
        open: false,
        file_fields: &["domain", "domain_title", "domain_risks", "prefix"],
    },
    SchemaSpec {
        schema: "decision",
        kind: Kind::Decision,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("status"),
            data("owner"),
            data("options"),
            data("selected"),
            data("rationale"),
            data("review_triggers"),
            data("constraint"),
            data("constraint_label"),
            data("consequences"),
            data("source_text"),
            edge("supersedes", "supersedes"),
            // 置き換えは両側に書かれる。 逆向きの写しは edge にせず、二重にしない。
            Field {
                name: "superseded_by",
                role: Role::Mirror("supersedes"),
            },
            edge("evidence", "considers"),
            // 次の欄は目標の形（提案書 5.5、`DEC-PLT-048`）。 `revises_requirements` と
            // `removes_requirements` は5.5 に例が無く、同じ流儀で補った。
            edge("relies_on_hypotheses", "relies_on_hypotheses"),
            edge("considers_evidence", "considers"),
            edge("establishes_requirements", "establishes"),
            edge("revises_requirements", "revises"),
            edge("removes_requirements", "removes"),
            // 閉じる問い。 正規の `resolved_by` は Q から DEC なので、逆向きの写し。
            // engine が逆向きの規則を読めるまで edge にしない。
            Field {
                name: "question",
                role: Role::Mirror("resolved_by"),
            },
            // locator の一覧。 engine が値から種類を決められるまで edge にしない。
            data("provenance"),
            edge("affects_requirements", "affects"),
            edge("affects_decisions", "affects"),
            edge("affects_questions", "affects"),
            edge("affects_evidence", "affects"),
            edge("affects_fsl", "affects"),
            edge("affects_components", "affects"),
            edge("affects_targets", "affects"),
            edge("affects_budgets", "affects"),
        ],
        open: false,
        file_fields: &[],
    },
    SchemaSpec {
        schema: "question",
        kind: Kind::Question,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("status"),
            data("owner"),
            data("why_it_matters"),
            data("how_to_close"),
            data("closed_how"),
            data("source_text"),
            data("section_lead"),
            // 閉じた判断を指す。 古い名前（closed_by）と新しい名前（resolved_by）が併存している。
            edge("closed_by", "resolved_by"),
            edge("resolved_by", "resolved_by"),
            edge("blocks_profiles", "blocks_profiles"),
            edge("affects_requirements", "affects"),
            edge("affects_decisions", "affects"),
            edge("affects_questions", "affects"),
            edge("affects_evidence", "affects"),
            edge("affects_fsl", "affects"),
            edge("affects_components", "affects"),
            edge("affects_targets", "affects"),
            edge("affects_budgets", "affects"),
        ],
        open: false,
        file_fields: &[],
    },
    SchemaSpec {
        schema: "evidence",
        kind: Kind::Evidence,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("kind"),
            data("source"),
            // 目標の `provenance` は locator の一覧だが、これは出どころを説明する文章。
            // 同じ名前の別物なので、locator として読まない。
            data("provenance"),
            data("confidence"),
            // `considers`（DEC → EVID）の逆向きの規則で読む（`DEC-PLT-048`）。 判断の記録は
            // 書き換えない。
            Field {
                name: "supports_decisions",
                role: Role::Inverse("considers"),
            },
            edge("supports_requirements", "supports_requirements"),
            edge("affects_requirements", "affects"),
            edge("affects_decisions", "affects"),
            edge("affects_questions", "affects"),
            edge("affects_evidence", "affects"),
            edge("affects_fsl", "affects"),
            edge("affects_components", "affects"),
            edge("affects_targets", "affects"),
            edge("affects_budgets", "affects"),
        ],
        open: true,
        file_fields: &[],
    },
    // 上限と配分。 上限は定量の REQ、配分は REQ に付くデータ、合計・余白・モード別の
    // ピークは Budget View が計算する（5.1.5、実装計画 9.1）。
    SchemaSpec {
        schema: "budget",
        kind: Kind::Requirement,
        meaning: Meaning::Decomposed(&[
            Part::Object(Kind::Requirement),
            Part::Data("allocations"),
            Part::View("budget"),
        ]),
        title: Some("title"),
        fields: &[
            data("title"),
            data("limit"),
            data("unit"),
            data("scope"),
            data("note"),
            data("allocations"),
            edge("source_requirements", "source_requirements"),
            edge("derives_from", "derives_from"),
        ],
        open: false,
        file_fields: &[],
    },
    // 目標値。 守る線（REQ）、予測（HYP）、実測（EVID）、目標を置く選択（DEC）が混ざっていて、
    // 機械では分けられない。 D00 では分類せず、D08 で1件ずつ分類する（`DEC-PLT-048`）。
    SchemaSpec {
        schema: "target-set",
        kind: Kind::UnclassifiedTarget,
        meaning: Meaning::Unclassified {
            candidates: &[
                Kind::Requirement,
                Kind::Hypothesis,
                Kind::Evidence,
                Kind::Decision,
            ],
        },
        title: Some("item"),
        fields: &[
            data("item"),
            data("goal"),
            data("note"),
            edge("decided_by", "decided_by"),
            edge("derives_from", "derives_from"),
        ],
        open: false,
        file_fields: &["domain"],
    },
    // 代表 workload。 workload の定義、代表に選んだ判断、実測の basis、式の結果に分かれる。
    SchemaSpec {
        schema: "scale-reference",
        kind: Kind::Workload,
        meaning: Meaning::Decomposed(&[
            Part::Object(Kind::Workload),
            Part::Object(Kind::Decision),
            Part::Object(Kind::Evidence),
            Part::View("calculation"),
        ]),
        title: Some("title"),
        fields: &[
            data("title"),
            data("basis"),
            data("scope"),
            data("rationale"),
            data("note"),
            data("derived"),
            edge("source_requirements", "source_requirements"),
            edge("source_targets", "source_targets"),
        ],
        open: false,
        file_fields: &[],
    },
    // 部品の台帳。 部品の存在と版は manifest から引き、採否は DEC、license と計測は EVID。
    // `CMP-*` を dependency の identity として残さない（実装計画 9.4）。
    SchemaSpec {
        schema: "component-ledger",
        kind: Kind::Dependency,
        meaning: Meaning::Decomposed(&[
            Part::Object(Kind::Dependency),
            Part::Object(Kind::Decision),
            Part::Object(Kind::Evidence),
            Part::View("component-inventory"),
        ]),
        title: Some("name"),
        fields: &[
            data("name"),
            data("purpose"),
            data("license"),
            data("status"),
            data("note"),
            data("domains"),
            edge("decided_by", "decided_by"),
            // 採用の部品だけが `implements`。 ほかは仮の名前（`DEC-PLT-048`）。 どちらになるかは
            // `status` で決まるので、`lift` が次の2つの欄に振り分ける。
            Field {
                name: "supports_requirements",
                role: Role::Lifted("implements"),
            },
            edge("implements_requirements", "implements"),
            edge("unadopted_requirements", "supports_requirements"),
        ],
        open: false,
        file_fields: &[],
    },
    // 試験の登録。 繰り返し使う検証方法なので PROBE。 1回の実行は Receipt で、meta には持たない。
    SchemaSpec {
        schema: "test-portfolio",
        kind: Kind::Probe,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("runner"),
            data("package"),
            data("target"),
            data("platforms"),
            data("backends"),
            data("min_cases"),
            data("manual"),
            // 行き先が要件のものだけ `targets`。 残りは仮の名前（`DEC-PLT-048`）。 `lift` が
            // 次の2つの欄に振り分ける。
            Field {
                name: "contracts",
                role: Role::Lifted("targets"),
            },
            edge("targets_requirements", "targets"),
            edge("other_contracts", "contracts"),
        ],
        open: false,
        file_fields: &[],
    },
    // リリース対象。 運用の定義（operational resource）として残す（`DEC-PLT-048`）。
    SchemaSpec {
        schema: "profile",
        kind: Kind::ReleaseProfile,
        meaning: Meaning::Direct,
        title: Some("title"),
        fields: &[
            data("title"),
            data("status"),
            data("note"),
            data("excludes"),
            edge("includes_requirements", "includes_requirements"),
            edge("includes_fsl", "includes_contracts"),
            edge("decisions", "includes_decisions"),
            edge("budgets", "includes_budgets"),
        ],
        open: false,
        file_fields: &[],
    },
    // FSL の contract。 meta のファイルではなく、`super::lift` が FSL の索引から記録として
    // 組み立てる（id と宣言の場所だけ）。 REQ との関係は、REQ の `formalized_as` を逆向きに
    // 読む規則が作る。
    SchemaSpec {
        schema: CONTRACT_SCHEMA,
        kind: Kind::FormalContract,
        meaning: Meaning::Direct,
        title: None,
        fields: &[data("site")],
        open: false,
        file_fields: &[],
    },
    // 目標の形（提案書 5.5、`DEC-PLT-048`）。 まだ読み込みが受け付けない
    // （`SHAPES` に無い。 X03 への要求）。 `decision` と `evidence` は今の schema 名を引き継ぐ。
    // 逆向きに持つ欄（`question`・`established_by`・`supporting_evidence`・
    // `contradicting_evidence`）は、engine が逆向きの規則を読めるまで edge にしない。
    SchemaSpec {
        schema: "design-hypothesis",
        kind: Kind::Hypothesis,
        meaning: Meaning::Direct,
        title: None,
        fields: &[
            data("question"),
            data("hypothesis_kind"),
            data("statement"),
            data("scope"),
            data("assessment"),
            Field {
                name: "supporting_evidence",
                role: Role::Mirror("assesses"),
            },
            Field {
                name: "contradicting_evidence",
                role: Role::Mirror("assesses"),
            },
        ],
        open: false,
        file_fields: &[],
    },
    SchemaSpec {
        schema: "requirement",
        kind: Kind::Requirement,
        meaning: Meaning::Direct,
        title: None,
        fields: &[
            data("statement"),
            Field {
                name: "established_by",
                role: Role::Mirror("establishes"),
            },
            // 定量の REQ（5.1.5）。
            data("kind"),
            data("metric"),
            data("operator"),
            data("limit"),
            data("unit"),
            data("scope"),
        ],
        open: false,
        file_fields: &[],
    },
    // 欄の名前は決まっていない。 schema 名だけを登録する。
    SchemaSpec {
        schema: "probe",
        kind: Kind::Probe,
        meaning: Meaning::Direct,
        title: None,
        fields: &[],
        open: true,
        file_fields: &[],
    },
    SchemaSpec {
        schema: "workload",
        kind: Kind::Workload,
        meaning: Meaning::Direct,
        title: None,
        fields: &[],
        open: true,
        file_fields: &[],
    },
];

/// 組み立てた contract の schema 名。 meta のファイルには現れない。
pub(crate) const CONTRACT_SCHEMA: &str = "formal-contract";

/// `schema` の意味。
pub(crate) fn spec(schema: &str) -> Option<&'static SchemaSpec> {
    SCHEMAS.iter().find(|s| s.schema == schema)
}

impl SchemaSpec {
    pub(crate) fn field(&self, name: &str) -> Option<&'static Field> {
        self.fields.iter().find(|f| f.name == name)
    }

    /// この欄は、記録が持ってよい欄か。 構造の欄と、一覧にある欄と、開いた schema の自由記述。
    pub(crate) fn knows(&self, name: &str) -> bool {
        self.open || STRUCTURAL.contains(&name) || self.field(name).is_some()
    }

    /// この欄は、収集ファイルの直下に置いてよい欄か。
    pub(crate) fn knows_file_field(&self, name: &str) -> bool {
        STRUCTURAL.contains(&name) || self.file_fields.contains(&name)
    }
}

/// FSL の contract の種類。 目標の命名は OBL / INV / FB / ACC（提案書 5.2.1、`DEC-PLT-048`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ContractKind {
    Obligation,
    Invariant,
    ForbiddenBehavior,
    /// FSL の `acceptance`。 形式 contract の1種類で、目標の接頭辞は `ACC`（`DEC-PLT-048`）。
    Acceptance,
}

impl ContractKind {
    /// 今の FSL の ID の接頭辞から種類を決める。 知らない接頭辞は `None`。
    pub(crate) fn of(id: &Id) -> Option<Self> {
        match id.prefix() {
            "REQ" => Some(Self::Obligation),
            "INV" => Some(Self::Invariant),
            "FB" => Some(Self::ForbiddenBehavior),
            "AC" => Some(Self::Acceptance),
            _ => None,
        }
    }

    /// 目標の ID の接頭辞（提案書 5.2.1、`DEC-PLT-048`）。
    pub(crate) fn target_prefix(self) -> &'static str {
        match self {
            Self::Obligation => "OBL",
            Self::Invariant => "INV",
            Self::ForbiddenBehavior => "FB",
            Self::Acceptance => "ACC",
        }
    }
}

/// 記録が legacy のどこから来たか。 cutover（D08）が、旧 ID と新 ID を対応づけるのに引く。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LegacyOrigin<'a> {
    pub(crate) schema: &'static str,
    pub(crate) path: &'a Path,
    pub(crate) line: Option<usize>,
}

/// target の意味で見た1件。 元の記録は書き換えず、見方だけを変える。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Object<'a> {
    record: &'a Record,
    id: &'a Id,
    spec: &'static SchemaSpec,
    /// 記録を含む snapshot。 他の記録から引く値（[`Contract::formalizes`]）を読むのに要る。
    snapshot: Option<&'a KnowledgeSnapshot>,
}

impl<'a> Object<'a> {
    /// 知らない schema・ID の形に読めない記録は `None`。 黙って落とさず、
    /// [`super::Design::audit`] が出す。
    ///
    /// snapshot を持たないので、他の記録から引く値（[`Contract::formalizes`]）は空になる。
    /// 記録の全体から読むときは [`Self::in_snapshot`]。
    pub(crate) fn new(record: &'a Record) -> Option<Self> {
        Some(Self {
            id: record.id()?,
            spec: spec(record.schema())?,
            record,
            snapshot: None,
        })
    }

    /// [`Self::new`] に、記録を含む snapshot を持たせる。
    pub(crate) fn in_snapshot(record: &'a Record, snapshot: &'a KnowledgeSnapshot) -> Option<Self> {
        Self::new(record).map(|o| Self {
            snapshot: Some(snapshot),
            ..o
        })
    }

    pub(crate) fn id(&self) -> &'a Id {
        self.id
    }

    pub(crate) fn kind(&self) -> Kind {
        self.spec.kind
    }

    pub(crate) fn layer(&self) -> Layer {
        self.spec.kind.layer()
    }

    pub(crate) fn meaning(&self) -> &'static Meaning {
        &self.spec.meaning
    }

    /// 表題。 持たない種類は `None`。
    pub(crate) fn title(&self) -> Option<&'a str> {
        let record: &'a Record = self.record;
        record.str(self.spec.title?)
    }

    pub(crate) fn origin(&self) -> LegacyOrigin<'a> {
        let provenance = self.record.provenance();
        LegacyOrigin {
            schema: self.spec.schema,
            path: provenance.path(),
            line: provenance.line(),
        }
    }

    /// 分類できていない。 REQ / HYP / EVID / DEC のどれかに決める材料が、機械の側に無い。
    pub(crate) fn is_unclassified(&self) -> bool {
        matches!(self.spec.meaning, Meaning::Unclassified { .. })
    }

    /// 意味で決まる種類。 分類できないもの・semantic object ではないものは `None`。
    pub(crate) fn semantic_kind(&self) -> Option<Kind> {
        match self.spec.meaning {
            Meaning::Direct | Meaning::Decomposed(_) if self.layer() == Layer::Semantic => {
                Some(self.spec.kind)
            }
            _ => None,
        }
    }

    pub(crate) fn as_requirement(&self) -> Option<Requirement<'a>> {
        (self.spec.schema == "requirement-set").then_some(Requirement(self.record))
    }

    pub(crate) fn as_quantity(&self) -> Option<Quantity<'a>> {
        (self.spec.schema == "budget").then_some(Quantity(self.record))
    }

    pub(crate) fn as_decision(&self) -> Option<Decision<'a>> {
        (self.spec.schema == "decision").then_some(Decision(self.record))
    }

    pub(crate) fn as_question(&self) -> Option<Question<'a>> {
        (self.spec.schema == "question").then_some(Question(self.record))
    }

    pub(crate) fn as_evidence(&self) -> Option<Evidence<'a>> {
        (self.spec.schema == "evidence").then_some(Evidence(self.record))
    }

    pub(crate) fn as_workload(&self) -> Option<Workload<'a>> {
        (self.spec.schema == "scale-reference").then_some(Workload(self.record))
    }

    pub(crate) fn as_component(&self) -> Option<Component<'a>> {
        (self.spec.schema == "component-ledger").then_some(Component(self.record))
    }

    pub(crate) fn as_probe(&self) -> Option<Probe<'a>> {
        (self.spec.schema == "test-portfolio").then_some(Probe(self.record))
    }

    pub(crate) fn as_target(&self) -> Option<UnclassifiedTarget<'a>> {
        (self.spec.schema == "target-set").then_some(UnclassifiedTarget(self.record))
    }

    pub(crate) fn as_profile(&self) -> Option<ReleaseProfile<'a>> {
        (self.spec.schema == "profile").then_some(ReleaseProfile(self.record))
    }

    pub(crate) fn as_contract(&self) -> Option<Contract<'a>> {
        (self.spec.schema == CONTRACT_SCHEMA).then_some(Contract {
            record: self.record,
            id: self.id,
            snapshot: self.snapshot,
        })
    }
}

/// 欄を読む口。 `Record::str` の寿命を、借りた記録の寿命まで延ばす。
fn text<'a>(record: &'a Record, key: &str) -> Option<&'a str> {
    record.str(key)
}

/// 技術要件。 REQ。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Requirement<'a>(&'a Record);

impl<'a> Requirement<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    /// 確度のタグ（Fact / Assumption / Unknown / Risk）。
    pub(crate) fn confidence(&self) -> Option<&'a str> {
        text(self.0, "confidence")
    }

    pub(crate) fn statement(&self) -> Option<&'a str> {
        text(self.0, "statement")
    }

    pub(crate) fn notes(&self) -> Vec<String> {
        self.0.strs("notes")
    }

    pub(crate) fn depends_on(&self) -> Vec<String> {
        self.0.strs("depends_on")
    }

    /// 外部部品が要らないという宣言。 legacy の欄そのもの。 目標では要件の網羅を
    /// この真偽で表さない（実装計画 8、D07b）ので、新しく読む側は頼らない。
    pub(crate) fn legacy_needs_component(&self) -> Option<bool> {
        self.0.bool("needs_component")
    }
}

/// 定量の REQ。 legacy の `BUDGET`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Quantity<'a>(&'a Record);

impl<'a> Quantity<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    pub(crate) fn limit(&self) -> Option<i64> {
        self.0.int("limit")
    }

    pub(crate) fn unit(&self) -> Option<&'a str> {
        text(self.0, "unit")
    }

    /// 上限が成り立つ条件。 legacy では文章で、workload の locator ではない。
    pub(crate) fn scope(&self) -> Option<&'a str> {
        text(self.0, "scope")
    }

    pub(crate) fn note(&self) -> Option<&'a str> {
        text(self.0, "note")
    }

    /// 配分。 小計・ピーク・余白は持たない。 Budget View が計算する（D07a）。
    pub(crate) fn allocations(&self) -> Vec<Allocation<'a>> {
        self.0
            .tables("allocations")
            .into_iter()
            .map(Allocation)
            .collect()
    }
}

/// 配分の1行。 `measured` が偽のものは観測ではない（実装計画 9.1）ので、Evidence として読まない。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Allocation<'a>(Fields<'a>);

impl<'a> Allocation<'a> {
    /// 行の名前。 古い行は `name`、新しい行は `item` で書いている。
    pub(crate) fn label(&self) -> Option<&str> {
        self.0.str("name").or_else(|| self.0.str("item"))
    }

    /// `step` / `subtotal`。
    pub(crate) fn kind(&self) -> Option<&str> {
        self.0.str("kind")
    }

    pub(crate) fn value(&self) -> Option<i64> {
        self.0.int("value")
    }

    pub(crate) fn measured(&self) -> Option<bool> {
        self.0.bool("measured")
    }

    /// 実行モード。 無い行はどのモードにも乗る共通分。
    pub(crate) fn mode(&self) -> Option<&str> {
        self.0.str("mode")
    }

    pub(crate) fn stated(&self) -> Option<&str> {
        self.0.str("stated")
    }

    pub(crate) fn source(&self) -> Option<&str> {
        self.0.str("source")
    }

    pub(crate) fn confidence(&self) -> Option<&str> {
        self.0.str("confidence")
    }

    pub(crate) fn note(&self) -> Option<&str> {
        self.0.str("note")
    }
}

/// 判断。 DEC。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Decision<'a>(&'a Record);

impl<'a> Decision<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    /// `accepted` / `superseded` など。
    pub(crate) fn status(&self) -> Option<&'a str> {
        text(self.0, "status")
    }

    pub(crate) fn owner(&self) -> Option<&'a str> {
        text(self.0, "owner")
    }

    pub(crate) fn options(&self) -> Vec<String> {
        self.0.strs("options")
    }

    pub(crate) fn selected(&self) -> Option<&'a str> {
        text(self.0, "selected")
    }

    pub(crate) fn rationale(&self) -> Option<&'a str> {
        text(self.0, "rationale")
    }

    pub(crate) fn review_triggers(&self) -> Vec<String> {
        self.0.strs("review_triggers")
    }

    pub(crate) fn consequences(&self) -> Vec<String> {
        self.0.strs("consequences")
    }

    /// この判断が置き換えるもの。 graph の `supersedes` と同じ。
    pub(crate) fn supersedes(&self) -> Vec<String> {
        self.0.strs("supersedes")
    }

    /// 置き換えた後継。 graph の edge にしない写し。
    pub(crate) fn superseded_by(&self) -> Option<&'a str> {
        text(self.0, "superseded_by")
    }

    /// 縛る制約の名前と本文。 判断が REQ を作らずに持っている条件。
    pub(crate) fn constraint(&self) -> Option<(&'a str, &'a str)> {
        Some((
            text(self.0, "constraint_label")?,
            text(self.0, "constraint")?,
        ))
    }
}

/// 未解決の問い。 Q。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Question<'a>(&'a Record);

impl<'a> Question<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    /// `open` / `closed`。
    pub(crate) fn status(&self) -> Option<&'a str> {
        text(self.0, "status")
    }

    pub(crate) fn why_it_matters(&self) -> Option<&'a str> {
        text(self.0, "why_it_matters")
    }

    pub(crate) fn how_to_close(&self) -> Option<&'a str> {
        text(self.0, "how_to_close")
    }

    /// 閉じた判断。 古い欄名（`closed_by`）も新しい欄名（`resolved_by`）も同じ意味で読む。
    /// 両方書いてあるときは `resolved_by`。
    pub(crate) fn resolved_by(&self) -> Option<&'a str> {
        text(self.0, "resolved_by").or_else(|| text(self.0, "closed_by"))
    }

    pub(crate) fn closed_how(&self) -> Option<&'a str> {
        text(self.0, "closed_how")
    }
}

/// 調査・観測の記録。 EVID。 欄は調べたことごとに違うので、型つきで読むのは共通の欄だけ。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Evidence<'a>(&'a Record);

impl<'a> Evidence<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    /// 調査の種類（`measurement` / `component_survey` など）。
    pub(crate) fn kind(&self) -> Option<&'a str> {
        text(self.0, "kind")
    }

    pub(crate) fn source(&self) -> Option<&'a str> {
        text(self.0, "source")
    }

    /// 出どころの説明。 目標の `provenance`（locator の一覧）ではなく文章。
    pub(crate) fn provenance_note(&self) -> Option<&'a str> {
        text(self.0, "provenance")
    }

    pub(crate) fn confidence(&self) -> Option<&'a str> {
        text(self.0, "confidence")
    }

    /// 欄名を指定して、自由記述の欄を読む。 欄は調査ごとに違う。
    pub(crate) fn narrative(&self, field: &str) -> Option<&'a str> {
        text(self.0, field)
    }
}

/// 代表 workload。 legacy の `SCALE`。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Workload<'a>(&'a Record);

impl<'a> Workload<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    /// workload の定義。
    pub(crate) fn basis(&self) -> Option<&'a str> {
        text(self.0, "basis")
    }

    pub(crate) fn scope(&self) -> Option<&'a str> {
        text(self.0, "scope")
    }

    /// 代表に選んだ根拠。 目標では DEC に分かれる部分。
    pub(crate) fn selection_rationale(&self) -> Option<&'a str> {
        text(self.0, "rationale")
    }

    pub(crate) fn note(&self) -> Option<&'a str> {
        text(self.0, "note")
    }

    /// 基点から導く値。 実測した basis と、式の結果が混ざっている。 どちらかは
    /// [`Calculation::measured`] が言う。
    pub(crate) fn calculations(&self) -> Vec<Calculation<'a>> {
        self.0
            .tables("derived")
            .into_iter()
            .map(Calculation)
            .collect()
    }
}

/// 導出の1行。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Calculation<'a>(Fields<'a>);

impl Calculation<'_> {
    pub(crate) fn name(&self) -> Option<&str> {
        self.0.str("name")
    }

    /// 値。 係数が実測待ちのものは持たない。 埋めると根拠の無い数字が固まる。
    pub(crate) fn value(&self) -> Option<i64> {
        self.0.int("value")
    }

    pub(crate) fn unit(&self) -> Option<&str> {
        self.0.str("unit")
    }

    pub(crate) fn formula(&self) -> Option<&str> {
        self.0.str("formula")
    }

    pub(crate) fn measured(&self) -> Option<bool> {
        self.0.bool("measured")
    }

    pub(crate) fn note(&self) -> Option<&str> {
        self.0.str("note")
    }
}

/// 部品の台帳の1行。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Component<'a>(&'a Record);

impl<'a> Component<'a> {
    pub(crate) fn name(&self) -> Option<&'a str> {
        text(self.0, "name")
    }

    pub(crate) fn purpose(&self) -> Option<&'a str> {
        text(self.0, "purpose")
    }

    /// license の記述。 目標では EVID に戻る部分。
    pub(crate) fn license(&self) -> Option<&'a str> {
        text(self.0, "license")
    }

    /// 採否の語。 値は legacy の語彙のまま。 [`Adoption::parse`] が読む。
    pub(crate) fn status(&self) -> Option<&'a str> {
        text(self.0, "status")
    }

    pub(crate) fn adoption(&self) -> Option<Adoption> {
        Adoption::parse(self.status()?)
    }

    pub(crate) fn note(&self) -> Option<&'a str> {
        text(self.0, "note")
    }

    pub(crate) fn domains(&self) -> Vec<String> {
        self.0.strs("domains")
    }

    /// 採否を決めた判断。
    pub(crate) fn decided_by(&self) -> Option<&'a str> {
        text(self.0, "decided_by")
    }

    /// この部品が実現する要件。 graph の `implements`。
    pub(crate) fn supports_requirements(&self) -> Vec<String> {
        self.0.strs("supports_requirements")
    }
}

/// 台帳の採否の語。 語彙は legacy のまま1対1で持つ。 まとめ直さない
/// （「不適」と「不採用」を同じ扱いにするかは、まだ誰も決めていない）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Adoption {
    /// 採用
    Adopted,
    /// 不適
    Unsuitable,
    /// 不採用
    Rejected,
    /// 候補外
    OutOfScope,
    /// 採用候補
    Candidate,
    /// 条件付き
    Conditional,
    /// 要調査
    NeedsResearch,
    /// 要選定
    NeedsSelection,
    /// 参照 / 参照のみ
    Reference,
}

impl Adoption {
    /// 知らない語は `None`。 黙って候補として扱わない。
    pub(crate) fn parse(raw: &str) -> Option<Self> {
        Some(match raw {
            "採用" => Self::Adopted,
            "不適" => Self::Unsuitable,
            "不採用" => Self::Rejected,
            "候補外" => Self::OutOfScope,
            "採用候補" => Self::Candidate,
            "条件付き" => Self::Conditional,
            "要調査" => Self::NeedsResearch,
            "要選定" => Self::NeedsSelection,
            "参照" | "参照のみ" => Self::Reference,
            _ => return None,
        })
    }
}

/// 試験の登録。 PROBE。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Probe<'a>(&'a Record);

impl<'a> Probe<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    pub(crate) fn runner(&self) -> Option<&'a str> {
        text(self.0, "runner")
    }

    pub(crate) fn package(&self) -> Option<&'a str> {
        text(self.0, "package")
    }

    pub(crate) fn target(&self) -> Option<&'a str> {
        text(self.0, "target")
    }

    pub(crate) fn platforms(&self) -> Vec<String> {
        self.0.strs("platforms")
    }

    pub(crate) fn backends(&self) -> Vec<String> {
        self.0.strs("backends")
    }

    /// 実行した件数の下限。
    pub(crate) fn min_cases(&self) -> Option<i64> {
        self.0.int("min_cases")
    }

    /// 手動の試験の数。
    pub(crate) fn manual(&self) -> Option<i64> {
        self.0.int("manual")
    }

    /// 支える契約。 REQ・FSL の contract・判断が混ざる。 graph の `contracts`。
    pub(crate) fn contracts(&self) -> Vec<String> {
        self.0.strs("contracts")
    }
}

/// 分類できていない目標値。
#[derive(Debug, Clone, Copy)]
pub(crate) struct UnclassifiedTarget<'a>(&'a Record);

impl<'a> UnclassifiedTarget<'a> {
    pub(crate) fn item(&self) -> Option<&'a str> {
        text(self.0, "item")
    }

    pub(crate) fn goal(&self) -> Option<&'a str> {
        text(self.0, "goal")
    }

    pub(crate) fn note(&self) -> Option<&'a str> {
        text(self.0, "note")
    }

    pub(crate) fn decided_by(&self) -> Option<&'a str> {
        text(self.0, "decided_by")
    }

    pub(crate) fn derives_from(&self) -> Vec<String> {
        self.0.strs("derives_from")
    }

    /// 行き先の候補。 どれにするかは、人が意味で決める。
    pub(crate) fn candidates(&self) -> &'static [Kind] {
        match SCHEMAS.iter().find(|s| s.schema == "target-set") {
            Some(SchemaSpec {
                meaning: Meaning::Unclassified { candidates },
                ..
            }) => candidates,
            _ => &[],
        }
    }
}

/// リリース対象。 運用の定義で、semantic object ではない。
#[derive(Debug, Clone, Copy)]
pub(crate) struct ReleaseProfile<'a>(&'a Record);

impl<'a> ReleaseProfile<'a> {
    pub(crate) fn title(&self) -> Option<&'a str> {
        text(self.0, "title")
    }

    pub(crate) fn status(&self) -> Option<&'a str> {
        text(self.0, "status")
    }

    pub(crate) fn note(&self) -> Option<&'a str> {
        text(self.0, "note")
    }

    pub(crate) fn excludes(&self) -> Vec<String> {
        self.0.strs("excludes")
    }
}

/// FSL の contract。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Contract<'a> {
    record: &'a Record,
    id: &'a Id,
    snapshot: Option<&'a KnowledgeSnapshot>,
}

impl<'a> Contract<'a> {
    /// 接頭辞から決めた種類。 知らない接頭辞は `None`。
    pub(crate) fn kind(&self) -> Option<ContractKind> {
        ContractKind::of(self.id)
    }

    /// FSL の中で宣言されている場所（`specs/….fsl:行`）。 本文は持たない。 正本は FSL。
    pub(crate) fn site(&self) -> Option<&'a str> {
        text(self.record, "site")
    }

    /// この contract が一部を形式にしている REQ。 一部であって、完全な表現ではない
    /// （提案書 5.2.2）。 初版は被覆の度合いを持たない。
    ///
    /// REQ が `formalized_as` で挙げているものを、記録の並びのまま引く。 contract の記録には
    /// 持たせない——REQ の欄を書き写すと、REQ を直すたびに contract の内容の digest が変わる。
    /// snapshot を持たない [`Object::new`] で作った object では空。
    pub(crate) fn formalizes(&self) -> Vec<String> {
        let Some(snapshot) = self.snapshot else {
            return Vec::new();
        };
        snapshot
            .by_schema("requirement-set")
            .filter(|r| {
                r.strs("formalized_as")
                    .iter()
                    .any(|c| c == self.id.as_str())
            })
            .filter_map(|r| r.id().map(ToString::to_string))
            .collect()
    }
}
