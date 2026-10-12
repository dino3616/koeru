//! D01 の試験。 fixture の ID は `fix` を領域にする（`check-references` が拾わない綴り）。 目標の形の
//! ID は提案書 5.5 の例を使う。 実在する meta を読ませる試験が最後に1本ある。

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde_json::Value;

use super::rules::{NOT_TRAVERSED, RULES, Tier, is_ruled};
use super::{Format, Output, Request, execute, parse};
use crate::knowledge::design::{RELATIONS, Standing};
use crate::repo::RepoView;

type Files = BTreeMap<String, String>;

const FIXTURE_ROOT: &str = "/fixture";

// ## fixture（legacy）

const REQUIREMENTS: &str = "schema = 'requirement-set'
domain = 'fix'
prefix = 'TR-fix'

[[requirement]]
id = 'TR-fix-01'
title = '形式にされている要件'
confidence = 'Fact'
statement = '本文1'
formalized_as = ['REQ-fix-001']

[[requirement]]
id = 'TR-fix-02'
title = '1に依る要件'
confidence = 'Fact'
statement = '本文2'
depends_on = ['TR-fix-01']

[[requirement]]
id = 'TR-fix-03'
title = '誰にも結びつかない要件'
confidence = 'Fact'
statement = '本文3'
";

const DECISION_OLD: &str = "schema = 'decision'
id = 'DEC-fix-001'
title = '以前の判断'
status = 'accepted'
owner = 'x'
options = ['a', 'b']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
affects_requirements = ['TR-fix-01']
";

/// `DEC_OLD` が後継に置き換えられた版。
const DECISION_OLD_SUPERSEDED: &str = "schema = 'decision'
id = 'DEC-fix-001'
title = '以前の判断'
status = 'superseded'
owner = 'x'
options = ['a', 'b']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
superseded_by = 'DEC-fix-002'
affects_requirements = ['TR-fix-01']
";

const DECISION_NEW: &str = "schema = 'decision'
id = 'DEC-fix-002'
title = '現在の判断'
status = 'accepted'
owner = 'x'
options = ['案A', '案B', '案C']
selected = '案A'
rationale = '理由'
review_triggers = ['t']
constraint_label = '守ること'
constraint = '制約の本文'
supersedes = ['DEC-fix-001']
evidence = ['EVID-fix-001']
affects_questions = ['Q-fix-002']
establishes_requirements = ['TR-fix-02']
";

const QUESTION_CLOSED: &str = "schema = 'question'
id = 'Q-fix-001'
title = '閉じた問い'
status = 'closed'
owner = 'x'
why_it_matters = 'なぜ'
how_to_close = 'どう'
closed_by = 'DEC-fix-002'
";

const QUESTION_OPEN: &str = "schema = 'question'
id = 'Q-fix-002'
title = '未解決の問い'
status = 'open'
owner = 'x'
why_it_matters = 'なぜ'
how_to_close = 'どう'
blocks_profiles = ['PROFILE-fix-1']
";

const PROFILE: &str = "schema = 'profile'
id = 'PROFILE-fix-1'
title = '対象'
status = 'planned'
includes_requirements = ['TR-fix-01']
";

const EVIDENCE: &str = "schema = 'evidence'
id = 'EVID-fix-001'
title = '調査'
kind = 'measurement'
source = '出典'
provenance = '出どころの説明'
confidence = 'Fact'
";

const FSL: &str = "model Fix {
  @requirement(\"REQ-fix-001\", \"本文\")
}
";

/// 全部が揃った legacy の版。
fn legacy() -> Files {
    [
        ("meta/requirements/fix.toml", REQUIREMENTS),
        ("meta/decisions/DEC-fix-001.toml", DECISION_OLD_SUPERSEDED),
        ("meta/decisions/DEC-fix-002.toml", DECISION_NEW),
        ("meta/questions/Q-fix-001.toml", QUESTION_CLOSED),
        ("meta/questions/Q-fix-002.toml", QUESTION_OPEN),
        ("meta/profiles/PROFILE-fix-1.toml", PROFILE),
        ("meta/evidence/EVID-fix-001.toml", EVIDENCE),
        ("specs/fix.fsl", FSL),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_owned(), b.to_owned()))
    .collect()
}

// ## fixture（目標の形）

fn hypothesis(contradicting: &str, scope: &str) -> String {
    format!(
        "schema = 'design-hypothesis'
id = 'HYP-91MTWX'
question = 'Q-fix-002'
hypothesis_kind = 'semantic-comprehension'
statement = '初めて使う人が説明できる'
scope = '{scope}'
assessment = 'untested'
supporting_evidence = ['EVID-fix-001']
contradicting_evidence = [{contradicting}]
"
    )
}

fn target_decision(relies: &str) -> String {
    format!(
        "schema = 'decision'
id = 'DEC-6CQ3NA'
title = '目標の形の判断'
status = 'accepted'
owner = 'x'
options = ['a']
selected = 'a'
rationale = '理由'
review_triggers = ['t']
{relies}
establishes_requirements = ['REQ-5H8NQ2']
"
    )
}

const TARGET_REQUIREMENT: &str = "schema = 'requirement'
id = 'REQ-5H8NQ2'
statement = '現在可能な創作行為を利用者が認識できなければならない'
established_by = 'DEC-6CQ3NA'
";

const CONTRADICTING_EVIDENCE: &str = "schema = 'evidence'
id = 'EVID-2V8JRF'
title = '反証する調査'
kind = 'observation'
source = '出典'
provenance = '出どころの説明'
confidence = 'Fact'
";

