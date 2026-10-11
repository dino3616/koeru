//! リポジトリの本文から、旧 ID の出現を拾う。
//!
//! 旧 ID は `knowledge::refs` の接頭辞の一覧に頼らず、対応表の鍵そのものを探す
//! （対応表が一覧に無い接頭辞を持ち込むことがある）。 区切りの規則は
//! `refs::id_spans` と同じで、前後が英数字かハイフンの位置は採らない
//! ——長い ID の一部（`DEC-fix-0410` の中の `DEC-fix-041`）に化けさせないため。
//!
//! 見るファイルは3種類に分かれる。
//! - `meta/**/*.toml` と `specs/**/*.fsl` は、定義と宣言を持つので種類まで分ける
//! - `check-references` が走査する拡張子の本文は、自由文の言及として書き換える
//! - どれにも当たらないファイルは書き換え方が分からない。 言及が見つかったら
//!   「分類できない参照」として別に数える

use std::path::Path;

use super::mapping::Mapping;
use crate::knowledge::Id;
use crate::repo::{META_DIR, RepoView, SPEC_DIR};

/// ID を引きうる本文の拡張子。
///
/// `checks::references::SCANNED_EXT` と同じ集合。 あちらは非公開で、`checks/` は
/// X07 の持ち物なので触らず、値を写している。 ずれると、`check-references` が
/// 見るファイルと、ここで書き換えるファイルが食い違う——食い違っても検査は
/// 落ちないので、下の試験が原文と突き合わせて固定する。
pub(crate) const SCANNED_EXT: &[&str] = &["md", "rs", "ts", "tsx", "nix", "graphql"];

/// 書き換える場所の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditKind {
    /// meta の `id = '…'`。 記録そのものの定義。
    Definition,
    /// FSL の `@requirement("…"` / `acceptance` / `forbidden` の宣言。
    FslDeclaration,
    /// meta の TOML にある、定義の行以外の値（relation の欄など）。
    RelationValue,
    /// 手書き文書・ソース・FSL の宣言以外の行にある言及。
    Mention,
}

/// 旧 ID の出現1箇所。 `col_start..col_end` は行の中のバイト位置。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) path: String,
    /// 1 始まり。
    pub(crate) line: usize,
    pub(crate) col_start: usize,
    pub(crate) col_end: usize,
    pub(crate) old: Id,
    pub(crate) new: Id,
    pub(crate) kind: EditKind,
}

/// 書き換え方の分からないファイルにあった、旧 ID の出現。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnknownInboundRef {
    pub(crate) path: String,
    /// 1 始まり。
    pub(crate) line: usize,
    pub(crate) old: Id,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ScanResult {
    pub(crate) edits: Vec<Edit>,
    pub(crate) unknown: Vec<UnknownInboundRef>,
}

enum FileClass {
    MetaToml,
    Fsl,
    Scanned,
    Unclassified,
}

fn classify(rel: &str) -> FileClass {
    if rel.starts_with(&format!("{META_DIR}/")) && rel.ends_with(".toml") {
        FileClass::MetaToml
    } else if rel.starts_with(&format!("{SPEC_DIR}/")) && rel.ends_with(".fsl") {
        FileClass::Fsl
    } else if is_scanned_ext(rel) {
        FileClass::Scanned
    } else {
        FileClass::Unclassified
    }
}

/// `check-references` と同じ規則。 生成物（`.gen.ts`）は書き換えない。
fn is_scanned_ext(rel: &str) -> bool {
    if rel.ends_with(".gen.ts") {
        return false;
    }
    Path::new(rel)
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| SCANNED_EXT.contains(&e))
}

/// symlink は辿らない。 `CLAUDE.md` は `AGENTS.md` を指しているので、辿ると同じ行を
/// 二度書き換える。 しかも rename で差し替えると、symlink が実体のファイルに化ける。
fn is_symlink(view: &RepoView, rel: &str) -> bool {
    std::fs::symlink_metadata(view.root().join(rel)).is_ok_and(|m| m.file_type().is_symlink())
}

