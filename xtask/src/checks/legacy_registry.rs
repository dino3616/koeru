//! `check-meta` が graph で参照先を確かめるための、いまの meta の語彙の登録。
//!
//! 本物のオントロジーと legacy adapter は D00 が `knowledge/design.rs` に持つ。 ここは
//! それが入るまでの、`check-meta` だけの互換 view で、D08 の切り替えで旧い schema が
//! 無くなったら D00 の登録に置き換えて消す。 だから `checks` の中に閉じていて、
//! `graph` にも `knowledge` にも語彙を足さない（X05 の engine は具体語を持たない）。
//!
//! 欄の並びと文言は、`check-meta` が graph に載せ替える前から持っていたものをそのまま写している。

use crate::graph::{NodeKey, NodeKind, Registry, RelationClass, RelationRule, SemanticGraph};
use crate::knowledge::Id;

/// 技術要件（`requirement-set` の項目）の node の種類。
pub(super) const REQUIREMENT_KIND: &str = "requirement";
/// FSL の要求の node の種類。 meta の node ではないので、`meta` を指す欄の行き先にならない。
pub(super) const FSL_KIND: &str = "fsl-requirement";

/// 技術要件を指す欄。 行き先が技術要件でなければ「存在しない」。
pub(super) const REQUIREMENT_FIELDS: &[&str] = &[
    "affects_requirements",
    "supports_requirements",
    "source_requirements",
    "derives_from_requirements",
];

/// FSL の要求を指す欄。
pub(super) const FSL_FIELDS: &[&str] = &["affects_fsl", "includes_fsl"];

/// meta のどれかを指す欄。 FSL の要求は行き先にならない。
pub(super) const META_FIELDS: &[&str] = &[
    "affects_decisions",
    "supports_decisions",
    "affects_questions",
    "affects_evidence",
    "affects_components",
    "affects_targets",
    "affects_budgets",
    "source_targets",
    "derives_from",
    "blocks_profiles",
    "decisions",
    "budgets",
    "supersedes",
];

/// 1件1ファイルの schema。 上の欄が graph の edge になるのはこの記録だけ。
///
/// 収集ファイルの項目（`[[component]]` など）はこれらの欄を持たない。 収集ファイルの
/// 直下に置かれた欄は記録ではないので edge にならず、`check-meta` が `SourceRecord` で
/// 読んで、行き先の実在だけを graph に尋ねる。
const ENTITY_SCHEMAS: &[&str] = &[
    "decision",
    "question",
    "evidence",
    "budget",
    "profile",
    "scale-reference",
];

const NODE_KINDS: &[NodeKind] = &[
    NodeKind {
        schema: "requirement-set",
        kind: REQUIREMENT_KIND,
    },
    NodeKind {
        schema: "decision",
        kind: "decision",
    },
    NodeKind {
        schema: "question",
        kind: "question",
    },
    NodeKind {
        schema: "evidence",
        kind: "evidence",
    },
    NodeKind {
        schema: "component-ledger",
        kind: "component",
    },
    NodeKind {
        schema: "budget",
        kind: "budget",
    },
    NodeKind {
        schema: "target-set",
        kind: "target",
    },
    NodeKind {
        schema: "scale-reference",
        kind: "scale-reference",
    },
    NodeKind {
        schema: "profile",
        kind: "profile",
    },
    NodeKind {
        schema: "test-portfolio",
        kind: "suite",
    },
];

/// いまの meta の全 schema を node にし、上の欄を edge にする登録。
///
/// 欄の名前をそのまま relation の名前にする。 `supersedes` だけは後継を作る関係なので
/// `Supersession`、ほかは参照。
pub(super) fn registry() -> Registry {
    let mut relations = Vec::new();
    for schema in ENTITY_SCHEMAS {
        for field in REQUIREMENT_FIELDS
            .iter()
            .chain(FSL_FIELDS)
            .chain(META_FIELDS)
        {
            relations.push(RelationRule {
                from_schema: schema,
                field,
                relation: field,
                class: if *field == "supersedes" {
                    RelationClass::Supersession
                } else {
                    RelationClass::Reference
                },
            });
        }
    }
    Registry {
        node_kinds: NODE_KINDS.to_vec(),
        relations,
        locators: Vec::new(),
        fsl_node_kind: Some(FSL_KIND),
    }
}

/// 参照先を graph で引く。
///
/// 欄の値は ID の形をしているとは限らない。 形をしていなければ、node のどれでもないので
/// 「存在しない」——文字列の集合で引いていたときと同じ答えになる。
pub(super) struct Targets<'a> {
    graph: &'a SemanticGraph,
}

impl<'a> Targets<'a> {
    pub(super) fn new(graph: &'a SemanticGraph) -> Self {
        Self { graph }
    }

    /// `raw` が、この種類の node として在る。
    pub(super) fn is_kind(&self, raw: &str, kind: &str) -> bool {
        self.kind_of(raw).is_some_and(|k| k == kind)
    }

    /// `raw` が、FSL の要求ではない meta の node として在る。
    pub(super) fn is_meta(&self, raw: &str) -> bool {
        self.kind_of(raw).is_some_and(|k| k != FSL_KIND)
    }

    fn kind_of(&self, raw: &str) -> Option<&'static str> {
        let id: Id = raw.parse().ok()?;
        self.graph.node(&NodeKey::Object(id)).map(|n| n.kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 欄の一覧が registry に漏れなく載る。 載せ忘れた欄は、edge にならないだけで
    /// 検査は通ってしまう。
    #[test]
    fn 全ての欄が全ての_entity_schema_に登録される() {
        let reg = registry();
        let fields = REQUIREMENT_FIELDS.len() + FSL_FIELDS.len() + META_FIELDS.len();
        assert_eq!(reg.relations.len(), ENTITY_SCHEMAS.len() * fields);
        for schema in ENTITY_SCHEMAS {
            for field in REQUIREMENT_FIELDS
                .iter()
                .chain(FSL_FIELDS)
                .chain(META_FIELDS)
            {
                assert!(
                    reg.relations
                        .iter()
                        .any(|r| r.from_schema == *schema && r.field == *field),
                    "{schema} の {field} が無い"
                );
            }
        }
    }

    #[test]
    fn 置き換えの欄だけが後継の関係になる() {
        let reg = registry();
        for r in &reg.relations {
            assert_eq!(
                r.class == RelationClass::Supersession,
                r.field == "supersedes",
                "{}",
                r.field
            );
        }
    }

    /// 収集ファイルの schema も node にする。 `affects_components` などの行き先になる。
    #[test]
    fn 収集ファイルの項目も_node_になる() {
        let reg = registry();
        for schema in [
            "requirement-set",
            "component-ledger",
            "target-set",
            "test-portfolio",
        ] {
            assert!(reg.kind_for_schema(schema).is_some(), "{schema}");
        }
        assert_eq!(reg.fsl_node_kind, Some(FSL_KIND));
    }
}
