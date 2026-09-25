//! canonical SDL と operation の検査（`DEC-PLT-035`, `DEC-PLT-040`）。
//!
//! `specs/application/` だけを読む。 Rust の実装も画面も組み立てずに、契約を契約として
//! 検査するため。 GraphQL の仕様どおりかは apollo-compiler が見て、ここはその上に
//! 仕様が言わない規則を足す。 規則はどれも判断記録か
//! `docs/reports/architecture/08-graphql-application-contract.md` に根がある。
//! このファイルの「§」はその文書の節。
//!
//! main と PR のあいだの後方互換の差分はここで見ない。 GraphQL Inspector の役目
//! （`DEC-PLT-040`）。 ここで捕まるのは、見本の operation が使っている欄を消した・
//! 型を変えたときに operation の検証が落ちる、という使われ方からの検出だけ。

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::process::ExitCode;

use apollo_compiler::ast::{DirectiveList, Type};
use apollo_compiler::diagnostic::ToCliReport;
use apollo_compiler::executable::{ExecutableDocument, Selection, SelectionSet};
use apollo_compiler::parser::{SourceMap, SourceSpan};
use apollo_compiler::schema::ExtendedType;
use apollo_compiler::validation::{DiagnosticList, Valid};
use apollo_compiler::{Name, Schema};

use crate::{Report, list_of, str_of};

/// 契約は領域ごとのファイルに分かれていて、この下の `*.graphql` すべてで1つになる。
const SCHEMA: &str = "specs/application/schema";
const OPERATIONS: &str = "specs/application/operations";
const CAPABILITIES: &str = "specs/application/capabilities.toml";

/// 古くなりうる入力と、そのとき返す結果。
///
/// 貸与は閉じれば死に、版は進み、操作の識別子は使い回されうる。 仕事は保持の期間を
/// 過ぎると消える。 どれも利用者の手で普通に起きることなので、`errors[]` ではなく
/// 結果として返す（`DEC-PLT-038`）。 これを取る root の欄は、対になる型を結果の union に持つ。
///
/// 識別子を一般に `ReferenceNotFound` と対にしない。 デバイスが消えたら `DeviceUnavailable`、
/// 鳴り終えた再生は `PlaybackAlreadyEnded` のように、欄ごとに言い方が違う。
const STALE: &[(&str, &str)] = &[
    ("ProjectLease", "ProjectLeaseExpired"),
    ("InputLeaseId", "InputLeaseEnded"),
    ("EditingSessionId", "EditingSessionEnded"),
    ("Revision", "RevisionConflict"),
    ("OperationId", "OperationIdReused"),
    ("EventCursor", "EventGap"),
    ("JobId", "ReferenceNotFound"),
];

/// パスを指す名前の語尾（`08-graphql-application-contract.md` の §12）。
///
/// host が選んだパスを契約へ流すと、それが何でも読める権限になる。 小文字にして比べる。
const PATH_SUFFIXES: &[&str] = &["path", "dir", "directory", "folder"];

/// 組み込みの scalar。 識別子の欄にこれを使わない（`DEC-RCL-017`）。
const PLAIN_SCALARS: &[&str] = &["String", "Int", "Float", "Boolean"];

/// 組み込みの `ID`。 どの欄にも使わない。 識別子は種類ごとの scalar にする（`DEC-PLT-042`）。
/// 1つでも `ID` にすると、別の種類の識別子を渡しても文書の検証が通る。
const BUILT_IN_ID: &str = "ID";

/// `@deprecated` の理由を書かなかったときに入る既定の文。
const DEFAULT_DEPRECATION: &str = "No longer supported";

/// `cargo xtask check-schema`
pub(crate) fn check_schema(root: &Path, mut rep: Report) -> ExitCode {
    let sources = match read_graphql(root, SCHEMA) {
        Ok(s) => s,
        Err(e) => {
            rep.error(e);
            return rep.finish("check-schema");
        }
    };
    let schema = match parse_schema(&sources) {
        Ok(s) => s,
        Err(errors) => {
            errors.into_iter().for_each(|e| rep.error(e));
            return rep.finish("check-schema");
        }
    };
    lint_schema(&schema).into_iter().for_each(|e| rep.error(e));

    let docs = match read_graphql(root, OPERATIONS) {
        Ok(d) => d,
        Err(e) => {
            rep.error(e);
            return rep.finish("check-schema");
        }
    };
    match parse_documents(&schema, &docs) {
        Ok(doc) => {
            lint_documents(&doc).into_iter().for_each(|e| rep.error(e));
            let count = |kind| {
                doc.operations
                    .iter()
                    .filter(|o| o.operation_type.name() == kind)
                    .count()
            };
            rep.note(format!(
                "operation {} 件（query {} / mutation {} / subscription {}）、fragment {} 件、文書 {} 本",
                doc.operations.len(),
                count("query"),
                count("mutation"),
                count("subscription"),
                doc.fragments.len(),
                docs.len()
            ));
            let unused = unexercised(&schema, &doc);
            if !unused.is_empty() {
                rep.note(format!(
                    "見本の operation が使っていない root の欄 {} 個: {}",
                    unused.len(),
                    unused.join(", ")
                ));
            }
        }
        Err(errors) => errors.into_iter().for_each(|e| rep.error(e)),
    }

    match fs::read_to_string(root.join(CAPABILITIES)).map(|t| t.parse::<toml::Table>()) {
        Ok(Ok(table)) => check_capabilities(&schema, &table)
            .into_iter()
            .for_each(|e| rep.error(e)),
        Ok(Err(e)) => rep.error(format!("{CAPABILITIES} を TOML として読めない: {e}")),
        Err(e) => rep.error(format!("{CAPABILITIES} を読めない: {e}")),
    }

    let (types, roots) = sizes(&schema);
    rep.note(format!(
        "型 {types} 個、root の欄 {roots} 個、SDL {} 本",
        sources.len()
    ));
    rep.finish("check-schema")
}

/// SDL のファイルをすべて1つの schema に組み、GraphQL の仕様どおりかを確かめる。
///
/// root の型は1つのファイルが定義し、ほかの領域は `extend type` で欄を足す。 extension が
/// 定義より前のファイルにあっても、apollo-compiler は定義を読んだ時点で付け直す。 どの
/// 定義も読んだファイルの場所を持つので、診断と規則の違反はファイルを名指す。
fn parse_schema(sources: &[(String, String)]) -> Result<Valid<Schema>, Vec<String>> {
    // 1本も無いまま組むと、組み込みの型だけの空の契約を検査して通る。
    if sources.is_empty() {
        return Err(vec![format!("{SCHEMA} に SDL のファイルが1本も無い")]);
    }
    let mut builder = Schema::builder();
    for (path, src) in sources {
        builder = builder.parse(src.as_str(), path);
    }
    builder
        .build()
        .map_err(|e| diagnostics(&e.errors))?
        .validate()
        .map_err(|e| diagnostics(&e.errors))
}