/// 目標の形の fixture を足す。 判断は仮説に依り、仮説は支持だけを持つ。
fn with_target(mut files: Files) -> Files {
    for (path, body) in [
        (
            "meta/hypotheses/HYP-91MTWX.toml",
            hypothesis("", "音源一覧"),
        ),
        (
            "meta/decisions/DEC-6CQ3NA.toml",
            target_decision("relies_on_hypotheses = ['HYP-91MTWX']"),
        ),
        (
            "meta/requirements/REQ-5H8NQ2.toml",
            TARGET_REQUIREMENT.to_owned(),
        ),
    ] {
        files.insert(path.to_owned(), body);
    }
    files
}

/// 領域を大文字にする。 `id_tokens` は大文字の領域しか ID として拾わない。 小文字のまま書くと、
/// この試験が引用として読ませたい ID が拾われない。 大文字で書くと実リポジトリの
/// `check-references` が実体の無い ID として落とすので、書くときは小文字で、ここで直す。
fn up(text: &str) -> String {
    text.replace("-fix", "-FIX")
}

/// path と本文の ID を、大文字の領域にそろえる。 引用から拾う試験の fixture。
fn upper(files: Files) -> Files {
    files.into_iter().map(|(p, b)| (up(&p), up(&b))).collect()
}

fn put(mut files: Files, path: &str, body: &str) -> Files {
    files.insert(path.to_owned(), body.to_owned());
    files
}

fn without(mut files: Files, path: &str) -> Files {
    files.remove(path);
    files
}

// ## 実行

fn request(roots: &[&str]) -> Request {
    Request {
        roots: roots.iter().map(|r| (*r).to_owned()).collect(),
        at: None,
        from: None,
        format: Format::Json,
        budget: None,
    }
}

fn view(files: &Files) -> RepoView {
    RepoView::in_memory(Path::new(FIXTURE_ROOT), files.clone())
}

fn run(request: &Request, at: &Files, from: Option<&Files>) -> Output {
    let from_view = from.map(view);
    execute(
        Path::new(FIXTURE_ROOT),
        request,
        &view(at),
        from_view.as_ref(),
    )
}

fn json(output: &Output) -> Value {
    serde_json::from_str(&output.text).expect("JSON として読める")
}

fn compared(roots: &[&str], at: &Files, from: &Files) -> Output {
    let mut r = request(roots);
    r.from = Some("BASE".to_owned());
    run(&r, at, Some(from))
}

fn ctx(v: &Value) -> &Value {
    &v["context"]
}

fn ids(v: &Value, key: &str) -> BTreeSet<String> {
    ctx(v)[key]
        .as_array()
        .expect("配列")
        .iter()
        .map(|i| i["id"].as_str().expect("id").to_owned())
        .collect()
}

fn item<'a>(v: &'a Value, id: &str) -> &'a Value {
    ctx(v)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("{id} が項目に無い"))
}

fn rules_of(item: &Value) -> BTreeSet<String> {
    item["reasons"]
        .as_array()
        .expect("配列")
        .iter()
        .map(|r| r["rule"].as_str().expect("rule").to_owned())
        .collect()
}

fn changes<'a>(v: &'a Value, class: &str) -> &'a Vec<Value> {
    ctx(v)["changes"][class].as_array().expect("分類の配列")
}

fn diagnostic_codes(v: &Value) -> BTreeSet<String> {
    v["diagnostics"]
        .as_array()
        .expect("配列")
        .iter()
        .map(|d| d["code"].as_str().expect("code").to_owned())
        .collect()
}

// ## UNMAPPED

#[test]
fn 引用が1つも無い_path_は_unmapped_で_関係が無いとは言わない() {
    let at = put(
        legacy(),
        "crates/koeru-app/ui/src/a.tsx",
        "export const A = 1;\n",
    );
    let mut md = request(&["crates/koeru-app/ui/src/a.tsx"]);
    md.format = Format::Markdown;
    let out = run(&md, &at, None);
    // UNMAPPED だけでは失敗にしない。 欠けとして出す。
    assert!(out.ok);
    assert!(
        out.text
            .contains("UNMAPPED:\n  crates/koeru-app/ui/src/a.tsx")
    );
    assert!(out.text.contains(
        "関係する設計契約を特定できていません。\n「関係する契約がない」という意味ではありません。"
    ));

    let v = json(&run(
        &request(&["crates/koeru-app/ui/src/a.tsx"]),
        &at,
        None,
    ));
    assert_eq!(v["ok"], true);
    assert_eq!(ctx(&v)["items"], Value::Array(Vec::new()));
    assert_eq!(ctx(&v)["gaps"]["unmapped"][0]["reason"], "no-citations");
    assert!(diagnostic_codes(&v).contains("context.unmapped"));
}

#[test]
fn 引用から拾った入口は_引用であることを残す() {
    let at = upper(put(
        legacy(),
        "docs/note.md",
        "一行目\n`DEC-fix-002` を引く。\n`DEC-fix-099` は実体が無い。\n",
    ));
    let v = json(&run(&request(&["docs/note.md"]), &at, None));
    let seed = &ctx(&v)["roots"][0]["seeds"][0];
    assert_eq!(seed["id"], up("DEC-fix-002"));
    assert_eq!(seed["via"], "citation");
    assert_eq!(seed["lines"], serde_json::json!([2]));
    // 実体の無い引用は黙って捨てず、欠けに残る。
    let broken = ctx(&v)["gaps"]["broken"].as_array().expect("配列");
    assert!(broken.iter().any(|b| b["kind"] == "unresolved-citation"
        && b["id"] == up("DEC-fix-099")
        && b["line"] == 3));

    let mut md = request(&["docs/note.md"]);
    md.format = Format::Markdown;
    let text = run(&md, &at, None).text;
    assert!(text.contains("引用から拾った入口であり、満たしている証明ではない"));
}

