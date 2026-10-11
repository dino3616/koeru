//! Budget View（D07a）。 上限・小計・モード別のピーク・余白を、配分から数え直す。
//!
//! 上限は定量の REQ が持ち、配分は REQ に付くデータで、合計・ピーク・余白は計算で出す
//! （提案書 5.1.5、実装計画 9.1）。 手書きの正本にしないので、ここにあるのは値を持ち回る型と、
//! それを配分から作る [`BudgetView::build`] だけで、書き戻す口は無い。
//!
//! 数え方は、今の `check-budgets` が合否を持っているあいだ、それと同じ。 違うと D08 の
//! 切り替えで「予算の合計が変わった」ことになるので、`#[cfg(test)]` が実在の meta と fixture の
//! 両方で突き合わせる。
//!
//! - 合計に入れるのは `step` の行だけ（`kind` が無い行も `step`）。 小計の行と参考の行は
//!   二重計上になるので入れない
//! - `mode` の無い行は、どのモードにも乗る共通分。 `mode` を持つ行はそのモードでだけ数える
//! - 上限と比べるのは、モードごとの合計（共通分 + そのモード）の最大値。 モードが1つも
//!   無ければ共通分
//! - `measured` が真でない行は、観測ではなく枠。 [`Step::observed_value`] は枠を返さない
//!
//! ここは SCALE（[`WorkloadView`]）の式を再計算しない。 式は文章で書かれていて、
//! 機械が読める形ではないので、値の由来（観測・式の結果・未設定）を分けるまでを持つ。
//! 目標値（TGT）は、意味で分類されるまで数量として読まない（`DEC-PLT-048`）。

use std::collections::BTreeMap;

use crate::graph::{NodeKey, SemanticGraph};
use crate::knowledge::Id;
use crate::knowledge::design::Design;

/// 上限を導いた要件への関係。 D00 の仮の名前で、D08 が正規の名前へ移すときにここも追従する。
const SOURCE_REQUIREMENTS: &str = "source_requirements";
/// 上限が前提にする workload への関係。
const DERIVES_FROM: &str = "derives_from";

/// 数え直して見つかった、黙って捨てられない問題。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum BudgetIssue {
    /// `limit` が整数でない（無い、または文字列・小数）。 この予算は数えられず、
    /// [`BudgetView::budgets`] に載らない。
    LimitNotInteger { id: Id },
    /// 配分の `kind` が `step` / `subtotal` / `reference` のどれでもない。 合計には入れず、
    /// [`SkippedRows::unknown`] に数える。
    UnknownRowKind { id: Id, kind: String },
}

/// 合計に入る配分の1行（`step`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Step {
    /// legacy では `name` と `item` の両方の綴りがある。
    pub(crate) label: Option<String>,
    /// 数値が無い（または整数でない）行は、合計に 0 として入り、
    /// [`Budget::steps_without_value`] に数える。
    pub(crate) value: Option<i64>,
    /// 無ければ共通分。
    pub(crate) mode: Option<String>,
    /// `measured = true` と書いてある行だけ真。 書いていない行は偽で、枠として扱う。
    pub(crate) measured: bool,
}

impl Step {
    /// 合計に入る値。 数値が無い行は 0。
    pub(crate) fn contribution(&self) -> i64 {
        self.value.unwrap_or(0)
    }

    /// 観測として読める値。 `measured` が真でない行は、数値があっても返さない
    /// ——枠は Evidence ではない（実装計画 9.1）。
    pub(crate) fn observed_value(&self) -> Option<i64> {
        self.measured.then_some(self.value).flatten()
    }
}

/// 合計に入れなかった行の数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SkippedRows {
    /// 小計の行。 値は文章（`stated`）で、足し直せる形ではない。
    pub(crate) subtotal: usize,
    /// 参考の行。 実測の引用などで、配分ではない。
    pub(crate) reference: usize,
    /// `kind` が知らない値の行。 [`BudgetIssue::UnknownRowKind`] も出る。
    pub(crate) unknown: usize,
}

/// 1つのモードの小計。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModeTotal {
    pub(crate) mode: String,
    /// そのモードだけの行の合計。
    pub(crate) own: i64,
    /// 共通分を含む合計。 上限と比べる値。
    pub(crate) total: i64,
}

/// 上限と比べる山。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Peak {
    /// 山になったモード。 モードが無ければ `None`。 同点が複数あるときは、名前順で最後の
    /// モード（今の `check-budgets` と同じ）。 全部は [`Tally::modes_at_peak`]。
    pub(crate) mode: Option<String>,
    pub(crate) total: i64,
}

/// 配分から数えた値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tally {
    /// どのモードにも乗る行の合計。
    pub(crate) common: i64,
    /// モードごとの小計。 モードの名前順。
    pub(crate) modes: Vec<ModeTotal>,
    pub(crate) peak: Peak,
}