/// operation の文書をすべて1つにまとめて検証する。
///
/// fragment は部品の近くに置き、経路が operation に束ねる（`DEC-PLT-037`）。 文書を
/// 1本ずつ検証すると、別の文書の fragment を使ったものが全部落ちる。
fn parse_documents(
    schema: &Valid<Schema>,
    docs: &[(String, String)],
) -> Result<Valid<ExecutableDocument>, Vec<String>> {
    // 見本が1件も無いまま通すと、検査が何も見ていないことに誰も気づかない（§17）。
    if docs.is_empty() {
        return Err(vec![format!("{OPERATIONS} に operation の文書が1本も無い")]);
    }
    let mut errors = DiagnosticList::new(SourceMap::default());
    let mut builder = ExecutableDocument::builder(Some(schema), &mut errors);
    for (path, src) in docs {
        builder = builder.parse(src.as_str(), path);
    }
    let doc = builder.build();
    if !errors.is_empty() {
        return Err(diagnostics(&errors));
    }
    doc.validate(schema).map_err(|e| diagnostics(&e.errors))
}

/// `root` からの `dir` の下の `*.graphql` を、パスの順に。 パスは `root` からの相対で持ち、
/// 診断の場所になる。 順を決めておくのは、診断と数の並びを実行ごとに変えないため。
fn read_graphql(root: &Path, dir: &str) -> Result<Vec<(String, String)>, String> {
    let mut paths = Vec::new();
    let mut stack = vec![root.join(dir)];
    while let Some(dir) = stack.pop() {
        let rd = fs::read_dir(&dir).map_err(|e| format!("{} を読めない: {e}", dir.display()))?;
        for entry in rd.filter_map(Result::ok) {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "graphql") {
                paths.push(p);
            }
        }
    }
    paths.sort();
    paths
        .into_iter()
        .map(|p| {
            let rel = p.strip_prefix(root).unwrap_or(&p).display().to_string();
            fs::read_to_string(&p)
                .map(|src| (rel.clone(), src))
                .map_err(|e| format!("{rel} を読めない: {e}"))
        })
        .collect()
}

/// apollo-compiler の診断を、`パス:行:桁: 文` の1行ずつに。
fn diagnostics(list: &DiagnosticList) -> Vec<String> {
    list.iter()
        .map(|d| {
            let at = d.error.location().and_then(|span| {
                let file = d.sources.get(&span.file_id())?;
                let range = d.line_column_range()?;
                Some(format!(
                    "{}:{}:{}",
                    file.path().display(),
                    range.start.line,
                    range.start.column
                ))
            });
            match at {
                Some(at) => format!("{at}: {}", d.error),
                None => d.error.to_string(),
            }
        })
        .collect()
}

/// 定義の場所。 位置を持たないもの（組み込み）は空。
fn place(sources: &SourceMap, span: Option<SourceSpan>) -> String {
    span.and_then(|s| {
        let file = sources.get(&s.file_id())?;
        let range = s.line_column_range(sources)?;
        Some(format!(
            "{}:{}:{}: ",
            file.path().display(),
            range.start.line,
            range.start.column
        ))
    })
    .unwrap_or_default()
}

fn root_names(schema: &Schema) -> Vec<(&'static str, Name)> {
    let def = &schema.schema_definition;
    [
        ("Query", &def.query),
        ("Mutation", &def.mutation),
        ("Subscription", &def.subscription),
    ]
    .into_iter()
    .filter_map(|(kind, n)| n.as_ref().map(|n| (kind, n.name.clone())))
    .collect()
}

/// 自分で定義した型（組み込みと introspection を除く）。
fn own_types(schema: &Schema) -> impl Iterator<Item = (&Name, &ExtendedType)> {
    schema.types.iter().filter(|(_, t)| !t.is_built_in())
}

fn sizes(schema: &Schema) -> (usize, usize) {
    let roots = root_names(schema)
        .iter()
        .filter_map(|(_, n)| schema.get_object(n))
        .map(|o| o.fields.len())
        .sum();
    (own_types(schema).count(), roots)
}

/// GraphQL の仕様が言わない、この契約の規則。
fn lint_schema(schema: &Schema) -> Vec<String> {
    let mut out = Vec::new();
    lint_names(schema, &mut out);
    lint_roots(schema, &mut out);
    lint_mutations(schema, &mut out);
    lint_stale_outcomes(schema, &mut out);
    lint_cycles(schema, &mut out);
    lint_deprecations(schema, &mut out);
    out
}

fn is_pascal(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_camel(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_lowercase()) && s.chars().all(|c| c.is_ascii_alphanumeric())
}

fn is_screaming(s: &str) -> bool {
    s.starts_with(|c: char| c.is_ascii_uppercase())
        && s.chars()
            .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
}