#[test]
fn 引いている_id_がどれも実体を持たない_path_も_unmapped() {
    let at = upper(put(
        legacy(),
        "docs/note.md",
        "`DEC-fix-099` だけを引く。\n",
    ));
    let v = json(&run(&request(&["docs/note.md"]), &at, None));
    assert_eq!(ctx(&v)["gaps"]["unmapped"][0]["reason"], "none-resolved");
    assert!(
        ctx(&v)["gaps"]["broken"]
            .as_array()
            .expect("配列")
            .iter()
            .any(|b| b["kind"] == "unresolved-citation")
    );
}

#[test]
fn 正本のファイルを指すと_そのファイルの記録が入口になる() {
    let v = json(&run(
        &request(&["meta/decisions/DEC-fix-002.toml"]),
        &legacy(),
        None,
    ));
    let seeds = ctx(&v)["roots"][0]["seeds"].as_array().expect("配列");
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0]["id"], "DEC-fix-002");
    assert_eq!(seeds[0]["via"], "owner");
}

#[test]
fn どの版にも無い_path_と_id_は不完全で終わる() {
    let at = legacy();
    for root in ["no/such/file.rs", "DEC-fix-099"] {
        let out = run(&request(&[root]), &at, None);
        assert!(!out.ok, "{root}");
        let v = json(&out);
        assert_eq!(v["ok"], false, "{root}");
        assert_eq!(ctx(&v)["complete"], false, "{root}");
        assert!(
            diagnostic_codes(&v)
                .iter()
                .any(|c| c.starts_with("context.")),
            "{root}"
        );
    }
    // 比較元にはあるなら、読む地点で消えただけ。 比較に出すので、失敗にしない。
    let from = with_target(legacy());
    let at = without(from.clone(), "meta/decisions/DEC-6CQ3NA.toml");
    let out = compared(&["DEC-6CQ3NA"], &at, &from);
    assert!(out.ok);
}

// ## 比較

#[test]
fn 消えた_edge_は旧い版の近傍から比較に出る() {
    let from = with_target(legacy());
    let at = put(
        with_target(legacy()),
        "meta/decisions/DEC-6CQ3NA.toml",
        &target_decision(""),
    );
    let v = json(&compared(&["DEC-6CQ3NA"], &at, &from));

    // 読む地点の Context に、外れた仮説は無い。 head で消えただけで「無かった」ことにはならない。
    let now = ids(&v, "items");
    assert!(!now.contains("HYP-91MTWX"));
    let removed = changes(&v, "edge-removed");
    assert!(removed.iter().any(|e| e["from"] == "DEC-6CQ3NA"
        && e["relation"] == "relies_on_hypotheses"
        && e["to"] == "HYP-91MTWX"));
    // 旧い版の出どころを持つ。
    let edge = removed
        .iter()
        .find(|e| e["relation"] == "relies_on_hypotheses")
        .expect("消えた edge");
    assert_eq!(edge["source"]["path"], "meta/decisions/DEC-6CQ3NA.toml");
    // 仮説は closure から外れただけで、記録は残っている。
    let gone = changes(&v, "removed");
    let hyp = gone
        .iter()
        .find(|c| c["id"] == "HYP-91MTWX")
        .expect("外れた仮説");
    assert_eq!(hyp["gone_from_graph"], false);
    // 判断の出る edge が変わった。
    assert!(
        changes(&v, "changed")
            .iter()
            .any(|c| c["id"] == "DEC-6CQ3NA" && c["how"] != "content")
    );
}

#[test]
fn 旧い版にしか無い_node_は消えた事実ごと残る() {
    let from = with_target(legacy());
    let at = without(from.clone(), "meta/requirements/REQ-5H8NQ2.toml");
    let v = json(&compared(&["DEC-6CQ3NA"], &at, &from));
    let removed = changes(&v, "removed");
    let req = removed
        .iter()
        .find(|c| c["id"] == "REQ-5H8NQ2")
        .expect("消えた要件");
    assert_eq!(req["gone_from_graph"], true);
    // 読む地点では、判断が指す先が無い。 解けない対応として出る。
    assert!(
        changes(&v, "unresolved-mapping")
            .iter()
            .any(|c| c["to"] == "REQ-5H8NQ2")
    );
    // 旧い版の出どころが md に出る。
    let mut r = request(&["DEC-6CQ3NA"]);
    r.from = Some("BASE".to_owned());
    r.format = Format::Markdown;
    let md = run(&r, &at, Some(&from)).text;
    assert!(md.contains("記録そのものが無くなった"));
    assert!(md.contains("meta/requirements/REQ-5H8NQ2.toml"));
}

