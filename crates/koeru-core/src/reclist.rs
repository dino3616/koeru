//! 録音リストの生成（`TR-RCL-03`, `TR-RCL-08`, `TR-REC-33`, `TR-REC-34`, `TR-REC-35`）。
//!
//! ファイル名は CP932 で表現でき、大小を無視して一意で、前後・連続・全角の空白を持たない
//! （`TR-RCL-08`, `TR-REC-33`〜`35`）。ここで作った名前がそのまま配布物に出る。（`TR-RCL-03` / `TR-RCL-08` / `TR-RCL-27`）。
//!
//! インベントリからアルゴリズムで生成する。 第三者の配布リストを同梱しない
//! （`TR-RCL-02`）。
//!
//! ## 決定性
//!
//! 同じプリセットと同じインベントリ版からは、行の順序を含めて常に同一のリストを得る
//! （`TR-RCL-27`）。乱数を使わない。
//!
//! ## ファイル名
//!
//! 書き出しは ASCII 固定（`DEC-PKG-004`）。行テキストは日本語のまま持ち、
//! ファイル名は行 ID から ASCII で生成する（`TR-RCL-08`）。

use std::collections::BTreeSet;

use crate::alias::{self, Method};
use crate::inventory::{Unit, UnitSet, consonants, transition_vowels, units};
use crate::names;

/// 録音リストの1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 行 ID。台帳との突き合わせはこれで行う（`TR-RCL-18`）。
    pub id: String,
    /// 読み上げるテキスト。日本語のまま（`TR-RCL-08`）。
    pub text: String,
    /// この行が生む収録単位。
    pub units: Vec<Unit>,
    /// ファイル名。ASCII 固定（`DEC-PKG-004`）。拡張子を含まない。
    pub file_stem: String,
}

/// 生成の失敗。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ReclistError {
    /// 1行あたりの単位数が範囲外。
    #[error("1行あたりの単位数 {got} が範囲外（1〜{max}）")]
    UnitsPerRow { got: usize, max: usize },
    /// ファイル名の条件を満たせなかった（`TR-RCL-08`）。
    #[error("ファイル名の条件を満たせない行がある")]
    UnsafeFileName,
    /// その方式には行が短すぎる。
    ///
    /// 連続音と CVVC は隣接から遷移を作るので、1モーラの行では1つも覆えない。
    #[error("{method} には1行あたり2単位以上が要る")]
    RowTooShort { method: &'static str },
}

impl ReclistError {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::UnitsPerRow { .. } => "reclist.units_per_row_out_of_range",
            Self::UnsafeFileName => "reclist.unsafe_file_name",
            Self::RowTooShort { .. } => "reclist.row_too_short",
        }
    }
}

/// 1行あたりの単位数の上限（`TR-RCL-03`）。
pub const MAX_UNITS_PER_ROW: usize = 8;
/// 既定の単位数。
pub const DEFAULT_UNITS_PER_ROW: usize = 5;

/// 単独音の録音リストを生成する（`TR-RCL-03`）。
///
/// 同一行内の単位は子音行が揃うように並べる（例: か き く け こ）。
/// インベントリが既にその順で並んでいるので、順に詰めるだけで揃う。
#[tracing::instrument(fields(set = ?set, per_row))]
pub fn generate_single(set: UnitSet, per_row: usize) -> Result<Vec<Row>, ReclistError> {
    single_for(set, per_row, None)
}

/// 単独音を、欲しいエイリアスだけに絞って生成する（`TR-RCL-16`, `DEC-RCL-011`）。
///
/// 単独音のエイリアスは仮名そのもの。 絞るのは単位の並びを間引くだけで、
/// 子音行が揃う並び（`TR-RCL-03`）はそのまま保たれる。
fn single_for(
    set: UnitSet,
    per_row: usize,
    wanted: Option<&BTreeSet<String>>,
) -> Result<Vec<Row>, ReclistError> {
    if per_row == 0 || per_row > MAX_UNITS_PER_ROW {
        return Err(ReclistError::UnitsPerRow {
            got: per_row,
            max: MAX_UNITS_PER_ROW,
        });
    }
    let all: Vec<Unit> = units(set)
        .into_iter()
        .filter(|u| wanted.is_none_or(|w| w.contains(u.kana)))
        .collect();
    let mut rows = Vec::new();
    let mut chunk: Vec<Unit> = Vec::new();
    let flush = |chunk: &mut Vec<Unit>, rows: &mut Vec<Row>| {
        if chunk.is_empty() {
            return;
        }
        let index = rows.len() + 1;
        let id = format!("s{index:03}");
        let text = chunk.iter().map(|u| u.kana).collect::<Vec<_>>().join(" ");
        rows.push(Row {
            file_stem: id.clone(),
            id,
            text,
            units: std::mem::take(chunk),
        });
    };

    for u in all {
        // 子音が変わったら行を切る。 そうしないと「こ が」のように行がまたぐ。
        let boundary = chunk.first().is_some_and(|f| f.consonant != u.consonant);
        if boundary || chunk.len() >= per_row {
            flush(&mut chunk, &mut rows);
        }
        chunk.push(u);
    }
    flush(&mut chunk, &mut rows);

    validate_file_names(&rows)?;
    tracing::debug!(rows = rows.len(), "録音リストを生成した");
    Ok(rows)
}

