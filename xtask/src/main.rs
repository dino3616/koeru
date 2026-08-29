//! KOERU の仕様ゲート。
//!
//! FSL は形式的な契約の正本で、Decision / Question / Evidence / Budget / Profile は扱わない。
//! このツールはその外側だけを担当し、**meta が FSL と技術要件の ID へ実際に繋がっているか**を確かめる。
//! 仕様コンパイラではない。FSL のグラフへ外部情報を接続するブリッジとリリースゲートである。
//!
//! - `check-meta`     meta の必須項目と、参照先 ID の実在を確かめる
//! - `check-budgets`  配分の合計が上限を超えていないかを確かめる
//! - `check-profile`  未決の Question が塞いでいるリリースプロファイルを落とす

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
const REQUIREMENTS_DOC: &str = "docs/tech-requirements.md";

/// meta の種類。ディレクトリ名と ID の接頭辞、必須項目を持つ。
#[derive(Debug, Clone, Copy)]
struct Kind {
    dir: &'static str,
    prefix: &'static str,
    required: &'static [&'static str],
}

const KINDS: &[Kind] = &[
    Kind {
        dir: "decisions",
        prefix: "DEC-",
        required: &[
            "id",
            "title",
            "status",
            "owner",
            "options",
            "selected",
            "rationale",
            "review_triggers",
        ],
    },
    Kind {
        dir: "questions",
        prefix: "Q-",
        required: &[
            "id",
            "title",
            "status",
            "owner",
            "why_it_matters",
            "how_to_close",
        ],
    },
    Kind {
        dir: "evidence",
        prefix: "EVID-",
        required: &["id", "title", "kind", "source", "provenance", "confidence"],
    },
    Kind {
        dir: "budgets",
        prefix: "BUDGET-",
        required: &["id", "title", "limit", "unit", "scope"],
    },
    Kind {
        dir: "profiles",
        prefix: "PROFILE-",
        required: &["id", "title", "status"],
    },
];

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    kind: &'static str,
    table: toml::Table,
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
    let entries = match load(&root) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("meta の読み込みに失敗: {e}");
            return ExitCode::FAILURE;
        }
    };

    match args.first().map(String::as_str) {
        Some("check-meta") => check_meta(&root, &entries),
        Some("check-budgets") => check_budgets(&entries),
        Some("check-profile") => match args.get(1) {
            Some(id) => check_profile(&entries, id),
            None => {
                eprintln!("使い方: cargo xtask check-profile <PROFILE-ID>");
                ExitCode::FAILURE
            }
        },
        _ => {
            println!("使い方: cargo xtask <check-meta|check-budgets|check-profile <ID>>");
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

fn load(root: &Path) -> Result<Vec<Entry>, String> {
    let mut out = Vec::new();
    for kind in KINDS {
        let dir = root.join(META_DIR).join(kind.dir);
        if !dir.is_dir() {
            continue;
        }
        let mut paths: Vec<PathBuf> = fs::read_dir(&dir)
            .map_err(|e| format!("{}: {e}", dir.display()))?
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "toml"))
            .collect();
        paths.sort();
        for path in paths {
            let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
            let table = text
                .parse::<toml::Table>()
                .map_err(|e| format!("{}: {e}", path.display()))?;
            out.push(Entry {
                path,
                kind: kind.dir,
                table,
            });
        }
    }
    Ok(out)
}

fn str_of<'a>(t: &'a toml::Table, key: &str) -> Option<&'a str> {
    t.get(key).and_then(toml::Value::as_str)
}

/// 文字列配列を読む。項目が文字列でない場合は空として扱わず、呼び出し側で気づけるよう空を返す。
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

/// 技術要件が所有している TR-* を集める。見出し行が正本。
fn tr_ids(root: &Path) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    let Ok(text) = fs::read_to_string(root.join(REQUIREMENTS_DOC)) else {
        return ids;
    };
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("#### ")
            && let Some(id) = rest.split_whitespace().next()
            && id.starts_with("TR-")
        {
            ids.insert(id.to_owned());
        }
    }
    ids
}

fn check_meta(root: &Path, entries: &[Entry]) -> ExitCode {
    let mut rep = Report::default();
    let fsl = fsl_ids(root);
    let tr = tr_ids(root);
    rep.note(format!(
        "FSL の要求 ID {} 件 / 技術要件 {} 件を読んだ",
        fsl.len(),
        tr.len()
    ));

    let mut ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for e in entries {
        let Some(kind) = KINDS.iter().find(|k| k.dir == e.kind) else {
            continue;
        };
        let file = e.path.display().to_string();

        for key in kind.required {
            if !e.table.contains_key(*key) {
                rep.error(format!("{file}: 必須項目 `{key}` が無い"));
            }
        }

        let Some(id) = str_of(&e.table, "id") else {
            continue;
        };
        if !id.starts_with(kind.prefix) {
            rep.error(format!(
                "{file}: id `{id}` は `{}` で始まる必要がある",
                kind.prefix
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
        if let Some(prev) = ids.insert(id.to_owned(), e.path.clone()) {
            rep.error(format!(
                "{file}: id `{id}` が {} と重複している",
                prev.display()
            ));
        }
    }

    // 参照先が実在するかを確かめる。ここが「決定と要件が結ばれていない」の再発を止める箇所。
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
        // undecided_in が指すファイルに、実際に未決の印があるか
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

    let counts = KINDS
        .iter()
        .map(|k| {
            format!(
                "{} {}",
                k.dir,
                entries.iter().filter(|e| e.kind == k.dir).count()
            )
        })
        .collect::<Vec<_>>()
        .join(" / ");
    rep.note(counts);
    rep.finish("check-meta")
}

fn check_budgets(entries: &[Entry]) -> ExitCode {
    let mut rep = Report::default();
    for e in entries.iter().filter(|e| e.kind == "budgets") {
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
        for a in allocations {
            let Some(t) = a.as_table() else { continue };
            total += t
                .get("value")
                .and_then(toml::Value::as_integer)
                .unwrap_or(0);
            if t.get("measured").and_then(toml::Value::as_bool) != Some(true) {
                unmeasured += 1;
            }
        }

        let ratio = if limit > 0 { total * 100 / limit } else { 0 };
        rep.note(format!(
            "{id}: 配分 {total}{unit} / 上限 {limit}{unit}（{ratio}%）実測済みでない配分 {unmeasured}/{} 件",
            allocations.len()
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

fn check_profile(entries: &[Entry], profile_id: &str) -> ExitCode {
    let mut rep = Report::default();
    let Some(profile) = entries
        .iter()
        .filter(|e| e.kind == "profiles")
        .find(|e| str_of(&e.table, "id") == Some(profile_id))
    else {
        rep.error(format!("`{profile_id}` というプロファイルが無い"));
        return rep.finish("check-profile");
    };

    let blocking: Vec<&Entry> = entries
        .iter()
        .filter(|e| e.kind == "questions")
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
