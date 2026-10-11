//! `cargo xtask check-meta`
//!
//! 記録は [`KnowledgeSnapshot`] から読み、欄の参照先は `legacy_registry` の登録で組んだ
//! `SemanticGraph` で引く。 `toml::Table` は触らない。 出力は載せ替える前と1字も違わない
//! ——文言も、並びも。
//!
//! 参照先の並びは graph の edge の並びに任せない。 edge は `Relation` の順（起点の ID、
//! 行き先、名前）に並び直されるが、報告の並びは「ファイルの順、欄の宣言の順、欄の中の順」
//! だった。 graph は「その ID がその種類の node として在るか」を答える口として使い、
//! どの順に尋ねるかはここが決める。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::PathBuf;

use super::CheckContext;
use super::legacy_registry::{
    FSL_FIELDS, FSL_KIND, META_FIELDS, REQUIREMENT_FIELDS, REQUIREMENT_KIND, Targets, registry,
};
use crate::diagnostic::{Location, Report};
use crate::graph::SemanticGraph;
use crate::knowledge::{KnowledgeSnapshot, Record, find_tr, fsl_ids};

const CONFIDENCE: &[&str] = &["Fact", "Assumption", "Unknown", "Risk"];

/// 記録の場所。 `id` を書いた行まで。
fn at(r: &Record) -> Option<Location> {
    Some(Location::new(r.provenance().path(), r.provenance().line()))
}

/// ファイルだけを指す場所。 同じ ID が2度出るとき、行は最初の出現を指してしまうので使わない。
fn at_file(r: &Record) -> Option<Location> {
    Some(Location::new(r.provenance().path(), None))
}

/// 技術要件の登録簿そのものを検査する。ここが「本文の無い ID を参照する」の再発を止める箇所。
fn check_requirements(
    snapshot: &KnowledgeSnapshot,
    fsl: &BTreeSet<String>,
    rep: &mut Report,
) -> BTreeSet<String> {
    // 同じ ID は後のものが前のものを置き換える。 重複は、置き換えたときに数える。
    let mut reqs: BTreeMap<String, &Record> = BTreeMap::new();
    for r in snapshot.by_schema("requirement-set") {
        let Some(id) = r.str("id") else { continue };
        if reqs.insert(id.to_owned(), r).is_some() {
            rep.error_at(
                "meta.duplicate-requirement-id",
                at_file(r),
                format!(
                    "{}: id `{id}` が重複している",
                    r.provenance().path().display()
                ),
            );
        }
    }
    let ids: BTreeSet<String> = reqs.keys().cloned().collect();
    // 文言ごとに1件。 同じ要件が同じ ID を2度引いても1度しか出さない。
    let mut dangling: BTreeMap<String, Option<Location>> = BTreeMap::new();
    for (id, r) in &reqs {
        match r.str("confidence") {
            Some(c) if CONFIDENCE.contains(&c) => {}
            Some(c) => rep.error_at(
                "meta.invalid-confidence",
                at(r),
                format!("{id}: 確度 `{c}` は Fact / Assumption / Unknown のいずれでもない"),
            ),
            None => {}
        }
        if r.str("statement").is_none_or(str::is_empty) {
            rep.error_at("meta.empty-statement", at(r), format!("{id}: 本文が空"));
        }
        for d in r.strs("depends_on") {
            if !ids.contains(&d) {
                rep.error_at(
                    "meta.unknown-depends-on",
                    at(r),
                    format!("{id}: depends_on の `{d}` は登録簿に存在しない"),
                );
            }
        }
        for f in r.strs("formalized_as") {
            if !fsl.contains(&f) {
                rep.error_at(
                    "meta.unknown-formalized-as",
                    at(r),
                    format!("{id}: formalized_as の `{f}` は FSL に存在しない"),
                );
            }
        }
        let mut text = r.str("statement").unwrap_or_default().to_owned();
        for n in r.strs("notes") {
            text.push('\n');
            text.push_str(&n);
        }
        for referenced in find_tr(&text) {
            if !ids.contains(&referenced) {
                dangling
                    .entry(format!(
                        "{id} が参照する `{referenced}` は登録簿に存在しない"
                    ))
                    .or_insert_with(|| at(r));
            }
        }
    }
    for (message, location) in dangling {
        rep.error_at("meta.unknown-requirement-text-reference", location, message);
    }
    ids
}