/// 行が生むエイリアス（`TR-RCL-18`）。
///
/// 行に持たせず、単位列と方式から導く。 同じ値を2箇所に置くと片方だけが変わる
/// （`TR-RCL-01`）。綴りの定義は [`crate::alias`] が持つ。
///
/// 並びは行の中での初出順。 順がぶれると、同じリストから違う差分が出る
/// （`TR-RCL-27`）。
#[must_use]
pub fn row_aliases(method: Method, line: &[Unit]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    match method {
        Method::Single => {
            for u in line {
                push_unique(&mut out, u.kana.to_owned());
            }
        }
        // 先頭は `- CV`、以降は「直前の母音 CV」（`TR-SYN-12`）。
        Method::Sequential => {
            for (i, u) in line.iter().enumerate() {
                let a = match i.checked_sub(1) {
                    Some(prev) => format!("{} {}", line[prev].vowel, u.kana),
                    None => format!("- {}", u.kana),
                };
                push_unique(&mut out, a);
            }
        }
        // CV は先頭だけ語頭形、以降は素（`DEC-SYN-011`）。
        // 隣接から VC、行末から語尾（`TR-RCL-05`）。
        Method::Cvvc => {
            for (i, u) in line.iter().enumerate() {
                let cv = if i == 0 {
                    format!("- {}", u.kana)
                } else {
                    u.kana.to_owned()
                };
                push_unique(&mut out, cv);
                if let Some(next) = line.get(i + 1)
                    && !next.consonant.is_empty()
                {
                    push_unique(&mut out, alias::vc_alias(u.vowel, next.consonant));
                }
            }
            if let Some(last) = line.last() {
                push_unique(&mut out, alias::ending_alias(last.vowel));
            }
        }
    }
    out
}

fn push_unique(out: &mut Vec<String>, a: String) {
    if !out.contains(&a) {
        out.push(a);
    }
}

/// 行に単位を足してよいか（`TR-RCL-07`）。
///
/// 見るのは (b) 同一 CV の3連続と (c) 撥音の連続だけ。 (a) のモーラ数は
/// 呼び出し側のループが持つ。
///
/// **被覆が制約に優先する**（`DEC-RCL-009`）。 守ると覆えなくなる辺があるときは、
/// 呼び出し側がここを通さずに足す。辺「n ん」がその例で、母音クラス `n` を持つ
/// 単位は「ん」だけなので、連続させないと永久に覆えない。
fn may_append(table: &[Unit], line: &[usize], next: usize) -> bool {
    if line.len() >= 2 && line[line.len() - 1] == next && line[line.len() - 2] == next {
        return false;
    }
    if let Some(&last) = line.last()
        && table[last].kana == "ん"
        && table[next].kana == "ん"
    {
        return false;
    }
    true
}

/// 添字の行を [`Row`] へ畳む。
fn rows_from(table: &[Unit], lines: &[Vec<usize>], prefix: char) -> Vec<Row> {
    lines
        .iter()
        .enumerate()
        .map(|(i, line)| {
            let id = format!("{prefix}{:03}", i + 1);
            let units: Vec<Unit> = line.iter().map(|&u| table[u].clone()).collect();
            Row {
                file_stem: id.clone(),
                id,
                text: units.iter().map(|u| u.kana).collect::<Vec<_>>().join(" "),
                units,
            }
        })
        .collect()
}

fn check_per_row(per_row: usize, method: &'static str, min: usize) -> Result<(), ReclistError> {
    if per_row == 0 || per_row > MAX_UNITS_PER_ROW {
        return Err(ReclistError::UnitsPerRow {
            got: per_row,
            max: MAX_UNITS_PER_ROW,
        });
    }
    if per_row < min {
        return Err(ReclistError::RowTooShort { method });
    }
    Ok(())
}

