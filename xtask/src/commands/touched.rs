//! `cargo xtask touched [<base>]`

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use super::index_decisions::DECISION_INDEX;
use crate::diagnostic::Report;
use crate::knowledge::{Entry, fsl_sites, id_index, id_tokens, list_of, str_of};
use crate::repo::{SKIPPED_DIRS, git, nul_paths};

/// `touched` が本文として読む拡張子。
///
/// `check-references` の走査より広い。 あちらは手書き文書とソースの引用が
/// 実体に解決するかを見るが、こちらは**変更が何に触れたか**を出すので、
/// ID を引いているものは形を問わず読む——配色の CSS も workflow も引いている。
const TOUCHED_EXT: &[&str] = &[
    "md", "rs", "ts", "tsx", "fsl", "css", "yml", "yaml", "json", "toml", "nix", "graphql",
];

/// 差分に付ける文脈の幅。**拡張子ごとに変える。**
///
/// コードの引用は変更した行ではなく、その関数や部品の doc コメントに書かれている。
/// 既定の3行では届かないので広げる。
///
/// 文書は逆で、引用は箇条書きの行そのものにある。同じ幅で広げると隣の項目まで入る
/// ——`AGENTS.md` を1行直しただけで、前後の箇条書きが引く ID が全部出た。
///
/// SDL はコードの側。 引用は欄の上の description か、型の頭にある。
const DIFF_SCOPES: &[(usize, &[&str])] = &[
    (25, &["*.rs", "*.ts", "*.tsx", "*.graphql"]),
    (
        3,
        &[
            "*.md", "*.fsl", "*.css", "*.yml", "*.yaml", "*.json", "*.toml", "*.nix",
        ],
    ),
];

/// hunk の見出しから、片側の開始行と行数を読む。
///
/// **行数が明示の 0 なら、その側には行が無い。** 追加だけ・削除だけの hunk で
/// 反対側を 1 行として数えると、触っていない項目を巻き込む。
/// 行数が書かれていない形（`@@ -1 +1 @@`）は 1 行。
fn hunk_span(head: &str, mark: char) -> Option<(usize, usize)> {
    /*
     * 数として読むのは、数字だけでできた字に限る。
     *
     * **`parse::<usize>()` は先頭の `+` を受ける。** `@@ -837 +837 @@` の
     * `-` 側を読むと次の字が `+837` で、これが行数として通っていた。
     * 1 行の書き換えが 837 行分に化けて、触っていない項目が何十件も並んだ。**踏んだ。**
     */
    let digits = |x: &&str| x.chars().all(|c| c.is_ascii_digit()) && !x.is_empty();
    let mut it = head.split(mark).nth(1)?.split([',', ' ']);
    let at = it.next().filter(digits)?.parse().ok()?;
    // 行数が書かれていない形（`@@ -1 +1 @@`）は 1 行。
    let len = it
        .next()
        .filter(digits)
        .and_then(|x| x.parse().ok())
        .unwrap_or(1);
    (len > 0).then_some((at, len))
}

