//! FSL の原文から、宣言されている ID を拾う。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use crate::repo::SPEC_DIR;

/// FSL 仕様が所有している要求 ID を集める。
/// `fslc` に依存せず原文から拾う。ID 規約そのものの検査は `fslc lint --project` が担当する。
pub(crate) fn fsl_ids(root: &Path) -> BTreeSet<String> {
    fsl_sites(root).into_keys().collect()
}

/// FSL 仕様が所有している要求 ID を、書かれている場所とともに集める。
///
/// 本文は出さない。 条文の正本は FSL で、`fslc` を通さずに読むと
/// 「書かれているもの」と「検証されているもの」がずれる。場所だけ指す。
pub(crate) fn fsl_sites(root: &Path) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut stack = vec![root.join(SPEC_DIR)];
    while let Some(dir) = stack.pop() {
        let Ok(rd) = fs::read_dir(&dir) else { continue };
        for e in rd.filter_map(Result::ok) {
            let p = e.path();
            if p.is_dir() {
                stack.push(p);
                continue;
            }
            if !p.extension().is_some_and(|x| x == "fsl") {
                continue;
            }
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            for (n, line) in text.lines().enumerate() {
                let mut ids = BTreeSet::new();
                collect_ids(line, &mut ids);
                for id in ids {
                    out.entry(id).or_insert_with(|| format!("{rel}:{}", n + 1));
                }
            }
        }
    }
    out
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
