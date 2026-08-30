//! 録音リストの生成（`TR-RCL-03`, `TR-RCL-08`, `TR-REC-33`, `TR-REC-34`, `TR-REC-35`）。
//!
//! **ファイル名は CP932 で表現でき、大小を無視して一意で、前後・連続・全角の空白を持たない**
//! （`TR-RCL-08`, `TR-REC-33`〜`35`）。ここで作った名前がそのまま配布物に出る。（`TR-RCL-03` / `TR-RCL-08` / `TR-RCL-27`）。
//!
//! **インベントリからアルゴリズムで生成する。** 第三者の配布リストを同梱しない
//! （`TR-RCL-02`）。
//!
//! ## 決定性
//!
//! **同じプリセットと同じインベントリ版からは、行の順序を含めて常に同一のリストを得る**
//! （`TR-RCL-27`）。乱数を使わない。
//!
//! ## ファイル名
//!
//! **書き出しは ASCII 固定**（`DEC-PKG-004`）。行テキストは日本語のまま持ち、
//! ファイル名は行 ID から ASCII で生成する（`TR-RCL-08`）。

use crate::inventory::{Unit, UnitSet, units};

/// 録音リストの1行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    /// 行 ID。**台帳との突き合わせはこれで行う**（`TR-RCL-18`）。
    pub id: String,
    /// 読み上げるテキスト。**日本語のまま**（`TR-RCL-08`）。
    pub text: String,
    /// この行が生む収録単位。
    pub units: Vec<Unit>,
    /// ファイル名。**ASCII 固定**（`DEC-PKG-004`）。拡張子を含まない。
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
}

impl ReclistError {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::UnitsPerRow { .. } => "reclist.units_per_row_out_of_range",
            Self::UnsafeFileName => "reclist.unsafe_file_name",
        }
    }
}

/// 1行あたりの単位数の上限（`TR-RCL-03`）。
pub const MAX_UNITS_PER_ROW: usize = 8;
/// 既定の単位数。
pub const DEFAULT_UNITS_PER_ROW: usize = 5;

/// Windows の予約名（`TR-RCL-08`）。
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 単独音の録音リストを生成する（`TR-RCL-03`）。
///
/// **同一行内の単位は子音行が揃うように並べる**（例: か き く け こ）。
/// インベントリが既にその順で並んでいるので、順に詰めるだけで揃う。
#[tracing::instrument(fields(set = ?set, per_row))]
pub fn generate_single(set: UnitSet, per_row: usize) -> Result<Vec<Row>, ReclistError> {
    if per_row == 0 || per_row > MAX_UNITS_PER_ROW {
        return Err(ReclistError::UnitsPerRow {
            got: per_row,
            max: MAX_UNITS_PER_ROW,
        });
    }
    let all = units(set);
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
        // **子音が変わったら行を切る。** そうしないと「こ が」のように行がまたぐ。
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

/// ファイル名の5条件を確かめる（`TR-RCL-08`）。
///
/// **失敗を黙って通さない。**
fn validate_file_names(rows: &[Row]) -> Result<(), ReclistError> {
    let mut seen = std::collections::BTreeSet::new();
    for r in rows {
        let n = &r.file_stem;
        // (a) ASCII 英数字・ハイフン・アンダースコアだけ
        if !n
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(ReclistError::UnsafeFileName);
        }
        // (b) 禁止文字は (a) で除かれている / (c) 予約名
        if RESERVED.contains(&n.to_ascii_uppercase().as_str()) {
            return Err(ReclistError::UnsafeFileName);
        }
        // (d) 一意
        if !seen.insert(n.to_ascii_lowercase()) {
            return Err(ReclistError::UnsafeFileName);
        }
        // (e) 拡張子込みで 255 バイト以内
        if n.len() + ".wav".len() > 255 {
            return Err(ReclistError::UnsafeFileName);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **中核 102 単位が全部リストに入る**（`DEC-RCL-004`）。
    #[test]
    fn 中核セットの全単位が行に入る() {
        let rows = generate_single(UnitSet::Core, 5).expect("生成できる");
        let total: usize = rows.iter().map(|r| r.units.len()).sum();
        assert_eq!(total, 102, "全単位が入る");
        assert!(rows.len() <= 40, "行数が過大でない: {} 行", rows.len());
    }

    /// **拡張 144 単位が全部リストに入る。**
    #[test]
    fn 拡張セットの全単位が行に入る() {
        let rows = generate_single(UnitSet::Extended, 5).expect("生成できる");
        let total: usize = rows.iter().map(|r| r.units.len()).sum();
        assert_eq!(total, 144, "全単位が入る");
        assert!(rows.len() <= 60, "行数が過大でない: {} 行", rows.len());
    }

    /// **同一行内の単位は子音行が揃う**（TR-RCL-03）。
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

    /// **生成が決定的**（TR-RCL-27）。
    #[test]
    fn 何度生成しても同じになる() {
        let a = generate_single(UnitSet::Core, 5).expect("生成できる");
        let b = generate_single(UnitSet::Core, 5).expect("生成できる");
        assert_eq!(a, b);
    }

    /// **ファイル名は ASCII で一意**（TR-RCL-08 / DEC-PKG-004）。
    #[test]
    fn ファイル名は_ascii_で一意() {
        let rows = generate_single(UnitSet::Extended, 5).expect("生成できる");
        let mut seen = std::collections::BTreeSet::new();
        for r in &rows {
            assert!(r.file_stem.is_ascii(), "{} が ASCII でない", r.file_stem);
            assert!(seen.insert(&r.file_stem), "{} が重複", r.file_stem);
        }
    }

    /// **読み上げるテキストは日本語のまま**（TR-RCL-08）。
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

    /// **予約名を弾く**（TR-RCL-08）。
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

    /// **大文字小文字だけが違う名前も重複として弾く**（TR-REC-34）。
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