#[test]
fn 置き換えられた判断は_後継が現在の_context_に出る() {
    let from = put(legacy(), "meta/decisions/DEC-fix-001.toml", DECISION_OLD);
    let from = without(from, "meta/decisions/DEC-fix-002.toml");
    let at = legacy();

    // 後継を持つ以前の判断を root にする。
    let v = json(&compared(&["DEC-fix-001"], &at, &from));
    let old = item(&v, "DEC-fix-001");
    assert_eq!(old["superseded_by"], serde_json::json!(["DEC-fix-002"]));
    let successor = item(&v, "DEC-fix-002");
    assert_eq!(successor["tier"], "required");
    assert!(rules_of(successor).contains("supersession"));
    // 後継は seed と同じに読まれる。 後継が置く要件も入る。
    assert!(rules_of(item(&v, "TR-fix-02")).contains("normative-ancestor"));

    let superseded = changes(&v, "superseded");
    assert!(
        superseded
            .iter()
            .any(|c| c["id"] == "DEC-fix-001" && c["by"] == serde_json::json!(["DEC-fix-002"]))
    );
    assert!(
        changes(&v, "added")
            .iter()
            .any(|c| c["id"] == "DEC-fix-002" && c["new_in_graph"] == true)
    );

    // 比較元が無くても、後継は現在の Context に出る。
    let v = json(&run(&request(&["DEC-fix-001"]), &at, None));
    assert!(rules_of(item(&v, "DEC-fix-002")).contains("supersession"));
    let mut md = request(&["DEC-fix-001"]);
    md.format = Format::Markdown;
    assert!(
        run(&md, &at, None)
            .text
            .contains("現在有効なのは `DEC-fix-002`")
    );
}

#[test]
fn 反証する調査は新しい反証として出て_支持と同じ面に並ぶ() {
    let from = with_target(legacy());
    let at = put(
        put(
            with_target(legacy()),
            "meta/hypotheses/HYP-91MTWX.toml",
            &hypothesis("'EVID-2V8JRF'", "音源一覧"),
        ),
        "meta/evidence/EVID-2V8JRF.toml",
        CONTRADICTING_EVIDENCE,
    );
    let v = json(&compared(&["DEC-6CQ3NA"], &at, &from));

    let fresh = changes(&v, "newly-contradicted");
    assert_eq!(fresh.len(), 1);
    assert_eq!(fresh[0]["hypothesis"], "HYP-91MTWX");
    assert_eq!(fresh[0]["evidence"], serde_json::json!(["EVID-2V8JRF"]));

    // 反証は必須の closure。 支持と同じ仮説の項目にまとまる。
    let evidence = item(&v, "EVID-2V8JRF");
    assert_eq!(evidence["tier"], "required");
    assert!(rules_of(evidence).contains("contradiction-closure"));
    assert!(rules_of(item(&v, "EVID-fix-001")).contains("empirical-support"));
    let h = &ctx(&v)["hypotheses"][0];
    assert_eq!(h["supporting"], serde_json::json!(["EVID-fix-001"]));
    assert_eq!(h["contradicting"], serde_json::json!(["EVID-2V8JRF"]));

    let mut r = request(&["DEC-6CQ3NA"]);
    r.from = Some("BASE".to_owned());
    r.format = Format::Markdown;
    let md = run(&r, &at, Some(&from)).text;
    assert!(md.contains("`HYP-91MTWX` に新しい反証: `EVID-2V8JRF`"));

    // すでにあった反証は、新しいものとして出さない。
    let both = compared(&["DEC-6CQ3NA"], &at, &at);
    assert!(changes(&json(&both), "newly-contradicted").is_empty());
}

#[test]
fn 反証が無い仮説に比較元が無ければ_新しくとは言わない() {
    let at = put(
        put(
            with_target(legacy()),
            "meta/hypotheses/HYP-91MTWX.toml",
            &hypothesis("'EVID-2V8JRF'", "音源一覧"),
        ),
        "meta/evidence/EVID-2V8JRF.toml",
        CONTRADICTING_EVIDENCE,
    );
    let mut r = request(&["DEC-6CQ3NA"]);
    r.format = Format::Markdown;
    let md = run(&r, &at, None).text;
    assert!(md.contains("比較元（`--from`）が無いので、「新しく」は判定できない"));
    assert!(md.contains("`HYP-91MTWX` ← `EVID-2V8JRF`"));
    assert!(json(&run(&request(&["DEC-6CQ3NA"]), &at, None))["context"]["changes"].is_null());
}

#[test]
fn 仮説が挙げる調査が無ければ_欠けとして残す() {
    let at = put(
        with_target(legacy()),
        "meta/hypotheses/HYP-91MTWX.toml",
        &hypothesis("'EVID-2V8JRF'", "音源一覧"),
    );
    let v = json(&run(&request(&["DEC-6CQ3NA"]), &at, None));
    assert!(
        ctx(&v)["gaps"]["broken"]
            .as_array()
            .expect("配列")
            .iter()
            .any(|b| b["kind"] == "missing-evidence" && b["evidence"] == "EVID-2V8JRF")
    );
}

#[test]
fn 範囲の変更と問いの開閉が出る() {
    let from = put(
        with_target(legacy()),
        "meta/hypotheses/HYP-91MTWX.toml",
        &hypothesis("", "音源一覧"),
    );
    let closed = QUESTION_OPEN.replace("status = 'open'", "status = 'resolved'");
    let from = put(from, "meta/questions/Q-fix-002.toml", &closed);
    let at = put(
        with_target(legacy()),
        "meta/hypotheses/HYP-91MTWX.toml",
        &hypothesis("", "音源一覧と収録中の画面"),
    );
    // 読む地点では未解決。 判断が `affects_questions` で指す。
    let v = json(&compared(&["DEC-fix-002", "DEC-6CQ3NA"], &at, &from));
    assert_eq!(
        changes(&v, "scope-changed")
            .iter()
            .map(|c| c["hypothesis"].clone())
            .collect::<Vec<_>>(),
        vec![Value::from("HYP-91MTWX")]
    );
    assert!(
        changes(&v, "question-opened")
            .iter()
            .any(|c| c["id"] == "Q-fix-002")
    );

    // 逆向きなら、閉じた。
    let v = json(&compared(&["DEC-fix-002"], &from, &at));
    assert!(
        changes(&v, "question-closed")
            .iter()
            .any(|c| c["id"] == "Q-fix-002")
    );
}