/// `view` の全ファイルを見て、旧 ID の出現を集める。 結果は path・行・桁の順に並ぶ。
///
/// 読めないファイル（UTF-8 でない＝バイナリ）は、書き換えようが無いので素通りする。
pub(crate) fn scan(view: &RepoView, mapping: &Mapping) -> ScanResult {
    let mut edits = Vec::new();
    let mut unknown = Vec::new();

    let mut all = view.walk("");
    all.sort();
    for rel in &all {
        if is_symlink(view, rel) {
            continue;
        }
        let Ok(Some(text)) = view.read(rel) else {
            continue;
        };
        match classify(rel) {
            FileClass::MetaToml => scan_lines(rel, &text, mapping, toml_line_kind, &mut edits),
            FileClass::Fsl => scan_lines(rel, &text, mapping, fsl_line_kind, &mut edits),
            FileClass::Scanned => {
                scan_lines(rel, &text, mapping, |_| EditKind::Mention, &mut edits)
            }
            FileClass::Unclassified => scan_unknown(rel, &text, mapping, &mut unknown),
        }
    }

    edits.sort_by(|a, b| {
        (a.path.as_str(), a.line, a.col_start).cmp(&(b.path.as_str(), b.line, b.col_start))
    });
    unknown.sort_by(|a, b| (a.path.as_str(), a.line).cmp(&(b.path.as_str(), b.line)));
    ScanResult { edits, unknown }
}

fn scan_lines(
    rel: &str,
    text: &str,
    mapping: &Mapping,
    kind_of: impl Fn(&str) -> EditKind,
    out: &mut Vec<Edit>,
) {
    for (n, line) in text.lines().enumerate() {
        for (old, new) in mapping.pairs() {
            for (start, end) in token_matches(line, old.as_str()) {
                out.push(Edit {
                    path: rel.to_owned(),
                    line: n + 1,
                    col_start: start,
                    col_end: end,
                    old: old.clone(),
                    new: new.clone(),
                    kind: kind_of(line),
                });
            }
        }
    }
}

fn scan_unknown(rel: &str, text: &str, mapping: &Mapping, out: &mut Vec<UnknownInboundRef>) {
    for (n, line) in text.lines().enumerate() {
        for old in mapping.olds() {
            for _ in token_matches(line, old.as_str()) {
                out.push(UnknownInboundRef {
                    path: rel.to_owned(),
                    line: n + 1,
                    old: old.clone(),
                });
            }
        }
    }
}

/// `id = '…'` の行は定義、それ以外の行にある出現は値。
///
/// どの欄が relation かは registry の持ち物で、ここでは知らない。
/// 種類は報告のための分類で、書き換えの結果には効かない。
fn toml_line_kind(line: &str) -> EditKind {
    let is_definition = line
        .trim_start()
        .strip_prefix("id")
        .is_some_and(|rest| rest.trim_start().starts_with('='));
    if is_definition {
        EditKind::Definition
    } else {
        EditKind::RelationValue
    }
}

/// `knowledge::fsl` が宣言として拾う行と同じ形。
fn fsl_line_kind(line: &str) -> EditKind {
    let t = line.trim();
    let declared = t.starts_with("@requirement(\"")
        || t.starts_with("acceptance ")
        || t.starts_with("forbidden ");
    if declared {
        EditKind::FslDeclaration
    } else {
        EditKind::Mention
    }
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'-'
}

/// `line` の中の `needle` を、前後が英数字・ハイフンでない位置だけ返す。
///
/// 返すのは行の中のバイト位置。 `needle` は ASCII の ID なので、見つかる位置は
/// 必ず文字の境目に当たる。
pub(crate) fn token_matches(line: &str, needle: &str) -> Vec<(usize, usize)> {
    if needle.is_empty() {
        return Vec::new();
    }
    let bytes = line.as_bytes();
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(found) = line.get(from..).and_then(|s| s.find(needle)) {
        let start = from + found;
        let end = start + needle.len();
        let before_ok = start == 0 || !is_ident_byte(bytes[start - 1]);
        let after_ok = end >= bytes.len() || !is_ident_byte(bytes[end]);
        if before_ok && after_ok {
            out.push((start, end));
        }
        // 1つ進める。 採らなかった出現の内側に、別の出現があるかもしれない。
        from = start + 1;
    }
    out
}

/// `text` の中の旧 ID をすべて新 ID へ置き換える。
///
/// **原文だけを見て1回で置き換える。** 置き換えた結果をもう一度探さないので、
/// 入れ替え（A→B と B→A）も連鎖（A→B と B→C）も、順序に依らず正しく写る。
pub(crate) fn substitute(text: &str, mapping: &Mapping) -> String {
    let mut out = String::with_capacity(text.len());
    for piece in text.split_inclusive('\n') {
        match piece.strip_suffix('\n') {
            Some(body) => {
                out.push_str(&substitute_line(body, mapping));
                out.push('\n');
            }
            None => out.push_str(&substitute_line(piece, mapping)),
        }
    }
    out
}