impl Tally {
    pub(crate) fn of(steps: &[Step]) -> Self {
        let mut common = 0i64;
        let mut own: BTreeMap<&str, i64> = BTreeMap::new();
        for step in steps {
            let v = step.contribution();
            match step.mode.as_deref() {
                Some(mode) => {
                    let sum = own.entry(mode).or_default();
                    *sum = sum.saturating_add(v);
                }
                None => common = common.saturating_add(v),
            }
        }
        let modes: Vec<ModeTotal> = own
            .into_iter()
            .map(|(mode, own)| ModeTotal {
                mode: mode.to_owned(),
                own,
                total: common.saturating_add(own),
            })
            .collect();
        // `max_by_key` は同点なら最後を返す。 今の `check-budgets` の山の選び方と同じ。
        let peak = modes.iter().max_by_key(|m| m.total).map_or_else(
            || Peak {
                mode: None,
                total: common,
            },
            |m| Peak {
                mode: Some(m.mode.clone()),
                total: m.total,
            },
        );
        Self {
            common,
            modes,
            peak,
        }
    }

    /// 山と同じ合計になったモード。 モードが無ければ空。
    pub(crate) fn modes_at_peak(&self) -> Vec<&str> {
        self.modes
            .iter()
            .filter(|m| m.total == self.peak.total)
            .map(|m| m.mode.as_str())
            .collect()
    }
}

/// 上限が1つの予算。 定量の REQ と、それに付く配分から数えた値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Budget {
    pub(crate) id: Id,
    pub(crate) title: Option<String>,
    pub(crate) unit: String,
    /// 上限が成り立つ条件。 legacy では文章で、workload の locator ではない。
    pub(crate) scope: Option<String>,
    pub(crate) limit: i64,
    /// 上限を導いた要件。
    pub(crate) sources: Vec<Id>,
    /// 上限が前提にするもの。 workload に解けるかは [`BudgetView::workload`] が言う。
    pub(crate) derives_from: Vec<Id>,
    pub(crate) steps: Vec<Step>,
    pub(crate) skipped: SkippedRows,
    pub(crate) tally: Tally,
}

impl Budget {
    pub(crate) fn peak(&self) -> &Peak {
        &self.tally.peak
    }

    /// 上限から山を引いた余白。 負なら超過。 ちょうどなら 0 で、超過ではない。
    pub(crate) fn margin(&self) -> i64 {
        self.limit.saturating_sub(self.tally.peak.total)
    }

    pub(crate) fn is_over_limit(&self) -> bool {
        self.tally.peak.total > self.limit
    }

    /// 山が上限の何パーセントか（小数点以下は切り捨て）。 上限が正でなければ 0。
    pub(crate) fn percent_of_limit(&self) -> i64 {
        if self.limit <= 0 {
            return 0;
        }
        let percent = i128::from(self.tally.peak.total) * 100 / i128::from(self.limit);
        i64::try_from(percent.clamp(i64::MIN.into(), i64::MAX.into())).unwrap_or(0)
    }

    /// 観測の宣言（`measured = true`）が無い行の数。
    pub(crate) fn steps_unmeasured(&self) -> usize {
        self.steps.iter().filter(|s| !s.measured).count()
    }

    /// 数値が無い行の数。 合計には 0 として入っている。
    pub(crate) fn steps_without_value(&self) -> usize {
        self.steps.iter().filter(|s| s.value.is_none()).count()
    }

    /// 観測として読める行。 枠（`measured` が真でない行）は入らない。
    pub(crate) fn observations(&self) -> impl Iterator<Item = &Step> {
        self.steps.iter().filter(|s| s.observed_value().is_some())
    }
}

/// workload から導く値の、由来。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ValueBasis {
    /// 実測した値（`measured = true` で、数値がある）。
    Observed,
    /// 式から出した値。 数値はあるが、観測の宣言は無い。
    Computed,
    /// 数値が無い。 係数が実測待ちで、埋めると根拠の無い数字が固まる。
    Pending,
}

/// workload から導く値の1行。 式は文章のままで、再計算しない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DerivedValue {
    pub(crate) name: Option<String>,
    pub(crate) value: Option<i64>,
    pub(crate) unit: Option<String>,
    pub(crate) formula: Option<String>,
    pub(crate) basis: ValueBasis,
}

/// 代表 workload と、そこから導く値。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkloadView {
    pub(crate) id: Id,
    pub(crate) title: Option<String>,
    /// workload の定義。
    pub(crate) definition: Option<String>,
    pub(crate) scope: Option<String>,
    /// 定義の元になった要件。
    pub(crate) sources: Vec<Id>,
    pub(crate) derived: Vec<DerivedValue>,
}

/// 予算と workload の、数え直した値。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct BudgetView {
    budgets: Vec<Budget>,
    workloads: Vec<WorkloadView>,
    issues: Vec<BudgetIssue>,
}

