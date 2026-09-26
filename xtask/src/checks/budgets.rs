//! `cargo xtask check-budgets`

use std::collections::BTreeMap;
use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, str_of, with_schema};

pub(crate) fn check_budgets(entries: &[Entry], mut rep: Report) -> ExitCode {
    for e in with_schema(entries, "budget") {
        let file = e.path.display().to_string();
        let id = str_of(&e.table, "id").unwrap_or("?");
        let Some(limit) = e.table.get("limit").and_then(toml::Value::as_integer) else {
            rep.error(format!("{file}: `limit` が整数でない"));
            continue;
        };
        let unit = str_of(&e.table, "unit").unwrap_or("");
        let allocations = e
            .table
            .get("allocations")
            .and_then(toml::Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or_default();

        // 同時に常駐しない工程を足し合わせない。 `mode` を持つ行はそのモードでだけ数え、
        // 持たない行はどのモードにも乗る。上限と比べるのはモードごとの合計の最大値。
        let mut common = 0i64;
        let mut per_mode: BTreeMap<String, i64> = BTreeMap::new();
        let mut unmeasured = 0usize;
        let mut steps = 0usize;
        let mut without_value = 0usize;
        for a in allocations {
            let Some(t) = a.as_table() else { continue };
            // 小計行と参考行は二重計上になるので合計に入れない。
            let kind = t
                .get("kind")
                .and_then(toml::Value::as_str)
                .unwrap_or("step");
            if kind != "step" {
                continue;
            }
            steps += 1;
            let v = match t.get("value").and_then(toml::Value::as_integer) {
                Some(v) => v,
                None => {
                    without_value += 1;
                    0
                }
            };
            match str_of(t, "mode") {
                Some(m) => *per_mode.entry(m.to_owned()).or_default() += v,
                None => common += v,
            }
            if t.get("measured").and_then(toml::Value::as_bool) != Some(true) {
                unmeasured += 1;
            }
        }

        let (peak_mode, total) = if per_mode.is_empty() {
            ("—".to_owned(), common)
        } else {
            per_mode
                .iter()
                .map(|(m, v)| (m.clone(), common + v))
                .max_by_key(|(_, v)| *v)
                .unwrap_or(("—".to_owned(), common))
        };

        let ratio = if limit > 0 { total * 100 / limit } else { 0 };
        rep.note(format!(
            "{id}: 山 {total}{unit}（{peak_mode}） / 上限 {limit}{unit}（{ratio}%）工程 {steps} 件 / 実測済みでない {unmeasured} / 数値未設定 {without_value}"
        ));
        for (m, v) in &per_mode {
            rep.note(format!(
                "    {m}: {}{unit}（共通 {common}{unit} を含む）",
                common + v
            ));
        }
        if total > limit {
            rep.error(format!(
                "{id}: {peak_mode} の合計 {total}{unit} が上限 {limit}{unit} を超えている（{}{unit} 超過）",
                total - limit
            ));
        }
    }
    rep.finish("check-budgets")
}