/// 連続音の録音リストを生成する（`TR-RCL-04`）。
///
/// 「先行母音クラス × CV 単位」を有向辺とみなし、1行を有向路として全辺を覆う。
///
/// 段が2つある。 第1段で全単位を1度ずつ行頭に置く——行の先頭は `- CV` を生む
/// 唯一の位置なので、語頭 CV を全種そろえるにはここしかない（`TR-RCL-21`）。
/// 第2段で、第1段が拾い切れなかった辺を短い行で回収する。
///
/// 第2段の行は語頭 CV を重複して生む。 始点の母音を作る単位を行頭に置くしかなく、
/// その単位の語頭は第1段で既に出ている。重複は書き出し側が1つに畳む。
#[tracing::instrument(fields(set = ?set, per_row))]
pub fn generate_sequential(set: UnitSet, per_row: usize) -> Result<Vec<Row>, ReclistError> {
    sequential_for(set, per_row, None)
}

/// 連続音を、欲しいエイリアスだけに絞って生成する（`TR-RCL-16`, `DEC-RCL-011`）。
///
/// `wanted` が `None` なら全部。 与えれば、そこに載っている綴りだけを狙う。
fn sequential_for(
    set: UnitSet,
    per_row: usize,
    wanted: Option<&BTreeSet<String>>,
) -> Result<Vec<Row>, ReclistError> {
    check_per_row(per_row, "連続音", 2)?;
    let table = units(set);
    let vowels = transition_vowels(set);
    let want = |a: &str| wanted.is_none_or(|w| w.contains(a));

    // 辺 = (先行母音の添字, 単位の添字)。
    let mut remaining: BTreeSet<(usize, usize)> = (0..vowels.len())
        .flat_map(|v| (0..table.len()).map(move |u| (v, u)))
        .filter(|(v, u)| want(&format!("{} {}", vowels[*v], table[*u].kana)))
        .collect();
    let vowel_of = |u: usize| {
        vowels
            .iter()
            .position(|v| *v == table[u].vowel)
            .unwrap_or_default()
    };

    let mut lines: Vec<Vec<usize>> = Vec::new();
    // 行頭は `- CV` を生む唯一の位置。 欲しい語頭だけを行頭に置く。
    for start in 0..table.len() {
        if !want(&format!("- {}", table[start].kana)) {
            continue;
        }
        let mut line = vec![start];
        extend_sequential(&table, &mut line, &mut remaining, per_row, vowel_of);
        lines.push(line);
    }

    while let Some(&(vi, ui)) = remaining.iter().next() {
        // 始点の母音を作る単位を行頭に置く。 そうしないとその辺へ入れない。
        let carrier = (0..table.len())
            .find(|&w| table[w].vowel == vowels[vi])
            .unwrap_or(ui);
        let mut line = vec![carrier, ui];
        remaining.remove(&(vi, ui));
        extend_sequential(&table, &mut line, &mut remaining, per_row, vowel_of);
        lines.push(line);
    }

    let rows = rows_from(&table, &lines, 'q');
    validate_file_names(&rows)?;
    tracing::debug!(rows = rows.len(), "連続音の録音リストを生成した");
    Ok(rows)
}

fn extend_sequential(
    table: &[Unit],
    line: &mut Vec<usize>,
    remaining: &mut BTreeSet<(usize, usize)>,
    per_row: usize,
    vowel_of: impl Fn(usize) -> usize,
) {
    while line.len() < per_row {
        let Some(&last) = line.last() else { break };
        let vi = vowel_of(last);
        let next =
            (0..table.len()).find(|&c| remaining.contains(&(vi, c)) && may_append(table, line, c));
        let Some(c) = next else { break };
        remaining.remove(&(vi, c));
        line.push(c);
    }
}

/// CVVC の録音リストを生成する（`TR-RCL-05`）。
///
/// 1行で3種を同時に回収する。 先頭の `- CV`、以降の素の CV、隣接から出る VC、
/// 行末の語尾。どれか1種だけを狙うと行数が3倍になる。
///
/// 段が4つある。 第1段で全単位を行頭に置き、ついでに VC と素の CV を拾う。
/// 残った VC・素の CV・語尾を、第2〜4段がそれぞれ回収する。
#[tracing::instrument(fields(set = ?set, per_row))]
pub fn generate_cvvc(set: UnitSet, per_row: usize) -> Result<Vec<Row>, ReclistError> {
    cvvc_for(set, per_row, None)
}

