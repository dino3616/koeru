//! 型つきの読みモデル（X03）。
//!
//! `load` が返す `Vec<Entry>` は、まだ `toml::Table` をそのまま持つ。 このモジュールは
//! それを「記録」の集まりに写し、ID・schema・出どころ・欄への型つきの口だけを外へ出す。
//! `toml::Table` / `toml::Value` そのものは公開の口に出さない。
//!
//! 1ファイル1記録（`decision` など）も、1ファイルの中の複数記録（`requirement-set` の
//! `[[requirement]]` など）も、同じ [`Record`] として持つ。 どちらだったかは
//! [`Provenance::index`] が言う——収集ファイルの中の何番目かで、1件1ファイルの記録なら無い。

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use super::fsl::fsl_sites_view;
use super::ids::Id;
use super::model::{Entry, list_of, str_of};
use crate::repo::RepoView;

/// 記録の出どころ。
///
/// `check-profile` は出どころを見ない。 X05 の graph 構築が、node / edge の
/// provenance を作るのに `path` / `line` を読む（`graph::build::provenance_of`）。
#[derive(Debug, Clone)]
pub(crate) struct Provenance {
    path: PathBuf,
    /// 収集ファイルの中の何番目か（0始まり）。1件1ファイルの記録なら無い。
    ///
    /// 今はまだ試験だけが読む。
    #[allow(dead_code)]
    index: Option<usize>,
    /// `id = '…'` の行番号。 安く取れる場合だけ持つ——1ファイルにつき読み直しは1回。
    line: Option<usize>,
}

impl Provenance {
    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// 今はまだ試験だけが呼ぶ。
    #[allow(dead_code)]
    pub(crate) fn index(&self) -> Option<usize> {
        self.index
    }

    pub(crate) fn line(&self) -> Option<usize> {
        self.line
    }
}

/// 入れ子の表を読む口。 `toml::Table` をそのまま渡さないための薄い包み。
///
/// `Record::tables` が返す。 今の消費者はまだ入れ子の表を読まないので、
/// ここも試験だけが引く。
#[allow(dead_code)]
#[derive(Debug, Clone, Copy)]
pub(crate) struct Fields<'a>(&'a toml::Table);

#[allow(dead_code)]
impl Fields<'_> {
    pub(crate) fn str(&self, key: &str) -> Option<&str> {
        str_of(self.0, key)
    }

    pub(crate) fn strs(&self, key: &str) -> Vec<String> {
        list_of(self.0, key)
    }

    pub(crate) fn int(&self, key: &str) -> Option<i64> {
        self.0.get(key).and_then(toml::Value::as_integer)
    }

    pub(crate) fn bool(&self, key: &str) -> Option<bool> {
        self.0.get(key).and_then(toml::Value::as_bool)
    }
}

/// 1件の記録。 1件1ファイルの entity も、収集ファイルの中の1件も、同じ形で持つ。
#[derive(Debug, Clone)]
pub(crate) struct Record {
    id: Option<Id>,
    schema: &'static str,
    provenance: Provenance,
    fields: toml::Table,
}

impl Record {
    /// 型つきの ID。 `id` が無い・ID の形をしていない記録では無い
    /// ——`load` の側がその欠けをすでに診断へ積んでいるので、ここでは落とさず素通りする。
    pub(crate) fn id(&self) -> Option<&Id> {
        self.id.as_ref()
    }

    pub(crate) fn str(&self, key: &str) -> Option<&str> {
        str_of(&self.fields, key)
    }

    pub(crate) fn strs(&self, key: &str) -> Vec<String> {
        list_of(&self.fields, key)
    }
}