/// 名前の形・パス・識別子の型。
///
/// 形を揃えるのは、fragment と生成する型の名前がここから機械的に決まるため（§8）。
/// パスは host の権限を契約へ持ち込む（§12）。 識別子を文字列にすると、エイリアスや
/// 結合した文字列がそのまま鍵になる（`DEC-RCL-017`）。 組み込みの `ID` は種類を
/// 区別しない（[`BUILT_IN_ID`]）。
fn lint_names(schema: &Schema, out: &mut Vec<String>) {
    let src = &schema.sources;
    // 欄・引数・input の欄を集めてから見る。 (持ち主, 名前, 型, 場所)。
    let mut values: Vec<(String, &Name, &Type, Option<SourceSpan>)> = Vec::new();

    for (name, ty) in own_types(schema) {
        let at = place(src, ty_location(ty));
        if !is_pascal(name) {
            out.push(format!("{at}{name}: 型の名前は PascalCase にする"));
        }
        let is_input = matches!(ty, ExtendedType::InputObject(_));
        if is_input != name.ends_with("Input") {
            out.push(format!(
                "{at}{name}: `Input` で終わる名前は input の型だけに使い、input の型はどれも `Input` で終える"
            ));
        }
        let fields = match ty {
            ExtendedType::Object(o) => Some(&o.fields),
            ExtendedType::Interface(i) => Some(&i.fields),
            _ => None,
        };
        for f in fields.into_iter().flat_map(|m| m.values()) {
            values.push((name.to_string(), &f.name, &f.ty, f.location()));
            for a in &f.arguments {
                values.push((format!("{name}.{}", f.name), &a.name, &a.ty, a.location()));
            }
        }
        if let ExtendedType::InputObject(i) = ty {
            for f in i.fields.values() {
                values.push((name.to_string(), &f.name, &f.ty, f.location()));
            }
        }
        if let ExtendedType::Enum(e) = ty {
            for v in e.values.values() {
                if !is_screaming(&v.value) {
                    out.push(format!(
                        "{}{name}.{}: enum の値は SCREAMING_SNAKE_CASE にする",
                        place(src, v.location()),
                        v.value
                    ));
                }
            }
        }
    }

    for (owner, name, ty, span) in values {
        let at = place(src, span);
        if !is_camel(name) {
            out.push(format!(
                "{at}{owner}.{name}: 欄と引数の名前は camelCase にする"
            ));
        }
        let lower = name.to_ascii_lowercase();
        if PATH_SUFFIXES.iter().any(|s| lower.ends_with(s)) {
            out.push(format!(
                "{at}{owner}.{name}: パスを契約に入れない。 host が選んだものは意味のある入力に写す"
            ));
        }
        let inner = ty.inner_named_type().as_str();
        if (name == "id" || name.ends_with("Id") || name.ends_with("Ids"))
            && PLAIN_SCALARS.contains(&inner)
        {
            out.push(format!(
                "{at}{owner}.{name}: 識別子を {inner} にしない。 種類ごとの scalar にする"
            ));
        }
        if inner == BUILT_IN_ID {
            out.push(format!(
                "{at}{owner}.{name}: 組み込みの ID を使わない。 種類ごとの scalar にする"
            ));
        }
        if (name == "key" || name.ends_with("Key")) && inner == "String" {
            out.push(format!(
                "{at}{owner}.{name}: 文字列を鍵にしない。 識別子で指す"
            ));
        }
    }
}

fn ty_location(ty: &ExtendedType) -> Option<SourceSpan> {
    match ty {
        ExtendedType::Scalar(t) => t.location(),
        ExtendedType::Object(t) => t.location(),
        ExtendedType::Interface(t) => t.location(),
        ExtendedType::Union(t) => t.location(),
        ExtendedType::Enum(t) => t.location(),
        ExtendedType::InputObject(t) => t.location(),
    }
}

/// root の欄は null を返さない。 Subscription は union を返す。
///
/// null の伝播を状態の代わりにしない（`DEC-PLT-038`）。 Subscription が object を
/// 返すと、貸与が切れた・続きが欠けたを値で言えず、`errors[]` へ落ちる（`DEC-PLT-036`）。
fn lint_roots(schema: &Schema, out: &mut Vec<String>) {
    let src = &schema.sources;
    for (kind, root) in root_names(schema) {
        let Some(obj) = schema.get_object(&root) else {
            continue;
        };
        for f in obj.fields.values() {
            let at = place(src, f.location());
            if !f.ty.is_non_null() {
                out.push(format!(
                    "{at}{root}.{}: root の結果を null にしない。 起きうる結果は union で返す",
                    f.name
                ));
            }
            if kind == "Subscription" && schema.get_union(f.ty.inner_named_type()).is_none() {
                out.push(format!(
                    "{at}{root}.{}: Subscription は union を返す。 終わりと欠けを値で言うため",
                    f.name
                ));
            }
        }
    }
}

fn upper_first(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_ascii_uppercase().to_string() + c.as_str())
        .unwrap_or_default()
}

/// Mutation の形（§6、`DEC-PLT-035`）。
///
/// `x(input: XInput!): XPayload!` で、payload は `outcome: XOutcome!` の union を持つ。
/// 確定を伴うものは `operationId` を取り、payload に受領証を持つ。 片方だけだと、
/// 応答を失ったときに同じ操作かを照合できないか、照合できない受領証が返る。
fn lint_mutations(schema: &Schema, out: &mut Vec<String>) {
    let src = &schema.sources;
    let mut payloads = BTreeSet::new();
    let mutation = root_names(schema)
        .into_iter()
        .find(|(k, _)| *k == "Mutation")
        .and_then(|(_, n)| schema.get_object(&n).cloned());
    if let Some(obj) = mutation {
        for f in obj.fields.values() {
            let at = place(src, f.location());
            let base = upper_first(&f.name);
            let (input, payload, outcome) = (
                format!("{base}Input"),
                format!("{base}Payload"),
                format!("{base}Outcome"),
            );
            // 名前から作った期待値ではなく、実際に返している型を数える。 期待値を数えると、
            // 別の型を返している mutation の payload まで「使われている」ことになる。
            payloads.insert(f.ty.inner_named_type().to_string());

            let arg_ok = f.arguments.len() == 1
                && f.arguments[0].name == "input"
                && *f.arguments[0].ty == Type::NonNullNamed(Name::new_unchecked(&input));
            if !arg_ok {
                out.push(format!(
                    "{at}Mutation.{}: 引数は `input: {input}!` の1つだけにする",
                    f.name
                ));
            }
            if f.ty != Type::NonNullNamed(Name::new_unchecked(&payload)) {
                out.push(format!(
                    "{at}Mutation.{}: 結果は `{payload}!` にする",
                    f.name
                ));
                continue;
            }
            let Some(p) = schema.get_object(&payload) else {
                out.push(format!(
                    "{at}Mutation.{}: `{payload}` が object でない",
                    f.name
                ));
                continue;
            };
            let outcome_ok = p.fields.get("outcome").is_some_and(|o| {
                o.ty == Type::NonNullNamed(Name::new_unchecked(&outcome))
                    && schema.get_union(&outcome).is_some()
            });
            if !outcome_ok {
                out.push(format!(
                    "{}{payload}: `outcome: {outcome}!` を持ち、`{outcome}` を union にする",
                    place(src, p.location())
                ));
            }
            let receipt = p.fields.get("receipt").is_some_and(|r| {
                r.ty == Type::NonNullNamed(Name::new_unchecked("MutationReceipt"))
            });
            let operation_id = schema.get_input_object(&input).is_some_and(|i| {
                i.fields
                    .values()
                    .any(|v| *v.ty == Type::NonNullNamed(Name::new_unchecked("OperationId")))
            });
            if receipt != operation_id {
                out.push(format!(
                    "{at}Mutation.{}: `operationId: OperationId!` を取るなら payload に `receipt: MutationReceipt!` を持ち、取らないなら持たない",
                    f.name
                ));
            }
        }
    }
    for (name, ty) in own_types(schema) {
        if name.ends_with("Payload") && !payloads.contains(name.as_str()) {
            out.push(format!(
                "{}{name}: `Payload` で終わる名前は Mutation の結果だけに使う",
                place(src, ty_location(ty))
            ));
        }
    }
}