/// CVVC を、欲しいエイリアスだけに絞って生成する（`TR-RCL-16`, `DEC-RCL-011`）。
fn cvvc_for(
    set: UnitSet,
    per_row: usize,
    wanted: Option<&BTreeSet<String>>,
) -> Result<Vec<Row>, ReclistError> {
    check_per_row(per_row, "CVVC", 2)?;
    let table = units(set);
    let vowels = transition_vowels(set);
    let cons = consonants(set);
    let want = |a: &str| wanted.is_none_or(|w| w.contains(a));

    let vowel_of = |u: usize| {
        vowels
            .iter()
            .position(|v| *v == table[u].vowel)
            .unwrap_or_default()
    };
    // 欲しいもの。覆ったら消す。
    let mut want_mid: BTreeSet<usize> = (0..table.len()).filter(|u| want(table[*u].kana)).collect();
    let mut want_vc: BTreeSet<(usize, usize)> = (0..vowels.len())
        .flat_map(|v| (0..cons.len()).map(move |c| (v, c)))
        .filter(|(v, c)| want(&alias::vc_alias(vowels[*v], cons[*c])))
        .collect();
    let mut want_end: BTreeSet<usize> = (0..vowels.len())
        .filter(|v| want(&alias::ending_alias(vowels[*v])))
        .collect();
    let consonant_of = |u: usize| cons.iter().position(|c| *c == table[u].consonant);

    let mut lines: Vec<Vec<usize>> = Vec::new();
    let close = |line: &mut Vec<usize>, want_end: &mut BTreeSet<usize>| {
        if let Some(&last) = line.last() {
            want_end.remove(&vowel_of(last));
        }
    };

    // 第1段。欲しい語頭だけを行頭へ。
    for start in 0..table.len() {
        if !want(&format!("- {}", table[start].kana)) {
            continue;
        }
        let mut line = vec![start];
        extend_cvvc(
            &table,
            &mut line,
            &mut want_mid,
            &mut want_vc,
            per_row,
            &vowel_of,
            &consonant_of,
        );
        close(&mut line, &mut want_end);
        lines.push(line);
    }

    // 第2段。残った VC。始点の母音を作る単位と、その子音を持つ単位を並べる。
    while let Some(&(vi, ci)) = want_vc.iter().next() {
        let carrier = (0..table.len())
            .find(|&w| table[w].vowel == vowels[vi])
            .unwrap_or(0);
        let target = (0..table.len())
            .find(|&w| table[w].consonant == cons[ci])
            .unwrap_or(0);
        let mut line = vec![carrier, target];
        want_vc.remove(&(vi, ci));
        want_mid.remove(&target);
        extend_cvvc(
            &table,
            &mut line,
            &mut want_mid,
            &mut want_vc,
            per_row,
            &vowel_of,
            &consonant_of,
        );
        close(&mut line, &mut want_end);
        lines.push(line);
    }

    // 第3段。残った素の CV。行頭以外に置けばよい。
    while let Some(&mid) = want_mid.iter().next() {
        let mut line = vec![0, mid];
        want_mid.remove(&mid);
        extend_cvvc(
            &table,
            &mut line,
            &mut want_mid,
            &mut want_vc,
            per_row,
            &vowel_of,
            &consonant_of,
        );
        close(&mut line, &mut want_end);
        lines.push(line);
    }

    // 第4段。残った語尾。その母音の単位で行を終える。
    while let Some(&vi) = want_end.iter().next() {
        let tail = (0..table.len())
            .find(|&w| table[w].vowel == vowels[vi])
            .unwrap_or(0);
        let head = (0..table.len()).find(|&w| w != tail).unwrap_or(tail);
        lines.push(vec![head, tail]);
        want_end.remove(&vi);
    }

    let rows = rows_from(&table, &lines, 'c');
    validate_file_names(&rows)?;
    tracing::debug!(rows = rows.len(), "CVVC の録音リストを生成した");
    Ok(rows)
}

fn extend_cvvc(
    table: &[Unit],
    line: &mut Vec<usize>,
    want_mid: &mut BTreeSet<usize>,
    want_vc: &mut BTreeSet<(usize, usize)>,
    per_row: usize,
    vowel_of: &impl Fn(usize) -> usize,
    consonant_of: &impl Fn(usize) -> Option<usize>,
) {
    while line.len() < per_row {
        let Some(&last) = line.last() else { break };
        let vi = vowel_of(last);
        // 何か新しく覆えるものだけを足す。 覆えないものを足すと行が伸びるだけ。
        let next = (0..table.len()).find(|&c| {
            if !may_append(table, line, c) {
                return false;
            }
            let new_vc = consonant_of(c).is_some_and(|ci| want_vc.contains(&(vi, ci)));
            new_vc || want_mid.contains(&c)
        });
        let Some(c) = next else { break };
        if let Some(ci) = consonant_of(c) {
            want_vc.remove(&(vi, ci));
        }
        want_mid.remove(&c);
        line.push(c);
    }
}

