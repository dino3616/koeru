//! meta を読む。

use std::fs;
use std::path::Path;

use super::model::{Entry, SHAPES, str_of};
use crate::diagnostic::Report;
use crate::repo::META_DIR;

/// meta を読む。**形を名乗らないファイル、名乗った形と中身が合わないファイルは、
/// 読み飛ばさずに落とす。** 黙って0件になる経路を作らないため。
pub(crate) fn load(root: &Path, rep: &mut Report) -> Vec<Entry> {
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