fn substitute_line(line: &str, mapping: &Mapping) -> String {
    let mut spans: Vec<(usize, usize, &str)> = Vec::new();
    for (old, new) in mapping.pairs() {
        for (start, end) in token_matches(line, old.as_str()) {
            spans.push((start, end, new.as_str()));
        }
    }
    if spans.is_empty() {
        return line.to_owned();
    }
    spans.sort_by_key(|&(start, _, _)| start);
    let mut out = String::with_capacity(line.len());
    let mut pos = 0;
    for (start, end, new) in spans {
        if start < pos {
            continue;
        }
        out.push_str(&line[pos..start]);
        out.push_str(new);
        pos = end;
    }
    out.push_str(&line[pos..]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 拡張子の集合が `checks::references::SCANNED_EXT` と揃っている。
    ///
    /// 非公開の定数は import できないので、原文と突き合わせる。
    #[test]
    fn 拡張子の集合は_check_references_と揃っている() {
        let src = include_str!("../checks/references.rs");
        let declared = src
            .split("const SCANNED_EXT: &[&str] = &[")
            .nth(1)
            .and_then(|rest| rest.split(']').next())
            .expect("references.rs に SCANNED_EXT の宣言がある");
        let theirs: Vec<&str> = declared
            .split(',')
            .map(|s| s.trim().trim_matches('"'))
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(theirs, SCANNED_EXT);
    }

    #[test]
    fn 前後が英数字かハイフンの出現は採らない() {
        let line =
            "DEC-fix-041 と DEC-fix-0410 と XDEC-fix-041 と DEC-fix-041-x と (DEC-fix-041)。";
        let spans = token_matches(line, "DEC-fix-041");
        let hits: Vec<&str> = spans.iter().map(|&(s, e)| &line[s..e]).collect();
        assert_eq!(hits.len(), 2, "{spans:?}");
        assert_eq!(spans[0].0, 0);
        assert_eq!(&line[spans[1].0 - 1..spans[1].0], "(");
    }

    #[test]
    fn 置き換えは原文だけを見て1回で行う() {
        let m = Mapping::parse([("TR-fix-01", "TR-fix-02"), ("TR-fix-02", "TR-fix-01")]).unwrap();
        assert_eq!(
            substitute("a TR-fix-01 b TR-fix-02\nTR-fix-01\n", &m),
            "a TR-fix-02 b TR-fix-01\nTR-fix-02\n"
        );
        let chain =
            Mapping::parse([("TR-fix-01", "TR-fix-02"), ("TR-fix-02", "TR-fix-03")]).unwrap();
        assert_eq!(
            substitute("TR-fix-01 TR-fix-02", &chain),
            "TR-fix-02 TR-fix-03"
        );
    }

    #[test]
    fn 末尾の改行の有無を保つ() {
        let m = Mapping::parse([("TR-fix-01", "TR-fix-02")]).unwrap();
        assert_eq!(substitute("TR-fix-01", &m), "TR-fix-02");
        assert_eq!(substitute("TR-fix-01\n", &m), "TR-fix-02\n");
        assert_eq!(substitute("", &m), "");
    }

    #[test]
    fn 日本語に挟まれた出現も採る() {
        let line = "判断（TR-fix-01）を参照。";
        let spans = token_matches(line, "TR-fix-01");
        assert_eq!(spans.len(), 1);
        assert_eq!(&line[spans[0].0..spans[0].1], "TR-fix-01");
    }

    #[test]
    fn 行の種類を分ける() {
        assert_eq!(toml_line_kind("id = 'X-fix-01'"), EditKind::Definition);
        assert_eq!(toml_line_kind("  id='X-fix-01'"), EditKind::Definition);
        assert_eq!(
            toml_line_kind("supersedes = ['X-fix-01']"),
            EditKind::RelationValue
        );
        assert_eq!(
            fsl_line_kind("  @requirement(\"X-fix-01\""),
            EditKind::FslDeclaration
        );
        assert_eq!(
            fsl_line_kind("  acceptance X-fix-01 \"見本\" {"),
            EditKind::FslDeclaration
        );
        assert_eq!(fsl_line_kind("  // X-fix-01 を見よ"), EditKind::Mention);
    }
}
