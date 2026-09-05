//! KOERU の仕様ゲート。
//!
//! FSL は形式的な契約の正本で、Decision / Question / Evidence / Budget / Profile は扱わない。
//! このツールはその外側だけを担当し、meta が FSL と技術要件の ID へ実際に繋がっているかを確かめる。
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

/// meta のファイル形式。ファイル自身が `schema` で名乗る。
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
    /// 項目にも ID を持たせる。 引けないものは参照できず、参照できないものは検査できない。
    collection: Option<(&'static str, &'static str, &'static [&'static str])>,
    /// entity が持ってよい表の配列。ここに無い `[[key]]` は打ち間違いとして弾く。
    /// **collection と同じ穴が entity 側にも空いていた。** 一部だけ綴りを間違えると
    /// 配列は空にならず、その分だけ黙って減る。
    entity_arrays: &'static [&'static str],
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
        Some("check-coverage") => check_coverage(&entries, rep),
        Some("check-references") => check_references(&root, &entries, rep),
        Some("check-profile") => match args.get(1) {
            Some(id) => check_profile(&entries, id, rep),
            None => {
                eprintln!("使い方: cargo xtask check-profile <PROFILE-ID>");
                ExitCode::FAILURE
            }
        },
        Some("index-decisions") => index_decisions(&root, &entries, rep),
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
                "使い方: cargo xtask <check-meta|check-budgets|check-coverage\n  check-references|check-profile <ID>\n  index-decisions|dump-requirements>"
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
    // entity 側の表配列にも、collection と同じ網を掛ける。
    // `[[allocations]]` を `[[allocatoins]]` と書いても、以前は黙って0件になっていた。
    if e.shape.entity.is_some() {
        for (k, v) in &e.table {
            let is_table_array = v
                .as_array()
                .is_some_and(|a| !a.is_empty() && a.iter().all(toml::Value::is_table));
            if is_table_array && !e.shape.entity_arrays.contains(&k.as_str()) {
                rep.error(format!(
                    "{file}: schema `{}` の知らない `[[{k}]]` がある。許すのは {:?} だけ",
                    e.shape.schema, e.shape.entity_arrays
                ));
            }
        }
        for want in e.shape.entity_arrays {
            let present = e
                .table
                .get(*want)
                .and_then(toml::Value::as_array)
                .is_some_and(|a| !a.is_empty());
            if !present {
                rep.error(format!(
                    "{file}: schema `{}` は `[[{want}]]` を1件以上持つ必要がある",
                    e.shape.schema
                ));
            }
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

/// 手書き文書とソースコメントの ID 参照が、実体に解決できるかを検査する。
///
/// `check-meta` は TOML の中の参照しか見ない。しかし ID は Markdown と
/// ソースコメントにも書かれていて、そちらは誰も検査していなかった。
/// 参照が 1,600 件を超えた時点で、宙に浮いた ID が3件できていた。
///
/// 手書き文書には ID で参照させる、という規律（`AGENTS.md` の禁止事項6）は、
/// 参照が生きていることを機械が確かめないと成立しない。
fn check_references(root: &Path, entries: &[Entry], mut rep: Report) -> ExitCode {
    let mut known = fsl_ids(root);
    // ID ごとの原文。`ID の「…」` と書かれた引用を突き合わせるために持つ。
    let mut source: BTreeMap<String, String> = BTreeMap::new();
    for e in entries {
        if let Some(id) = str_of(&e.table, "id") {
            known.insert(id.to_owned());
            source.insert(id.to_owned(), flatten(&e.table));
        }
        for item in e.items() {
            if let Some(id) = str_of(&item, "id") {
                known.insert(id.to_owned());
                source.insert(id.to_owned(), flatten(&item));
            }
        }
    }

    let mut refs: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut total = 0usize;
    let mut stale: Vec<String> = Vec::new();
    let mut quoted = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                // 生成物と調達物は対象外。ここに ID の正本は無い。
                if matches!(
                    name.as_ref(),
                    ".git" | "target" | "node_modules" | "vendor" | "models" | "dist" | "generated"
                ) {
                    continue;
                }
                stack.push(p);
                continue;
            }
            if !p
                .extension()
                .is_some_and(|x| matches!(x.to_string_lossy().as_ref(), "md" | "rs" | "ts" | "tsx"))
            {
                continue;
            }
            if name.ends_with(".gen.ts") {
                continue;
            }
            // symlink は辿らない。`CLAUDE.md` は `AGENTS.md` を指しているので、
            // 辿ると同じ行を二度報告することになる。
            if fs::symlink_metadata(&p).is_ok_and(|m| m.file_type().is_symlink()) {
                continue;
            }
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            for (n, line) in text.lines().enumerate() {
                for id in id_tokens(line) {
                    total += 1;
                    if !known.contains(&id) {
                        refs.entry(id).or_default().push(format!("{rel}:{}", n + 1));
                    }
                }
                for (id, quote) in citations(line) {
                    quoted += 1;
                    let Some(src) = source.get(&id) else { continue };
                    if !squash(src).contains(&squash(&quote)) {
                        stale.push(format!(
                            "{rel}:{}: {id} に「{quote}」という文字列が無い",
                            n + 1
                        ));
                    }
                }
            }
        }
    }

    for (id, at) in &refs {
        rep.error(format!("{id} の実体が無い（{}）", at.join(", ")));
    }
    for at in &stale {
        rep.error(at.clone());
    }
    rep.note(format!("ID 参照 {total} 件、実体 {} 種", known.len()));
    rep.note(format!("引用 {quoted} 件"));
    rep.finish("check-references")
}