impl BudgetView {
    /// `design` の定量の REQ と workload を読み、`graph` の関係を引いて数え直す。
    /// 並びは ID の順。
    pub(crate) fn build(design: &Design, graph: &SemanticGraph) -> Self {
        let mut view = Self::default();
        for object in design.objects() {
            let id = object.id();
            if let Some(quantity) = object.as_quantity() {
                let Some(limit) = quantity.limit() else {
                    view.issues
                        .push(BudgetIssue::LimitNotInteger { id: id.clone() });
                    continue;
                };
                let mut steps = Vec::new();
                let mut skipped = SkippedRows::default();
                for row in quantity.allocations() {
                    match row.kind().unwrap_or("step") {
                        "step" => steps.push(Step {
                            label: row.label().map(str::to_owned),
                            value: row.value(),
                            mode: row.mode().map(str::to_owned),
                            measured: row.measured() == Some(true),
                        }),
                        "subtotal" => skipped.subtotal += 1,
                        "reference" => skipped.reference += 1,
                        other => {
                            skipped.unknown += 1;
                            view.issues.push(BudgetIssue::UnknownRowKind {
                                id: id.clone(),
                                kind: other.to_owned(),
                            });
                        }
                    }
                }
                let tally = Tally::of(&steps);
                view.budgets.push(Budget {
                    id: id.clone(),
                    title: quantity.title().map(str::to_owned),
                    unit: quantity.unit().unwrap_or_default().to_owned(),
                    scope: quantity.scope().map(str::to_owned),
                    limit,
                    sources: related(graph, id, SOURCE_REQUIREMENTS),
                    derives_from: related(graph, id, DERIVES_FROM),
                    steps,
                    skipped,
                    tally,
                });
            } else if let Some(workload) = object.as_workload() {
                view.workloads.push(WorkloadView {
                    id: id.clone(),
                    title: workload.title().map(str::to_owned),
                    definition: workload.basis().map(str::to_owned),
                    scope: workload.scope().map(str::to_owned),
                    sources: related(graph, id, SOURCE_REQUIREMENTS),
                    derived: workload
                        .calculations()
                        .iter()
                        .map(|c| DerivedValue {
                            name: c.name().map(str::to_owned),
                            value: c.value(),
                            unit: c.unit().map(str::to_owned),
                            formula: c.formula().map(str::to_owned),
                            basis: match (c.value(), c.measured() == Some(true)) {
                                (None, _) => ValueBasis::Pending,
                                (Some(_), true) => ValueBasis::Observed,
                                (Some(_), false) => ValueBasis::Computed,
                            },
                        })
                        .collect(),
                });
            }
        }
        view.budgets.sort_by(|a, b| a.id.cmp(&b.id));
        view.workloads.sort_by(|a, b| a.id.cmp(&b.id));
        view
    }

    /// 型つきの口と graph を、ここで組む。 graph を使い回したい呼び側は [`Self::build`]。
    pub(crate) fn from_design(design: &Design) -> Self {
        Self::build(design, &design.graph())
    }

    pub(crate) fn budgets(&self) -> &[Budget] {
        &self.budgets
    }

    pub(crate) fn budget(&self, id: &Id) -> Option<&Budget> {
        self.budgets.iter().find(|b| &b.id == id)
    }

    pub(crate) fn workloads(&self) -> &[WorkloadView] {
        &self.workloads
    }

    pub(crate) fn workload(&self, id: &Id) -> Option<&WorkloadView> {
        self.workloads.iter().find(|w| &w.id == id)
    }

    pub(crate) fn issues(&self) -> &[BudgetIssue] {
        &self.issues
    }

    /// 山が上限を超えている予算。
    pub(crate) fn over_limit(&self) -> impl Iterator<Item = &Budget> {
        self.budgets.iter().filter(|b| b.is_over_limit())
    }
}

/// `from` から `relation` で出る edge の行き先。 ID の順で、重複しない。
/// 行き先が記録として在るかは見ない（宙に浮いた edge は `Design::audit` が出す）。
fn related(graph: &SemanticGraph, from: &Id, relation: &str) -> Vec<Id> {
    let mut out: Vec<Id> = graph
        .outgoing(&NodeKey::Object(from.clone()))
        .iter()
        .filter(|r| r.name == relation)
        .filter_map(|r| match &r.to {
            NodeKey::Object(id) => Some(id.clone()),
            NodeKey::Locator { .. } => None,
        })
        .collect();
    out.sort();
    out.dedup();
    out
}