/// 選んだノート群を歌えるようにする最小の行を詰め直す（`TR-RCL-16`, `DEC-RCL-011`）。
///
/// > 曲先行のミニ音源は、選んだノート群が要求するエイリアスだけを被覆する行として
/// > 詰め直す。フルリストの部分集合に限らない
///
/// **部分集合に限ると読む量が跳ね上がる。** 行の途中でやめられないので、
/// 9 単位が欲しくても 27 モーラ読むことになる（`DEC-RCL-011` の実測）。
///
/// 詰め直した行が生むエイリアスの綴りは、フルリストの行が生むものと同じ。
/// カバレッジはエイリアスで数えるので（`TR-PKG-22`）、**録った分はそのまま
/// フル方式の被覆に効く。** 録り直しにはならない。
///
/// 行 ID は `p` で始まる。 プリセットから生成した行（`s` / `q` / `c`）と
/// 混ざらない——台帳は出どころを見分けられる必要がある（`TR-RCL-18`）。
///
/// 決定性は保つ（`TR-RCL-27`）。 同じ `required` からは常に同じリストを得る。
///
/// # Errors
///
/// 1行あたりの単位数が範囲外、その方式に短すぎる、ファイル名の条件を満たせない。
#[tracing::instrument(skip(required), fields(set = ?set, per_row, want = required.len()))]
pub fn repack(
    method: Method,
    set: UnitSet,
    required: &BTreeSet<String>,
    per_row: usize,
) -> Result<Vec<Row>, ReclistError> {
    let rows = match method {
        Method::Single => single_for(set, per_row, Some(required))?,
        Method::Sequential => sequential_for(set, per_row, Some(required))?,
        Method::Cvvc => cvvc_for(set, per_row, Some(required))?,
    };
    // 出どころが分かる ID に振り直す。
    let rows: Vec<Row> = rows
        .into_iter()
        .enumerate()
        .map(|(i, r)| {
            let id = format!("p{:03}", i + 1);
            Row {
                file_stem: id.clone(),
                id,
                ..r
            }
        })
        .collect();
    validate_file_names(&rows)?;
    tracing::debug!(rows = rows.len(), "選択から録音リストを詰め直した");
    Ok(rows)
}

