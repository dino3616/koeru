//! `cargo xtask index-decisions [--check]`

use std::fs;
use std::path::Path;
use std::process::ExitCode;

use crate::diagnostic::Report;
use crate::knowledge::{Entry, str_of, with_schema};

/// 判断記録の索引。`index-decisions` が書く。
///
/// 中身は全判断の一覧なので、1件足すと差分がそこら中の ID を引く。
/// 人が書いた引用ではないので、変更が触れた契約を数えるときは外す。
pub(crate) const DECISION_INDEX: &str = "meta/decisions/README.md";

/// 判断記録の索引を作る。
///
/// 手で書くと、記録を足したときに片方だけが古くなる（禁止事項6）。
/// `check` なら書かずに突き合わせるだけ——CI はこちらを使う。
pub(crate) fn index_decisions(
    root: &Path,
    entries: &[Entry],
    check: bool,
    mut rep: Report,
) -> ExitCode {
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

    // 行継続（`\` 改行）で書かない。 続けた行の字下げがそのまま文字列に入り、
    // Markdown が9スペース分をコードブロックとして読む——説明も表の見出しも
    // コードとして描かれ、表が組まれない。生の文字列で、字下げせずに書く。
    let mut out = String::from(
        r"# 判断記録の索引

`schema = 'decision'` のファイルの一覧。この索引は手で書かない。
`cargo xtask index-decisions` が `meta/decisions/*.toml` から作る。
中身を直すのは各 TOML 側で、索引は作り直す。

読み方と規律は [../README.md](../README.md)。置き換えの関係（`supersedes` /
`superseded_by` / `status = 'superseded'`）は `cargo xtask check-meta` が双方向で検査する。

| ID | 何についての判断か | 決めたこと | 状態 |
|---|---|---|---|
",
    );
    for (id, label, title, status) in &rows {
        out.push_str(&format!(
            "| [{id}]({id}.toml) | {label} | {title} | {status} |\n"
        ));
    }
    out.push_str(&format!("\n{} 件。\n", rows.len()));

    let dest = root.join(DECISION_INDEX);
    let current = fs::read_to_string(&dest).unwrap_or_default();
    if check {
        if current != out {
            rep.error(
                format!("{DECISION_INDEX} が古い。`cargo xtask index-decisions` で作り直す")
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