/// fixture の ID は `fix` を領域にする（`check-references` が拾わない綴り）。 実在する meta を
/// 読ませる試験が、最後に1本ある。
#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;
    use crate::checks::{CHECKS, CheckContext};
    use crate::diagnostic::Report;
    use crate::knowledge::{KnowledgeSnapshot, load_view};
    use crate::repo::RepoView;

    fn id(raw: &str) -> Id {
        raw.parse().expect("ID の形")
    }

    /// 1つの予算の meta。 `rows` は `[[allocations]]` の中身。
    fn budget_toml(id: &str, limit: &str, rows: &[&str]) -> String {
        let mut s = format!(
            "schema = 'budget'\nid = '{id}'\ntitle = '予算'\nlimit = {limit}\nunit = 'MB'\nscope = '条件'\n"
        );
        for row in rows {
            s.push_str("\n[[allocations]]\n");
            s.push_str(row);
            s.push('\n');
        }
        s
    }

    fn files_of(budgets: &[(&str, String)]) -> BTreeMap<String, String> {
        budgets
            .iter()
            .map(|(id, body)| (format!("meta/budgets/{id}.toml"), body.clone()))
            .collect()
    }

    fn in_memory(files: &BTreeMap<String, String>) -> RepoView {
        RepoView::in_memory(Path::new("/fixture"), files.clone())
    }

    fn view_of(files: &BTreeMap<String, String>) -> BudgetView {
        BudgetView::from_design(&Design::load(&in_memory(files)))
    }

    /// 1つだけ置いた予算を数える。 読み込みの問題は無いことも確かめる。
    fn one(limit: &str, rows: &[&str]) -> Budget {
        let files = files_of(&[("BUDGET-fix-001", budget_toml("BUDGET-fix-001", limit, rows))]);
        let design = Design::load(&in_memory(&files));
        assert_eq!(design.issues(), &[]);
        let view = BudgetView::from_design(&design);
        assert_eq!(view.issues(), &[]);
        view.budget(&id("BUDGET-fix-001"))
            .expect("予算が載る")
            .clone()
    }

    const COMMON_MEASURED: &str = "name = '共通'\nkind = 'step'\nvalue = 100\nmeasured = true";
    const COMMON_NO_VALUE: &str = "name = '値なし'\nkind = 'step'\nmeasured = false";
    const REC_400: &str =
        "name = '収録 a'\nkind = 'step'\nmode = '収録中'\nvalue = 400\nmeasured = false";
    const REC_100: &str = "item = '収録 b'\nmode = '収録中'\nvalue = 100";
    const EDIT_300: &str =
        "name = '編集'\nkind = 'step'\nmode = '編集中'\nvalue = 300\nmeasured = false";
    const SUBTOTAL: &str = "name = '小計'\nkind = 'subtotal'\nstated = '1.0s'";
    const REFERENCE: &str = "name = '参考'\nkind = 'reference'\nstated = '実測'";

    // ## 数え方

    #[test]
    fn 共通分とモード別の小計と山と余白を配分から数える() {
        let b = one(
            "1000",
            &[
                COMMON_MEASURED,
                COMMON_NO_VALUE,
                REC_400,
                REC_100,
                EDIT_300,
                SUBTOTAL,
                REFERENCE,
            ],
        );
        assert_eq!(b.limit, 1000);
        assert_eq!(b.unit, "MB");
        assert_eq!(b.tally.common, 100);
        assert_eq!(
            b.tally.modes,
            vec![
                ModeTotal {
                    mode: "収録中".to_owned(),
                    own: 500,
                    total: 600,
                },
                ModeTotal {
                    mode: "編集中".to_owned(),
                    own: 300,
                    total: 400,
                },
            ]
        );
        assert_eq!(b.peak().mode.as_deref(), Some("収録中"));
        assert_eq!(b.peak().total, 600);
        assert_eq!(b.margin(), 400);
        assert_eq!(b.percent_of_limit(), 60);
        assert!(!b.is_over_limit());
    }

    #[test]
    fn 小計と参考の行は合計に入れず数だけ持つ() {
        let b = one("1000", &[COMMON_MEASURED, SUBTOTAL, SUBTOTAL, REFERENCE]);
        assert_eq!(b.steps.len(), 1);
        assert_eq!(b.tally.common, 100);
        assert_eq!(
            b.skipped,
            SkippedRows {
                subtotal: 2,
                reference: 1,
                unknown: 0,
            }
        );
    }

    #[test]
    fn kind_を書かない行は_step_で_name_も_item_も_label_になる() {
        let b = one("1000", &[REC_100, COMMON_MEASURED]);
        assert_eq!(b.steps.len(), 2);
        assert_eq!(b.steps[0].label.as_deref(), Some("収録 b"));
        assert_eq!(b.steps[1].label.as_deref(), Some("共通"));
    }

    #[test]
    fn 数値の無い行は_0_として入り数えられる() {
        let b = one("1000", &[COMMON_MEASURED, COMMON_NO_VALUE]);
        assert_eq!(b.tally.common, 100);
        assert_eq!(b.steps_without_value(), 1);
    }

    #[test]
    fn 共通分はどのモードにも乗り_モード同士は足さない() {
        let b = one("1000", &[COMMON_MEASURED, REC_400, EDIT_300]);
        assert_eq!(b.peak().total, 500);
        // 足し合わせると 800 になる。 同時に常駐しない工程を足さない。
        assert_ne!(b.peak().total, 100 + 400 + 300);
    }

    #[test]
    fn 共通分が無ければモードだけで数え_モードが無ければ共通分が山になる() {
        let only_modes = one("1000", &[REC_400, EDIT_300]);
        assert_eq!(only_modes.tally.common, 0);
        assert_eq!(only_modes.peak().total, 400);
        assert_eq!(only_modes.peak().mode.as_deref(), Some("収録中"));

        let only_common = one("1000", &[COMMON_MEASURED]);
        assert!(only_common.tally.modes.is_empty());
        assert_eq!(only_common.peak().mode, None);
        assert_eq!(only_common.peak().total, 100);
    }

    #[test]
    fn 山が同点なら名前順で最後のモードを選び_同点は全部引ける() {
        let tie = "name = '編集'\nmode = '編集中'\nvalue = 400";
        let b = one("1000", &[REC_400, tie]);
        // `check-budgets` と同じ選び方。
        assert_eq!(b.peak().mode.as_deref(), Some("編集中"));
        assert_eq!(b.tally.modes_at_peak(), vec!["収録中", "編集中"]);
    }

    // ## 境界

    #[test]
    fn 合計に入る配分が0件なら山は0で余白は上限そのまま() {
        // 読み込みは配分の表を1件以上要求する。 合計に入る行が無いのは、小計だけの予算。
        let b = one("1000", &[SUBTOTAL]);
        assert!(b.steps.is_empty());
        assert_eq!(b.skipped.subtotal, 1);
        assert_eq!(b.tally.common, 0);
        assert_eq!(b.peak().total, 0);
        assert_eq!(b.peak().mode, None);
        assert_eq!(b.margin(), 1000);
        assert_eq!(b.percent_of_limit(), 0);
        assert!(!b.is_over_limit());
    }

    #[test]
    fn 行が1つも無い配分の数え方も山は0でモードは無い() {
        let t = Tally::of(&[]);
        assert_eq!(t.common, 0);
        assert!(t.modes.is_empty());
        assert_eq!(
            t.peak,
            Peak {
                mode: None,
                total: 0
            }
        );
        assert!(t.modes_at_peak().is_empty());
    }

    #[test]
    fn 配分が1件ならそれが山() {
        let b = one("1000", &[REC_400]);
        assert_eq!(b.peak().total, 400);
        assert_eq!(b.peak().mode.as_deref(), Some("収録中"));
        assert_eq!(b.margin(), 600);
        assert_eq!(b.percent_of_limit(), 40);
    }

    #[test]
    fn 上限ちょうどは超過ではなく_1_超えると超過() {
        let exact = one("400", &[REC_400]);
        assert_eq!(exact.margin(), 0);
        assert_eq!(exact.percent_of_limit(), 100);
        assert!(!exact.is_over_limit());

        let over = one("399", &[REC_400]);
        assert_eq!(over.margin(), -1);
        assert_eq!(over.percent_of_limit(), 100);
        assert!(over.is_over_limit());
    }

    #[test]
    fn 割合は小数点以下を切り捨て_上限が正でなければ_0() {
        // 400 / 1500 = 26.67%
        assert_eq!(one("1500", &[REC_400]).percent_of_limit(), 26);
        let zero = one("0", &[REC_400]);
        assert_eq!(zero.percent_of_limit(), 0);
        assert_eq!(zero.margin(), -400);
        assert!(zero.is_over_limit());
    }

    #[test]
    fn 整数でない上限の予算は数えず_問題として出る() {
        let files = files_of(&[
            (
                "BUDGET-fix-001",
                budget_toml("BUDGET-fix-001", "'千'", &[REC_400]),
            ),
            (
                "BUDGET-fix-002",
                budget_toml("BUDGET-fix-002", "10.5", &[REC_400]),
            ),
            (
                "BUDGET-fix-003",
                budget_toml("BUDGET-fix-003", "1000", &[REC_400]),
            ),
        ]);
        let view = view_of(&files);
        assert_eq!(
            view.issues(),
            &[
                BudgetIssue::LimitNotInteger {
                    id: id("BUDGET-fix-001")
                },
                BudgetIssue::LimitNotInteger {
                    id: id("BUDGET-fix-002")
                },
            ]
        );
        let ids: Vec<&str> = view.budgets().iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, vec!["BUDGET-fix-003"]);
    }

    #[test]
    fn 知らない_kind_の行は合計に入れず問題として出る() {
        let files = files_of(&[(
            "BUDGET-fix-001",
            budget_toml(
                "BUDGET-fix-001",
                "1000",
                &[REC_400, "name = '謎'\nkind = 'mystery'\nvalue = 999"],
            ),
        )]);
        let view = view_of(&files);
        assert_eq!(
            view.issues(),
            &[BudgetIssue::UnknownRowKind {
                id: id("BUDGET-fix-001"),
                kind: "mystery".to_owned(),
            }]
        );
        let b = &view.budgets()[0];
        assert_eq!(b.peak().total, 400);
        assert_eq!(b.skipped.unknown, 1);
    }

    #[test]
    fn 予算は_id_の順に並ぶ() {
        let files = files_of(&[
            (
                "BUDGET-fix-002",
                budget_toml("BUDGET-fix-002", "10", &[REC_400]),
            ),
            (
                "BUDGET-fix-001",
                budget_toml("BUDGET-fix-001", "10", &[REC_400]),
            ),
        ]);
        let ids: Vec<String> = view_of(&files)
            .budgets()
            .iter()
            .map(|b| b.id.to_string())
            .collect();
        assert_eq!(ids, vec!["BUDGET-fix-001", "BUDGET-fix-002"]);
    }

    #[test]
    fn 超過している予算だけが_over_limit_に出る() {
        let files = files_of(&[
            (
                "BUDGET-fix-001",
                budget_toml("BUDGET-fix-001", "399", &[REC_400]),
            ),
            (
                "BUDGET-fix-002",
                budget_toml("BUDGET-fix-002", "400", &[REC_400]),
            ),
        ]);
        let view = view_of(&files);
        let over: Vec<&str> = view.over_limit().map(|b| b.id.as_str()).collect();
        assert_eq!(over, vec!["BUDGET-fix-001"]);
    }

    // ## 枠は観測ではない

    #[test]
    fn measured_が真でない行は数値があっても観測にならない() {
        let b = one(
            "1000",
            &[COMMON_MEASURED, REC_400, REC_100, COMMON_NO_VALUE],
        );
        // `measured = true` は 1 行だけ。 書いていない行（REC_100）も枠。
        let observed: Vec<Option<&str>> = b.observations().map(|s| s.label.as_deref()).collect();
        assert_eq!(observed, vec![Some("共通")]);
        assert_eq!(b.steps_unmeasured(), 3);
        // 枠の値は合計には入る。 観測でないことと、配分でないことは別。
        assert_eq!(b.peak().total, 600);
        for step in b.steps.iter().filter(|s| !s.measured) {
            assert_eq!(step.observed_value(), None);
        }
    }

    #[test]
    fn 観測の宣言が真でも数値が無ければ観測の値は無い() {
        let step = Step {
            label: None,
            value: None,
            mode: None,
            measured: true,
        };
        assert_eq!(step.observed_value(), None);
        assert_eq!(step.contribution(), 0);
    }

    // ## 関係と workload

    const SCALE: &str = "schema = 'scale-reference'