/// 古くなりうる入力を取る root の欄は、そのときの結果を union に持つ（[`STALE`]）。
fn lint_stale_outcomes(schema: &Schema, out: &mut Vec<String>) {
    let src = &schema.sources;
    for (kind, root) in root_names(schema) {
        let Some(obj) = schema.get_object(&root) else {
            continue;
        };
        for f in obj.fields.values() {
            // 引数と、input の型の引数なら1段下の欄まで。
            let mut taken: BTreeSet<&str> = BTreeSet::new();
            for a in &f.arguments {
                let t = a.ty.inner_named_type();
                taken.insert(t.as_str());
                if let Some(i) = schema.get_input_object(t) {
                    taken.extend(i.fields.values().map(|v| v.ty.inner_named_type().as_str()));
                }
            }
            let needed: Vec<&str> = STALE
                .iter()
                .filter(|(input, _)| taken.contains(input))
                .map(|(_, outcome)| *outcome)
                .collect();
            if needed.is_empty() {
                continue;
            }
            let result = if kind == "Mutation" {
                schema
                    .get_object(f.ty.inner_named_type())
                    .and_then(|p| p.fields.get("outcome"))
                    .map(|o| o.ty.inner_named_type().clone())
            } else {
                Some(f.ty.inner_named_type().clone())
            };
            let members: BTreeSet<&str> = result
                .as_ref()
                .and_then(|r| schema.get_union(r))
                .map(|u| u.members.iter().map(|m| m.name.as_str()).collect())
                .unwrap_or_default();
            for n in needed {
                if !members.contains(n) {
                    out.push(format!(
                        "{}{root}.{}: 古くなりうる入力を取るので、結果の union に `{n}` を持つ",
                        place(src, f.location()),
                        f.name
                    ));
                }
            }
        }
    }
}

/// root 以外の出力の型が、欄をたどって自分へ戻らない（`03-task-dag.md` の T03 が挙げる反例）。
///
/// 戻れると、consumer は深さに上限の無い読みを書ける。 大きなものは root の窓の欄に置き、
/// 戻る向きの参照は識別子の scalar で持つ（§5）。
fn lint_cycles(schema: &Schema, out: &mut Vec<String>) {
    let roots: BTreeSet<Name> = root_names(schema).into_iter().map(|(_, n)| n).collect();
    let implementers = schema.implementers_map();
    let composite = |n: &Name| {
        !roots.contains(n)
            && matches!(
                schema.types.get(n),
                Some(ExtendedType::Object(_) | ExtendedType::Interface(_) | ExtendedType::Union(_))
            )
    };
    let mut edges: BTreeMap<Name, BTreeSet<Name>> = BTreeMap::new();
    for (name, ty) in own_types(schema) {
        if roots.contains(name) {
            continue;
        }
        let next = edges.entry(name.clone()).or_default();
        match ty {
            ExtendedType::Object(o) => next.extend(
                o.fields
                    .values()
                    .map(|f| f.ty.inner_named_type().clone())
                    .filter(|t| composite(t)),
            ),
            ExtendedType::Interface(i) => {
                next.extend(
                    i.fields
                        .values()
                        .map(|f| f.ty.inner_named_type().clone())
                        .filter(|t| composite(t)),
                );
                if let Some(imp) = implementers.get(name) {
                    next.extend(imp.objects.iter().chain(&imp.interfaces).cloned());
                }
            }
            ExtendedType::Union(u) => next.extend(u.members.iter().map(|m| m.name.clone())),
            _ => {}
        }
    }

    // 色つきの深さ優先。 同じ環を別の入口から2度言わない。
    fn visit(
        n: &Name,
        edges: &BTreeMap<Name, BTreeSet<Name>>,
        done: &mut BTreeSet<Name>,
        stack: &mut Vec<Name>,
        found: &mut BTreeSet<Vec<Name>>,
    ) {
        if done.contains(n) {
            return;
        }
        if let Some(i) = stack.iter().position(|s| s == n) {
            let mut cycle = stack[i..].to_vec();
            cycle.sort();
            found.insert(cycle);
            return;
        }
        stack.push(n.clone());
        for m in edges.get(n).into_iter().flatten() {
            visit(m, edges, done, stack, found);
        }
        stack.pop();
        done.insert(n.clone());
    }
    let mut done = BTreeSet::new();
    let mut found = BTreeSet::new();
    for n in edges.keys() {
        visit(n, &edges, &mut done, &mut Vec::new(), &mut found);
    }
    for cycle in found {
        let names: Vec<&str> = cycle.iter().map(Name::as_str).collect();
        out.push(format!(
            "参照が環になっている: {}。 戻る向きは識別子で持つ",
            names.join(" / ")
        ));
    }
}

/// `@deprecated` には、代わりに何を使うかを書く（§14）。
///
/// 改名は「新しい欄を足す → 古い欄を非推奨にする → 使われなくなってから消す」の順で、
/// 理由が無いと consumer は移る先を知らない。
fn lint_deprecations(schema: &Schema, out: &mut Vec<String>) {
    let src = &schema.sources;
    let mut check = |what: String, directives: &DirectiveList, span: Option<SourceSpan>| {
        let Some(d) = directives.get("deprecated") else {
            return;
        };
        let reason = d
            .specified_argument_by_name("reason")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .unwrap_or_default();
        if reason.is_empty() || reason == DEFAULT_DEPRECATION {
            out.push(format!(
                "{}{what}: `@deprecated` に reason を書き、代わりに使うものを名指す",
                place(src, span)
            ));
        }
    };
    for (name, ty) in own_types(schema) {
        match ty {
            ExtendedType::Object(o) => {
                for f in o.fields.values() {
                    check(format!("{name}.{}", f.name), &f.directives, f.location());
                    for a in &f.arguments {
                        check(
                            format!("{name}.{}({})", f.name, a.name),
                            &a.directives,
                            a.location(),
                        );
                    }
                }
            }
            ExtendedType::Interface(i) => {
                for f in i.fields.values() {
                    check(format!("{name}.{}", f.name), &f.directives, f.location());
                }
            }
            ExtendedType::InputObject(i) => {
                for f in i.fields.values() {
                    check(format!("{name}.{}", f.name), &f.directives, f.location());
                }
            }
            ExtendedType::Enum(e) => {
                for v in e.values.values() {
                    check(format!("{name}.{}", v.value), &v.directives, v.location());
                }
            }
            ExtendedType::Scalar(_) | ExtendedType::Union(_) => {}
        }
    }
}