impl Record {
    /// この記録の schema。 X05 の graph 構築が、`Registry` の node 種類・
    /// relation 規則をどの記録に適用するかを選ぶのに読む。
    pub(crate) fn schema(&self) -> &'static str {
        self.schema
    }

    /// この記録の出どころ。 X05 の graph 構築が node / edge の provenance を
    /// 作るのに読む。
    pub(crate) fn provenance(&self) -> &Provenance {
        &self.provenance
    }

    /// 今はまだ試験だけが呼ぶ。 数値の欄を読みたい消費者（D07a/D07b）が使う。
    #[allow(dead_code)]
    pub(crate) fn int(&self, key: &str) -> Option<i64> {
        self.fields.get(key).and_then(toml::Value::as_integer)
    }

    /// 今はまだ試験だけが呼ぶ。 論理値の欄を読みたい消費者（D07a/D07b）が使う。
    #[allow(dead_code)]
    pub(crate) fn bool(&self, key: &str) -> Option<bool> {
        self.fields.get(key).and_then(toml::Value::as_bool)
    }

    /// 入れ子の表の配列（`[[allocations]]` / `[[derived]]` のような entity_arrays）。
    ///
    /// 今はまだ試験だけが呼ぶ。 コレクションの由来を区別したい消費者（D07a/D07b）が使う。
    #[allow(dead_code)]
    pub(crate) fn tables(&self, key: &str) -> Vec<Fields<'_>> {
        self.fields
            .get(key)
            .and_then(toml::Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(toml::Value::as_table)
                    .map(Fields)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// 記録の欄を決定的に文字列化したもの。 `toml::Table` そのものは公開しない
    /// ——上の層へ表を漏らさないという境界（`knowledge` の purity）を保ったまま、
    /// X05 の graph diff が「内容が変わったか」を比べられるようにする。
    ///
    /// `toml::Table` は既定で `BTreeMap` 実装（`preserve_order` feature を
    /// 有効にしていない）なので、キーの並びはすでに決定的。
    pub(crate) fn canonical_fields(&self) -> String {
        toml::to_string(&self.fields).unwrap_or_default()
    }
}

/// meta と FSL を読んだ、型つきの読みモデル。
///
/// `load` が返す `Vec<Entry>` から作る。 読み込みの段の診断（schema なし・必須項目欠けなど）は
/// `load` がすでに `Report` へ積んでいるので、ここでは二重に出さない——出すのは
/// ここでしか分からないもの（重複 ID）だけ。
#[derive(Debug, Default)]
pub(crate) struct KnowledgeSnapshot {
    records: Vec<Record>,
    // ID からの索引は `get` / `contains_id` からしか読まない。 どちらも
    // 今はまだ試験だけが呼ぶ（下の `impl` を見る）。
    #[allow(dead_code)]
    by_id: BTreeMap<Id, usize>,
    // FSL の索引は `fsl_ids` / `fsl_site` が読む（X05 の graph 構築、FSL の ID を
    // node にする registry のとき）。
    fsl_sites: BTreeMap<Id, String>,
}

impl KnowledgeSnapshot {
    /// meta だけから作る。 FSL の索引は空のまま——要る消費者は [`Self::with_fsl`] を足す。
    pub(crate) fn from_entries(entries: &[Entry]) -> (Self, Vec<String>) {
        let mut records = Vec::new();
        let mut by_id: BTreeMap<Id, usize> = BTreeMap::new();
        let mut dups = Vec::new();

        for e in entries {
            let lines = id_lines(&e.text);

            if e.shape.entity.is_some() {
                push_record(
                    &mut records,
                    &mut by_id,
                    &mut dups,
                    e,
                    e.table.clone(),
                    None,
                    &lines,
                );
            }
            for (index, item) in e.items().into_iter().enumerate() {
                push_record(
                    &mut records,
                    &mut by_id,
                    &mut dups,
                    e,
                    item,
                    Some(index),
                    &lines,
                );
            }
        }

        (
            Self {
                records,
                by_id,
                fsl_sites: BTreeMap::new(),
            },
            dups,
        )
    }

    /// 指定した schema の記録だけ。 `with_schema` と同じ絞り方。
    pub(crate) fn by_schema<'a>(&'a self, schema: &'a str) -> impl Iterator<Item = &'a Record> {
        self.records.iter().filter(move |r| r.schema == schema)
    }
}

// `check-profile` より先の消費者が使う口。 FSL の索引・全件反復・ID の有無だけを
// 見る絞り込みは、schema を1つ選ぶ `by_schema` では書けない。
impl KnowledgeSnapshot {
    /// FSL の ID と出どころ（作業ツリーの `specs/` の原文から拾ったもの）も
    /// 合わせて持つ。 版を選びたい消費者（X05 の graph 構築）は
    /// [`Self::with_fsl_view`] を呼ぶ。
    ///
    /// 今はまだ試験だけが呼ぶ。
    #[allow(dead_code)]
    pub(crate) fn with_fsl(self, root: &Path) -> Self {
        self.with_fsl_view(&RepoView::working_tree(root))
    }

    /// `view` の版から、FSL の ID と出どころも合わせて持つ。
    pub(crate) fn with_fsl_view(mut self, view: &RepoView) -> Self {
        self.fsl_sites = fsl_sites_view(view)
            .into_iter()
            .filter_map(|(id, site)| id.parse::<Id>().ok().map(|id| (id, site)))
            .collect();
        self
    }