/// 判断記録の索引を作る。
///
/// 手で書くと、記録を足したときに片方だけが古くなる（禁止事項6）。
/// `--check` を付けると書かずに突き合わせるだけ——CI はこちらを使う。
fn index_decisions(root: &Path, entries: &[Entry], mut rep: Report) -> ExitCode {
    let mut rows: Vec<(String, String, String, String)> = Vec::new();
    for e in with_schema(entries, "decision") {
        let g = |k: &str| str_of(&e.table, k).unwrap_or_default().to_owned();
        let id = g("id");
        if id.is_empty() {
            continue;
        }
        rows.push((id, g("constraint_label"), g("title"), g("status")));
    }
    rows.sort();

    let mut out = String::from(
        "# 判断記録の索引\n\n         `schema = 'decision'` のファイルの一覧。この索引は手で書かない。\n         `cargo xtask index-decisions` が `meta/decisions/*.toml` から作る。\n         中身を直すのは各 TOML 側で、索引は作り直す。\n\n         読み方と規律は [../README.md](../README.md)。置き換えの関係（`supersedes` /\n         `superseded_by` / `status = 'superseded'`）は `cargo xtask check-meta` が双方向で検査する。\n\n         | ID | 何についての判断か | 決めたこと | 状態 |\n         |---|---|---|---|\n",
    );
    for (id, label, title, status) in &rows {
        out.push_str(&format!(
            "| [{id}]({id}.toml) | {label} | {title} | {status} |\n"
        ));
    }
    out.push_str(&format!("\n{} 件。\n", rows.len()));

    let dest = root.join("meta/decisions/README.md");
    let current = fs::read_to_string(&dest).unwrap_or_default();
    if std::env::args().any(|a| a == "--check") {
        if current != out {
            rep.error(
                "meta/decisions/README.md が古い。`cargo xtask index-decisions` で作り直す"
                    .to_owned(),
            );
        }
        rep.note(format!("判断記録 {} 件", rows.len()));
        return rep.finish("index-decisions");
    }
    if let Err(e) = fs::write(&dest, &out) {
        rep.error(format!("索引を書けない: {e}"));
    }
    rep.note(format!("判断記録 {} 件を索引にした", rows.len()));
    rep.finish("index-decisions")
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

/// 表の中の文字列を全部つなぐ。引用がどのフィールドに書かれていても拾えるように。
fn flatten(t: &toml::Table) -> String {
    fn walk(v: &toml::Value, out: &mut String) {
        match v {
            toml::Value::String(s) => {
                out.push_str(s);
                out.push('\n');
            }
            toml::Value::Array(a) => a.iter().for_each(|v| walk(v, out)),
            toml::Value::Table(t) => t.values().for_each(|v| walk(v, out)),
            _ => {}
        }
    }
    let mut out = String::new();
    t.values().for_each(|v| walk(v, &mut out));
    out
}

/// 引用として突き合わせる形は `ID の「…」` だけ。
///
/// 日本語の「」は引用にも強調にも使う。 どちらも検査すると、例示のつもりの
/// 「オフセットだけ直した」まで「原文に無い」と言われる。 そこで
/// 「ID の」を前に置いたときだけ逐語引用とみなす、と決めてある。
/// 逐語で引けないものは `（`TR-SYN-01`。…）` の形で言い換える。
fn citations(line: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for (id, _, end) in id_spans(line) {
        // ID の直後の `` ` `` と空白を飛ばして、「の「」が続くかを見る。
        let rest = line[end..].trim_start_matches(['`', ' ', '\u{3000}']);
        let Some(rest) = rest.strip_prefix("の") else {
            continue;
        };
        let rest = rest.trim_start();
        let Some(rest) = rest.strip_prefix('「') else {
            continue;
        };
        let Some(close) = rest.find('」') else {
            continue;
        };
        out.push((id, rest[..close].to_owned()));
    }
    out
}

/// 引用の突き合わせ用に、空白と約物を落とす。
///
/// 原文は改行やカギ括弧を挟んで書かれていることがあり、
/// そのまま比べると「一字違う」だけで落ちる。 意味を変えない字だけ落とす。
fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| {
            !c.is_whitespace()
                && !matches!(
                    c,
                    '、' | '。' | '，' | '．' | ',' | '.' | '「' | '」' | '`' | '\'' | '"'
                )
        })
        .collect()
}

