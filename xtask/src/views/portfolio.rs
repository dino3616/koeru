//! Verification Portfolio（D07c）。 要る Probe と、実行された Receipt を突き合わせる。
//!
//! **Probe が登録されていることは、走らせたことを意味しない。 Receipt があることは、その
//! Probe が言いたいことを証明しない**（提案書 10.0）。 登録の `platforms` × `backends` の
//! 組を1つのセルとして、そこへ届いた Receipt を並べる。 セルの状態は提案書 10.2 の実行状態に、
//! 受領証が無いこと（`missing`）と、合格と書いてあっても信用できないこと（`insufficient`）を
//! 足した6つ。
//!
//! | 状態 | 意味 |
//! |---|---|
//! | `executed-and-passed` | 走って、仕事をして、床を満たして、通った |
//! | `executed-and-failed` | 走って、落ちた |
//! | `skipped` | 走らせなかった（`Skipped`）、または走れなかった（`NotRun`） |
//! | `not-applicable` | その組は、どの環境も作らない。 受領証を求めない組（macOS で書いていない OS の席を強制したもの）に届いていない。 または native が要る Probe の native でない backend |
//! | `missing` | その環境の受領証が届いていない |
//! | `insufficient` | 受領証は通ったと言うが、合格に数えない（false green の候補） |
//!
//! **false green にしない。** 通った受領証でも、次のどれかなら `insufficient` に分けて、合格に
//! 数えない。 件数を並べても、どれが信用できないかは分からないので、理由を値で持つ
//! （[`Insufficiency`]）。
//!
//! - 実際の仕事が 0 件（0 件を開いて通った、前提を欠いて早期 return した）
//! - 見つけた件数か実行した件数が、登録の床（`min_cases`）に届かない
//! - native が要る Probe を、native でない backend で走らせた
//! - 要る前提（fixture・LFS の実体・モデル）が無かったと、受領証が言っている
//! - `applies` が偽なのに通った、または対象外と書いてあるのに登録は対象と言う
//!
//! セルに置けない受領証は [`Unplaced`] に分けて持つ。 環境が不明、登録に無い Probe、作れない
//! 環境の名乗り、登録の外の環境で落ちたもの。 どれも合格に数えない。
//!
//! 同じセルに受領証が複数届いたときは、落ちたものを残し、次に合格、不足、走らせなかった、の
//! 順で1つ選ぶ。 同じ順位なら先に渡されたもの。 受領証に時刻が無いので、新しさでは選ばない。
//!
//! 受領証は何度も走らせた結果を重ねて渡せる（CI の macOS・Linux・Windows の artifact を並べる）。
//! ここは合否を持たない（検査は `test-receipt` が持つ）。 値と要約を返すだけ。
//!
//! 出力の形（JSON）は D07 が持つ（実装計画 3.2）。 Budget・Coverage の view と同じく、
//! `command` / `ok` / `diagnostics` の封筒に値を足す形で CLI が出す。

use std::collections::BTreeSet;

use serde_json::{Value, json};

use crate::diagnostic::Report;
use crate::knowledge::{Entry, KnowledgeSnapshot};
use crate::probe::{
    Environment, ExecutionContext, NotRunReason, ProbeDefinition, Receipt, Status, suites,
};
use crate::verification::{Facts, ProbeSpec};

/// 要る Probe の定義。 今の登録（`meta/suites/`）と、記録に実行の契約まで書いた Probe
/// （schema `probe`、D02）を、ID 順に並べる。
///
/// 実行の契約を書いていない Probe（人の観察の手順）は走らせる runner が無いので、
/// Portfolio には入らない。 人の観察の側は [`super::debt`] が持つ。
pub(crate) fn definitions(
    entries: &[Entry],
    snapshot: &KnowledgeSnapshot,
    rep: &mut Report,
) -> Vec<ProbeDefinition> {
    let mut out = suites(entries, rep);
    out.extend(
        Facts::from_snapshot(snapshot)
            .probes
            .iter()
            .filter_map(ProbeSpec::definition),
    );
    out.sort_by(|a, b| a.id.cmp(&b.id));
    out
}

/// 受領証と、それを読んだ場所。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Observed {
    /// 読んだファイル（ディレクトリは渡した順の名前、ファイル名は昇順）。
    pub(crate) source: String,
    pub(crate) receipt: Receipt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum CellState {
    /// 並びは、複数の受領証から1つ選ぶ順位でもある。
    Failed,
    Passed,
    Insufficient,
    Skipped,
    NotApplicable,
    Missing,
}