/// 置き換えの関係が両側から見えているかを検査する。
///
/// `supersedes` は片側にしか書かれない。 置き換えられた側を開いた人には、
/// `status = 'accepted'` としか見えず、もう使われていないことが分からない。
/// 片側だけ直すと必ずそうなるので、両方が揃っていることをここで固定する。
fn check_supersession(snapshot: &KnowledgeSnapshot, rep: &mut Report) {
    let mut status: BTreeMap<String, String> = BTreeMap::new();
    let mut by: BTreeMap<String, String> = BTreeMap::new();
    let mut supersedes: BTreeMap<String, Vec<String>> = BTreeMap::new();
    // 後の検査が、名指された ID の場所を引けるように。
    let mut place: BTreeMap<String, &Record> = BTreeMap::new();

    for r in snapshot.by_schema("decision") {
        let Some(id) = r.str("id") else {
            continue;
        };
        place.insert(id.to_owned(), r);
        if let Some(st) = r.str("status") {
            status.insert(id.to_owned(), st.to_owned());
        }
        if let Some(b) = r.str("superseded_by") {
            by.insert(id.to_owned(), b.to_owned());
        }
        let old = r.strs("supersedes");
        if !old.is_empty() {
            supersedes.insert(id.to_owned(), old);
        }
    }
    let loc = |id: &String| place.get(id).and_then(|r| at(r));

    // 名指された側は superseded で、名指した相手を指し返している。
    for (newer, olds) in &supersedes {
        for old in olds {
            match status.get(old) {
                None => rep.error_at(
                    "meta.supersession-unknown-target",
                    loc(newer),
                    format!("{newer} の supersedes が指す {old} が無い"),
                ),
                Some(st) if st != "superseded" => rep.error_at(
                    "meta.supersession-status-mismatch",
                    loc(old),
                    format!(
                        "{newer} が {old} を置き換えているのに、{old} の status が `{st}` のまま"
                    ),
                ),
                Some(_) => {}
            }
            match by.get(old) {
                Some(b) if b == newer => {}
                Some(b) => rep.error_at(
                    "meta.supersession-back-reference-mismatch",
                    loc(old),
                    format!("{old} の superseded_by が `{b}` だが、置き換えているのは {newer}"),
                ),
                None => rep.error_at(
                    "meta.supersession-back-reference-missing",
                    loc(old),
                    format!("{old} に superseded_by = '{newer}' が無い"),
                ),
            }
        }
    }

    // 逆向き。superseded を名乗るなら、置き換えた相手がそう言っている。
    for (old, st) in &status {
        if st != "superseded" {
            continue;
        }
        let Some(newer) = by.get(old) else {
            rep.error_at(
                "meta.supersession-missing-superseded-by",
                loc(old),
                format!("{old} は superseded だが superseded_by が無い"),
            );
            continue;
        };
        if !supersedes.get(newer).is_some_and(|v| v.contains(old)) {
            rep.error_at(
                "meta.supersession-not-listed",
                loc(old),
                format!("{old} は {newer} に置き換えられたと言うが、{newer} の supersedes に無い"),
            );
        }
    }
}