/// ファイル名の条件を確かめる（`TR-RCL-08`）。
///
/// 文字・予約名・長さの規則は [`crate::names`] が持つ。 ここで見るのは
/// 「リストの中で一意か」だけ——他は録音リストに固有の条件ではない。
///
/// 失敗を黙って通さない。
fn validate_file_names(rows: &[Row]) -> Result<(), ReclistError> {
    let files: Vec<String> = rows
        .iter()
        .map(|r| format!("{}.wav", r.file_stem))
        .collect();
    for f in &files {
        if !names::check_file_name(f).is_empty() {
            return Err(ReclistError::UnsafeFileName);
        }
    }
    let refs: Vec<&str> = files.iter().map(String::as_str).collect();
    if !names::case_collisions(&refs).is_empty() {
        return Err(ReclistError::UnsafeFileName);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中核 102 単位が全部リストに入る（`DEC-RCL-004`）。
    #[test]
    fn 中核セットの全単位が行に入る() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let total: usize = rows.iter().map(|r| r.units.len()).sum();
        assert_eq!(total, 102, "全単位が入る");
        assert!(rows.len() <= 40, "行数が過大でない: {} 行", rows.len());
    }

    /// 拡張 144 単位が全部リストに入る。
    #[test]
    fn 拡張セットの全単位が行に入る() {
        let rows = generate_single(UnitSet::Extended, 5).expect("生成できる");
        let total: usize = rows.iter().map(|r| r.units.len()).sum();
        assert_eq!(total, 144, "全単位が入る");
        assert!(rows.len() <= 60, "行数が過大でない: {} 行", rows.len());
    }

    /// 同一行内の単位は子音行が揃う（`TR-RCL-03`）。
    #[test]
    fn 行の中で子音が揃う() {
        for r in generate_single(UnitSet::Core, 5).expect("生成できる") {
            let first = r.units[0].consonant;
            assert!(
                r.units.iter().all(|u| u.consonant == first),
                "行 {} に別の子音が混ざる: {}",
                r.id,
                r.text
            );
        }
    }

    /// 生成が決定的（`TR-RCL-27`）。
    #[test]
    fn 何度生成しても同じになる() {
        let a = generate_single(UnitSet::Core, 5).expect("生成できる");
        let b = generate_single(UnitSet::Core, 5).expect("生成できる");
        assert_eq!(a, b);
    }

    /// ファイル名は ASCII で一意（`TR-RCL-08` / `DEC-PKG-004`）。
    #[test]
    fn ファイル名は_ascii_で一意() {
        let rows = generate_single(UnitSet::Extended, 5).expect("生成できる");
        let mut seen = std::collections::BTreeSet::new();
        for r in &rows {
            assert!(r.file_stem.is_ascii(), "{} が ASCII でない", r.file_stem);
            assert!(seen.insert(&r.file_stem), "{} が重複", r.file_stem);
        }
    }

    /// 読み上げるテキストは日本語のまま（`TR-RCL-08`）。
    #[test]
    fn 読み上げテキストは日本語のまま() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        assert!(rows[0].text.contains('あ'), "{}", rows[0].text);
        assert!(!rows[0].text.is_ascii());
    }

    #[test]
    fn 単位数が範囲外なら弾く() {
        assert!(matches!(
            generate_single(UnitSet::Core, 0),
            Err(ReclistError::UnitsPerRow { .. })
        ));
        assert!(matches!(
            generate_single(UnitSet::Core, 9),
            Err(ReclistError::UnitsPerRow { .. })
        ));
    }

    /// 上限の 8 単位でも生成できる。
    #[test]
    fn 上限の八単位でも生成できる() {
        let rows = generate_single(UnitSet::Core, MAX_UNITS_PER_ROW).expect("生成できる");
        assert!(rows.iter().all(|r| r.units.len() <= MAX_UNITS_PER_ROW));
        let total: usize = rows.iter().map(|r| r.units.len()).sum();
        assert_eq!(total, 102);
    }

    /// その方式の要求表を、生成したリストが過不足なく覆う。
    ///
    /// 要求表は [`crate::alias`] の綴りから組む。 ここで別に組み直すと、
    /// 表と生成器が同じ間違いをしたときに気づけない。
    fn covered(method: Method, rows: &[Row]) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        for r in rows {
            out.extend(row_aliases(method, &r.units));
        }
        out
    }

    /// 連続音は全辺を覆う（`TR-RCL-04`）。
    ///
    /// 語頭 CV 144 ＋ 先行母音 6 × CV 144 = 1008。
    #[test]
    fn 連続音は全辺を覆う() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let got = covered(Method::Sequential, &rows);
        let table = units(UnitSet::Extended);
        let vowels = transition_vowels(UnitSet::Extended);
        for u in &table {
            assert!(got.contains(&format!("- {}", u.kana)), "語頭 {}", u.kana);
            for v in &vowels {
                assert!(got.contains(&format!("{v} {}", u.kana)), "{v} {}", u.kana);
            }
        }
        assert_eq!(got.len(), table.len() * (vowels.len() + 1));
    }

    /// 遷移に無駄が無い。 行の長さの合計から行数を引くと、覆った辺の数に一致する。
    #[test]
    fn 連続音は同じ辺を二度録らない() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let moras: usize = rows.iter().map(|r| r.units.len()).sum();
        let table = units(UnitSet::Extended).len();
        let vowels = transition_vowels(UnitSet::Extended).len();
        assert_eq!(
            moras - rows.len(),
            table * vowels,
            "遷移の数が辺の数と一致する"
        );
    }

    /// CVVC は CV・VC・語尾を覆う（`TR-RCL-05`）。
    ///
    /// 語頭 CV 144 ＋ 素の CV 144 ＋ VC 180 ＋ 語尾 6 = 474。
    #[test]
    fn cvvc_は三種すべてを覆う() {
        let rows = generate_cvvc(UnitSet::Extended, 8).expect("生成できる");
        let got = covered(Method::Cvvc, &rows);
        let table = units(UnitSet::Extended);
        let vowels = transition_vowels(UnitSet::Extended);
        for u in &table {
            assert!(got.contains(&format!("- {}", u.kana)), "語頭 {}", u.kana);
            assert!(got.contains(u.kana), "素の {}", u.kana);
        }
        for vc in crate::inventory::vc_units(UnitSet::Extended) {
            let a = alias::vc_alias(vc.vowel, vc.consonant);
            assert!(got.contains(&a), "VC {a}");
        }
        for v in &vowels {
            assert!(got.contains(&alias::ending_alias(v)), "語尾 {v}");
        }
        assert_eq!(got.len(), table.len() * 2 + 180 + vowels.len());
    }

    /// 行数の下限は語頭 CV の種類数（`DEC-RCL-009`）。
    ///
    /// 語頭 CV は行の先頭にしか出ない。 これを下回る生成は、どこかの語頭を落としている。
    #[test]
    fn cvvc_の行数は語頭_cv_の数を下回らない() {
        let rows = generate_cvvc(UnitSet::Extended, 8).expect("生成できる");
        assert!(
            rows.len() >= units(UnitSet::Extended).len(),
            "{} 行",
            rows.len()
        );
        let moras: usize = rows.iter().map(|r| r.units.len()).sum();
        assert!(moras <= 480, "モーラ数が過大: {moras}");
    }

    /// 同じエイリアスを1行の中で二度作らない（`TR-ALN-20` (6)）。
    ///
    /// 同一 WAV 内で重複すると、oto がどちらを指すか決められない。
    #[test]
    fn 行の中でエイリアスが重複しない() {
        for (method, rows) in [
            (
                Method::Sequential,
                generate_sequential(UnitSet::Extended, 8).expect("生成できる"),
            ),
            (
                Method::Cvvc,
                generate_cvvc(UnitSet::Extended, 8).expect("生成できる"),
            ),
        ] {
            for r in &rows {
                let a = row_aliases(method, &r.units);
                let uniq: BTreeSet<&String> = a.iter().collect();
                assert_eq!(a.len(), uniq.len(), "行 {} が重複を持つ: {a:?}", r.id);
            }
        }
    }

    /// 制約 (b) と (c) を守る。 ただし被覆が優先する（`DEC-RCL-009`）。
    ///
    /// 「ん」の連続は辺「n ん」を覆う行にだけ出る。 それ以外に出たら制約の取りこぼし。
    #[test]
    fn 制約は被覆を落とさない範囲で守られる() {
        let rows = generate_sequential(UnitSet::Extended, 8).expect("生成できる");
        let mut nn_rows = 0;
        for r in &rows {
            for w in r.units.windows(3) {
                assert!(
                    !(w[0] == w[1] && w[1] == w[2]),
                    "行 {} に同一 CV の3連続: {}",
                    r.id,
                    r.text
                );
            }
            if r.units
                .windows(2)
                .any(|w| w[0].kana == "ん" && w[1].kana == "ん")
            {
                nn_rows += 1;
            }
        }
        assert_eq!(nn_rows, 1, "「ん」の連続は辺「n ん」を覆う1行だけ");
    }

    /// 生成が決定的（`TR-RCL-27`）。
    #[test]
    fn 方式を広げても生成は決定的() {
        assert_eq!(
            generate_sequential(UnitSet::Extended, 8).expect("生成できる"),
            generate_sequential(UnitSet::Extended, 8).expect("生成できる")
        );
        assert_eq!(
            generate_cvvc(UnitSet::Core, 5).expect("生成できる"),
            generate_cvvc(UnitSet::Core, 5).expect("生成できる")
        );
    }

    /// 1モーラの行では隣接が作れない。
    #[test]
    fn 一単位の行では方式が成り立たない() {
        assert!(matches!(
            generate_sequential(UnitSet::Core, 1),
            Err(ReclistError::RowTooShort { .. })
        ));
        assert!(matches!(
            generate_cvvc(UnitSet::Core, 1),
            Err(ReclistError::RowTooShort { .. })
        ));
    }

    /// 単独音のエイリアスは仮名そのもの（`TR-SYN-12`）。
    #[test]
    fn 単独音の行が生むのは仮名だけ() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let got = covered(Method::Single, &rows);
        assert_eq!(got.len(), 102);
        assert!(got.contains("か"));
    }

    /// 予約名を弾く（`TR-RCL-08`）。
    #[test]
    fn 予約名は弾かれる() {
        let rows = vec![Row {
            id: "con".into(),
            text: "こん".into(),
            units: Vec::new(),
            file_stem: "CON".into(),
        }];
        assert_eq!(
            validate_file_names(&rows),
            Err(ReclistError::UnsafeFileName)
        );
    }

    /// 大文字小文字だけが違う名前も重複として弾く（`TR-REC-34`）。
    #[test]
    fn 大文字小文字だけの違いも重複扱い() {
        let mk = |s: &str| Row {
            id: s.into(),
            text: s.into(),
            units: Vec::new(),
            file_stem: s.into(),
        };
        assert_eq!(
            validate_file_names(&[mk("s001"), mk("S001")]),
            Err(ReclistError::UnsafeFileName)
        );
    }
}