impl CellState {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Passed => "executed-and-passed",
            Self::Failed => "executed-and-failed",
            Self::Skipped => "skipped",
            Self::NotApplicable => "not-applicable",
            Self::Missing => "missing",
            Self::Insufficient => "insufficient",
        }
    }
}

/// 通ったと書いてあるのに、合格に数えない理由。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Insufficiency {
    /// 実際に仕事をしていない。 `ignored` は、手動のハーネスとして外してある件数。
    NoActualWork { ignored: u64 },
    /// 見つけた件数か実行した件数が、登録の床に届かない。
    BelowMinCases {
        discovered: u64,
        executed: u64,
        min_cases: u64,
    },
    /// native が要る Probe を、native でない backend で走らせた。
    NativeRequired,
    /// 要る前提が無かったと、受領証が言っている。
    MissingPrerequisite(Vec<String>),
    /// 受領証の `applies` と状態が食い違う、または登録が対象と言う環境で対象外と言う。
    ApplicabilityDisagrees,
}

impl Insufficiency {
    pub(crate) fn code(&self) -> &'static str {
        match self {
            Self::NoActualWork { .. } => "no-actual-work",
            Self::BelowMinCases { .. } => "below-min-cases",
            Self::NativeRequired => "native-required",
            Self::MissingPrerequisite(_) => "missing-prerequisite",
            Self::ApplicabilityDisagrees => "applicability-disagrees",
        }
    }

    pub(crate) fn message(&self) -> String {
        match self {
            Self::NoActualWork { ignored } => format!(
                "実際の仕事が 0 件（無視 {ignored} 件）。 0 件を開いて通ったものは合格に数えない"
            ),
            Self::BelowMinCases {
                discovered,
                executed,
                min_cases,
            } => format!(
                "見つけた {discovered} 件・実行した {executed} 件が、登録の下限 {min_cases} 件に届かない"
            ),
            Self::NativeRequired => {
                "native が要る Probe を、native でない backend で走らせた".to_owned()
            }
            Self::MissingPrerequisite(missing) => {
                format!("要る前提が無かった: {}", missing.join(", "))
            }
            Self::ApplicabilityDisagrees => {
                "受領証の対象かどうかが、状態か登録と食い違う".to_owned()
            }
        }
    }
}

/// セルに届いた受領証のうち、状態を決めたもの。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Witness {
    pub(crate) source: String,
    pub(crate) status: Status,
    pub(crate) git_sha: String,
    pub(crate) dirty: bool,
    pub(crate) discovered: u64,
    pub(crate) actual_work: u64,
}

/// Probe の1つの（platform, backend）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cell {
    pub(crate) platform: String,
    pub(crate) backend: String,
    pub(crate) state: CellState,
    /// `Insufficient` のときだけ空でない。
    pub(crate) insufficiencies: Vec<Insufficiency>,
    /// 状態を決めた受領証。 `Missing` と、受領証なしで `NotApplicable` のときは無い。
    pub(crate) witness: Option<Witness>,
    /// 同じセルに届いた受領証の数。
    pub(crate) received: usize,
}

/// Probe 1件のセル。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeRow {
    pub(crate) probe: String,
    pub(crate) runner: String,
    pub(crate) package: String,
    pub(crate) target: String,
    pub(crate) requires_native: bool,
    pub(crate) min_cases: u64,
    /// platform、backend の順。
    pub(crate) cells: Vec<Cell>,
}

/// セルに置けなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnplacedReason {
    /// 環境の欄が無い（古い形の受領証、版が取れなかった受領証）。
    UnknownEnvironment,
    /// 登録に無い Probe の受領証。
    UnknownProbe,
    /// どの環境も作らない組（Linux の native など）を名乗っている。
    UnreachableEnvironment,
    /// Probe が対象にしていない環境の受領証で、対象外ではない状態（落ちたなど）。
    OutsideDeclaration,
}

impl UnplacedReason {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::UnknownEnvironment => "unknown-environment",
            Self::UnknownProbe => "unknown-probe",
            Self::UnreachableEnvironment => "unreachable-environment",
            Self::OutsideDeclaration => "outside-declaration",
        }
    }

    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::UnknownEnvironment => {
                "走らせた環境（platform・backend・版）が分からない。 どのセルの結果にも数えない"
            }
            Self::UnknownProbe => "登録に無い Probe の受領証",
            Self::UnreachableEnvironment => {
                "どの環境も作らない platform と backend の組を名乗っている"
            }
            Self::OutsideDeclaration => {
                "Probe が対象にしていない環境で、対象外ではない結果が出ている"
            }
        }
    }
}