pub(crate) fn check_meta(ctx: &CheckContext<'_>, rep: &mut Report) {
    let snapshot = ctx.snapshot;
    let fsl = fsl_ids(ctx.root);
    let tr = check_requirements(snapshot, &fsl, rep);
    check_supersession(snapshot, rep);

    // 要件はちょうど1つのマイルストーンに属する。どこにも属さない要件は、
    // 誰も作らないまま残る。 二重に属すると、二度作るか、どちらもやらない。
    let profiles: Vec<&Record> = snapshot.by_schema("profile").collect();
    if !profiles.is_empty() {
        let mut owner: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for r in &profiles {
            let id = r.str("id").unwrap_or("?").to_owned();
            for req in r.strs("includes_requirements") {
                owner.entry(req).or_default().push(id.clone());
            }
        }
        for (r, ms) in &owner {
            if ms.len() > 1 {
                rep.error_code(
                    "meta.requirement-in-many-milestones",
                    format!("{r} が複数のマイルストーンに属している: {}", ms.join(", ")),
                );
            }
        }
        let orphans: Vec<&String> = tr.iter().filter(|r| !owner.contains_key(*r)).collect();
        if !orphans.is_empty() {
            let head: Vec<String> = orphans.iter().take(5).map(|s| (*s).clone()).collect();
            rep.error_code(
                "meta.requirement-without-milestone",
                format!(
                    "どのマイルストーンにも属さない要件が {} 件ある: {} ...",
                    orphans.len(),
                    head.join(", ")
                ),
            );
        }
    }

    let decisions: BTreeSet<String> = snapshot
        .by_schema("decision")
        .filter_map(|r| r.str("id").map(str::to_owned))
        .collect();
    let mut components = 0usize;
    for r in snapshot.by_schema("component-ledger") {
        components += 1;
        let file = r.provenance().path().display();
        // 採否は判断記録が持つ。台帳が指す先が実在すること。
        if let Some(d) = r.str("decided_by")
            && !decisions.contains(d)
        {
            rep.error_at(
                "meta.component-unknown-decision",
                at(r),
                format!("{file}: decided_by の `{d}` という判断記録は存在しない"),
            );
        }
        // 決め切った採否には、理由と撤回条件が要る。 それを持つのは判断記録だけ。
        // 「採用候補」「条件付き」「要調査」はまだ決めていないので対象外。
        if matches!(r.str("status"), Some("採用" | "不適")) && r.str("decided_by").is_none() {
            let id = r.str("id").unwrap_or("?");
            rep.error_at(
                "meta.component-missing-decision",
                at(r),
                format!(
                    "{file}: `{id}` は採否を決めているのに decided_by が無い。\
                     理由と撤回条件を持つ判断記録へ繋ぐこと"
                ),
            );
        }
    }
    let targets = snapshot.by_schema("target-set").count();

    // 参照できる ID。1件1ファイルのものと、収集ファイルの項目の両方。
    let mut ids: BTreeMap<String, PathBuf> = BTreeMap::new();
    for r in snapshot.records() {
        let Some(id) = r.str("id") else { continue };
        let path = r.provenance().path().to_path_buf();
        if let Some(prev) = ids.insert(id.to_owned(), path) {
            rep.error_at(
                "meta.duplicate-id",
                at_file(r),
                format!(
                    "{}: id `{id}` が {} と重複している",
                    r.provenance().path().display(),
                    prev.display()
                ),
            );
        }
    }

    let graph = SemanticGraph::from_snapshot(snapshot, &registry());
    let known = Targets::new(&graph, snapshot, &fsl);
    // 欄を引くのは、読んだファイルごとの表の直下。 1件1ファイルの記録も、収集ファイルの
    // 直下に置かれた欄（項目ではなくファイルそのものの欄）も同じ扱い（`SourceRecord`）。
    // 場所はファイルまで——直下の欄には、`id` のような行を引く手掛かりが無い。
    for r in snapshot.sources() {
        let file = r.path().display().to_string();
        let at = || Some(Location::new(r.path(), None));
        // 同じ ID が同じ一覧に2度出るのは、たいてい編集の取りこぼし。
        // 害は薄いが、書き換えを間違えた合図としては確かなので落とす。
        for (key, value) in r.duplicate_string_values() {
            rep.error_at(
                "meta.duplicate-list-entry",
                at(),
                format!("{file}: {key} に `{value}` が2度出ている"),
            );
        }
        for (fields, kind, label, code) in [
            (
                REQUIREMENT_FIELDS,
                REQUIREMENT_KIND,
                "技術要件",
                "meta.unknown-requirement-reference",
            ),
            (
                FSL_FIELDS,
                FSL_KIND,
                "FSL の要求",
                "meta.unknown-fsl-reference",
            ),
        ] {
            for key in fields {
                for target in r.strs(key) {
                    if !known.is_kind(&target, kind) {
                        rep.error_at(
                            code,
                            at(),
                            format!("{file}: {key} の `{target}` は{label}に存在しない"),
                        );
                    }
                }
            }
        }
        for key in META_FIELDS {
            for target in r.strs(key) {
                if !known.is_meta(&target) {
                    rep.error_at(
                        "meta.unknown-reference",
                        at(),
                        format!("{file}: {key} の `{target}` という meta は存在しない"),
                    );
                }
            }
        }
        for rel in r.strs("undecided_in") {
            let p = ctx.root.join(&rel);
            match fs::read_to_string(&p) {
                // 正式表記は `@undecided(...)`。文字列形式は非推奨だが移行期のため両方見る。
                Ok(text) if text.contains("@undecided(") || text.contains("undecided:") => {}
                Ok(_) => rep.error_at(
                    "meta.undecided-marker-missing",
                    at(),
                    format!("{file}: {rel} に未決の印が無い"),
                ),
                Err(_) => rep.error_at(
                    "meta.undecided-file-unreadable",
                    at(),
                    format!("{file}: {rel} が読めない"),
                ),
            }
        }
    }

    // suite が支える契約（`DEC-PLT-039`）。 技術要件・FSL・meta のどれかに実在すること。
    // 収集ファイルの項目の中の欄なので、上の走査（ファイル直下だけを見る）には入らない。
    for r in snapshot.by_schema("test-portfolio") {
        let id = r.str("id").unwrap_or("?");
        for c in r.strs("contracts") {
            if !(tr.contains(&c) || fsl.contains(&c) || ids.contains_key(&c)) {
                rep.error_at(
                    "meta.suite-unknown-contract",
                    at(r),
                    format!(
                        "{}: {id} の contracts の `{c}` は存在しない",
                        r.provenance().path().display()
                    ),
                );
            }
        }
    }

    rep.note_code(
        "meta.summary",
        format!(
            "FSL の要求 {} 件 / 技術要件 {} 件 / 部品台帳 {components} 件 / 性能目標 {targets} 件",
            fsl.len(),
            tr.len()
        ),
    );
    // ファイル単位の数え方。 記録ではなく、読んだファイルの数。
    rep.note_code(
        "meta.summary",
        snapshot
            .source_counts()
            .map(|(schema, count)| format!("{schema} {count}"))
            .collect::<Vec<_>>()
            .join(" / "),
    );
}