#[test]
fn legacy_と目標の形が混ざった版も比べられる() {
    let from = legacy();
    let at = with_target(legacy());
    let v = json(&compared(&["TR-fix-02", "DEC-6CQ3NA"], &at, &from));
    assert_eq!(v["ok"], true);
    let kinds: BTreeSet<&str> = ctx(&v)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .filter_map(|i| i["kind"].as_str())
        .collect();
    assert!(kinds.contains("requirement"));
    assert!(kinds.contains("decision"));
    assert!(kinds.contains("hypothesis"));
    // 目標の形の要件は、本文を持つ。
    assert_eq!(
        item(&v, "REQ-5H8NQ2")["facts"]["statement"],
        "現在可能な創作行為を利用者が認識できなければならない"
    );
    // 比較元に無かった root は、対応が変わったと出る。
    assert!(
        changes(&v, "unresolved-mapping")
            .iter()
            .any(|c| c["kind"] == "mapping-changed" && c["root"] == "DEC-6CQ3NA")
    );
    assert!(changes(&v, "added").iter().any(|c| c["id"] == "HYP-91MTWX"));
}

// ## current を省略しない

#[test]
fn from_があっても_current_state_を省略しない() {
    let at = legacy();
    // 変化が1つも無くても、current は出る。
    let v = json(&compared(&["DEC-fix-002"], &at, &at));
    assert!(ids(&v, "items").contains("DEC-fix-002"));
    assert!(!ctx(&v)["changes"].is_null());
    assert!(changes(&v, "added").is_empty());

    let mut r = request(&["DEC-fix-002"]);
    r.from = Some("BASE".to_owned());
    r.format = Format::Markdown;
    let md = run(&r, &at, Some(&at)).text;
    let cards = md.find("## 要約").expect("要約");
    let details = md.find("### 取得した項目").expect("取得した項目");
    let changes = md.find("## 変化").expect("変化");
    assert!(cards < details && details < changes);
    assert!(md.contains("`DEC-fix-002`（判断）"));
}

// ## closure の規則

#[test]
fn 判断の_closure_は規則のとおりに入る() {
    let v = json(&run(
        &request(&["DEC-fix-002"]),
        &with_target(legacy()),
        None,
    ));
    // 制約・要件の連なり・未解決の問いとリリース対象・後継の前は周辺。
    let tier = |id: &str| item(&v, id)["tier"].clone();
    assert_eq!(tier("DEC-fix-002"), "required");
    assert_eq!(tier("TR-fix-02"), "required");
    assert!(rules_of(item(&v, "TR-fix-02")).contains("normative-ancestor"));
    // `depends_on` の連なりも辿る。
    assert_eq!(tier("TR-fix-01"), "required");
    assert_eq!(tier("Q-fix-002"), "required");
    assert!(rules_of(item(&v, "Q-fix-002")).contains("open-uncertainty"));
    assert_eq!(tier("PROFILE-fix-1"), "required");
    assert!(rules_of(item(&v, "PROFILE-fix-1")).contains("blocked-release"));
    // 形式 contract は、一部だけを形式にしている edge で入る。
    assert_eq!(tier("REQ-fix-001"), "required");
    let edges = ctx(&v)["edges"].as_array().expect("配列");
    assert!(
        edges
            .iter()
            .any(|e| e["relation"] == "formalizes" && e["partial"] == true)
    );
    // 周辺は seed からの1歩。 参照した調査・閉じた問い・置き換えた以前の判断。
    for id in ["EVID-fix-001", "Q-fix-001", "DEC-fix-001"] {
        assert!(
            item(&v, id)["tier"] == "peripheral",
            "{id} は周辺であるはず"
        );
    }
    // 誰にも結びつかない要件は入らない。
    assert!(!ids(&v, "items").contains("TR-fix-03"));
}

#[test]
fn 形式_contract_は一部だけを形式にしていると出る() {
    let mut r = request(&["TR-fix-01"]);
    r.format = Format::Markdown;
    let md = run(&r, &legacy(), None).text;
    assert!(md.contains("形式 contract は要件の一部だけを形式にしている。 完全な被覆ではない"));
    assert!(md.contains("`REQ-fix-001` → `TR-fix-01`"));
}

#[test]
fn 他のファイルが引く_id_は_closure_を広げない() {
    // `mentions` は引用であって依存ではない。 別のファイルが引いている ID は、
    // root のファイルが引かなければ入口にならない。
    let at = upper(put(
        put(legacy(), "docs/a.md", "`DEC-fix-002` を引く。\n"),
        "docs/b.md",
        "`DEC-fix-003` と `DEC-fix-002` を引く。\n",
    ));
    let v = json(&run(&request(&["docs/a.md"]), &at, None));
    let seeds = ctx(&v)["roots"][0]["seeds"].as_array().expect("配列");
    assert_eq!(seeds.len(), 1);
    assert_eq!(seeds[0]["id"], up("DEC-fix-002"));
    // 入口は1つ。 b.md だけが引く判断は、どの項目にも入らない。
    let roots: Vec<&Value> = ctx(&v)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .filter(|i| rules_of(i).contains("root"))
        .collect();
    assert_eq!(roots.len(), 1);
    assert!(!ids(&v, "items").contains(&up("DEC-fix-003")));
}