/// セルに置けなかった受領証。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unplaced {
    pub(crate) source: String,
    pub(crate) probe: String,
    pub(crate) status: Status,
    pub(crate) environment: Option<(String, String)>,
    pub(crate) reason: UnplacedReason,
}

/// セルの数を状態ごとに。 合否ではない。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct Summary {
    pub(crate) probes: usize,
    pub(crate) cells: usize,
    pub(crate) passed: usize,
    pub(crate) failed: usize,
    pub(crate) skipped: usize,
    pub(crate) not_applicable: usize,
    pub(crate) missing: usize,
    pub(crate) insufficient: usize,
    /// セルに置けなかった受領証の数。 セルの数には入らない。
    pub(crate) unplaced: usize,
}

/// 要る Probe と、届いた受領証の突き合わせ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Portfolio {
    /// Probe の ID 順。
    pub(crate) probes: Vec<ProbeRow>,
    pub(crate) unplaced: Vec<Unplaced>,
}

const PLATFORM_ORDER: [&str; 3] = ["macos", "linux", "windows"];
const BACKEND_ORDER: [&str; 2] = ["native", "unsupported"];

/// 既知の語を決まった順に、知らない語を後ろに昇順で。
fn ordered(values: &[String], known: &[&str]) -> Vec<String> {
    let set: BTreeSet<&String> = values.iter().collect();
    let mut out: Vec<String> = known
        .iter()
        .filter(|k| set.iter().any(|v| v.as_str() == **k))
        .map(|k| (*k).to_owned())
        .collect();
    out.extend(
        set.into_iter()
            .filter(|v| !known.contains(&v.as_str()))
            .cloned(),
    );
    out
}

impl Portfolio {
    /// `probes` の定義の全セルに、`observed` の受領証を置く。
    pub(crate) fn build(probes: &[ProbeDefinition], observed: &[Observed]) -> Self {
        let mut rows: Vec<ProbeRow> = probes.iter().map(|p| row(p, observed)).collect();
        rows.sort_by(|a, b| a.probe.cmp(&b.probe));
        let unplaced = unplaced(probes, observed);
        Self {
            probes: rows,
            unplaced,
        }
    }

    pub(crate) fn cells(&self) -> impl Iterator<Item = (&ProbeRow, &Cell)> {
        self.probes
            .iter()
            .flat_map(|p| p.cells.iter().map(move |c| (p, c)))
    }

    pub(crate) fn summary(&self) -> Summary {
        let mut s = Summary {
            probes: self.probes.len(),
            unplaced: self.unplaced.len(),
            ..Summary::default()
        };
        for (_, c) in self.cells() {
            s.cells += 1;
            *match c.state {
                CellState::Passed => &mut s.passed,
                CellState::Failed => &mut s.failed,
                CellState::Skipped => &mut s.skipped,
                CellState::NotApplicable => &mut s.not_applicable,
                CellState::Missing => &mut s.missing,
                CellState::Insufficient => &mut s.insufficient,
            } += 1;
        }
        s
    }
}

fn row(p: &ProbeDefinition, observed: &[Observed]) -> ProbeRow {
    let mut cells = Vec::new();
    for platform in ordered(&p.platforms, &PLATFORM_ORDER) {
        for backend in ordered(&p.backends, &BACKEND_ORDER) {
            cells.push(cell(p, &platform, &backend, observed));
        }
    }
    ProbeRow {
        probe: p.id.clone(),
        runner: p.runner.clone(),
        package: p.package.clone(),
        target: p.target.clone(),
        requires_native: p.requires_native,
        min_cases: p.min_cases,
        cells,
    }
}

