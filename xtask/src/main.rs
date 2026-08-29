//! KOERU の仕様ゲート。
//!
//! FSL は形式的な契約の正本で、Decision / Question / Evidence / Budget / Profile は扱わない。
//! このツールはその外側だけを担当し、**meta が FSL と技術要件の ID へ実際に繋がっているか**を確かめる。
//! 仕様コンパイラではない。FSL のグラフへ外部情報を接続するブリッジとリリースゲートである。
//!
//! - `check-meta`     meta の形式と必須項目、参照先 ID の実在を確かめる
//! - `check-budgets`  配分の合計が上限を超えていないかを確かめる
//! - `check-profile`  未決の Question が塞いでいるリリースプロファイルを落とす
//! - `dump-requirements`  要件の登録簿を区切り文字形式で書き出す（外部ツール向け）

// ここは CLI なので、結果を標準出力へ出す。tracing に寄せる対象ではない。
#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    process::ExitCode,
};

const META_DIR: &str = "meta";
const SPEC_DIR: &str = "specs";
const CONFIDENCE: &[&str] = &["Fact", "Assumption", "Unknown", "Risk"];

/// meta のファイル形式。**ファイル自身が `schema` で名乗る。**
///
/// 名乗らないファイルは落とす。ディレクトリの中身から形を推測すると、
/// 打ち間違えた収集ファイルが「0件を貢献した」ことに誰も気づけない。
#[derive(Debug, Clone, Copy)]
struct Shape {
    schema: &'static str,
    dir: &'static str,
    /// 1件1ファイル。ID の接頭辞と必須項目。
    entity: Option<(&'static str, &'static [&'static str])>,
    /// 配列で複数件。配列のキー、項目 ID の接頭辞、各項目の必須項目。
    /// **項目にも ID を持たせる。** 引けないものは参照できず、参照できないものは検査できない。
    collection: Option<(&'static str, &'static str, &'static [&'static str])>,
}

const SHAPES: &[Shape] = &[
    Shape {
        schema: "requirement-set",
        dir: "requirements",
        entity: None,
        collection: Some((
            "requirement",
            "TR-",
            &["id", "title", "confidence", "statement"],
        )),
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
    },
    Shape {
        schema: "evidence",
        dir: "evidence",
        entity: Some((
            "EVID-",
            &["id", "title", "kind", "source", "provenance", "confidence"],
        )),
        collection: None,
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
    },
    Shape {
        schema: "budget",
        dir: "budgets",
        entity: Some(("BUDGET-", &["id", "title", "limit", "unit", "scope"])),
        collection: None,
    },
    Shape {
        schema: "target-set",
        dir: "budgets",
        entity: None,
        collection: Some(("target", "TGT-", &["id", "item", "goal"])),
    },
    Shape {
        schema: "profile",
        dir: "profiles",
        entity: Some(("PROFILE-", &["id", "title", "status"])),
        collection: None,
    },
];

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    shape: &'static Shape,
    table: toml::Table,
}