#[cfg(test)]
mod repack_tests {
    use super::*;
    use crate::song::{Note, Provenance, Song};

    fn song(lyrics: &[&str], midis: &[i32]) -> Song {
        Song {
            title: "t".to_owned(),
            notes: lyrics
                .iter()
                .zip(midis)
                .map(|(l, m)| Note {
                    lyric: (*l).to_owned(),
                    midi: *m,
                    ticks: 480,
                })
                .collect(),
            provenance: Provenance {
                source: String::new(),
                license: String::new(),
            },
            tempo_bpm: 120.0,
            default_portamento_ms: 0.0,
        }
    }

    fn covered(method: Method, rows: &[Row]) -> BTreeSet<String> {
        rows.iter()
            .flat_map(|r| row_aliases(method, &r.units))
            .collect()
    }

    /// 選んだ音だけを覆う（`TR-RCL-16`, `DEC-RCL-011`）。
    #[test]
    fn 選んだ音だけを覆う() {
        let s = song(&["さ", "く", "ら"], &[60, 60, 62]);
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            let need = s.required_aliases(method, UnitSet::Core);
            let rows = repack(method, UnitSet::Core, &need, 8).expect("詰め直せる");
            let got = covered(method, &rows);
            assert!(need.is_subset(&got), "{method:?} が要るものを覆わない");
        }
    }

    /// 部分集合よりずっと短い（`DEC-RCL-011` の実測）。
    #[test]
    fn 部分集合より短い() {
        let s = song(&["さ", "く", "ら"], &[60, 60, 62]);
        let need = s.required_aliases(Method::Sequential, UnitSet::Core);

        let full = generate_sequential(UnitSet::Core, 8).expect("生成できる");
        let plan = crate::plan::rows_to_cover(
            &s.required_aliases(Method::Single, UnitSet::Core),
            &generate_single(UnitSet::Core, 5).expect("生成できる"),
        );
        let packed = repack(Method::Sequential, UnitSet::Core, &need, 8).expect("詰め直せる");
        let packed_moras: usize = packed.iter().map(|r| r.units.len()).sum();
        let subset_moras: usize = plan.rows.iter().map(|r| r.units.len()).sum();

        assert!(packed.len() < full.len(), "フルリストより短い");
        assert!(
            packed_moras < subset_moras,
            "部分集合 {subset_moras} モーラより短い: {packed_moras}"
        );
    }

    /// 出どころが分かる行 ID（`TR-RCL-18`）。
    ///
    /// プリセットから生成した行（`s` / `q` / `c`）と混ざらない。
    #[test]
    fn 詰め直した行は_id_で見分けられる() {
        let s = song(&["か"], &[60]);
        let need = s.required_aliases(Method::Single, UnitSet::Core);
        let rows = repack(Method::Single, UnitSet::Core, &need, 5).expect("詰め直せる");
        assert!(
            rows.iter().all(|r| r.id.starts_with('p')),
            "{:?}",
            rows[0].id
        );
        assert!(rows.iter().all(|r| r.file_stem.is_ascii()));
    }

    /// 同じ選択からは常に同じリスト（`TR-RCL-27`, `DEC-RCL-011`）。
    #[test]
    fn 同じ選択からは同じリスト() {
        let s = song(&["さ", "く", "ら"], &[60, 60, 62]);
        let need = s.required_aliases(Method::Sequential, UnitSet::Core);
        assert_eq!(
            repack(Method::Sequential, UnitSet::Core, &need, 8).expect("詰め直せる"),
            repack(Method::Sequential, UnitSet::Core, &need, 8).expect("詰め直せる")
        );
    }

    /// 詰め直した行のエイリアスは、フルリストの綴りと同じ（`DEC-RCL-011`）。
    ///
    /// **これが「録り直しにならない」の根拠。** 綴りが同じなので、
    /// カバレッジ（エイリアスで数える）にそのまま効く。
    #[test]
    fn 綴りがフルリストと同じ() {
        let s = song(&["さ", "く", "ら"], &[60, 60, 62]);
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            let need = s.required_aliases(method, UnitSet::Core);
            let packed = covered(
                method,
                &repack(method, UnitSet::Core, &need, 8).expect("詰め直せる"),
            );
            let full = covered(
                method,
                &match method {
                    Method::Single => generate_single(UnitSet::Core, 5),
                    Method::Sequential => generate_sequential(UnitSet::Core, 8),
                    Method::Cvvc => generate_cvvc(UnitSet::Core, 8),
                }
                .expect("生成できる"),
            );
            let stray: Vec<&String> = packed.difference(&full).collect();
            assert!(
                stray.is_empty(),
                "{method:?} がフルリストに無い綴りを作る: {stray:?}"
            );
        }
    }

    /// 何も要らなければ何も作らない。
    #[test]
    fn 空の選択は空のリスト() {
        let need = BTreeSet::new();
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            assert!(
                repack(method, UnitSet::Core, &need, 8)
                    .expect("通る")
                    .is_empty()
            );
        }
    }
}
