//! meta のファイル形式と、読んだ1ファイル。
//!
//! 読んだものはまだ `toml::Table` のまま持つ。 型のついた記録にして上の層へ表を
//! 漏らさないようにするのは X03 の仕事で、ここはその置き場所。

use std::collections::BTreeMap;
use std::path::PathBuf;

/// meta のファイル形式。ファイル自身が `schema` で名乗る。
///
/// 名乗らないファイルは落とす。ディレクトリの中身から形を推測すると、
/// 打ち間違えた収集ファイルが「0件を貢献した」ことに誰も気づけない。
#[derive(Debug, Clone, Copy)]
pub(crate) struct Shape {
    pub(crate) schema: &'static str,
    pub(crate) dir: &'static str,
    /// 1件1ファイル。ID の接頭辞と必須項目。
    pub(crate) entity: Option<(&'static str, &'static [&'static str])>,
    /// 配列で複数件。配列のキー、項目 ID の接頭辞、各項目の必須項目。
    /// 項目にも ID を持たせる。 引けないものは参照できず、参照できないものは検査できない。
    pub(crate) collection: Option<(&'static str, &'static str, &'static [&'static str])>,
    /// entity が持ってよい表の配列。ここに無い `[[key]]` は打ち間違いとして弾く。
    /// **collection と同じ穴が entity 側にも空いていた。** 一部だけ綴りを間違えると
    /// 配列は空にならず、その分だけ黙って減る。
    pub(crate) entity_arrays: &'static [&'static str],
}

pub(crate) const SHAPES: &[Shape] = &[
    Shape {
        schema: "requirement-set",
        dir: "requirements",
        entity: None,
        collection: Some((
            "requirement",
            "TR-",
            &["id", "title", "confidence", "statement"],
        )),
        entity_arrays: &[],
    },
    Shape {
        schema: "decision",
        dir: "decisions",
        entity: Some((
            "DEC-",
            &[
                "id",
                "title",
                "status",
                "owner",
                "options",
                "selected",
                "rationale",
                "review_triggers",
            ],
        )),
        collection: None,
        entity_arrays: &[],
    },
    Shape {
        schema: "question",
        dir: "questions",
        entity: Some((
            "Q-",
            &[
                "id",
                "title",
                "status",
                "owner",
                "why_it_matters",
                "how_to_close",
            ],
        )),
        collection: None,
        entity_arrays: &[],
    },
    Shape {
        schema: "evidence",
        dir: "evidence",
        entity: Some((
            "EVID-",
            &["id", "title", "kind", "source", "provenance", "confidence"],
        )),
        collection: None,
        entity_arrays: &[],
    },
    Shape {
        schema: "component-ledger",
        dir: "evidence",
        entity: None,
        collection: Some((
            "component",
            "CMP-",
            &["id", "name", "purpose", "license", "status"],
        )),
        entity_arrays: &[],
    },
    Shape {
        schema: "budget",
        dir: "budgets",
        entity: Some(("BUDGET-", &["id", "title", "limit", "unit", "scope"])),
        collection: None,
        entity_arrays: &["allocations"],
    },
    Shape {
        schema: "target-set",
        dir: "budgets",
        entity: None,
        collection: Some(("target", "TGT-", &["id", "item", "goal"])),
        entity_arrays: &[],
    },
    Shape {
        schema: "scale-reference",
        dir: "budgets",
        entity: Some(("SCALE-", &["id", "title", "basis", "scope", "rationale"])),
        collection: None,
        entity_arrays: &["derived"],
    },
    Shape {
        schema: "profile",
        dir: "profiles",
        entity: Some(("PROFILE-", &["id", "title", "status"])),
        collection: None,
        entity_arrays: &[],
    },
    // 必須の試験の登録（`DEC-PLT-039`）。 件数と前提は `receipt` が突き合わせる。
    Shape {
        schema: "test-portfolio",
        dir: "suites",
        entity: None,
        collection: Some((
            "suite",
            "SUITE-",
            &[
                "id",
                "title",
                "runner",
                "package",
                "target",
                "platforms",
                "min_cases",
            ],
        )),
        entity_arrays: &[],
    },
];

#[derive(Debug)]
pub(crate) struct Entry {
    pub(crate) path: PathBuf,
    pub(crate) shape: &'static Shape,
    pub(crate) table: toml::Table,
}

impl Entry {
    /// 収集ファイルの各項目。1件1ファイルの形なら空。
    pub(crate) fn items(&self) -> Vec<toml::Table> {
        let Some((key, _, _)) = self.shape.collection else {
            return Vec::new();
        };
        self.table
            .get(key)
            .and_then(toml::Value::as_array)
            .map(|a| a.iter().filter_map(|v| v.as_table().cloned()).collect())
            .unwrap_or_default()
    }
}

pub(crate) fn str_of<'a>(t: &'a toml::Table, key: &str) -> Option<&'a str> {
    t.get(key).and_then(toml::Value::as_str)
}

pub(crate) fn list_of(t: &toml::Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn with_schema<'a>(
    entries: &'a [Entry],
    schema: &'static str,
) -> impl Iterator<Item = &'a Entry> {
    entries.iter().filter(move |e| e.shape.schema == schema)
}

/// 技術要件の登録簿。重複した ID は別に返す。
pub(crate) fn requirements(entries: &[Entry]) -> (BTreeMap<String, toml::Table>, Vec<String>) {
    let mut out = BTreeMap::new();
    let mut dups = Vec::new();
    for e in with_schema(entries, "requirement-set") {
        for item in e.items() {
            let Some(id) = item.get("id").and_then(toml::Value::as_str) else {
                continue;
            };
            if out.insert(id.to_owned(), item.clone()).is_some() {
                dups.push(format!("{}: id `{id}` が重複している", e.path.display()));
            }
        }
    }
    (out, dups)
}

/// ID から、それを持つ項目の本文を引く。
///
/// 1件1ファイルの形も、収集ファイルの中の1件も、同じ引き方で返す。
/// どちらなのかは呼び側には関係がない——欲しいのは本文で、置き方ではない。
pub(crate) fn id_index(
    entries: &[Entry],
) -> BTreeMap<String, (&'static str, PathBuf, toml::Table)> {
    let mut out = BTreeMap::new();
    for e in entries {
        if let Some(id) = str_of(&e.table, "id").map(str::to_owned) {
            out.insert(id, (e.shape.schema, e.path.clone(), e.table.clone()));
        }
        for item in e.items() {
            if let Some(id) = str_of(&item, "id").map(str::to_owned) {
                out.insert(id, (e.shape.schema, e.path.clone(), item));
            }
        }
    }
    out
}