/// operation の規則。
///
/// - 名前を持つ。 persisted operation の manifest は名前で引く（§11）
/// - fragment は `持ち主_型`。 部品ごとに置くので、名前が持ち主を言う（§8）
/// - 非推奨の欄を選ばない。 見本は今の語彙を示す（§14）
fn lint_documents(doc: &ExecutableDocument) -> Vec<String> {
    let src = &doc.sources;
    let mut out = Vec::new();
    if let Some(op) = &doc.operations.anonymous {
        out.push(format!(
            "{}名前の無い operation。 manifest は名前で引く",
            place(src, op.location())
        ));
    }
    for (name, op) in &doc.operations.named {
        if !is_pascal(name) {
            out.push(format!(
                "{}{name}: operation の名前は PascalCase にする",
                place(src, op.location())
            ));
        }
        deprecated_in(&op.selection_set, src, name, &mut out);
    }
    for (name, fragment) in &doc.fragments {
        let ty = fragment.selection_set.ty.as_str();
        let owner = name.strip_suffix(&format!("_{ty}"));
        if !owner.is_some_and(is_pascal) {
            out.push(format!(
                "{}{name}: fragment の名前は `持ち主_{ty}` にする",
                place(src, fragment.location())
            ));
        }
        deprecated_in(&fragment.selection_set, src, name, &mut out);
    }
    out
}

fn deprecated_in(set: &SelectionSet, src: &SourceMap, owner: &str, out: &mut Vec<String>) {
    for s in &set.selections {
        match s {
            Selection::Field(f) => {
                if f.definition.directives.get("deprecated").is_some() {
                    out.push(format!(
                        "{}{owner}: 非推奨の `{}.{}` を選んでいる",
                        place(src, f.location()),
                        set.ty,
                        f.name
                    ));
                }
                deprecated_in(&f.selection_set, src, owner, out);
            }
            Selection::InlineFragment(i) => deprecated_in(&i.selection_set, src, owner, out),
            Selection::FragmentSpread(_) => {}
        }
    }
}

