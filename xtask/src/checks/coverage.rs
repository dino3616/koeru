//! `cargo xtask check-coverage`

use std::collections::BTreeSet;
use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, list_of, requirements, str_of, with_schema};

/// どの部品にも支えられていない要件を数える。
///
/// 通常の CI では走らせない。実装前は埋まっていないのが正常で、
/// 埋まらないまま実装に入るのが異常だという線引きにしている。
pub(crate) fn check_coverage(entries: &[Entry], mut rep: Report) -> ExitCode {
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