fn cell(p: &ProbeDefinition, platform: &str, backend: &str, observed: &[Observed]) -> Cell {
    let reachable = ExecutionContext::is_reachable(platform, backend);
    // native が要る Probe の、native でない backend と、受領証を求めない組。
    let wanted = ExecutionContext::is_required(platform, backend)
        && !(p.requires_native && backend != "native");
    let received: Vec<&Observed> = observed
        .iter()
        .filter(|o| o.receipt.probe == p.id)
        .filter(|o| {
            o.receipt
                .environment
                .as_ref()
                .is_some_and(|e| e.platform == platform && e.backend == backend)
        })
        .collect();
    let empty = |state| Cell {
        platform: platform.to_owned(),
        backend: backend.to_owned(),
        state,
        insufficiencies: Vec::new(),
        witness: None,
        received: received.len(),
    };
    // 作れない組は、受領証が届かないのが正しい。 名乗る受領証は `unplaced` に出る。
    if !reachable {
        return empty(CellState::NotApplicable);
    }
    let mut judged: Vec<(CellState, Vec<Insufficiency>, &Observed)> = received
        .iter()
        .map(|o| {
            let (state, why) = judge(p, o, wanted);
            (state, why, *o)
        })
        .collect();
    // 状態の順位が同じなら、先に渡されたもの（安定ソートで、渡された順を保つ）。
    judged.sort_by_key(|(state, _, _)| *state);
    match judged.into_iter().next() {
        None if wanted => empty(CellState::Missing),
        None => empty(CellState::NotApplicable),
        Some((state, insufficiencies, o)) => {
            let env = o
                .receipt
                .environment
                .as_ref()
                .expect("環境で絞り込んだ受領証は環境を持つ");
            Cell {
                platform: platform.to_owned(),
                backend: backend.to_owned(),
                state,
                insufficiencies,
                witness: Some(Witness {
                    source: o.source.clone(),
                    status: o.receipt.status,
                    git_sha: env.git_sha.clone(),
                    dirty: env.dirty,
                    discovered: o.receipt.counts.discovered,
                    actual_work: o.receipt.actual_work,
                }),
                received: received.len(),
            }
        }
    }
}

/// 1枚の受領証が、セルをどの状態にするか。 `wanted` は、その backend で受領証を求めるか。
fn judge(p: &ProbeDefinition, o: &Observed, wanted: bool) -> (CellState, Vec<Insufficiency>) {
    let r = &o.receipt;
    match r.status {
        Status::Failed => (CellState::Failed, Vec::new()),
        Status::Skipped | Status::NotRun(_) => (CellState::Skipped, Vec::new()),
        Status::NotApplicable if !wanted => (CellState::NotApplicable, Vec::new()),
        // 登録は対象と言う環境で、受領証は対象外と言う。 登録が変わったあとの古い受領証か、
        // 書き間違い。 どちらにしても、そのセルを確かめたことにはならない。
        Status::NotApplicable => (
            CellState::Insufficient,
            vec![Insufficiency::ApplicabilityDisagrees],
        ),
        Status::Passed => {
            let mut why = Vec::new();
            if !r.applies {
                why.push(Insufficiency::ApplicabilityDisagrees);
            }
            if p.requires_native && !r.environment.as_ref().is_some_and(Environment::is_native) {
                why.push(Insufficiency::NativeRequired);
            }
            if r.actual_work == 0 {
                why.push(Insufficiency::NoActualWork {
                    ignored: r.counts.ignored,
                });
            } else if r.counts.discovered < p.min_cases || r.actual_work < p.min_cases {
                why.push(Insufficiency::BelowMinCases {
                    discovered: r.counts.discovered,
                    executed: r.actual_work,
                    min_cases: p.min_cases,
                });
            }
            if !r.missing_prerequisites.is_empty() {
                why.push(Insufficiency::MissingPrerequisite(
                    r.missing_prerequisites.clone(),
                ));
            }
            if why.is_empty() {
                (CellState::Passed, why)
            } else {
                (CellState::Insufficient, why)
            }
        }
    }
}

fn unplaced(probes: &[ProbeDefinition], observed: &[Observed]) -> Vec<Unplaced> {
    let mut out = Vec::new();
    for o in observed {
        let r = &o.receipt;
        let reason = match (probes.iter().find(|p| p.id == r.probe), &r.environment) {
            (None, _) => Some(UnplacedReason::UnknownProbe),
            (Some(_), None) => Some(UnplacedReason::UnknownEnvironment),
            (Some(p), Some(e)) => {
                if !ExecutionContext::is_reachable(&e.platform, &e.backend) {
                    Some(UnplacedReason::UnreachableEnvironment)
                } else if r.status != Status::NotApplicable && !declares(p, e) {
                    Some(UnplacedReason::OutsideDeclaration)
                } else {
                    None
                }
            }
        };
        if let Some(reason) = reason {
            out.push(Unplaced {
                source: o.source.clone(),
                probe: r.probe.clone(),
                status: r.status,
                environment: r
                    .environment
                    .as_ref()
                    .map(|e| (e.platform.clone(), e.backend.clone())),
                reason,
            });
        }
    }
    out.sort_by(|a, b| (&a.probe, &a.source).cmp(&(&b.probe, &b.source)));
    out
}

/// Probe の登録が、その環境を対象にしているか。
fn declares(p: &ProbeDefinition, e: &Environment) -> bool {
    p.platforms.contains(&e.platform) && p.backends.contains(&e.backend)
}

