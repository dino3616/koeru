//! `cargo xtask check-references`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use super::CheckContext;
use crate::diagnostic::{Location, Report};
use crate::knowledge::{citations, fsl_ids, id_tokens, squash, str_of};
use crate::repo::SKIPPED_DIRS;

/// ID を引きうる本文の拡張子。
///
/// `nix` が入っているのは `flake.nix` が判断記録を引くから（`DEC-PLT-033`）。
/// 入れないと、あの中の `DEC-*` と `TR-*` だけが検査されないまま残る
/// ——参照できないものは検査できない（`meta/README.md`）。 `graphql` も同じで、
/// canonical SDL の description は規則を写さずに ID で引く（`DEC-PLT-035`）。
const SCANNED_EXT: &[&str] = &["md", "rs", "ts", "tsx", "nix", "graphql"];

/// 手書き文書とソースコメントの ID 参照が、実体に解決できるかを検査する。
///
/// `check-meta` は TOML の中の参照しか見ない。しかし ID は Markdown と
/// ソースコメントにも書かれていて、そちらは誰も検査していなかった。
/// 参照が 1,600 件を超えた時点で、宙に浮いた ID が3件できていた。
///
/// 手書き文書には ID で参照させる、という規律（`AGENTS.md` の禁止事項6）は、
/// 参照が生きていることを機械が確かめないと成立しない。
pub(crate) fn check_references(ctx: &CheckContext<'_>, rep: &mut Report) {
    let root = ctx.root;
    let mut known = fsl_ids(root);
    // ID ごとの原文。`ID の「…」` と書かれた引用を突き合わせるために持つ。
    let mut source: BTreeMap<String, String> = BTreeMap::new();
    for e in ctx.entries {
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

    // 実体の無い ID が書かれている場所。 文言は「相対のパス:行」に畳むが、場所は構造で持つ。
    let mut refs: BTreeMap<String, Vec<(PathBuf, usize)>> = BTreeMap::new();
    let mut total = 0usize;
    let mut stale: Vec<(Location, String)> = Vec::new();
    let mut quoted = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            let name = e.file_name();
            let name = name.to_string_lossy();
            if p.is_dir() {
                if SKIPPED_DIRS.contains(&name.as_ref()) {
                    continue;
                }
                stack.push(p);
                continue;
            }
            if !scanned(&p) {
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
            let rel = p.strip_prefix(root).unwrap_or(&p).to_path_buf();
            for (n, line) in text.lines().enumerate() {
                for id in id_tokens(line) {
                    total += 1;
                    if !known.contains(&id) {
                        refs.entry(id).or_default().push((rel.clone(), n + 1));
                    }
                }
                for (id, quote) in citations(line) {
                    quoted += 1;
                    let Some(src) = source.get(&id) else { continue };
                    if !squash(src).contains(&squash(&quote)) {
                        stale.push((
                            Location::new(&rel, Some(n + 1)),
                            format!(
                                "{}:{}: {id} に「{quote}」という文字列が無い",
                                rel.display(),
                                n + 1
                            ),
                        ));
                    }
                }
            }
        }
    }

    for (id, at) in &refs {
        let sites: Vec<String> = at
            .iter()
            .map(|(p, n)| format!("{}:{n}", p.display()))
            .collect();
        // 場所は複数ある。 構造で持つのは最初の1つで、全部は文言にある。
        let first = at.first().map(|(p, n)| Location::new(p, Some(*n)));
        rep.error_at(
            "references.unknown-id",
            first,
            format!("{id} の実体が無い（{}）", sites.join(", ")),
        );
    }
    for (location, message) in stale {
        rep.error_at("references.stale-citation", Some(location), message);
    }
    rep.note_code(
        "references.summary",
        format!("ID 参照 {total} 件、実体 {} 種", known.len()),
    );
    rep.note_code("references.summary", format!("引用 {quoted} 件"));
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

/// ID を引きうるファイルか。ディレクトリの除外は呼び側が見る。
fn scanned(path: &Path) -> bool {
    if path
        .file_name()
        .is_some_and(|n| n.to_string_lossy().ends_with(".gen.ts"))
    {
        return false;
    }
    path.extension()
        .is_some_and(|x| SCANNED_EXT.contains(&x.to_string_lossy().as_ref()))
}
