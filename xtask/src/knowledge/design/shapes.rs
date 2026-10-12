//! 目標の schema の、読み込みの形（D00 → X08）。
//!
//! 読み込み（`knowledge::load_view_with`）に `SHAPES` へ足して渡す。 置き場所と必須項目は提案書
//! 5.4 と 5.5 の例に合わせた。 必須にしたのは、その schema の記録が成り立つ最小の欄だけ
//! （`id` と本文）——提案書の例は「最小の拡張」で、必須かどうかは書かれていない。
//!
//! ここに無い目標の schema:
//! - `decision` と `evidence` は今の schema 名を引き継ぐ（`DEC-PLT-048`）。 `SHAPES` が名乗って
//!   いて、目標の欄（`question`・`considers_evidence` など）は足せる任意の欄なので、形を足さない
//! - `workload` は載せない。 `runs_with` の edge を張る D08 が、記録の形と locator の規則を決める
//!   （`DEC-PLT-050`: `meta/verification/workloads/`）
//!
//! `probe` は `meta/verification/probes/` に置く（`DEC-PLT-050`）。 読み込みは `meta/` の下を
//! 歩いて、ファイルの直上のディレクトリの名前（`probes`）だけを `dir` と突き合わせるので、
//! 読む根は足さない。

use crate::knowledge::model::{SHAPES, Shape};

/// 目標の schema の形。 `Design::load` が読み込みへ足す。
pub(crate) const TARGET_SHAPES: &[Shape] = &[
    // 経験的な予測。 `meta/hypotheses/HYP-<opaque>.toml`。
    Shape {
        schema: "design-hypothesis",
        dir: "hypotheses",
        entity: Some(("HYP-", &["id", "statement"])),
        collection: None,
        entity_arrays: &[],
    },
    // 現在の義務。 `meta/requirements/REQ-<opaque>.toml`。 今の `requirement-set`（技術要件の
    // 収集ファイル）と同じ置き場所で、schema 名と1件1ファイルであることで区別する。
    Shape {
        schema: "requirement",
        dir: "requirements",
        entity: Some(("REQ-", &["id", "statement"])),
        collection: None,
        entity_arrays: &[],
    },
    // 繰り返し使う検証方法（D02）。 `meta/verification/probes/PROBE-<opaque>.toml`。 必須にしたのは、
    // 何を（`targets`）どの方法で（`method`）観測するかだけ。 資格（`capabilities.toml`）は
    // この2つから決まる。 実行の契約は任意。
    Shape {
        schema: "probe",
        dir: "probes",
        entity: Some(("PROBE-", &["id", "targets", "method"])),
        collection: None,
        entity_arrays: &[],
    },
];

/// この schema の記録は、1件1ファイルで、ファイル名が ID と一致する（収集ファイルではない）。
pub(crate) fn is_one_per_file(schema: &str) -> bool {
    SHAPES
        .iter()
        .chain(TARGET_SHAPES)
        .any(|s| s.schema == schema && s.entity.is_some())
}

/// 目標の形でだけ書かれる schema。 記録の ID は、目標の形（`<接頭辞>-<opaque>`）でなければならない。
pub(crate) fn is_target_only(schema: &str) -> bool {
    TARGET_SHAPES.iter().any(|s| s.schema == schema)
}