#[test]
fn 規則の表は_mentions_を辿らず_全部の_relation_の扱いが決まっている() {
    assert!(RULES.iter().all(|r| r.relation != "mentions"));
    assert!(NOT_TRAVERSED.iter().any(|(name, _)| *name == "mentions"));
    // どの規則も、D00 に登録された relation を引く。 打ち間違いは、辿らないだけで気づけない。
    for rule in RULES {
        assert!(
            crate::knowledge::design::relation(rule.relation).is_some(),
            "{} は登録されていない relation",
            rule.relation
        );
    }
    // canonical な relation は、辿るか、辿らない理由を持つ。 足した relation の扱いを決め忘れない。
    for spec in RELATIONS
        .iter()
        .filter(|r| r.standing == Standing::Canonical)
    {
        let accounted =
            is_ruled(spec.name) || NOT_TRAVERSED.iter().any(|(name, _)| *name == spec.name);
        assert!(
            accounted,
            "{} を辿るか辿らないかが決まっていない",
            spec.name
        );
    }
    // 周辺の重さは必須の後ろ。
    assert!(Tier::Required < Tier::Peripheral(0));
}

#[test]
fn 循環があっても止まる() {
    let cycle = "schema = 'requirement-set'
domain = 'fix'
prefix = 'TR-fix'

[[requirement]]
id = 'TR-fix-01'
title = '1'
confidence = 'Fact'
statement = '本文'
depends_on = ['TR-fix-02']

[[requirement]]
id = 'TR-fix-02'
title = '2'
confidence = 'Fact'
statement = '本文'
depends_on = ['TR-fix-01']
";
    let d1 = "schema = 'decision'
id = 'DEC-fix-001'
title = '1'
status = 'accepted'
owner = 'x'
options = ['a']
selected = 'a'
rationale = 'r'
review_triggers = ['t']
supersedes = ['DEC-fix-002']
affects_decisions = ['DEC-fix-002']
establishes_requirements = ['TR-fix-01']
";
    let d2 = d1
        .replace("DEC-fix-001", "DEC-TMP")
        .replace("DEC-fix-002", "DEC-fix-001")
        .replace("DEC-TMP", "DEC-fix-002")
        .replace("TR-fix-01", "TR-fix-02");
    let at: Files = [
        ("meta/requirements/fix.toml", cycle),
        ("meta/decisions/DEC-fix-001.toml", d1),
        ("meta/decisions/DEC-fix-002.toml", d2.as_str()),
    ]
    .into_iter()
    .map(|(p, b)| (p.to_owned(), b.to_owned()))
    .collect();
    let v = json(&run(&request(&["DEC-fix-001"]), &at, None));
    let got = ids(&v, "items");
    for id in ["DEC-fix-001", "DEC-fix-002", "TR-fix-01", "TR-fix-02"] {
        assert!(got.contains(id), "{id}");
    }
}

#[test]
fn 要件を_root_にすると_置いた判断と依って立つ要件と_contract_が入る() {
    let v = json(&run(&request(&["TR-fix-02"]), &legacy(), None));
    let rules = |id: &str| rules_of(item(&v, id));
    assert!(rules("TR-fix-02").contains("root"));
    // 要件を置いた判断。
    assert!(rules("DEC-fix-002").contains("normative-origin"));
    // 依って立つ要件と、その要件を一部形式にしている contract。
    assert!(rules("TR-fix-01").contains("normative-ancestor"));
    assert!(rules("REQ-fix-001").contains("artifact-closure"));
}

#[test]
fn 問いを_root_にすると_閉じる判断と塞ぐリリース対象が入り_決めることの候補に挙がる() {
    let at = legacy();
    let v = json(&run(&request(&["Q-fix-002"]), &at, None));
    assert!(rules_of(item(&v, "PROFILE-fix-1")).contains("blocked-release"));
    let mut md = request(&["Q-fix-002"]);
    md.format = Format::Markdown;
    let text = run(&md, &at, None).text;
    assert!(text.contains("候補であって、決めることそのものではない"));
    assert!(
        text.contains(
            "- `Q-fix-002`（問い） 未解決の問い — 塞いでいるリリース対象: `PROFILE-fix-1`"
        )
    );

    // 閉じた問いは、閉じた判断を必須で引く。
    let v = json(&run(&request(&["Q-fix-001"]), &at, None));
    assert!(rules_of(item(&v, "DEC-fix-002")).contains("open-uncertainty"));
}

#[test]
fn 仮説を_root_にすると_支持と反証と前提にする判断が出る() {
    let at = put(
        put(
            with_target(legacy()),
            "meta/hypotheses/HYP-91MTWX.toml",
            &hypothesis("'EVID-2V8JRF'", "音源一覧"),
        ),
        "meta/evidence/EVID-2V8JRF.toml",
        CONTRADICTING_EVIDENCE,
    );
    let v = json(&run(&request(&["HYP-91MTWX"]), &at, None));
    assert!(rules_of(item(&v, "EVID-2V8JRF")).contains("contradiction-closure"));
    assert!(rules_of(item(&v, "EVID-fix-001")).contains("empirical-support"));
    // この仮説を前提にする判断は周辺。
    let dependent = item(&v, "DEC-6CQ3NA");
    assert_eq!(dependent["tier"], "peripheral");
    assert!(rules_of(dependent).contains("hypothesis-dependents"));
}

// ## 予算

