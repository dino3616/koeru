//! Verification Debt（D07c）。 まだ確かめていないことの待ち行列を、種類ごとに分けて出す。
//!
//! 正本ではない。 仮説・判断・調査・Probe の記録（D02 の
//! [`human_verification_debt`](crate::verification::human_verification_debt)）と、
//! Portfolio（[`super::portfolio`]）の不足から、いつでも引き直せる。
//!
//! **単一の score にしない。** 並びは種類ごとに分け、種類の中では ID 順。 何を先にするかは
//! 持たない。 人と会える一度の機会、CI を回せる一度の機会は、その場で人が選ぶ（提案書 10.3.1）。
//!
//! | 種類 | 何が足りないか |
//! |---|---|
//! | `human` | 人の観察でしか足りない仮説。 D02 の導出 |
//! | `missing` | 要る環境の受領証が届いていない |
//! | `skipped` | 走らせなかった、走れなかった |
//! | `false-green` | 通ったと書いてあるが、合格に数えない |
//! | `unplaced` | セルに置けなかった受領証（環境不明など） |
//!
//! 落ちたセルは債務に入れない。 落ちたのは観測であって、足りないことではない。
//!
//! 提案書 10.3.1 の項目のうち、記録に欄があるものだけを出す。 blast radius・reversibility・
//! 同じ session で観察できる他の仮説は、記録のどこにも欄が無いので出さない（D08 に回してある）。

use serde_json::{Value, json};

use super::portfolio::{Cell, CellState, Portfolio, Unplaced, status_label};
use crate::verification::{EvidenceStance, HumanDebtItem, Shortfall};

/// 実行の側の不足を、どの Probe のどのセルかを付けて持つ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CellDebt {
    pub(crate) probe: String,
    pub(crate) cell: Cell,
}

/// 種類ごとに分けた債務の一覧。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Debt {
    /// 仮説の ID 順。
    pub(crate) human: Vec<HumanDebtItem>,
    /// Probe の ID 順、platform・backend の順。
    pub(crate) missing: Vec<CellDebt>,
    pub(crate) skipped: Vec<CellDebt>,
    pub(crate) false_green: Vec<CellDebt>,
    pub(crate) unplaced: Vec<Unplaced>,
}

impl Debt {
    pub(crate) fn build(human: Vec<HumanDebtItem>, portfolio: &Portfolio) -> Self {
        let of = |state: CellState| -> Vec<CellDebt> {
            portfolio
                .cells()
                .filter(|(_, c)| c.state == state)
                .map(|(p, c)| CellDebt {
                    probe: p.probe.clone(),
                    cell: c.clone(),
                })
                .collect()
        };
        Self {
            human,
            missing: of(CellState::Missing),
            skipped: of(CellState::Skipped),
            false_green: of(CellState::Insufficient),
            unplaced: portfolio.unplaced.clone(),
        }
    }

    /// 債務の総数。 減らすことを目的にしない数で、score ではない。
    pub(crate) fn len(&self) -> usize {
        self.human.len()
            + self.missing.len()
            + self.skipped.len()
            + self.false_green.len()
            + self.unplaced.len()
    }
}

fn shortfall_label(s: Shortfall) -> &'static str {
    match s {
        Shortfall::NoEvidence => "調査が1つも付いていない",
        Shortfall::NoHumanObservation => "調査はあるが、どれも人の観察ではない",
    }
}

fn stance_label(s: EvidenceStance) -> &'static str {
    match s {
        EvidenceStance::Supporting => "支持",
        EvidenceStance::Contradicting => "反証",
        EvidenceStance::Assessing => "評価",
    }
}

fn list(items: &[String]) -> String {
    if items.is_empty() {
        "なし".to_owned()
    } else {
        items.join(", ")
    }
}