id = 'SCALE-fix-001'
title = '代表'
basis = '定義'
scope = '範囲'
rationale = '選んだ理由'
source_requirements = ['TR-fix-01']

[[derived]]
name = '総行数'
value = 444
unit = '行'
formula = '148 × 3'
measured = true

[[derived]]
name = '読み上げ時間'
value = 5328
unit = '秒'
formula = '444 × 12.0'
measured = false

[[derived]]
name = '容量'
unit = 'GB'
formula = '式'
measured = false

[[derived]]
name = '値の無い観測'
formula = '式'
measured = true
";

    fn with_scale() -> BTreeMap<String, String> {
        let mut budget = budget_toml("BUDGET-fix-001", "1000", &[REC_400]);
        budget.insert_str(
            budget.find("title").unwrap_or(0),
            "derives_from = ['SCALE-fix-001', 'SCALE-fix-404']\nsource_requirements = ['TR-fix-02', 'TR-fix-01']\n",
        );
        let mut f = files_of(&[("BUDGET-fix-001", budget)]);
        f.insert(
            "meta/budgets/SCALE-fix-001.toml".to_owned(),
            SCALE.to_owned(),
        );
        f
    }

    #[test]
    fn 予算は導出元と上限を導いた要件を_graph_の関係から持つ() {
        let view = view_of(&with_scale());
        let b = view.budget(&id("BUDGET-fix-001")).expect("予算が載る");
        assert_eq!(
            b.derives_from,
            vec![id("SCALE-fix-001"), id("SCALE-fix-404")]
        );
        assert_eq!(b.sources, vec![id("TR-fix-01"), id("TR-fix-02")]);
        // 解けるのは記録がある導出元だけ。 宙に浮いた edge は `Design::audit` が出す。
        assert!(view.workload(&id("SCALE-fix-001")).is_some());
        assert!(view.workload(&id("SCALE-fix-404")).is_none());
    }

    #[test]
    fn workload_の導出値は由来で分かれ_式は再計算されない() {
        let view = view_of(&with_scale());
        let w = view
            .workload(&id("SCALE-fix-001"))
            .expect("workload が載る");
        assert_eq!(w.definition.as_deref(), Some("定義"));
        assert_eq!(w.sources, vec![id("TR-fix-01")]);
        let basis: Vec<(Option<&str>, ValueBasis)> = w
            .derived
            .iter()
            .map(|d| (d.name.as_deref(), d.basis))
            .collect();
        assert_eq!(
            basis,
            vec![
                (Some("総行数"), ValueBasis::Observed),
                (Some("読み上げ時間"), ValueBasis::Computed),
                (Some("容量"), ValueBasis::Pending),
                (Some("値の無い観測"), ValueBasis::Pending),
            ]
        );
        // 式の文章は読んだままで、値も書いてあるとおり（5328 は式から出し直していない）。
        assert_eq!(w.derived[1].value, Some(5328));
        assert_eq!(w.derived[1].formula.as_deref(), Some("444 × 12.0"));
    }

    // ## 今の `check-budgets` との一致

    /// 今の `check-budgets` を、同じ記録に走らせる。 層は `views` の下に `checks` が無いが、
    /// ここは「同じ数が出る」ことの oracle として引くだけで、production は引かない。
    fn run_legacy(entries: &[crate::knowledge::Entry]) -> Report {
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(entries);
        assert!(dups.is_empty());
        let ctx = CheckContext {
            root: Path::new("/fixture"),
            entries,
            snapshot: &snapshot,
            args: &[],
        };
        let check = CHECKS
            .iter()
            .find(|c| c.name == "check-budgets")
            .expect("check-budgets が登録されている");
        let mut rep = Report::default();
        (check.run)(&ctx, &mut rep);
        rep
    }

    /// `check-budgets` が予算ごとに出す行を、view の値から同じ書式で組む。
    fn legacy_lines(b: &Budget) -> (String, Vec<String>) {
        let unit = &b.unit;
        let peak_mode = b.peak().mode.as_deref().unwrap_or("—");
        let peak = format!(
            "{}: 山 {}{unit}（{peak_mode}） / 上限 {}{unit}（{}%）工程 {} 件 / 実測済みでない {} / 数値未設定 {}",
            b.id,
            b.peak().total,
            b.limit,
            b.percent_of_limit(),
            b.steps.len(),
            b.steps_unmeasured(),
            b.steps_without_value(),
        );
        let modes = b
            .tally
            .modes
            .iter()
            .map(|m| {
                format!(
                    "    {}: {}{unit}（共通 {}{unit} を含む）",
                    m.mode, m.total, b.tally.common
                )
            })
            .collect();
        (peak, modes)
    }

    /// 同じ版を `check-budgets` と view の両方で読み、予算の数・山・モード別・超過が一致する。
    fn assert_agrees(view: &RepoView) -> BudgetView {
        let mut rep = Report::default();
        let entries = load_view(view, &mut rep);
        assert!(
            !rep.has_errors(),
            "meta がそもそも読めない: {:?}",
            rep.diagnostics()
        );
        let legacy = run_legacy(&entries);

        let mut peaks: BTreeMap<String, String> = BTreeMap::new();
        let mut modes: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut over: Vec<String> = Vec::new();
        let mut not_integer = 0usize;
        let mut current = String::new();
        for d in legacy.diagnostics() {
            match d.code {
                "budgets.peak" => {
                    let (id, _) = d.message.split_once(": ").expect("ID で始まる");
                    current = id.to_owned();
                    peaks.insert(current.clone(), d.message.clone());
                }
                "budgets.mode" => modes
                    .entry(current.clone())
                    .or_default()
                    .push(d.message.clone()),
                "budgets.over-limit" => {
                    let (id, _) = d.message.split_once(": ").expect("ID で始まる");
                    over.push(id.to_owned());
                }
                "budgets.limit-not-integer" => not_integer += 1,
                other => panic!("知らない診断 {other}"),
            }
        }

        let budgets = BudgetView::from_design(&Design::load(view));
        let counted = budgets
            .issues()
            .iter()
            .filter(|i| matches!(i, BudgetIssue::LimitNotInteger { .. }))
            .count();
        assert_eq!(counted, not_integer);
        assert_eq!(
            budgets
                .budgets()
                .iter()
                .map(|b| b.id.to_string())
                .collect::<Vec<_>>(),
            peaks.keys().cloned().collect::<Vec<_>>(),
            "数えた予算の集合が違う"
        );
        for b in budgets.budgets() {
            let (peak, mode_lines) = legacy_lines(b);
            assert_eq!(Some(&peak), peaks.get(b.id.as_str()), "{} の山", b.id);
            assert_eq!(
                mode_lines,
                modes.get(b.id.as_str()).cloned().unwrap_or_default(),
                "{} のモード別",
                b.id
            );
        }
        over.sort();
        let mut over_view: Vec<String> = budgets.over_limit().map(|b| b.id.to_string()).collect();
        over_view.sort();
        assert_eq!(over_view, over, "上限を超える予算の集合が違う");
        budgets
    }

    #[test]
    fn fixture_の予算は今の_check_budgets_と同じ数を出す() {
        let files = files_of(&[
            (
                "BUDGET-fix-001",
                budget_toml(
                    "BUDGET-fix-001",
                    "1000",
                    &[
                        COMMON_MEASURED,
                        COMMON_NO_VALUE,
                        REC_400,
                        REC_100,
                        EDIT_300,
                        SUBTOTAL,
                        REFERENCE,
                    ],
                ),
            ),
            // 上限ちょうど、超過、配分なし、モードなし、同点。
            (
                "BUDGET-fix-002",
                budget_toml("BUDGET-fix-002", "400", &[REC_400]),
            ),
            (
                "BUDGET-fix-003",
                budget_toml("BUDGET-fix-003", "399", &[REC_400]),
            ),
            (
                "BUDGET-fix-004",
                budget_toml("BUDGET-fix-004", "10", &[SUBTOTAL]),
            ),
            (
                "BUDGET-fix-005",
                budget_toml("BUDGET-fix-005", "1000", &[COMMON_MEASURED]),
            ),
            (
                "BUDGET-fix-006",
                budget_toml(
                    "BUDGET-fix-006",
                    "1000",
                    &[REC_400, "name = '編集'\nmode = '編集中'\nvalue = 400"],
                ),
            ),
            (
                "BUDGET-fix-007",
                budget_toml("BUDGET-fix-007", "0", &[REC_400]),
            ),
            (
                "BUDGET-fix-008",
                budget_toml("BUDGET-fix-008", "'千'", &[REC_400]),
            ),
        ]);
        let budgets = assert_agrees(&in_memory(&files));
        // 一致の試験が空振りしていない。
        assert_eq!(budgets.budgets().len(), 7);
        assert_eq!(budgets.over_limit().count(), 2);
    }

    /// 実在する meta を読ませる。 `BUDGET-MEMORY-001` の山（モード別の最大）が、
    /// `check-budgets` と同じになる（実装計画 8.1「BUDGET memory peak」）。
    #[test]
    fn 実在する_meta_の予算は今の_check_budgets_と同じ数を出す() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("リポジトリの根");
        let view = RepoView::working_tree(root);
        let budgets = assert_agrees(&view);
        assert_eq!(budgets.issues(), &[]);

        let mut rep = Report::default();
        let entries = load_view(&view, &mut rep);
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        assert_eq!(
            budgets.budgets().len(),
            snapshot.by_schema("budget").count(),
            "予算を取りこぼしていない"
        );

        let memory = budgets
            .budget(&id("BUDGET-MEMORY-001"))
            .expect("メモリの予算");
        assert!(memory.peak().mode.is_some(), "モード別の山を持つ");
        let scale = id("SCALE-REF-001");
        assert!(memory.derives_from.contains(&scale));
        let workload = budgets.workload(&scale).expect("代表 workload");
        let has = |basis| workload.derived.iter().any(|d| d.basis == basis);
        assert!(has(ValueBasis::Observed));
        assert!(has(ValueBasis::Pending));
    }

    /// 実在する meta の上限を山ちょうど・山の 1 下へ書き換えても、今の検査と同じ判定になる。
    /// 山は view から引くので、meta の配分が変わっても試験は古くならない。
    #[test]
    fn 上限を山に合わせた実在する_meta_でも今の_check_budgets_と同じ超過を出す() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("リポジトリの根");
        let name = "meta/budgets/BUDGET-MEMORY-001.toml";
        let text = std::fs::read_to_string(root.join(name)).expect("実在する予算");
        let real = BudgetView::from_design(&Design::load(&RepoView::working_tree(root)));
        let peak = real
            .budget(&id("BUDGET-MEMORY-001"))
            .expect("メモリの予算")
            .peak()
            .total;

        let with_limit = |limit: i64| {
            let body: Vec<String> = text
                .lines()
                .map(|l| {
                    if l.starts_with("limit = ") {
                        format!("limit = {limit}")
                    } else {
                        l.to_owned()
                    }
                })
                .collect();
            BTreeMap::from([(name.to_owned(), body.join("\n"))])
        };

        let exact = assert_agrees(&in_memory(&with_limit(peak)));
        assert_eq!(exact.over_limit().count(), 0);
        assert_eq!(exact.budgets()[0].margin(), 0);

        let over = assert_agrees(&in_memory(&with_limit(peak - 1)));
        assert_eq!(over.over_limit().count(), 1);
        assert_eq!(over.budgets()[0].margin(), -1);
    }
}