    /// meta と FSL の両方を持つ snapshot を一度に作る。 作業ツリー限定。
    ///
    /// 今はまだ試験だけが呼ぶ。
    #[allow(dead_code)]
    pub(crate) fn build(root: &Path, entries: &[Entry]) -> (Self, Vec<String>) {
        let (snapshot, dups) = Self::from_entries(entries);
        (snapshot.with_fsl(root), dups)
    }

    /// 全件。 X05 のグラフ構築が node を作るのに読む。
    pub(crate) fn records(&self) -> impl Iterator<Item = &Record> {
        self.records.iter()
    }

    /// 今はまだ試験だけが呼ぶ。
    #[allow(dead_code)]
    pub(crate) fn get(&self, id: &Id) -> Option<&Record> {
        self.by_id.get(id).map(|&i| &self.records[i])
    }

    /// 今はまだ試験だけが呼ぶ。
    #[allow(dead_code)]
    pub(crate) fn contains_id(&self, id: &Id) -> bool {
        self.by_id.contains_key(id)
    }

    /// FSL の ID の一覧。 X05 のグラフ構築が、FSL の ID も node にする
    /// registry のときに読む。
    pub(crate) fn fsl_ids(&self) -> impl Iterator<Item = &Id> {
        self.fsl_sites.keys()
    }

    /// FSL の ID が書かれている場所。
    pub(crate) fn fsl_site(&self, id: &Id) -> Option<&str> {
        self.fsl_sites.get(id).map(String::as_str)
    }
}

/// 1件の記録を組み立てて積む。 重複した ID は上書きしつつ、診断へ積む
/// ——`requirements` が `requirement-set` の中だけで行っていたのと同じ扱いを、
/// 全 schema へ広げたもの。
fn push_record(
    records: &mut Vec<Record>,
    by_id: &mut BTreeMap<Id, usize>,
    dups: &mut Vec<String>,
    e: &Entry,
    fields: toml::Table,
    index: Option<usize>,
    lines: &BTreeMap<String, usize>,
) {
    let id = str_of(&fields, "id").and_then(|s| s.parse::<Id>().ok());
    let line = id.as_ref().and_then(|id| lines.get(id.as_str()).copied());
    let record = Record {
        id: id.clone(),
        schema: e.shape.schema,
        provenance: Provenance {
            path: e.path.clone(),
            index,
            line,
        },
        fields,
    };
    if let Some(id) = id {
        let at = records.len();
        if by_id.insert(id.clone(), at).is_some() {
            dups.push(format!("{}: id `{id}` が重複している", e.path.display()));
        }
    }
    records.push(record);
}