impl Entry {
    /// 収集ファイルの各項目。1件1ファイルの形なら空。
    fn items(&self) -> Vec<toml::Table> {
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

#[derive(Debug, Default)]
struct Report {
    errors: Vec<String>,
    notes: Vec<String>,
}

impl Report {
    fn error(&mut self, msg: impl Into<String>) {
        self.errors.push(msg.into());
    }

    fn note(&mut self, msg: impl Into<String>) {
        self.notes.push(msg.into());
    }

    fn finish(&self, what: &str) -> ExitCode {
        for n in &self.notes {
            println!("  {n}");
        }
        if self.errors.is_empty() {
            println!("{what}: ok");
            ExitCode::SUCCESS
        } else {
            for e in &self.errors {
                println!("  NG {e}");
            }
            println!("{what}: {} 件", self.errors.len());
            ExitCode::FAILURE
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let root = match repo_root() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("リポジトリのルートが見つからない: {e}");
            return ExitCode::FAILURE;
        }
    };
    let mut rep = Report::default();
    let entries = load(&root, &mut rep);

    match args.first().map(String::as_str) {
        Some("check-meta") => check_meta(&root, &entries, rep),
        Some("check-budgets") => check_budgets(&entries, rep),
        Some("check-profile") => match args.get(1) {
            Some(id) => check_profile(&entries, id, rep),
            None => {
                eprintln!("使い方: cargo xtask check-profile <PROFILE-ID>");
                ExitCode::FAILURE
            }
        },
        Some("dump-requirements") => {
            // 移行の照合用。US(0x1f) 区切りのフィールド、RS(0x1e) 区切りのレコード。
            for (id, t) in requirements(&entries).0 {
                let get = |k: &str| {
                    t.get(k)
                        .and_then(toml::Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let mut fields = vec![id, get("title"), get("confidence")];
                fields.push(list_of(&t, "depends_on").join(","));
                fields.push(get("statement"));
                fields.extend(list_of(&t, "notes"));
                print!("{}\u{1e}", fields.join("\u{1f}"));
            }
            ExitCode::SUCCESS
        }
        _ => {
            println!(
                "使い方: cargo xtask <check-meta|check-budgets|check-profile <ID>|dump-requirements>"
            );
            ExitCode::FAILURE
        }
    }
}

fn repo_root() -> Result<PathBuf, String> {
    let mut dir = std::env::current_dir().map_err(|e| e.to_string())?;
    loop {
        if dir.join(META_DIR).is_dir() && dir.join(SPEC_DIR).is_dir() {
            return Ok(dir);
        }
        if !dir.pop() {
            return Err(format!("{META_DIR}/ と {SPEC_DIR}/ を持つ親が無い"));
        }
    }
}

/// meta を読む。**形を名乗らないファイル、名乗った形と中身が合わないファイルは、
/// 読み飛ばさずに落とす。** 黙って0件になる経路を作らないため。
fn load(root: &Path, rep: &mut Report) -> Vec<Entry> {
    let mut paths = Vec::new();
    let mut stack = vec![root.join(META_DIR)];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "toml") {
                paths.push(p);
            }
        }
    }
    paths.sort();

    let mut out = Vec::new();
    for path in paths {
        let file = path.display().to_string();
        let Ok(text) = fs::read_to_string(&path) else {
            rep.error(format!("{file}: 読めない"));
            continue;
        };
        let table = match text.parse::<toml::Table>() {
            Ok(t) => t,
            Err(e) => {
                rep.error(format!("{file}: TOML として読めない: {e}"));
                continue;
            }
        };
        let Some(schema) = table.get("schema").and_then(toml::Value::as_str) else {
            rep.error(format!(
                "{file}: `schema` が無い。ファイルは自分の形を名乗る必要がある"
            ));
            continue;
        };
        let Some(shape) = SHAPES.iter().find(|s| s.schema == schema) else {
            rep.error(format!("{file}: 知らない schema `{schema}`"));
            continue;
        };
        let dir = path
            .parent()
            .and_then(Path::file_name)
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        if dir != shape.dir {
            rep.error(format!(
                "{file}: schema `{schema}` は meta/{}/ に置く",
                shape.dir
            ));
            continue;
        }
        out.push(Entry { path, shape, table });
    }

    for e in &out {
        check_shape(e, rep);
    }
    out
}

/// 名乗った形どおりの中身になっているか。
fn check_shape(e: &Entry, rep: &mut Report) {
    let file = e.path.display().to_string();
    if let Some((prefix, required)) = e.shape.entity {
        for key in required {
            if !e.table.contains_key(*key) {
                rep.error(format!("{file}: 必須項目 `{key}` が無い"));
            }
        }
        match str_of(&e.table, "id") {
            Some(id) => {
                if !id.starts_with(prefix) {
                    rep.error(format!(
                        "{file}: id `{id}` は `{prefix}` で始まる必要がある"
                    ));
                }
                let stem = e
                    .path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default();
                if stem != id {
                    rep.error(format!("{file}: ファイル名が id `{id}` と一致しない"));
                }
            }
            None => rep.error(format!("{file}: `id` が文字列でない")),
        }
    }
    let Some((key, prefix, required)) = e.shape.collection else {
        return;
    };
    // 宣言したキー以外に表の配列があるのは、たいてい `[[component]]` を
    // `[[componnet]]` と書いたような打ち間違い。**一部だけ間違えると配列は空にならず、
    // その分だけ黙って減る。**
    for (k, v) in &e.table {
        if k == key {
            continue;
        }
        if v.as_array()
            .is_some_and(|a| !a.is_empty() && a.iter().all(toml::Value::is_table))
        {
            rep.error(format!(
                "{file}: schema `{}` の知らない `[[{k}]]` がある。`[[{key}]]` の打ち間違いではないか",
                e.shape.schema
            ));
        }
    }
    match e
        .table
        .get(key)
        .and_then(toml::Value::as_array)
        .map(Vec::as_slice)
    {
        // 0件を貢献するファイルは、たいてい打ち間違いである
        None | Some([]) => rep.error(format!(
            "{file}: schema `{}` は `[[{key}]]` を1件以上持つ必要がある",
            e.shape.schema
        )),
        Some(items) => {
            for (i, item) in items.iter().enumerate() {
                let Some(t) = item.as_table() else {
                    rep.error(format!("{file}: [[{key}]] の {i} 件目が表でない"));
                    continue;
                };
                for r in required {
                    if !t.contains_key(*r) {
                        rep.error(format!("{file}: [[{key}]] の {i} 件目に `{r}` が無い"));
                    }
                }
                if let Some(id) = t.get("id").and_then(toml::Value::as_str)
                    && !id.starts_with(prefix)
                {
                    rep.error(format!(
                        "{file}: [[{key}]] の id `{id}` は `{prefix}` で始まる必要がある"
                    ));
                }
            }
        }
    }
}

fn str_of<'a>(t: &'a toml::Table, key: &str) -> Option<&'a str> {
    t.get(key).and_then(toml::Value::as_str)
}