/// 行から `DEC-REC-007` の形の ID を拾う。
///
/// 前後が英数字・ハイフンでない位置だけを採る。`REQ-REC-005` のような
/// 別の名前空間も同じ形なので、解決先は呼び側が持つ集合が決める。
fn id_tokens(line: &str) -> Vec<String> {
    id_spans(line).into_iter().map(|(id, _, _)| id).collect()
}

/// [`id_tokens`] と同じものを、行内の位置つきで返す。
///
/// 位置が要るのは引用の検査だけ。`ID の「…」` の形かどうかは、
/// ID がどこで終わるかを知らないと判定できない。
fn id_spans(line: &str) -> Vec<(String, usize, usize)> {
    const PREFIX: &[&str] = &[
        "DEC", "TR", "Q", "EVID", "REQ", "PROFILE", "BUDGET", "SCALE", "INV", "CMP",
    ];
    let b = line.as_bytes();
    let mut out = Vec::new();
    for (i, _) in line.char_indices() {
        if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'-') {
            continue;
        }
        let rest = &line[i..];
        let Some(p) = PREFIX
            .iter()
            .find(|p| rest.starts_with(**p) && rest.as_bytes().get(p.len()) == Some(&b'-'))
        else {
            continue;
        };
        // <PREFIX>-<英大文字>-<数字>
        let after = &rest[p.len() + 1..];
        let area: String = after.chars().take_while(char::is_ascii_uppercase).collect();
        if area.is_empty() {
            continue;
        }
        let tail = &after[area.len()..];
        if !tail.starts_with('-') {
            continue;
        }
        let num: String = tail[1..].chars().take_while(char::is_ascii_digit).collect();
        if num.is_empty() {
            continue;
        }
        let end = i + p.len() + 1 + area.len() + 1 + num.len();
        if b.get(end).is_some_and(|c| c.is_ascii_alphanumeric()) {
            continue;
        }
        out.push((format!("{p}-{area}-{num}"), i, end));
    }
    out
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

/// どの部品にも支えられていない要件を数える。
///
/// 通常の CI では走らせない。実装前は埋まっていないのが正常で、
/// 埋まらないまま実装に入るのが異常だという線引きにしている。
fn check_coverage(entries: &[Entry], mut rep: Report) -> ExitCode {
    let (reqs, _) = requirements(entries);
    let mut supported: BTreeSet<String> = BTreeSet::new();
    let mut with_link = 0usize;
    let mut components = 0usize;
    for e in with_schema(entries, "component-ledger") {
        for t in e.items() {
            // 採らないと決めた部品は支えない。
            // `参照のみ` は数える。 「自前で書くが、仕様の出どころはここ」も答えのうち。
            if matches!(str_of(&t, "status"), Some("不適" | "候補外")) {
                continue;
            }
            components += 1;
            let s = list_of(&t, "supports_requirements");
            if !s.is_empty() {
                with_link += 1;
            }
            supported.extend(s);
        }
    }
    // 外部部品が要らない要件がある。 導出規約や表示の決まりは、書けば済む。
    // `needs_component = false` を宣言したものは数えない。
    let mut self_contained = 0usize;
    let mut uncovered: Vec<&str> = reqs
        .iter()
        .filter(|(_, t)| {
            let needs = t
                .get("needs_component")
                .and_then(toml::Value::as_bool)
                .unwrap_or(true);
            if !needs {
                self_contained += 1;
            }
            needs
        })
        .map(|(id, _)| id.as_str())
        .filter(|id| !supported.contains(*id))
        .collect();
    uncovered.sort_unstable();
    rep.note(format!(
        "採る見込みの部品 {components} 件 / うち要件を指しているもの {with_link} 件"
    ));
    rep.note(format!(
        "要件 {} 件 / 外部部品が要らないと宣言 {} 件 / 支える部品がある {} 件 / 無い {} 件",
        reqs.len(),
        self_contained,
        supported.len(),
        uncovered.len()
    ));
    for id in &uncovered {
        rep.error(format!("{id} を支える部品が1つも無い"));
    }
    // 両方を宣言しているのは、どちらかが間違っている。
    // 部品に支えられているなら「外部部品が要らない」は成り立たない。
    for (id, t) in &reqs {
        let free = t
            .get("needs_component")
            .and_then(toml::Value::as_bool)
            .is_some_and(|b| !b);
        if free && supported.contains(id) {
            rep.error(format!(
                "{id} は needs_component = false なのに、支える部品が宣言されている"
            ));
        }
    }
    rep.finish("check-coverage")
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