pub(crate) fn status_label(status: Status) -> &'static str {
    match status {
        Status::Passed => "passed",
        Status::Failed => "failed",
        Status::Skipped => "skipped",
        Status::NotApplicable => "not-applicable",
        Status::NotRun(NotRunReason::BuildFailed) => "not-run(build-failed)",
        Status::NotRun(NotRunReason::MissingPrerequisite) => "not-run(missing-prerequisite)",
        Status::NotRun(NotRunReason::UnknownRunner) => "not-run(unknown-runner)",
    }
}

impl Summary {
    /// 1行の要約。
    pub(crate) fn line(&self) -> String {
        format!(
            "Probe {} 件 / セル {} 個: executed-and-passed {} / executed-and-failed {} / skipped {} / \
             not-applicable {} / missing {} / insufficient {}（セルに置けない受領証 {} 件）",
            self.probes,
            self.cells,
            self.passed,
            self.failed,
            self.skipped,
            self.not_applicable,
            self.missing,
            self.insufficient,
            self.unplaced
        )
    }

    pub(crate) fn to_json(self) -> Value {
        json!({
            "probes": self.probes,
            "cells": self.cells,
            "executed-and-passed": self.passed,
            "executed-and-failed": self.failed,
            "skipped": self.skipped,
            "not-applicable": self.not_applicable,
            "missing": self.missing,
            "insufficient": self.insufficient,
            "unplaced": self.unplaced,
        })
    }
}

impl Cell {
    /// セルの1行の説明。
    pub(crate) fn describe(&self) -> String {
        let mut out = self.state.label().to_owned();
        if let Some(w) = &self.witness {
            let dirty = if w.dirty { "+dirty" } else { "" };
            let sha: String = w.git_sha.chars().take(9).collect();
            out.push_str(&format!(
                "  {sha}{dirty}  見つけた {} 件 / 実行 {} 件",
                w.discovered, w.actual_work
            ));
            if matches!(self.state, CellState::Skipped) {
                out.push_str(&format!("  （{}）", status_label(w.status)));
            }
        }
        for why in &self.insufficiencies {
            out.push_str(&format!("\n          不足: {}", why.message()));
        }
        out
    }

    fn to_json(&self) -> Value {
        let witness = self.witness.as_ref().map(|w| {
            json!({
                "source": w.source,
                "status": status_label(w.status),
                "git": w.git_sha,
                "dirty": w.dirty,
                "discovered": w.discovered,
                "actual_work": w.actual_work,
            })
        });
        json!({
            "platform": self.platform,
            "backend": self.backend,
            "state": self.state.label(),
            "insufficiencies": self.insufficiencies.iter().map(|i| json!({
                "code": i.code(),
                "message": i.message(),
            })).collect::<Vec<_>>(),
            "witness": witness,
            "received": self.received,
        })
    }
}

impl Portfolio {
    pub(crate) fn to_json(&self) -> Value {
        let probes: Vec<Value> = self
            .probes
            .iter()
            .map(|p| {
                json!({
                    "probe": p.probe,
                    "runner": p.runner,
                    "package": p.package,
                    "target": p.target,
                    "requires_native": p.requires_native,
                    "min_cases": p.min_cases,
                    "cells": p.cells.iter().map(Cell::to_json).collect::<Vec<_>>(),
                })
            })
            .collect();
        let unplaced: Vec<Value> = self
            .unplaced
            .iter()
            .map(|u| {
                json!({
                    "source": u.source,
                    "probe": u.probe,
                    "status": status_label(u.status),
                    "platform": u.environment.as_ref().map(|e| &e.0),
                    "backend": u.environment.as_ref().map(|e| &e.1),
                    "reason": u.reason.code(),
                })
            })
            .collect();
        json!({
            "summary": self.summary().to_json(),
            "probes": probes,
            "unplaced": unplaced,
        })
    }

    /// 端末に出す形。 まず要約、次に Probe ごとのセル、最後にセルに置けなかった受領証。
    pub(crate) fn render_human(&self) -> String {
        let mut out = String::new();
        out.push_str(&format!("  {}\n", self.summary().line()));
        for p in &self.probes {
            let native = if p.requires_native {
                "  [native が要る]"
            } else {
                ""
            };
            out.push_str(&format!(
                "\n  {}  {} {} （{}）{native}\n",
                p.probe, p.package, p.target, p.runner
            ));
            for c in &p.cells {
                out.push_str(&format!(
                    "    {:<8} {:<12} {}\n",
                    c.platform,
                    c.backend,
                    c.describe()
                ));
            }
        }
        if !self.unplaced.is_empty() {
            out.push_str("\n  セルに置けなかった受領証（合格に数えない）\n");
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
}

#[cfg(test)]
mod tests;