/// `design-debt --human`（と、無指定の `human` の節）の端末向けの形。 提案書 10.3.1 の項目のうち、記録に欄があるもの。
pub(crate) fn render_human_debt(items: &[HumanDebtItem]) -> String {
    let mut out = format!(
        "  人の観察が要る仮説 {} 件（優先度は付けない）\n",
        items.len()
    );
    for i in items {
        out.push_str(&format!("\n  {}（{}）\n", i.hypothesis, i.kind));
        out.push_str(&format!(
            "    なぜ人の観察が要るか: 支持できる観測方法がどれも人の観察 — {}\n",
            list(&i.why_human)
        ));
        out.push_str(&format!(
            "    依存している判断: {}\n    その判断が作る・直す要件: {}\n",
            list(&i.decisions),
            list(&i.requirements)
        ));
        let evidence: Vec<String> = i
            .evidence
            .iter()
            .map(|e| {
                format!(
                    "{}（{}、{}）",
                    e.id,
                    e.method.as_deref().unwrap_or("観測方法の欄なし"),
                    stance_label(e.stance)
                )
            })
            .collect();
        out.push_str(&format!(
            "    今ある調査: {}（独立した起源 {} 件）\n    不足: {}\n",
            list(&evidence),
            i.origins,
            shortfall_label(i.shortfall)
        ));
        let probes: Vec<String> = i
            .probes
            .iter()
            .map(|p| {
                let closes = if p.can_close_shortfall {
                    "足りない観察を取れる"
                } else {
                    "取れない"
                };
                format!("{}（{closes}）", p.id)
            })
            .collect();
        out.push_str(&format!(
            "    有効な Probe: {}（{}）\n",
            if i.has_valid_probe() {
                "ある"
            } else {
                "ない"
            },
            list(&probes)
        ));
    }
    out
}

/// `design-debt --human` の機械向けの形。
pub(crate) fn human_debt_json(items: &[HumanDebtItem]) -> Value {
    let items: Vec<Value> = items
        .iter()
        .map(|i| {
            json!({
                "hypothesis": i.hypothesis,
                "kind": i.kind,
                "why_human": i.why_human,
                "decisions": i.decisions,
                "requirements": i.requirements,
                "evidence": i.evidence.iter().map(|e| json!({
                    "id": e.id,
                    "method": e.method,
                    "stance": stance_label(e.stance),
                    "human_qualified": e.human_qualified,
                })).collect::<Vec<_>>(),
                "origins": i.origins,
                "shortfall": match i.shortfall {
                    Shortfall::NoEvidence => "no-evidence",
                    Shortfall::NoHumanObservation => "no-human-observation",
                },
                "probes": i.probes.iter().map(|p| json!({
                    "id": p.id,
                    "method": p.method,
                    "can_close_shortfall": p.can_close_shortfall,
                })).collect::<Vec<_>>(),
                "has_valid_probe": i.has_valid_probe(),
            })
        })
        .collect();
    json!({ "count": items.len(), "human": items })
}

impl Debt {
    /// 種類ごとの端末向けの形。
    pub(crate) fn render_human(&self) -> String {
        let mut out = format!("  債務 {} 件（種類ごと。 優先度は付けない）\n", self.len());
        if !self.human.is_empty() {
            out.push_str("\n  [human] 人の観察が要る仮説\n");
            out.push_str(&render_human_debt(&self.human));
        }
        let sections: [(&str, &[CellDebt]); 3] = [
            ("missing] 受領証が届いていない", &self.missing),
            ("skipped] 走らせなかった、走れなかった", &self.skipped),
            (
                "false-green] 通ったと書いてあるが、合格に数えない",
                &self.false_green,
            ),
        ];
        for (title, cells) in sections {
            if cells.is_empty() {
                continue;
            }
            out.push_str(&format!("\n  [{title}\n"));
            for c in cells {
                out.push_str(&format!(
                    "    {}  {}/{}  {}\n",
                    c.probe,
                    c.cell.platform,
                    c.cell.backend,
                    c.cell.describe()
                ));
            }
        }
        if !self.unplaced.is_empty() {
            out.push_str("\n  [unplaced] セルに置けなかった受領証\n");
            for u in &self.unplaced {
                out.push_str(&format!(
                    "    {}  {}  {}  {}\n",
                    u.probe,
                    status_label(u.status),
                    u.source,
                    u.reason.message()
                ));
            }
        }
        out
    }

    pub(crate) fn to_json(&self) -> Value {
        let cells = |v: &[CellDebt]| -> Vec<Value> {
            v.iter()
                .map(|c| {
                    json!({
                        "probe": c.probe,
                        "platform": c.cell.platform,
                        "backend": c.cell.backend,
                        "state": c.cell.state.label(),
                        "insufficiencies": c.cell.insufficiencies.iter()
                            .map(|i| i.code()).collect::<Vec<_>>(),
                    })
                })
                .collect()
        };
        json!({
            "count": self.len(),
            "human": human_debt_json(&self.human)["human"],
            "missing": cells(&self.missing),
            "skipped": cells(&self.skipped),
            "false_green": cells(&self.false_green),
            "unplaced": self.unplaced.iter().map(|u| json!({
                "probe": u.probe,
                "source": u.source,
                "reason": u.reason.code(),
            })).collect::<Vec<_>>(),
        })
    }
}

#[cfg(test)]
mod tests;