/// 変更が触れた ID を、レビューに要る本文ごと出す。
///
/// **レビューの入口を、読む人が思い出せるかどうかに委ねない。** 変更された
/// ファイルが引いている ID を集めれば、その変更が何の契約に触れたかは機械的に決まる。
/// 出すのは ID だけではなく本文まで——要件なら条文、判断なら選んだ案と覆る条件。
/// 探しに行かせると、探さないまま読まれる。
///
/// **出るのはコメントが引いているものだけ。** 1つも引いていない変更ファイルは
/// 別に並べる。そこが盲点で、盲点があること自体を見せるほうが、黙って0件を返すより良い。
///
/// 見るのは `<base>...HEAD` と、作業ツリーの未コミット分。
///
/// **ファイル単位では拾わない。** 変更されたファイルの ID を全部拾うと、
/// `AGENTS.md` を2行直しただけでそこに並ぶ数十件が出る。拾うのは差分の
/// hunk の中だけ——文脈を [`DIFF_SCOPES`] の行数だけ広げてあるので、変更した行の
/// 手前にある doc コメントの引用は入る。
pub(crate) fn touched(root: &Path, entries: &[Entry], base: &str, mut rep: Report) -> ExitCode {
    let range = format!("{base}...HEAD");
    let index = id_index(entries);
    let in_specs = fsl_sites(root);
    let mut cited: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    // 変更ファイル → hunk の中で ID を引いていたか。
    let mut files: BTreeMap<String, bool> = BTreeMap::new();
    let mut changed: BTreeSet<String> = BTreeSet::new();

    for spec in [range.as_str(), "HEAD"] {
        match git(root, &["diff", "--name-only", "-z", spec]) {
            Ok(out) => changed.extend(nul_paths(&out)),
            Err(e) => {
                rep.error(e);
                return rep.finish("touched");
            }
        }
    }

    /*
     * まだ `git add` していないファイルも見る。
     *
     * `git diff` は未追跡を返さないので、書いたばかりの実装や文書が
     * **数からも引用なし一覧からも丸ごと消える。** 書いている最中に読ませるのが
     * 主な使い道なので、そこで見えないのでは使えない。全体が追加なので全行を見る。
     */
    let untracked = match git(root, &["ls-files", "--others", "--exclude-standard", "-z"]) {
        Ok(out) => nul_paths(&out).collect::<Vec<_>>(),
        Err(e) => {
            rep.error(e);
            return rep.finish("touched");
        }
    };
    for rel in &untracked {
        changed.insert(rel.clone());
        let Some(rel) = readable(rel) else { continue };
        let Ok(text) = fs::read_to_string(root.join(&rel)) else {
            continue;
        };
        files.entry(rel.clone()).or_insert(false);
        for (n, line) in text.lines().enumerate() {
            for id in id_tokens(line) {
                cited
                    .entry(id)
                    .or_default()
                    .insert(format!("{rel}:{}", n + 1));
                files.insert(rel.clone(), true);
            }
        }
    }

    for (context, globs) in DIFF_SCOPES {
        let context = format!("-U{context}");
        // 作業ツリーの未コミット分も見る。PR を出す前に手元で読ませるのが主な使い道で、
        // コミットしてからでないと出ないのでは、書いている最中に使えない。
        for spec in [range.as_str(), "HEAD"] {
            let mut args = vec!["diff", context.as_str(), spec, "--"];
            args.extend_from_slice(globs);
            match git(root, &args) {
                Ok(diff) => scan_diff(&diff, &mut cited, &mut files),
                Err(e) => {
                    rep.error(e);
                    return rep.finish("touched");
                }
            }
        }
    }
    /*
     * 正本そのものの書き換えは、引用ではなく変更対象として数える。
     *
     * `meta/` の TOML は走査の対象ではない——ID を引いているのではなく、
     * ID を**持っている**。条文が書き換わったなら、その契約こそ読む対象なので、
     * 変わった行が属する項目を引き当てて並べる。
     *
     * **消した側も引き当てる。** 項目やファイルごと消すと、いまの版からは
     * 引き当てられない。base の原文と旧側の行番号で拾う——契約を消す変更こそ、
     * 出ないと困る。
     */
    let base_rev = git(root, &["merge-base", base, "HEAD"])
        .map(|x| x.trim().to_owned())
        .unwrap_or_else(|_| base.to_owned());
    let mut rewritten: BTreeSet<&str> = BTreeSet::new();
    for rel in changed
        .iter()
        .filter(|r| r.starts_with("meta/") && r.ends_with(".toml"))
    {
        rewritten.insert(rel.as_str());
        // 未追跡のものは全体が追加。ファイルが持つ ID を全部数える。
        if untracked.contains(rel) {
            if let Ok(text) = fs::read_to_string(root.join(rel)) {
                for (n, id) in owners(&text) {
                    let _ = n;
                    cited
                        .entry(id)
                        .or_default()
                        .insert(format!("{rel}（追加）"));
                }
            }
            continue;
        }
        let now = fs::read_to_string(root.join(rel)).unwrap_or_default();
        let own_now = owners(&now);
        for (spec, rev) in [(range.as_str(), base_rev.as_str()), ("HEAD", "HEAD")] {
            let was = git(root, &["show", &format!("{rev}:{rel}")]).unwrap_or_default();
            let own_was = owners(&was);
            let Ok(diff) = git(root, &["diff", "-U0", spec, "--", rel]) else {
                continue;
            };
            for line in diff.lines() {
                let Some(rest) = line.strip_prefix("@@ ") else {
                    continue;
                };
                // 消した側と残る側の両方を見る。消しただけの hunk は長さ 0。
                for (mark, owners, note) in [('-', &own_was, "削除"), ('+', &own_now, "書き換え")]
                {
                    let Some((at, len)) = hunk_span(rest, mark) else {
                        continue;
                    };
                    for n in at..at + len {
                        if let Some(id) = owner_at(owners, n) {
                            // 行ごとには並べない。 書き換わったのは項目1件で、
                            // どの行かは差分そのものが見せる。
                            rewritten.insert(rel.as_str());
                            cited
                                .entry(id.to_owned())
                                .or_default()
                                .insert(format!("{rel}（{note}）"));
                        }
                    }
                }
            }
        }
    }

    /*
     * 引用が無い変更ファイルは、**変更された全部から作る。**
     *
     * 読める形（`SCANNED_EXT`）だけから作ると、`Cargo.toml` や CSS や
     * workflow の YAML が一覧に出ない。出ないと「この道具は何も言っていない」
     * ことすら見えず、見たつもりになる。生成物だけは外す。
     */
    let cited_in: BTreeSet<&String> = files
        .iter()
        .filter(|(_, found)| **found)
        .map(|(rel, _)| rel)
        .collect();
    let silent: Vec<&String> = changed
        .iter()
        .filter(|rel| !cited_in.contains(rel) && rel.as_str() != DECISION_INDEX)
        .filter(|rel| !rewritten.contains(rel.as_str()))
        .filter(|rel| {
            !Path::new(rel)
                .components()
                .any(|c| SKIPPED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
        })
        .collect();

    println!("# {range} が触れた契約\n");
    for (id, at) in &cited {
        match index.get(id) {
            Some((schema, path, table)) => {
                let rel = path.strip_prefix(root).unwrap_or(path);
                // 部品台帳は題を `name` で持つ。形ごとに名前の欄が違う。
                let name = str_of(table, "title")
                    .or_else(|| str_of(table, "name"))
                    .unwrap_or("（題が無い）");
                println!("## {id} — {name}");
                println!("\n正本: `{}`", rel.display());
                println!(
                    "引いている場所: {}",
                    at.iter().cloned().collect::<Vec<_>>().join(", ")
                );
                print_brief(schema, table);
            }
            None if in_specs.contains_key(id) => {
                // 条文は FSL の正本にある。`fslc` を通さずに本文を写すと、
                // 「書かれているもの」と「検証されているもの」がずれる。場所だけ指す。
                println!("## {id}\n\n正本: `{}`（FSL）", in_specs[id]);
                println!(
                    "引いている場所: {}",
                    at.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            None if at.iter().all(|x| x.ends_with("（削除）")) => {
                println!(
                    "## {id}\n\n**この変更で消えた。** 元の本文は `git show {base_rev}:<path>`。"
                );
                println!(
                    "消えた場所: {}",
                    at.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
            None => {
                println!("## {id}\n\n**実体が無い。**（`check-references` が落とす）");
                println!(
                    "引いている場所: {}",
                    at.iter().cloned().collect::<Vec<_>>().join(", ")
                );
            }
        }
        println!();
    }

    if !silent.is_empty() {
        println!("## ID を1つも引いていない変更ファイル\n");
        println!("ここは契約との対応が機械では出ない。読んで決める。\n");
        for rel in &silent {
            println!("- `{rel}`");
        }
        println!();
    }

    // 読めた範囲を数で出す。 **この道具が変更の何割を語れているかは、
    // 読む側が知っていないと危ない。** 全部を見たつもりにさせない。
    rep.note(format!(
        "変更 {} ファイル、うち引用があったのは {}、触れた ID {} 件",
        changed.len(),
        cited_in.len(),
        cited.len()
    ));
    rep.finish("touched")
}

/// ID を引いているかを読める形のパスか。生成物と対象外は `None`。
fn readable(path: &str) -> Option<String> {
    let p = Path::new(path);
    // `meta/` の TOML は引用ではなく正本。 触れたことの数え方が違うので、
    // ここでは読まない（`touched` が別に引き当てる）。
    if (path.starts_with("meta/") && path.ends_with(".toml"))
        || path == DECISION_INDEX
        || p.components()
            .any(|c| SKIPPED_DIRS.contains(&c.as_os_str().to_string_lossy().as_ref()))
        || p.file_name()
            .is_some_and(|n| n.to_string_lossy().ends_with(".gen.ts"))
        || !p
            .extension()
            .is_some_and(|x| TOUCHED_EXT.contains(&x.to_string_lossy().as_ref()))
    {
        return None;
    }
    Some(path.to_owned())
}

/// TOML の各項目が何行目から始まるか。行から所属する ID を引くために持つ。
fn owners(text: &str) -> Vec<(usize, String)> {
    text.lines()
        .enumerate()
        .filter_map(|(n, line)| {
            let rest = line.trim_start().strip_prefix("id = ")?;
            let rest = rest.trim().strip_prefix('\'')?;
            let end = rest.find('\'')?;
            Some((n + 1, rest[..end].to_owned()))
        })
        .collect()
}

/// その行が属する項目。
///
/// 手前に項目が無い行（`schema` の宣言など）は、1件1ファイルなら
/// そのファイルの項目のものとして数える。 収集ファイルでは誰のものでもない。
fn owner_at(owners: &[(usize, String)], line: usize) -> Option<&str> {
    match owners.iter().rev().find(|(at, _)| *at <= line) {
        Some((_, id)) => Some(id),
        None if owners.len() == 1 => owners.first().map(|(_, id)| id.as_str()),
        None => None,
    }
}

/// 統合差分から、hunk の中に現れる ID を拾う。
///
/// **消した側も拾う。** 契約を実装していた箇所を丸ごと消したとき、その契約は
/// 残る側のどこにも現れない。レビューで一番見たいのはそこなので、
/// 旧パスと旧行で指して「削除」と印を付ける。
fn scan_diff(
    diff: &str,
    cited: &mut BTreeMap<String, BTreeSet<String>>,
    files: &mut BTreeMap<String, bool>,
) {
    let take = |path: &str| readable(path);
    let mut old_rel: Option<String> = None;
    let mut new_rel: Option<String> = None;
    let (mut old_no, mut new_no) = (0usize, 0usize);

    for line in diff.lines() {
        if let Some(path) = line.strip_prefix("--- a/") {
            old_rel = take(path);
            continue;
        }
        if let Some(path) = line.strip_prefix("+++ b/") {
            new_rel = take(path);
            if let Some(rel) = new_rel.as_deref() {
                files.entry(rel.to_owned()).or_insert(false);
            }
            continue;
        }
        // `@@ -12,7 +34,9 @@` の `12` と `34`。両側の行番号をここから数える。
        if let Some(rest) = line.strip_prefix("@@ ") {
            let at = |mark: char| {
                rest.split(mark)
                    .nth(1)
                    .and_then(|x| x.split([',', ' ']).next())
                    .and_then(|x| x.parse().ok())
                    .unwrap_or(0)
            };
            (old_no, new_no) = (at('-'), at('+'));
            continue;
        }
        let (rel, at, body) = match line.as_bytes().first() {
            Some(b' ') => (new_rel.as_deref(), format!("{new_no}"), &line[1..]),
            Some(b'+') => (new_rel.as_deref(), format!("{new_no}"), &line[1..]),
            Some(b'-') => (old_rel.as_deref(), format!("{old_no}（削除）"), &line[1..]),
            _ => continue,
        };
        if let Some(rel) = rel {
            for id in id_tokens(body) {
                cited.entry(id).or_default().insert(format!("{rel}:{at}"));
                files.insert(rel.to_owned(), true);
            }
        }
        match line.as_bytes().first() {
            Some(b' ') => {
                old_no += 1;
                new_no += 1;
            }
            Some(b'+') => new_no += 1,
            Some(b'-') => old_no += 1,
            _ => {}
        }
    }
}

/// レビューで要るものだけを、形ごとに選んで出す。
///
/// 全部出すと本文が埋まる。要件なら条文、判断なら選んだ案と**覆る条件**
/// ——この変更が引き金を引いていないかは、条件を並べないと誰も見ない。
fn print_brief(schema: &str, t: &toml::Table) {
    let field = |k: &str| str_of(t, k).unwrap_or_default().trim().to_owned();
    match schema {
        "requirement-set" => {
            println!("確信度: {}\n", field("confidence"));
            println!("{}", field("statement"));
        }
        "decision" => {
            println!(
                "状態: {} ／ 選んだ案: {}\n",
                field("status"),
                field("selected")
            );
            let triggers = list_of(t, "review_triggers");
            if !triggers.is_empty() {
                println!("覆る条件:");
                for x in &triggers {
                    println!("- {x}");
                }
            }
        }
        "question" => {
            println!("状態: {}\n", field("status"));
            println!("{}", field("why_it_matters"));
            let how = field("how_to_close");
            if !how.is_empty() {
                println!("\n閉じ方: {how}");
            }
        }
        "evidence" => {
            println!("種別: {} ／ 確信度: {}", field("kind"), field("confidence"));
        }
        "profile" => {
            println!("状態: {}", field("status"));
        }
        "component-ledger" => {
            println!(
                "{} ／ {} ／ {}",
                field("name"),
                field("license"),
                field("status")
            );
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan(diff: &str) -> (BTreeMap<String, BTreeSet<String>>, BTreeMap<String, bool>) {
        let mut cited = BTreeMap::new();
        let mut files = BTreeMap::new();
        scan_diff(diff, &mut cited, &mut files);
        (cited, files)
    }

    /// 行番号は残る側で数える。消した行では進まない。
    ///
    /// ずれると、レビューを無関係な行へ案内する。案内先が違うことは
    /// 引用が出ていること自体からは分からないので、ここで固定する。
    #[test]
    fn 行番号は残る側で数える() {
        let (cited, _) = scan(
            "diff --git a/src/a.rs b/src/a.rs\n\
             --- a/src/a.rs\n\
             +++ b/src/a.rs\n\
             @@ -10,3 +20,4 @@\n\
             \x20/// `TR-REC-02` を満たす。\n\
             -old\n\
             +new\n\
             +// `DEC-PLT-030`\n",
        );
        assert_eq!(
            cited["TR-REC-02"],
            BTreeSet::from(["src/a.rs:20".to_owned()])
        );
        assert_eq!(
            cited["DEC-PLT-030"],
            BTreeSet::from(["src/a.rs:22".to_owned()])
        );
    }

    /// ファイル名の行を本文として読まない。
    ///
    /// `+++ b/…` は `+` で始まる。本文と同じ扱いにすると、
    /// パスに ID を含むファイルが自分自身を引用したことになる。
    #[test]
    fn ファイル名の行は本文ではない() {
        let (cited, files) = scan(
            "diff --git a/docs/TR-REC-02.md b/docs/TR-REC-02.md\n\
             --- a/docs/TR-REC-02.md\n\
             +++ b/docs/TR-REC-02.md\n\
             @@ -1 +1 @@\n\
             +本文\n",
        );
        assert!(cited.is_empty());
        assert_eq!(files.get("docs/TR-REC-02.md"), Some(&false));
    }

    /// 消した側の引用も拾う。
    ///
    /// 契約を実装していた箇所を丸ごと消すと、残る側のどこにも現れない。
    /// そこを落とすと、レビューで一番見たい変更が一覧から消える。
    #[test]
    fn 消した側の引用も拾う() {
        let (cited, _) = scan(
            "diff --git a/src/a.rs b/src/a.rs\n\
             --- a/src/a.rs\n\
             +++ b/src/a.rs\n\
             @@ -10,2 +20,1 @@\n\
             -// `TR-REC-38` を満たす\n\
             -let margin = 300;\n\
             +let margin = 0;\n",
        );
        assert_eq!(
            cited["TR-REC-38"],
            BTreeSet::from(["src/a.rs:10（削除）".to_owned()])
        );
    }

    /// TOML の行から、その行が属する項目を引く。
    ///
    /// 収集ファイルでは、手前に項目が無い行は誰のものでもない。
    /// 1件1ファイルでは、宣言の行もその項目のものとして数える。
    #[test]
    fn 行から持ち主を引く() {
        let collection = owners(
            "schema = 'requirement-set'\n\
             [[requirement]]\n\
             id = 'TR-REC-02'\n\
             title = 'あ'\n\
             [[requirement]]\n\
             id = 'TR-REC-03'\n",
        );
        assert_eq!(owner_at(&collection, 1), None);
        assert_eq!(owner_at(&collection, 4), Some("TR-REC-02"));
        assert_eq!(owner_at(&collection, 6), Some("TR-REC-03"));

        let entity = owners("schema = 'decision'\nid = 'DEC-PLT-030'\n");
        assert_eq!(owner_at(&entity, 1), Some("DEC-PLT-030"));
    }

    /// 行数が 0 の側は、その hunk に行が無い。
    ///
    /// 追加だけ・削除だけの hunk で反対側を 1 行として数えると、
    /// 触っていない項目まで「書き換え」に混ざる。
    #[test]
    fn 長さ0の側は数えない() {
        assert_eq!(hunk_span("-3,4 +2,0 @@", '+'), None);
        assert_eq!(hunk_span("-3,4 +2,0 @@", '-'), Some((3, 4)));
        assert_eq!(hunk_span("-0,0 +1,5 @@", '+'), Some((1, 5)));
        // 行数を書かない形は 1 行。
        assert_eq!(hunk_span("-1 +1 @@", '+'), Some((1, 1)));
        // 反対側の見出しを行数として読まない。`+837` は数ではなく次の側の印。
        assert_eq!(
            hunk_span("-837 +837 @@ id = 'CMP-060'", '-'),
            Some((837, 1))
        );
        assert_eq!(
            hunk_span("-837 +837 @@ id = 'CMP-060'", '+'),
            Some((837, 1))
        );
    }

    /// 生成物と走査対象外は読まない。
    #[test]
    fn 生成物と対象外は読まない() {
        let diff = |path: &str| {
            format!(
                "diff --git a/{path} b/{path}\n\
                 --- a/{path}\n\
                 +++ b/{path}\n\
                 @@ -1 +1 @@\n\
                 +`TR-REC-02`\n"
            )
        };
        for path in [DECISION_INDEX, "meta/requirements/recording.toml"] {
            let (cited, files) = scan(&diff(path));
            assert!(cited.is_empty(), "{path} を読んでいる");
            assert!(files.is_empty(), "{path} を数えている");
        }
    }
}
