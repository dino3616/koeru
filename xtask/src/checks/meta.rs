//! `cargo xtask check-meta`

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, find_tr, fsl_ids, list_of, requirements, str_of, with_schema};

const CONFIDENCE: &[&str] = &["Fact", "Assumption", "Unknown", "Risk"];

/// 技術要件の登録簿そのものを検査する。ここが「本文の無い ID を参照する」の再発を止める箇所。
fn check_requirements(
    entries: &[Entry],
    fsl: &BTreeSet<String>,
    rep: &mut Report,
) -> BTreeSet<String> {
    let (reqs, dups) = requirements(entries);
    for d in dups {
        rep.error(d);
    }
    let ids: BTreeSet<String> = reqs.keys().cloned().collect();
    let mut dangling = BTreeSet::new();
    for (id, r) in &reqs {
        match r.get("confidence").and_then(toml::Value::as_str) {
            Some(c) if CONFIDENCE.contains(&c) => {}
            Some(c) => rep.error(format!(
                "{id}: 確度 `{c}` は Fact / Assumption / Unknown のいずれでもない"
            )),
            None => {}
        }
        if r.get("statement")
            .and_then(toml::Value::as_str)
            .is_none_or(str::is_empty)
        {
            rep.error(format!("{id}: 本文が空"));
        }
        for d in list_of(r, "depends_on") {
            if !ids.contains(&d) {
                rep.error(format!("{id}: depends_on の `{d}` は登録簿に存在しない"));
            }
        }
        for f in list_of(r, "formalized_as") {
            if !fsl.contains(&f) {
                rep.error(format!("{id}: formalized_as の `{f}` は FSL に存在しない"));
            }
        }
        let mut text = r
            .get("statement")
            .and_then(toml::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        for n in list_of(r, "notes") {
            text.push('\n');
            text.push_str(&n);
        }
        for referenced in find_tr(&text) {
            if !ids.contains(&referenced) {
                dangling.insert(format!(
                    "{id} が参照する `{referenced}` は登録簿に存在しない"
                ));
            }
        }
    }
    for d in dangling {
        rep.error(d);
    }
    ids
}

/// 置き換えの関係が両側から見えているかを検査する。
///
/// `supersedes` は片側にしか書かれない。 置き換えられた側を開いた人には、
/// `status = 'accepted'` としか見えず、もう使われていないことが分からない。
/// 片側だけ直すと必ずそうなるので、両方が揃っていることをここで固定する。
fn check_supersession(entries: &[Entry], rep: &mut Report) {
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    let mut by: BTreeMap<String, String> = BTreeMap::new();
    let mut supersedes: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for e in with_schema(entries, "decision") {
        let Some(id) = str_of(&e.table, "id") else {
            continue;
        };
        if let Some(st) = str_of(&e.table, "status") {
            status.insert(id.to_owned(), st.to_owned());
        }
        if let Some(b) = str_of(&e.table, "superseded_by") {
            by.insert(id.to_owned(), b.to_owned());
        }
        let old = list_of(&e.table, "supersedes");
        if !old.is_empty() {
            supersedes.insert(id.to_owned(), old);
        }
    }

    // 名指された側は superseded で、名指した相手を指し返している。
    for (newer, olds) in &supersedes {
        for old in olds {
            match status.get(old) {
                None => rep.error(format!("{newer} の supersedes が指す {old} が無い")),
                Some(st) if st != "superseded" => rep.error(format!(
                    "{newer} が {old} を置き換えているのに、{old} の status が `{st}` のまま"
                )),
                Some(_) => {}
            }
            match by.get(old) {
                Some(b) if b == newer => {}
                Some(b) => rep.error(format!(
                    "{old} の superseded_by が `{b}` だが、置き換えているのは {newer}"
                )),
                None => rep.error(format!("{old} に superseded_by = '{newer}' が無い")),
            }
        }
    }

    // 逆向き。superseded を名乗るなら、置き換えた相手がそう言っている。
    for (old, st) in &status {
        if st != "superseded" {
            continue;
        }
        let Some(newer) = by.get(old) else {
            rep.error(format!("{old} は superseded だが superseded_by が無い"));
            continue;
        };
        if !supersedes.get(newer).is_some_and(|v| v.contains(old)) {
            rep.error(format!(
                "{old} は {newer} に置き換えられたと言うが、{newer} の supersedes に無い"
            ));
        }
    }
}

pub(crate) fn check_meta(root: &Path, entries: &[Entry], mut rep: Report) -> ExitCode {
    let fsl = fsl_ids(root);
    let tr = check_requirements(entries, &fsl, &mut rep);
    check_supersession(entries, &mut rep);

    // 要件はちょうど1つのマイルストーンに属する。どこにも属さない要件は、
    // 誰も作らないまま残る。 二重に属すると、二度作るか、どちらもやらない。
    let profiles: Vec<&Entry> = with_schema(entries, "profile").collect();
    if !profiles.is_empty() {
        let mut owner: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for e in &profiles {
            let id = str_of(&e.table, "id").unwrap_or("?").to_owned();
            for r in list_of(&e.table, "includes_requirements") {
                owner.entry(r).or_default().push(id.clone());
            }
        }
        for (r, ms) in &owner {
            if ms.len() > 1 {
                rep.error(format!(
                    "{r} が複数のマイルストーンに属している: {}",
                    ms.join(", ")
                ));
            }
        }
        let orphans: Vec<&String> = tr.iter().filter(|r| !owner.contains_key(*r)).collect();
        if !orphans.is_empty() {
            let head: Vec<String> = orphans.iter().take(5).map(|s| (*s).clone()).collect();
            rep.error(format!(
                "どのマイルストーンにも属さない要件が {} 件ある: {} ...",
                orphans.len(),
                head.join(", ")
            ));
        }
    }

    let decisions: BTreeSet<String> = with_schema(entries, "decision")
        .filter_map(|e| str_of(&e.table, "id").map(str::to_owned))
        .collect();
    let mut components = 0usize;
    for e in with_schema(entries, "component-ledger") {
        for t in e.items() {
            components += 1;
            // 採否は判断記録が持つ。台帳が指す先が実在すること。
            if let Some(d) = str_of(&t, "decided_by")
                && !decisions.contains(d)
            {
                rep.error(format!(
                    "{}: decided_by の `{d}` という判断記録は存在しない",
                    e.path.display()
                ));
            }
            // 決め切った採否には、理由と撤回条件が要る。 それを持つのは判断記録だけ。
            // 「採用候補」「条件付き」「要調査」はまだ決めていないので対象外。
            if matches!(str_of(&t, "status"), Some("採用" | "不適"))
                && str_of(&t, "decided_by").is_none()
            {
                let id = str_of(&t, "id").unwrap_or("?");
                rep.error(format!(
                    "{}: `{id}` は採否を決めているのに decided_by が無い。\
                     理由と撤回条件を持つ判断記録へ繋ぐこと",
                    e.path.display()
                ));
            }
        }
    }
    let targets: usize = with_schema(entries, "target-set")
        .map(|e| e.items().len())
        .sum();

    // 参照できる ID。1件1ファイルのものと、収集ファイルの項目の両方。
    let mut ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for e in entries {
        let mut seen = Vec::new();
        if e.shape.entity.is_some() {
            seen.extend(str_of(&e.table, "id").map(str::to_owned));
        }
        for t in e.items() {
            seen.extend(str_of(&t, "id").map(str::to_owned));
        }
        for id in seen {
            if let Some(prev) = ids.insert(id.clone(), e.path.clone()) {
                rep.error(format!(
                    "{}: id `{id}` が {} と重複している",
                    e.path.display(),
                    prev.display()
                ));
            }
        }
    }
    for e in entries {
        let file = e.path.display().to_string();
        // 同じ ID が同じ一覧に2度出るのは、たいてい編集の取りこぼし。
        // 害は薄いが、書き換えを間違えた合図としては確かなので落とす。
        for (key, _) in &e.table {
            let items = list_of(&e.table, key);
            let mut seen = BTreeSet::new();
            for r in &items {
                if !seen.insert(r.clone()) {
                    rep.error(format!("{file}: {key} に `{r}` が2度出ている"));
                }
            }
        }
        for (key, universe, label) in [
            ("affects_requirements", &tr, "技術要件"),
            ("supports_requirements", &tr, "技術要件"),
            ("source_requirements", &tr, "技術要件"),
            ("derives_from_requirements", &tr, "技術要件"),
            ("affects_fsl", &fsl, "FSL の要求"),
            ("includes_fsl", &fsl, "FSL の要求"),
        ] {
            for r in list_of(&e.table, key) {
                if !universe.contains(&r) {
                    rep.error(format!("{file}: {key} の `{r}` は{label}に存在しない"));
                }
            }
        }
        for key in [
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
        ] {
            for r in list_of(&e.table, key) {
                if !ids.contains_key(&r) {
                    rep.error(format!("{file}: {key} の `{r}` という meta は存在しない"));
                }
            }
        }
        for rel in list_of(&e.table, "undecided_in") {
            let p = root.join(&rel);
            match fs::read_to_string(&p) {
                // 正式表記は `@undecided(...)`。文字列形式は非推奨だが移行期のため両方見る。
                Ok(text) if text.contains("@undecided(") || text.contains("undecided:") => {}
                Ok(_) => rep.error(format!("{file}: {rel} に未決の印が無い")),
                Err(_) => rep.error(format!("{file}: {rel} が読めない")),
            }
        }
    }

    // suite が支える契約（`DEC-PLT-039`）。 技術要件・FSL・meta のどれかに実在すること。
    // 収集ファイルの項目の中の欄なので、上の走査（表の直下だけを見る）には入らない。
    for e in with_schema(entries, "test-portfolio") {
        for t in e.items() {
            let id = str_of(&t, "id").unwrap_or("?");
            for c in list_of(&t, "contracts") {
                if !(tr.contains(&c) || fsl.contains(&c) || ids.contains_key(&c)) {
                    rep.error(format!(
                        "{}: {id} の contracts の `{c}` は存在しない",
                        e.path.display()
                    ));
                }
            }
        }
    }

    rep.note(format!(
        "FSL の要求 {} 件 / 技術要件 {} 件 / 部品台帳 {components} 件 / 性能目標 {targets} 件",
        fsl.len(),
        tr.len()
    ));
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in entries {
        *counts.entry(e.shape.schema).or_default() += 1;
    }
    rep.note(
        counts
            .iter()
            .map(|(k, v)| format!("{k} {v}"))
            .collect::<Vec<_>>()
            .join(" / "),
    );
    rep.finish("check-meta")
}
