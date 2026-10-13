//! FSL の原文から、宣言されている ID を拾う。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::repo::{RepoView, SPEC_DIR};

/// FSL 仕様が所有している要求 ID を集める。
/// `fslc` に依存せず原文から拾う。ID 規約そのものの検査は `fslc lint --project` が担当する。
pub(crate) fn fsl_ids(root: &Path) -> BTreeSet<String> {
    fsl_sites(root).into_keys().collect()
}

/// FSL 仕様が所有している要求 ID を、書かれている場所とともに集める。 作業ツリーだけを
/// 読む、[`fsl_sites_view`] への薄い包み。
pub(crate) fn fsl_sites(root: &Path) -> BTreeMap<String, String> {
    fsl_sites_view(&RepoView::working_tree(root))
}

/// `view` の版から、FSL 仕様が所有している要求 ID を、書かれている場所とともに集める。
///
/// 本文は出さない。 条文の正本は FSL で、`fslc` を通さずに読むと
/// 「書かれているもの」と「検証されているもの」がずれる。場所だけ指す。
pub(crate) fn fsl_sites_view(view: &RepoView) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let mut rels: Vec<String> = view
        .walk(SPEC_DIR)
        .into_iter()
        .filter(|rel| rel.ends_with(".fsl"))
        .collect();
    rels.sort();
    // 版から読むと、1件ずつでは `git` をファイルの数だけ起動する。 まとめて読む。
    for (rel, text) in rels.iter().zip(view.read_many(&rels)) {
        let Ok(Some(text)) = text else {
            continue;
        };
        for (n, line) in text.lines().enumerate() {
            let mut ids = BTreeSet::new();
            collect_ids(line, &mut ids);
            for id in ids {
                out.entry(id).or_insert_with(|| format!("{rel}:{}", n + 1));
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::path::Path;

    use super::*;

    /// `INV-*` は `@requirement("INV-…")` で宣言され、拾われる。 `invariant 名前 { … }` の名前は
    /// ID ではない（宣言の ID は注釈が持つ）ので、注釈の無い invariant は拾わない。
    #[test]
    fn invariant_の_id_は_requirement_の注釈から拾う() {
        let fsl = "model Fix {
  @requirement(\"INV-fix-001\", \"不変条件\")
  invariant NamedByIdentifier {
    true
  }

  invariant NoAnnotation {
    true
  }

  @requirement(\"INV-6CQ3NA\", \"目標の形\")
  trans KeepsThings {
    true
  }
}
";
        let view = RepoView::in_memory(
            Path::new("/fixture"),
            BTreeMap::from([("specs/fix.fsl".to_owned(), fsl.to_owned())]),
        );
        let sites = fsl_sites_view(&view);
        assert_eq!(
            sites.into_iter().collect::<Vec<_>>(),
            [
                ("INV-6CQ3NA".to_owned(), "specs/fix.fsl:11".to_owned()),
                ("INV-fix-001".to_owned(), "specs/fix.fsl:2".to_owned()),
            ]
        );
    }

    /// 実在する FSL の `INV-*` が拾われている。 README が挙げている不変条件は、すべて FSL の
    /// 注釈にある。
    #[test]
    fn 実在する_fsl_の_invariant_を拾えている() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("リポジトリの根");
        let ids = fsl_ids(root);
        for id in ["INV-PKG-001", "INV-PKG-002", "INV-REC-005", "INV-REC-101"] {
            assert!(ids.contains(id), "{id} を拾えていない");
        }
        assert!(ids.iter().filter(|id| id.starts_with("INV-")).count() > 1);
    }
}