#[test]
fn 予算は周辺だけを削り_削ったものを残す() {
    let at = with_target(legacy());
    let full = json(&run(&request(&["DEC-fix-002"]), &at, None));
    let required: BTreeSet<String> = ctx(&full)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .filter(|i| i["tier"] == "required")
        .map(|i| i["id"].as_str().expect("id").to_owned())
        .collect();
    let peripheral: BTreeSet<String> = ctx(&full)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .filter(|i| i["tier"] == "peripheral")
        .map(|i| i["id"].as_str().expect("id").to_owned())
        .collect();
    assert!(peripheral.len() >= 3, "{peripheral:?}");

    let mut r = request(&["DEC-fix-002"]);
    r.budget = Some(required.len() + 1);
    let v = json(&run(&r, &at, None));
    assert_eq!(v["ok"], true);
    // 必須は1件も削らない。 周辺は1件だけ残り、残りは削ったと数と内訳で出る。
    let kept: BTreeSet<String> = ids(&v, "items");
    assert!(required.is_subset(&kept));
    assert_eq!(kept.len(), required.len() + 1);
    let budget = &ctx(&v)["budget"];
    assert_eq!(budget["kept"], 1);
    assert_eq!(budget["exceeded"], false);
    let omitted: BTreeSet<String> = budget["omitted"]
        .as_array()
        .expect("配列")
        .iter()
        .map(|o| o["id"].as_str().expect("id").to_owned())
        .collect();
    assert_eq!(omitted.len(), peripheral.len() - 1);
    assert!(omitted.is_disjoint(&kept));
    assert_eq!(
        omitted.union(&kept).cloned().collect::<BTreeSet<_>>(),
        required
            .union(&peripheral)
            .cloned()
            .collect::<BTreeSet<_>>()
    );
    assert!(diagnostic_codes(&v).contains("context.omitted-by-budget"));
    // 削った周辺に付く edge も、一緒に出さない。
    for edge in ctx(&v)["edges"].as_array().expect("配列") {
        let from = edge["from"].as_str().expect("from");
        let to = edge["to"].as_str().expect("to");
        assert!(
            !(omitted.contains(from) || omitted.contains(to)),
            "{from} → {to}"
        );
    }

    // md も、削ったことを言う。
    let mut md = r.clone();
    md.format = Format::Markdown;
    let text = run(&md, &at, None).text;
    assert!(text.contains("予算で削った周辺"));
}

#[test]
fn 必須だけで予算を超えるなら_削らずに不完全で終わる() {
    let at = with_target(legacy());
    let full = json(&run(&request(&["DEC-fix-002"]), &at, None));
    let required = ctx(&full)["items"]
        .as_array()
        .expect("配列")
        .iter()
        .filter(|i| i["tier"] == "required")
        .count();
    assert!(required >= 3);

    let mut r = request(&["DEC-fix-002"]);
    r.budget = Some(2);
    let out = run(&r, &at, None);
    assert!(!out.ok);
    let v = json(&out);
    assert_eq!(v["ok"], false);
    assert_eq!(ctx(&v)["complete"], false);
    assert!(diagnostic_codes(&v).contains("context.required-over-budget"));
    // 必須は削らない。 周辺は全部削る。
    let kept = ctx(&v)["items"].as_array().expect("配列");
    assert_eq!(kept.len(), required);
    assert!(kept.iter().all(|i| i["tier"] == "required"));
    assert_eq!(ctx(&v)["budget"]["exceeded"], true);

    let mut md = r.clone();
    md.format = Format::Markdown;
    let out = run(&md, &at, None);
    assert!(!out.ok);
    assert!(out.text.contains("不完全"));
    assert!(out.text.contains("root を狭めるか"));
}

// ## 出力

/// md の「取得した項目」にある ID。 節は次の `### ` か `## ` までで、`#### ` は節の中。
fn md_items(md: &str) -> BTreeSet<String> {
    let start = md.find("### 取得した項目").expect("取得した項目");
    let rest = &md[start + 4..];
    let end = ["\n### ", "\n## "]
        .iter()
        .filter_map(|m| rest.find(m))
        .min()
        .unwrap_or(rest.len());
    rest[..end]
        .lines()
        .filter_map(|l| l.strip_prefix("- `"))
        .filter_map(|l| l.split('`').next())
        .map(str::to_owned)
        .collect()
}

#[test]
fn md_と_json_は同じ集合を出す() {
    let at = with_target(legacy());
    let from = legacy();
    for budget in [None, Some(5), Some(8)] {
        let mut r = request(&["DEC-fix-002", "DEC-6CQ3NA", "TR-fix-01"]);
        r.from = Some("BASE".to_owned());
        r.budget = budget;
        let json_out = run(&r, &at, Some(&from));
        let mut m = r.clone();
        m.format = Format::Markdown;
        let md_out = run(&m, &at, Some(&from));
        assert_eq!(json_out.ok, md_out.ok);
        assert_eq!(
            md_items(&md_out.text),
            ids(&json(&json_out), "items"),
            "予算 {budget:?}"
        );
    }
}

#[test]
fn 機械では出せない項目は捏造せず_理由を書く() {
    let mut r = request(&["DEC-fix-002"]);
    r.format = Format::Markdown;
    let md = run(&r, &legacy(), None).text;
    assert!(md.contains("### 1. 今回決めること\n\n機械では出せない。"));
    for heading in [
        "### 2. 現在守る制約",
        "### 3. 現在案を選んだ理由",
        "### 4. 別案と、以前採らなかった理由",
        "### 5. 新しく弱まった前提",
        "### 6. 未解決の問い",
        "### 7. 見るべき artifact",
        "### 8. 欠けている Context",
    ] {
        assert!(md.contains(heading), "{heading}");
    }
    let v = json(&run(&request(&["DEC-fix-002"]), &legacy(), None));
    assert_eq!(ctx(&v)["not_machine_derivable"][0]["card"], 1);
}