fn list_of(t: &toml::Table, key: &str) -> Vec<String> {
    t.get(key)
        .and_then(toml::Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

fn with_schema<'a>(entries: &'a [Entry], schema: &'static str) -> impl Iterator<Item = &'a Entry> {
    entries.iter().filter(move |e| e.shape.schema == schema)
}

/// 技術要件の登録簿。重複した ID は別に返す。
fn requirements(entries: &[Entry]) -> (BTreeMap<String, toml::Table>, Vec<String>) {
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

/// FSL 仕様が所有している要求 ID を集める。
/// `fslc` に依存せず原文から拾う。ID 規約そのものの検査は `fslc lint --project` が担当する。
fn fsl_ids(root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let mut stack = vec![root.join(SPEC_DIR)];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "fsl") {
                let Ok(text) = fs::read_to_string(&p) else {
                    continue;
                };
                collect_ids(&text, &mut ids);
            }
        }
    }
    ids
}

/// `@requirement("ID"` と、`acceptance ID` / `forbidden ID` の宣言 ID を拾う。
fn collect_ids(text: &str, out: &mut BTreeSet<String>) {
    for line in text.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("@requirement(\"")
            && let Some(end) = rest.find('"')
        {
            out.insert(rest[..end].to_owned());
        }
        for kw in ["acceptance ", "forbidden "] {
            if let Some(rest) = line.strip_prefix(kw)
                && let Some(id) = rest.split_whitespace().next()
            {
                out.insert(id.to_owned());
            }
        }
    }
}

/// 文中に現れる `TR-XXX-NN` を拾う。参照先が実在するかを見るために使う。
fn find_tr(text: &str) -> Vec<String> {
    let b: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i + 9 <= b.len() {
        if b[i] == 'T' && b[i + 1] == 'R' && b[i + 2] == '-' {
            let mut j = i + 3;
            let mut alpha = 0;
            while j < b.len() && b[j].is_ascii_uppercase() {
                j += 1;
                alpha += 1;
            }
            if alpha == 3 && j < b.len() && b[j] == '-' {
                let k = j + 1;
                let mut e = k;
                while e < b.len() && b[e].is_ascii_digit() {
                    e += 1;
                }
                if e > k {
                    out.push(b[i..e].iter().collect());
                    i = e;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

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

fn check_meta(root: &Path, entries: &[Entry], mut rep: Report) -> ExitCode {
    let fsl = fsl_ids(root);
    let tr = check_requirements(entries, &fsl, &mut rep);

    // 要件はちょうど1つのマイルストーンに属する。**どこにも属さない要件は、
    // 誰も作らないまま残る。** 二重に属すると、二度作るか、どちらもやらない。
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
        for (key, universe, label) in [
            ("affects_requirements", &tr, "技術要件"),
            ("supports_requirements", &tr, "技術要件"),
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

fn check_budgets(entries: &[Entry], mut rep: Report) -> ExitCode {
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

        let mut total = 0i64;
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
            match t.get("value").and_then(toml::Value::as_integer) {
                Some(v) => total += v,
                None => without_value += 1,
            }
            if t.get("measured").and_then(toml::Value::as_bool) != Some(true) {
                unmeasured += 1;
            }
        }

        let ratio = if limit > 0 { total * 100 / limit } else { 0 };
        rep.note(format!(
            "{id}: 配分 {total}{unit} / 上限 {limit}{unit}（{ratio}%）工程 {steps} 件 / 実測済みでない {unmeasured} / 数値未設定 {without_value}"
        ));
        if total > limit {
            rep.error(format!(
                "{id}: 配分の合計 {total}{unit} が上限 {limit}{unit} を超えている（{}{unit} 超過）",
                total - limit
            ));
        }
    }
    rep.finish("check-budgets")
}

fn check_profile(entries: &[Entry], profile_id: &str, mut rep: Report) -> ExitCode {
    let Some(profile) =
        with_schema(entries, "profile").find(|e| str_of(&e.table, "id") == Some(profile_id))
    else {
        rep.error(format!("`{profile_id}` というプロファイルが無い"));
        return rep.finish("check-profile");
    };

    let blocking: Vec<&Entry> = with_schema(entries, "question")
        .filter(|e| str_of(&e.table, "status") == Some("open"))
        .filter(|e| {
            list_of(&e.table, "blocks_profiles")
                .iter()
                .any(|p| p == profile_id)
        })
        .collect();

    rep.note(format!(
        "{profile_id}: FSL の要求 {} 件 / 決定 {} 件 / 予算 {} 件",
        list_of(&profile.table, "includes_fsl").len(),
        list_of(&profile.table, "decisions").len(),
        list_of(&profile.table, "budgets").len()
    ));

    for q in &blocking {
        let id = str_of(&q.table, "id").unwrap_or("?");
        let title = str_of(&q.table, "title").unwrap_or("");
        rep.error(format!(
            "{id} が未決のまま {profile_id} を塞いでいる: {title}"
        ));
    }
    rep.finish("check-profile")
}