/// 見本の operation が1度も選ばない root の欄。 落とさずに数だけ出す。
fn unexercised(schema: &Schema, doc: &ExecutableDocument) -> Vec<String> {
    fn roots_of(
        set: &SelectionSet,
        doc: &ExecutableDocument,
        seen: &mut BTreeSet<Name>,
        out: &mut BTreeSet<String>,
    ) {
        for s in &set.selections {
            match s {
                Selection::Field(f) => {
                    out.insert(format!("{}.{}", set.ty, f.name));
                }
                Selection::InlineFragment(i) => roots_of(&i.selection_set, doc, seen, out),
                Selection::FragmentSpread(sp) => {
                    if seen.insert(sp.fragment_name.clone())
                        && let Some(fr) = doc.fragments.get(&sp.fragment_name)
                    {
                        roots_of(&fr.selection_set, doc, seen, out);
                    }
                }
            }
        }
    }
    let mut used = BTreeSet::new();
    for op in doc.operations.iter() {
        roots_of(&op.selection_set, doc, &mut BTreeSet::new(), &mut used);
    }
    let mut out = Vec::new();
    for (_, root) in root_names(schema) {
        if let Some(obj) = schema.get_object(&root) {
            for f in obj.fields.keys() {
                let c = format!("{root}.{f}");
                if !used.contains(&c) {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// 能力の表（§11）。
///
/// root の欄はどれも能力と費用を1つずつ持つ。 表の座標は SDL に実在し、能力と費用は
/// 表の語彙から選ぶ。 語彙に使われないものを残さない——残すと、どの欄にも付かない
/// 能力を consumer に許すかどうかが、何も言わないまま決まる。
fn check_capabilities(schema: &Schema, table: &toml::Table) -> Vec<String> {
    let mut out = Vec::new();
    if str_of(table, "schema") != Some("capability-map") {
        out.push(format!(
            "{CAPABILITIES}: 先頭で `schema = 'capability-map'` を名乗る"
        ));
    }
    let vocabulary = table.get("vocabulary").and_then(toml::Value::as_table);
    let vocab = |key: &str| vocabulary.map(|v| list_of(v, key)).unwrap_or_default();
    let (capabilities, costs) = (vocab("capabilities"), vocab("costs"));
    for (key, list) in [("capabilities", &capabilities), ("costs", &costs)] {
        if list.is_empty() {
            out.push(format!("{CAPABILITIES}: `vocabulary.{key}` が空"));
        }
        let unique: BTreeSet<&String> = list.iter().collect();
        if unique.len() != list.len() {
            out.push(format!("{CAPABILITIES}: `vocabulary.{key}` に重複がある"));
        }
    }

    let fields = table
        .get("fields")
        .and_then(toml::Value::as_table)
        .cloned()
        .unwrap_or_default();
    let mut used_capabilities = BTreeSet::new();
    let mut used_costs = BTreeSet::new();
    for (coordinate, v) in &fields {
        let exists =
            coordinate
                .split_once('.')
                .is_some_and(|(ty, field)| match schema.types.get(ty) {
                    Some(ExtendedType::Object(o)) => o.fields.contains_key(field),
                    Some(ExtendedType::Interface(i)) => i.fields.contains_key(field),
                    _ => false,
                });
        if !exists {
            out.push(format!(
                "{CAPABILITIES}: `{coordinate}` という欄は SDL に無い"
            ));
        }
        let Some(entry) = v.as_table() else {
            out.push(format!("{CAPABILITIES}: `{coordinate}` は表にする"));
            continue;
        };
        match str_of(entry, "capability") {
            Some(c) if capabilities.iter().any(|x| x == c) => {
                used_capabilities.insert(c.to_owned());
            }
            Some(c) => out.push(format!(
                "{CAPABILITIES}: `{coordinate}` の能力 `{c}` は語彙に無い"
            )),
            None => out.push(format!(
                "{CAPABILITIES}: `{coordinate}` に capability が無い"
            )),
        }
        match str_of(entry, "cost") {
            Some(c) if costs.iter().any(|x| x == c) => {
                used_costs.insert(c.to_owned());
            }
            Some(c) => out.push(format!(
                "{CAPABILITIES}: `{coordinate}` の費用 `{c}` は語彙に無い"
            )),
            None => out.push(format!("{CAPABILITIES}: `{coordinate}` に cost が無い")),
        }
    }
    for (_, root) in root_names(schema) {
        if let Some(obj) = schema.get_object(&root) {
            for f in obj.fields.keys() {
                let c = format!("{root}.{f}");
                if !fields.contains_key(&c) {
                    out.push(format!("{CAPABILITIES}: root の欄 `{c}` に能力が無い"));
                }
            }
        }
    }
    for c in capabilities
        .iter()
        .filter(|c| !used_capabilities.contains(*c))
    {
        out.push(format!("{CAPABILITIES}: 能力 `{c}` を使う欄が無い"));
    }
    for c in costs.iter().filter(|c| !used_costs.contains(*c)) {
        out.push(format!("{CAPABILITIES}: 費用 `{c}` を使う欄が無い"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 規則を1つも破らない、最小の契約。 各試験はここから1箇所だけ変える。
    const BASE: &str = r#"
schema { query: Query mutation: Mutation subscription: Subscription }

scalar ProjectLease
scalar OperationId
scalar Revision
scalar EventCursor
scalar ThingId

enum FailureClass { REJECTED CONFLICT }

interface Problem { code: String! class: FailureClass! }
type ProjectLeaseExpired implements Problem { code: String! class: FailureClass! }
type OperationIdReused implements Problem { code: String! class: FailureClass! }
type RevisionConflict implements Problem { code: String! class: FailureClass! }
type EventGap { resumeAfter: EventCursor! }
type MutationReceipt { operationId: OperationId! revision: Revision! }

type Thing {
  id: ThingId!
  name: String!
  label: String @deprecated(reason: "`name` を使う")
}
union ThingResult = Thing | ProjectLeaseExpired
type Query { thing(lease: ProjectLease!): ThingResult! }

input RenameThingInput {
  lease: ProjectLease!
  operationId: OperationId!
  baseRevision: Revision!
  thing: ThingId!
  name: String!
}
type RenameThingPayload { receipt: MutationReceipt! outcome: RenameThingOutcome! }
union RenameThingOutcome = ThingRenamed | ProjectLeaseExpired | OperationIdReused | RevisionConflict
type ThingRenamed { thing: Thing! }
type Mutation { renameThing(input: RenameThingInput!): RenameThingPayload! }

union ThingEvent = ThingChanged | EventGap | ProjectLeaseExpired
type ThingChanged { cursor: EventCursor! }
type Subscription { thingEvents(lease: ProjectLease!, after: EventCursor): ThingEvent! }
"#;

    const READ_THING: &str = r"
query ReadThing($lease: ProjectLease!) {
  thing(lease: $lease) {
    __typename
    ... on Thing { ...ThingName_Thing }
  }
}
";

    const THING_NAME: &str = r"
fragment ThingName_Thing on Thing { id name }
";

    const RENAME_THING: &str = r"
mutation RenameThing($input: RenameThingInput!) {
  renameThing(input: $input) {
    receipt { operationId }
    outcome { __typename }
  }
}
";

    const CAPABILITY_MAP: &str = r"
schema = 'capability-map'
[vocabulary]
capabilities = ['thing.read', 'thing.modify']
costs = ['cheap-read', 'durable-write']
[fields]
'Query.thing' = { capability = 'thing.read', cost = 'cheap-read' }
'Mutation.renameThing' = { capability = 'thing.modify', cost = 'durable-write' }
'Subscription.thingEvents' = { capability = 'thing.read', cost = 'cheap-read' }
";

    /// `BASE` の1箇所を置き換える。 置き換える先が無ければ落とす
    /// ——見つからないまま元の契約を検査すると、試験が何も変えずに通る。
    fn with(from: &str, to: &str) -> String {
        assert!(BASE.contains(from), "BASE に `{from}` が無い");
        BASE.replacen(from, to, 1)
    }

    fn schema_of(src: &str) -> Valid<Schema> {
        parse_schema(&files(&[("schema.graphql", src)])).expect("GraphQL として正しい")
    }

    fn lint(src: &str) -> Vec<String> {
        lint_schema(&schema_of(src))
    }

    /// (パス, 中身) の並び。 SDL にも operation の文書にも使う。
    fn files(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(p, s)| ((*p).to_owned(), (*s).to_owned()))
            .collect()
    }

    fn base_docs() -> Vec<(String, String)> {
        files(&[
            ("read.graphql", READ_THING),
            ("fragments.graphql", THING_NAME),
            ("rename.graphql", RENAME_THING),
        ])
    }

    fn mentions(errors: &[String], needle: &str) -> bool {
        errors.iter().any(|e| e.contains(needle))
    }

    #[test]
    fn 最小の契約は通る() {
        let schema = schema_of(BASE);
        assert_eq!(lint_schema(&schema), Vec::<String>::new());
        let doc = parse_documents(&schema, &base_docs()).expect("見本が通る");
        assert_eq!(lint_documents(&doc), Vec::<String>::new());
        let map: toml::Table = CAPABILITY_MAP.parse().expect("TOML");
        assert_eq!(check_capabilities(&schema, &map), Vec::<String>::new());
    }

    #[test]
    fn fragment_は文書をまたいで使える() {
        // 1本ずつ検証すると、`ThingName_Thing` が別の文書にあるだけで落ちる。
        let schema = schema_of(BASE);
        assert!(parse_documents(&schema, &files(&[("read.graphql", READ_THING)])).is_err());
        assert!(parse_documents(&schema, &base_docs()).is_ok());
    }

    #[test]
    fn graphql_の誤りは_apollo_compiler_が落とす() {
        let broken = with("name: String!\n", "name: Missing!\n");
        let errors = parse_schema(&files(&[("schema.graphql", &broken)])).expect_err("未定義の型");
        assert!(mentions(&errors, "Missing"), "{errors:?}");
        assert!(
            mentions(&errors, "schema.graphql:"),
            "場所が付く: {errors:?}"
        );
    }

    /// 別の領域のファイルが root の型に足す欄。 `BASE` より前に読まれる名前で置く。
    const AREA: &str = r"
extend type Query { other(lease: ProjectLease!): OtherResult! }
type Other { id: ThingId! }
union OtherResult = Other | ProjectLeaseExpired
";

    #[test]
    fn 契約はファイルをまたいで1つに組む() {
        // extension が root の型の定義より前のファイルにあっても、定義に付け直される。
        let schema = parse_schema(&files(&[("area.graphql", AREA), ("base.graphql", BASE)]))
            .expect("GraphQL として正しい");
        assert!(schema.get_object("Other").is_some());
        assert!(
            schema
                .get_object("Query")
                .is_some_and(|q| q.fields.contains_key("other") && q.fields.contains_key("thing"))
        );
        assert_eq!(lint_schema(&schema), Vec::<String>::new());

        // 規則はファイルをまたいで効き、違反は欄を足したファイルを名指す。
        let stale = AREA.replace("Other | ProjectLeaseExpired", "Other");
        let schema = parse_schema(&files(&[("area.graphql", &stale), ("base.graphql", BASE)]))
            .expect("GraphQL として正しい");
        let errors = lint_schema(&schema);
        assert!(
            mentions(&errors, "area.graphql:2:") && mentions(&errors, "Query.other"),
            "{errors:?}"
        );
    }

    #[test]
    fn sdl_のディレクトリの下を読み尽くす() {
        // 試験のプロセスごとに場所を分ける。 前に落ちた回の残りは先に消す。
        let root = std::env::temp_dir().join(format!("koeru-xtask-schema-{}", std::process::id()));
        fs::remove_dir_all(&root).ok();
        let dir = root.join(SCHEMA);
        fs::create_dir_all(dir.join("nested")).expect("作れる");
        fs::write(dir.join("project.graphql"), BASE).expect("書ける");
        fs::write(dir.join("nested").join("area.graphql"), AREA).expect("書ける");
        fs::write(dir.join("notes.md"), "SDL ではない").expect("書ける");
        let read = read_graphql(&root, SCHEMA);
        fs::remove_dir_all(&root).expect("消せる");

        let sources = read.expect("読める");
        let paths: Vec<std::path::PathBuf> = sources.iter().map(|(p, _)| p.into()).collect();
        let base = Path::new(SCHEMA);
        assert_eq!(
            paths,
            [
                base.join("nested").join("area.graphql"),
                base.join("project.graphql")
            ]
        );
        let schema = parse_schema(&sources).expect("GraphQL として正しい");
        assert!(schema.get_object("Other").is_some() && schema.get_object("Thing").is_some());
    }

    #[test]
    fn sdl_が1本も無ければ落とす() {
        let errors = parse_schema(&[]).expect_err("0本");
        assert!(mentions(&errors, "SDL のファイルが1本も無い"), "{errors:?}");
    }

    #[test]
    fn mutation_は_input_を1つ取り_payload_を返す() {
        let errors = lint(&with(
            "renameThing(input: RenameThingInput!): RenameThingPayload!",
            "renameThing(thing: ThingId!, name: String!): ThingRenamed!",
        ));
        assert!(
            mentions(&errors, "`input: RenameThingInput!`"),
            "{errors:?}"
        );
        assert!(mentions(&errors, "`RenameThingPayload!`"), "{errors:?}");
        // 結果に使われなくなった payload の名前も落ちる。
        assert!(mentions(&errors, "Mutation の結果だけに使う"), "{errors:?}");
    }

    #[test]
    fn payload_は_outcome_の_union_を持つ() {
        let errors = lint(&with(
            "outcome: RenameThingOutcome!",
            "outcome: ThingRenamed!",
        ));
        assert!(
            mentions(&errors, "`outcome: RenameThingOutcome!`"),
            "{errors:?}"
        );
    }

    #[test]
    fn 受領証と_operation_id_は対になる() {
        let errors = lint(&with("receipt: MutationReceipt! ", ""));
        assert!(mentions(&errors, "receipt: MutationReceipt!"), "{errors:?}");
        let errors = lint(&with(
            "operationId: OperationId!\n  baseRevision",
            "baseRevision",
        ));
        assert!(mentions(&errors, "receipt: MutationReceipt!"), "{errors:?}");
    }

    #[test]
    fn 古くなりうる入力は結果の_union_で返す() {
        let errors = lint(&with("| OperationIdReused ", ""));
        assert!(mentions(&errors, "`OperationIdReused`"), "{errors:?}");
        let errors = lint(&with("| RevisionConflict\n", "\n"));
        assert!(mentions(&errors, "`RevisionConflict`"), "{errors:?}");
        let errors = lint(&with(
            "union ThingResult = Thing | ProjectLeaseExpired",
            "union ThingResult = Thing",
        ));
        assert!(mentions(&errors, "Query.thing"), "{errors:?}");
        let errors = lint(&with("ThingChanged | EventGap |", "ThingChanged |"));
        assert!(mentions(&errors, "`EventGap`"), "{errors:?}");
    }

    #[test]
    fn 仕事を指す_root_の欄は見つからないことを結果で返す() {
        // 仕事は保持の期間を過ぎると消える。 消えた仕事を観測し始めても `errors[]` にしない。
        let job = |members: &str| {
            format!(
                "{BASE}\nscalar JobId\ntype Job {{ id: JobId! }}\n\
                 type ReferenceNotFound implements Problem {{ code: String! class: FailureClass! }}\n\
                 union JobEvent = {members}\n\
                 extend type Subscription {{ jobEvents(lease: ProjectLease!, job: JobId!): JobEvent! }}\n"
            )
        };
        let errors = lint(&job("Job | ProjectLeaseExpired"));
        assert!(
            mentions(
                &errors,
                "Subscription.jobEvents: 古くなりうる入力を取るので、結果の union に `ReferenceNotFound`"
            ),
            "{errors:?}"
        );
        assert_eq!(
            lint(&job("Job | ProjectLeaseExpired | ReferenceNotFound")),
            Vec::<String>::new()
        );
    }

    #[test]
    fn root_の結果を_null_にしない() {
        let errors = lint(&with(
            "thing(lease: ProjectLease!): ThingResult!",
            "thing(lease: ProjectLease!): ThingResult",
        ));
        assert!(
            mentions(&errors, "Query.thing: root の結果を null にしない"),
            "{errors:?}"
        );
    }

    #[test]
    fn subscription_は_union_を返す() {
        let errors = lint(&with(
            "after: EventCursor): ThingEvent!",
            "after: EventCursor): ThingChanged!",
        ));
        assert!(
            mentions(&errors, "Subscription は union を返す"),
            "{errors:?}"
        );
    }

    #[test]
    fn パスを欄の名前にしない() {
        let errors = lint(&with(
            "name: String!\n",
            "name: String!\n  exportDir: String!\n",
        ));
        assert!(
            mentions(&errors, "Thing.exportDir: パスを契約に入れない"),
            "{errors:?}"
        );
        let errors = lint(&with(
            "  name: String!\n}",
            "  name: String!\n  sourcePath: String!\n}",
        ));
        assert!(
            mentions(&errors, "RenameThingInput.sourcePath"),
            "{errors:?}"
        );
    }

    #[test]
    fn 識別子と鍵を文字列にしない() {
        let errors = lint(&with("id: ThingId!", "id: String!"));
        assert!(
            mentions(&errors, "Thing.id: 識別子を String にしない"),
            "{errors:?}"
        );
        let errors = lint(&with(
            "name: String!\n",
            "name: String!\n  aliasKey: String!\n",
        ));
        assert!(
            mentions(&errors, "Thing.aliasKey: 文字列を鍵にしない"),
            "{errors:?}"
        );
    }

    #[test]
    fn 組み込みの_id_をどこにも使わない() {
        // 欄の名前によらない。 `thing: ID!` も、別の種類の識別子を受け取れてしまう。
        let errors = lint(&with("id: ThingId!", "id: ID!"));
        assert!(
            mentions(&errors, "Thing.id: 組み込みの ID を使わない"),
            "{errors:?}"
        );
        let errors = lint(&with("  thing: ThingId!\n", "  thing: ID!\n"));
        assert!(
            mentions(&errors, "RenameThingInput.thing: 組み込みの ID を使わない"),
            "{errors:?}"
        );
        let errors = lint(&with(
            "thing(lease: ProjectLease!): ThingResult!",
            "thing(lease: ProjectLease!, near: [ID!]): ThingResult!",
        ));
        assert!(
            mentions(&errors, "Query.thing.near: 組み込みの ID を使わない"),
            "{errors:?}"
        );
    }

    #[test]
    fn 名前の形を揃える() {
        let src = format!(
            "{BASE}\ntype thing_view {{ Title: String! }}\nenum Mode {{ fast }}\ninput Filter {{ tone: Int }}\ntype ViewInput {{ tone: Int }}\n"
        );
        let errors = lint(&src);
        assert!(
            mentions(&errors, "thing_view: 型の名前は PascalCase"),
            "{errors:?}"
        );
        assert!(
            mentions(&errors, "thing_view.Title: 欄と引数の名前は camelCase"),
            "{errors:?}"
        );
        assert!(mentions(&errors, "Mode.fast: enum の値は"), "{errors:?}");
        assert!(mentions(&errors, "Filter: `Input` で終わる"), "{errors:?}");
        assert!(
            mentions(&errors, "ViewInput: `Input` で終わる"),
            "{errors:?}"
        );
    }

    #[test]
    fn 参照が環を作らない() {
        let errors = lint(&with("name: String!\n", "name: String!\n  parent: Thing\n"));
        assert!(
            mentions(&errors, "参照が環になっている: Thing"),
            "{errors:?}"
        );
        // union をまたいでも戻れば環。
        let errors = lint(&with(
            "type ThingRenamed { thing: Thing! }",
            "type ThingRenamed { thing: Thing! }\ntype Owner { things: [ThingResult!]! }\nextend type Thing { owner: Owner! }",
        ));
        assert!(
            mentions(&errors, "Owner / Thing / ThingResult"),
            "{errors:?}"
        );
    }

    #[test]
    fn 非推奨には代わりを書く() {
        let errors = lint(&with(
            "@deprecated(reason: \"`name` を使う\")",
            "@deprecated",
        ));
        assert!(
            mentions(&errors, "Thing.label: `@deprecated` に reason"),
            "{errors:?}"
        );
        let errors = lint(&with(
            "@deprecated(reason: \"`name` を使う\")",
            "@deprecated(reason: \"No longer supported\")",
        ));
        assert!(mentions(&errors, "Thing.label"), "{errors:?}");
    }

    #[test]
    fn operation_が使う欄を消すと落ちる() {
        // 後方互換を壊す変更の代表。 見本が選んでいる欄が消えると、文書の検証が落ちる。
        let schema = schema_of(&with("  name: String!\n", ""));
        let errors = parse_documents(&schema, &base_docs()).expect_err("`name` が無い");
        assert!(mentions(&errors, "name"), "{errors:?}");
        assert!(
            mentions(&errors, "fragments.graphql:"),
            "場所が付く: {errors:?}"
        );
    }

    #[test]
    fn 引数の型を変えると_operation_が落ちる() {
        let schema = schema_of(&with(
            "thing(lease: ProjectLease!): ThingResult!",
            "thing(lease: ThingId!): ThingResult!",
        ));
        let errors = parse_documents(&schema, &base_docs()).expect_err("変数の型が合わない");
        assert!(mentions(&errors, "read.graphql:"), "{errors:?}");
    }

    #[test]
    fn 見本は非推奨の欄を選ばない() {
        let schema = schema_of(BASE);
        let using = files(&[
            ("read.graphql", READ_THING),
            (
                "fragments.graphql",
                "fragment ThingName_Thing on Thing { id name label }",
            ),
            ("rename.graphql", RENAME_THING),
        ]);
        let doc = parse_documents(&schema, &using).expect("非推奨でも GraphQL としては正しい");
        let errors = lint_documents(&doc);
        assert!(mentions(&errors, "非推奨の `Thing.label`"), "{errors:?}");
    }

    #[test]
    fn operation_は名前を持ち_fragment_は持ち主_型() {
        let schema = schema_of(BASE);
        let loose = files(&[(
            "loose.graphql",
            "query ($lease: ProjectLease!) { thing(lease: $lease) { ... on Thing { ...Name } } }\nfragment Name on Thing { name }",
        )]);
        let doc = parse_documents(&schema, &loose).expect("GraphQL としては正しい");
        let errors = lint_documents(&doc);
        assert!(mentions(&errors, "名前の無い operation"), "{errors:?}");
        assert!(
            mentions(&errors, "Name: fragment の名前は `持ち主_Thing`"),
            "{errors:?}"
        );
    }

    #[test]
    fn 見本が1件も無ければ落とす() {
        let errors = parse_documents(&schema_of(BASE), &[]).expect_err("0件");
        assert!(mentions(&errors, "1本も無い"), "{errors:?}");
    }

    #[test]
    fn 能力の表は_root_の欄を覆う() {
        let schema = schema_of(BASE);
        let missing: toml::Table = CAPABILITY_MAP
            .replace(
                "'Subscription.thingEvents' = { capability = 'thing.read', cost = 'cheap-read' }\n",
                "",
            )
            .parse()
            .expect("TOML");
        let errors = check_capabilities(&schema, &missing);
        assert!(
            mentions(&errors, "`Subscription.thingEvents` に能力が無い"),
            "{errors:?}"
        );

        let wrong: toml::Table = format!(
            "{CAPABILITY_MAP}'Thing.nickname' = {{ capability = 'thing.admin', cost = 'cheap-read' }}\n"
        )
        .parse()
        .expect("TOML");
        let errors = check_capabilities(&schema, &wrong);
        assert!(
            mentions(&errors, "`Thing.nickname` という欄は SDL に無い"),
            "{errors:?}"
        );
        assert!(
            mentions(&errors, "`thing.admin` は語彙に無い"),
            "{errors:?}"
        );

        let unused: toml::Table = CAPABILITY_MAP
            .replace("'thing.modify']", "'thing.modify', 'thing.export']")
            .parse()
            .expect("TOML");
        let errors = check_capabilities(&schema, &unused);
        assert!(
            mentions(&errors, "`thing.export` を使う欄が無い"),
            "{errors:?}"
        );
    }
}