#[test]
fn 別案は選んだ案を除いて出す() {
    let mut r = request(&["DEC-fix-002"]);
    r.format = Format::Markdown;
    let md = run(&r, &legacy(), None).text;
    assert!(md.contains("（選んだ案を除く）"));
    assert!(md.contains("  - 案B\n  - 案C"));
    assert!(!md.contains("  - 案A"));
}

#[test]
fn json_は封筒に_context_を足した形で_版と取得理由と出どころを持つ() {
    let mut r = request(&["DEC-fix-002"]);
    r.at = Some("HEAD".to_owned());
    let v = json(&run(&r, &legacy(), None));
    assert_eq!(v["command"], "context");
    assert_eq!(v["ok"], true);
    assert!(v["diagnostics"].is_array());
    assert_eq!(ctx(&v)["version"], 1);
    assert_eq!(ctx(&v)["at"]["kind"], "revision");
    assert_eq!(ctx(&v)["at"]["rev"], "HEAD");
    assert!(ctx(&v)["from"].is_null());

    let d = item(&v, "DEC-fix-002");
    assert_eq!(d["kind"], "decision");
    assert_eq!(d["origin"]["path"], "meta/decisions/DEC-fix-002.toml");
    assert!(d["origin"]["line"].is_number());
    assert!(
        d["fingerprint"]
            .as_str()
            .expect("fingerprint")
            .starts_with("fnv1a64:")
    );
    assert_eq!(d["reasons"][0]["rule"], "root");
    // 辿ってきた edge を持つ。
    let q = item(&v, "Q-fix-002");
    assert_eq!(q["reasons"][0]["via"]["relation"], "affects");

    // 指定が無ければ作業ツリー。
    let wt = json(&run(&request(&["DEC-fix-002"]), &legacy(), None));
    assert_eq!(ctx(&wt)["at"]["kind"], "working-tree");
}

#[test]
fn 出力は決定的() {
    let at = with_target(legacy());
    let from = legacy();
    for format in [Format::Json, Format::Markdown] {
        let mut r = request(&["TR-fix-02", "DEC-fix-002", "DEC-6CQ3NA"]);
        r.from = Some("BASE".to_owned());
        r.format = format;
        let a = run(&r, &at, Some(&from));
        let b = run(&r, &at, Some(&from));
        assert_eq!(a.text, b.text);
    }
}

// ## 引数

fn args(a: &[&str]) -> Vec<String> {
    a.iter().map(|s| (*s).to_owned()).collect()
}

#[test]
fn 引数は_at_と_from_だけで_base_と_to_は無い() {
    let ok = parse(&args(&[
        "--root",
        "DEC-PLT-025",
        "--root=docs/a.md",
        "--from",
        "origin/main",
        "--at=HEAD",
        "--format",
        "json",
        "--budget",
        "40",
    ]))
    .expect("読める");
    assert_eq!(ok.roots, vec!["DEC-PLT-025", "docs/a.md"]);
    assert_eq!(ok.from.as_deref(), Some("origin/main"));
    assert_eq!(ok.at.as_deref(), Some("HEAD"));
    assert_eq!(ok.format, Format::Json);
    assert_eq!(ok.budget, Some(40));
    // 既定は md、作業ツリー、比較なし。
    let default = parse(&args(&["--root", "DEC-PLT-025"])).expect("読める");
    assert_eq!(default.format, Format::Markdown);
    assert_eq!(
        (default.at, default.from, default.budget),
        (None, None, None)
    );

    for bad in [
        &["--root", "A", "--base", "x"][..],
        &["--root", "A", "--to", "x"][..],
        &["--root", "A", "--format", "human"][..],
        &["--root", "A", "--budget", "0"][..],
        &["--root", "A", "--budget", "many"][..],
        &["--root", "A", "--at"][..],
        &["--root", "A", "--view", "both"][..],
        &[][..],
    ] {
        assert!(parse(&args(bad)).is_err(), "{bad:?}");
    }
}

// ## 実在する meta

/// 実在する meta を読ませる。 落ちずに出て、件数の大きな変化に脆くならない形で見る。
#[test]
fn 実在する_meta_で落ちずに出る() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("リポジトリの根");
    let at = RepoView::working_tree(root);
    for format in [Format::Markdown, Format::Json] {
        let mut r = request(&["DEC-PLT-047"]);
        r.format = format;
        let out = execute(root, &r, &at, None);
        assert!(out.ok, "{}", out.text);
        match format {
            Format::Markdown => {
                assert!(out.text.contains("### 1. 今回決めること"));
                assert!(out.text.contains("### 8. 欠けている Context"));
                assert!(out.text.contains("`DEC-PLT-047`"));
            }
            Format::Json => {
                let v: Value = serde_json::from_str(&out.text).expect("JSON");
                assert!(ids(&v, "items").contains("DEC-PLT-047"));
                assert_eq!(item(&v, "DEC-PLT-047")["kind"], "decision");
            }
        }
    }
    // path の root も落ちない。
    let mut r = request(&["xtask/src/context/mod.rs"]);
    r.format = Format::Json;
    let out = execute(root, &r, &at, None);
    assert!(out.ok, "{}", out.text);
}