/// ファイルの中の `id = '…'` / `id = "…"` を、行番号に引けるようにする。
///
/// `toml::Table` は行番号を持たないので、`Entry::text`（`load_view` がすでに
/// 読んだ本文）から数え直す。 ここで改めてファイルを読み直さない
/// ——版から読んだ `Entry` では、作業ツリーを読み直すと版と合わない行番号になる。
fn id_lines(text: &str) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for (n, line) in text.lines().enumerate() {
        let Some(rest) = line.trim().strip_prefix("id") else {
            continue;
        };
        let Some(rest) = rest.trim_start().strip_prefix('=') else {
            continue;
        };
        let id = rest.trim().trim_matches(['\'', '"']);
        out.entry(id.to_owned()).or_insert(n + 1);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::atomic::{AtomicU64, Ordering};

    use super::*;
    use crate::diagnostic::Report;
    use crate::knowledge::load;

    /// 試験ごとに使い捨てる一時ディレクトリ。 `Drop` で消す——`tempfile` は引かず、
    /// `checks::schema` の試験と同じ形（`std::env::temp_dir()` の下）にする。
    struct TempMeta(PathBuf);

    impl Drop for TempMeta {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).ok();
        }
    }

    /// 一時ディレクトリに最小の meta を組む。 `load` を経由して `Entry` を作ることで、
    /// 読み込みの段の診断を二重に書かずに済ませる。
    fn fixture(files: &[(&str, &str)]) -> (TempMeta, Vec<Entry>, Report) {
        // 試験は並列に走るので、プロセス ID だけでは名前が衝突する。
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let root =
            std::env::temp_dir().join(format!("koeru-xtask-knowledge-{}-{n}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        for (rel, body) in files {
            let p = root.join("meta").join(rel);
            fs::create_dir_all(p.parent().expect("親がある")).expect("作れる");
            fs::write(p, body).expect("書ける");
        }
        let mut rep = Report::default();
        let entries = load(&root, &mut rep);
        (TempMeta(root), entries, rep)
    }

    const DECISION: &str = r#"
schema = 'decision'
id = 'DEC-fix-001'
title = '見本の判断'
status = 'accepted'
owner = 'someone'
options = ['a', 'b']
selected = 'a'
rationale = '…'
review_triggers = ['…']
"#;

    #[test]
    fn 単独ファイルの記録を持てる() {
        let (_dir, entries, rep) = fixture(&[("decisions/DEC-fix-001.toml", DECISION)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&entries);
        assert!(dups.is_empty());
        let id: Id = "DEC-fix-001".parse().unwrap();
        let rec = snapshot.get(&id).expect("引ける");
        assert_eq!(rec.schema(), "decision");
        assert_eq!(rec.str("title"), Some("見本の判断"));
        assert_eq!(rec.provenance().index(), None);
    }

    const REQUIREMENT_SET: &str = r#"
schema = 'requirement-set'

[[requirement]]
id = 'TR-fix-01'
title = '1件目'
confidence = 'Fact'
statement = '…'

[[requirement]]
id = 'TR-fix-02'
title = '2件目'
confidence = 'Fact'
statement = '…'

[[requirement]]
id = 'TR-fix-03'
title = '3件目'
confidence = 'Fact'
statement = '…'
"#;

    /// 収集ファイル（`requirement-set`）は複数の記録になり、`index` が項目の位置を持つ。
    ///
    /// ID の接頭辞を小文字にしているのは、このテストが xtask のソースの中にあるままで
    /// `check-references` の走査に入っても、実在しない ID として引っかからないようにするため
    /// （X00 の fixture が採った形と同じ）。
    #[test]
    fn 集約ファイルは複数の記録になる() {
        let (_dir, entries, rep) = fixture(&[("requirements/fix.toml", REQUIREMENT_SET)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&entries);
        assert!(dups.is_empty());
        let items: Vec<&Record> = snapshot.by_schema("requirement-set").collect();
        assert_eq!(items.len(), 3);
        let indices: Vec<Option<usize>> = items.iter().map(|r| r.provenance().index()).collect();
        assert_eq!(indices, [Some(0), Some(1), Some(2)]);
        let id: Id = "TR-fix-02".parse().unwrap();
        assert_eq!(
            snapshot.get(&id).and_then(|r| r.str("title")),
            Some("2件目")
        );
    }

    /// 空の収集ファイルは、`load` がすでに診断へ積んでいる。 snapshot は0件のまま
    /// 崩れずに済ませる——二重に落とさず、素通りするだけ。
    #[test]
    fn 空の収集ファイルは0件のまま素通りする() {
        let (_dir, entries, rep) =
            fixture(&[("requirements/empty.toml", "schema = 'requirement-set'\n")]);
        assert!(rep.has_errors(), "load が空の収集ファイルを落とすはず");
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&entries);
        assert!(dups.is_empty());
        assert_eq!(snapshot.by_schema("requirement-set").count(), 0);
    }

    /// 同じ ID を2つの項目が名乗ったら、診断に積む。 索引には後勝ちで残る
    /// ——`requirements` が `requirement-set` の中だけで行っていた扱いを、全 schema へ広げたもの。
    ///
    /// 収集ファイルの中で重複させる。 entity（1件1ファイル）で試そうとすると、
    /// ファイル名を id と一致させる `load` の検査に先に引っかかる——同じ id を
    /// 名乗る2つのファイルは、必ずどちらかのファイル名が id と合わなくなるため。
    #[test]
    fn 重複した_id_は診断になる() {
        const DUPLICATE: &str = r#"
schema = 'requirement-set'

[[requirement]]
id = 'TR-fix-01'
title = '1件目'
confidence = 'Fact'
statement = '…'

[[requirement]]
id = 'TR-fix-01'
title = '重複'
confidence = 'Fact'
statement = '…'
"#;
        let (_dir, entries, rep) = fixture(&[("requirements/dup.toml", DUPLICATE)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, dups) = KnowledgeSnapshot::from_entries(&entries);
        assert_eq!(dups.len(), 1);
        assert!(dups[0].contains("TR-fix-01"));
        assert!(dups[0].contains("重複している"));
        let id: Id = "TR-fix-01".parse().unwrap();
        assert!(snapshot.get(&id).is_some());
    }

    /// `id` の行番号を安く引ける。
    #[test]
    fn 行番号を引ける() {
        let (_dir, entries, _rep) = fixture(&[("decisions/DEC-fix-001.toml", DECISION)]);
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        let id: Id = "DEC-fix-001".parse().unwrap();
        let rec = snapshot.get(&id).expect("引ける");
        // 1行目は空行（`r#"` の直後の改行）なので、`id = '…'` は3行目にある。
        assert_eq!(rec.provenance().line(), Some(3));
    }

    /// 入れ子の表は `toml::Table` を渡さず、`Fields` 経由でだけ読める。
    #[test]
    fn 入れ子の表を_fields_で読む() {
        const BUDGET: &str = r#"
schema = 'budget'
id = 'BUDGET-fix-001'
title = '見本の予算'
limit = 100
unit = 'MiB'
scope = 'テスト'

[[allocations]]
kind = 'step'
value = 40
mode = 'idle'
measured = true
tags = ['a', 'b']
"#;
        let (_dir, entries, rep) = fixture(&[("budgets/BUDGET-fix-001.toml", BUDGET)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        let id: Id = "BUDGET-fix-001".parse().unwrap();
        let rec = snapshot.get(&id).expect("引ける");
        assert_eq!(rec.int("limit"), Some(100));
        let allocations = rec.tables("allocations");
        assert_eq!(allocations.len(), 1);
        assert_eq!(allocations[0].str("mode"), Some("idle"));
        assert_eq!(allocations[0].int("value"), Some(40));
        assert_eq!(allocations[0].bool("measured"), Some(true));
        assert_eq!(allocations[0].strs("tags"), ["a", "b"]);
    }

    /// 出どころのパスも引ける。
    #[test]
    fn 出どころのパスを引ける() {
        let (dir, entries, rep) = fixture(&[("decisions/DEC-fix-001.toml", DECISION)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        let id: Id = "DEC-fix-001".parse().unwrap();
        let rec = snapshot.get(&id).expect("引ける");
        assert_eq!(
            rec.provenance().path(),
            dir.0.join("meta/decisions/DEC-fix-001.toml")
        );
    }

    /// `records()` は全部、`contains_id` は索引だけを見る。
    #[test]
    fn 全件と_id_の有無を引ける() {
        let (_dir, entries, rep) = fixture(&[("requirements/fix.toml", REQUIREMENT_SET)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        assert_eq!(snapshot.records().count(), 3);
        assert!(snapshot.contains_id(&"TR-fix-01".parse().unwrap()));
        assert!(!snapshot.contains_id(&"TR-fix-99".parse().unwrap()));
    }

    /// 論理値の欄。
    #[test]
    fn 論理値の欄を読む() {
        let with_bool = DECISION.replace(
            "review_triggers = ['…']",
            "review_triggers = ['…']\nreviewed = true",
        );
        let (_dir, entries, rep) = fixture(&[("decisions/DEC-fix-001.toml", &with_bool)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let (snapshot, _) = KnowledgeSnapshot::from_entries(&entries);
        let id: Id = "DEC-fix-001".parse().unwrap();
        let rec = snapshot.get(&id).expect("引ける");
        assert_eq!(rec.bool("reviewed"), Some(true));
        assert_eq!(rec.bool("no-such-key"), None);
    }

    /// FSL の ID と出どころも、同じ snapshot から引ける。
    #[test]
    fn fsl_の_id_も引ける() {
        let (dir, entries, rep) = fixture(&[("decisions/DEC-fix-001.toml", DECISION)]);
        assert!(!rep.has_errors(), "{:?}", rep.diagnostics());
        let fsl_path = dir.0.join("specs/example.fsl");
        fs::create_dir_all(fsl_path.parent().expect("親がある")).expect("作れる");
        fs::write(&fsl_path, "  acceptance AC-fix-001 \"見本\" {\n  }\n").expect("書ける");

        let (snapshot, _) = KnowledgeSnapshot::build(&dir.0, &entries);
        let id: Id = "AC-fix-001".parse().unwrap();
        assert!(snapshot.fsl_ids().any(|i| i == &id));
        let site = snapshot.fsl_site(&id).expect("出どころが引ける");
        assert!(site.contains("example.fsl:1"), "{site}");
    }
}
