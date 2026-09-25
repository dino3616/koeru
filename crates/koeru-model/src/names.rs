//! 生成する名前の規則（`TR-PKG-16`, `TR-PKG-18`, `DEC-PKG-008`）。
//!
//! 録音リストのファイル名（`TR-RCL-08`）も配布パッケージのエントリ名も、
//! 最後は同じ制約に行き着く。 ここが単一の定義で、`crate::reclist` と
//! `koeru-package` の両方がここを通る。2箇所に書くと、片方だけが緩む。
//!
//! # 読み込む名前には課さない
//!
//! `TR-PKG-16` の「読み込む音源のファイル名にはこの制限を課さない」。
//! 日本語ファイル名の既存音源を開けなくなる。ここにあるのは生成側の規則だけ。

use std::collections::BTreeSet;

/// 拡張子を含めたファイル名の上限（バイト）。
///
/// FAT / NTFS / APFS のいずれも 255 で、ここが最も狭い共通線。
pub const MAX_NAME_BYTES: usize = 255;

/// Windows の予約名（`TR-PKG-16` (4)）。
///
/// 上付き数字の変種（`COM¹`）も基底名にできない。Windows は
/// `¹` `²` `³` を 1 / 2 / 3 と同一視する。
const RESERVED: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// 名前が使えない理由（`TR-PKG-16`）。
///
/// どれに引っかかったかを返す。 「使えません」だけでは直しようがない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameProblem {
    /// 空。
    Empty,
    /// 許していない文字が入っている。
    ///
    /// 予約文字・制御文字・`=` / `,` も全部ここに落ちる。
    /// 許可リスト方式なので、理由を分けても直し方は変わらない。
    DisallowedChars { chars: Vec<char> },
    /// 末尾がスペースかピリオド（`TR-PKG-16` (3)）。
    TrailingSpaceOrDot,
    /// Windows の予約名を基底名にしている（`TR-PKG-16` (4)）。
    ReservedBaseName,
    /// 長すぎる。
    TooLong { bytes: usize },
}

impl NameProblem {
    /// 送信してよい種別文字列。
    ///
    /// 名前そのものは送らない。 音源名やキャラクター名の一部でありうる。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Empty => "name.empty",
            Self::DisallowedChars { .. } => "name.disallowed_chars",
            Self::TrailingSpaceOrDot => "name.trailing_space_or_dot",
            Self::ReservedBaseName => "name.reserved_base_name",
            Self::TooLong { .. } => "name.too_long",
        }
    }
}

/// 許した文字かどうか（`TR-PKG-16` (1)）。
#[must_use]
pub const fn is_allowed(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '-' || c == '_'
}

/// 生成する名前を検査する（`TR-PKG-16`）。
///
/// 相手はパスの1区画。 区切りの `/` を含んだ文字列を渡さない。
/// 拡張子は含めてよい——(1) が `.` を弾くので、`a.wav` のような名前は
/// [`check_file_name`] のほうで検査する。
#[must_use]
pub fn check_segment(name: &str) -> Vec<NameProblem> {
    let mut out = Vec::new();
    if name.is_empty() {
        out.push(NameProblem::Empty);
        return out;
    }
    let bad: Vec<char> = name
        .chars()
        .filter(|c| !is_allowed(*c))
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if !bad.is_empty() {
        out.push(NameProblem::DisallowedChars { chars: bad });
    }
    // (1) を満たせば末尾はスペースにもピリオドにもならないが、
    // 条件として独立しているので別に見る。(1) を緩めたときに残る。
    if name.ends_with(' ') || name.ends_with('.') {
        out.push(NameProblem::TrailingSpaceOrDot);
    }
    if is_reserved(name) {
        out.push(NameProblem::ReservedBaseName);
    }
    if name.len() > MAX_NAME_BYTES {
        out.push(NameProblem::TooLong { bytes: name.len() });
    }
    out
}

/// 拡張子付きのファイル名を検査する（`TR-PKG-16`）。
///
/// 基底名だけを [`check_segment`] に掛け、予約名の判定も基底名で行う。
/// `CON.wav` は Windows で開けない。
#[must_use]
pub fn check_file_name(name: &str) -> Vec<NameProblem> {
    let (stem, ext) = match name.rsplit_once('.') {
        Some((s, e)) => (s, Some(e)),
        None => (name, None),
    };
    let mut out = check_segment(stem);
    if let Some(ext) = ext {
        for p in check_segment(ext) {
            // 拡張子が予約名（`a.con`）でも Windows は開ける。基底名だけを見る。
            if p != NameProblem::ReservedBaseName && !out.contains(&p) {
                out.push(p);
            }
        }
    }
    if name.len() > MAX_NAME_BYTES && !out.iter().any(|p| matches!(p, NameProblem::TooLong { .. }))
    {
        out.push(NameProblem::TooLong { bytes: name.len() });
    }
    out
}

/// Windows の予約名か（上付き数字の変種を含む）。
fn is_reserved(name: &str) -> bool {
    let folded: String = name
        .chars()
        .map(|c| match c {
            '¹' => '1',
            '²' => '2',
            '³' => '3',
            other => other.to_ascii_uppercase(),
        })
        .collect();
    RESERVED.contains(&folded.as_str())
}

/// 小文字化したときに衝突する名前を探す（`TR-PKG-20`）。
///
/// 返すのは衝突した小文字形。 macOS / Windows では再現せず、
/// 受け手が Linux で使ったときだけ壊れる。
#[must_use]
pub fn case_collisions(names: &[&str]) -> Vec<String> {
    let mut seen = BTreeSet::new();
    let mut dup = BTreeSet::new();
    for n in names {
        let lower = n.to_lowercase();
        if !seen.insert(lower.clone()) {
            dup.insert(lower);
        }
    }
    dup.into_iter().collect()
}

/// エイリアスが持ってはいけない形（`TR-PKG-18`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AliasProblem {
    /// 先頭または末尾に半角スペースがある。
    EdgeSpace,
    /// 半角スペースが2個以上続く。
    DoubleSpace,
    /// 全角スペース U+3000 を含む。
    IdeographicSpace,
    /// NFC ではない。
    NotNfc,
    /// 空。
    Empty,
}

impl AliasProblem {
    /// 送信してよい種別文字列。エイリアスそのものは送らない。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::EdgeSpace => "alias.edge_space",
            Self::DoubleSpace => "alias.double_space",
            Self::IdeographicSpace => "alias.ideographic_space",
            Self::NotNfc => "alias.not_nfc",
            Self::Empty => "alias.empty",
        }
    }
}

/// 生成するエイリアスを検査する（`TR-PKG-18`）。
#[must_use]
pub fn check_alias(alias: &str) -> Vec<AliasProblem> {
    let mut out = Vec::new();
    if alias.is_empty() {
        out.push(AliasProblem::Empty);
        return out;
    }
    if alias.starts_with(' ') || alias.ends_with(' ') {
        out.push(AliasProblem::EdgeSpace);
    }
    if alias.contains("  ") {
        out.push(AliasProblem::DoubleSpace);
    }
    if alias.contains('\u{3000}') {
        out.push(AliasProblem::IdeographicSpace);
    }
    if crate::text::to_nfc(alias) != alias {
        out.push(AliasProblem::NotNfc);
    }
    out
}

/// 表示名から配布名の既定値を作る（`DEC-PKG-008`）。
///
/// 非 ASCII を落とすので、日本語だけの名前からは何も残らない。
/// そのときは [`FALLBACK_DISTRIBUTION_NAME`] に倒す。**ここで作るのは
/// 入力欄の初期値であって、確定した名前ではない。** 本人が読める名前に
/// 直せることが前提（`DEC-PKG-008`）。
#[must_use]
pub fn default_distribution_name(display_name: &str) -> String {
    let mut out = String::with_capacity(display_name.len());
    for c in display_name.chars() {
        if is_allowed(c) {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            // 連続する置換を1つにまとめる。`a---b` のような名前を作らない。
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-');
    let mut name = if trimmed.is_empty() {
        FALLBACK_DISTRIBUTION_NAME.to_owned()
    } else {
        trimmed.to_owned()
    };
    // 予約名を既定値にしない。 直させるより、初めから避ける。
    if is_reserved(&name) {
        name.push_str("-voice");
    }
    while name.len() > MAX_NAME_BYTES {
        name.pop();
    }
    name
}

/// 表示名から何も残らなかったときの配布名（`DEC-PKG-008`）。
pub const FALLBACK_DISTRIBUTION_NAME: &str = "voice";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_だけを通す() {
        assert!(check_segment("s001").is_empty());
        assert!(check_segment("a-b_c9").is_empty());
        assert_eq!(
            check_segment("あ"),
            vec![NameProblem::DisallowedChars { chars: vec!['あ'] }]
        );
    }

    /// `TR-PKG-16` (2)(5) の禁止文字は、許可リストの外側として落ちる。
    #[test]
    fn 予約文字と_oto_を壊す文字を通さない() {
        for c in ['<', '>', ':', '"', '/', '\\', '|', '?', '*', '\0', '=', ','] {
            let name = format!("a{c}b");
            assert!(!check_segment(&name).is_empty(), "{c} を通してはいけない");
        }
        assert!(!check_segment("a\u{1}b").is_empty(), "制御文字");
    }

    #[test]
    fn 末尾のスペースとピリオドを弾く() {
        assert!(check_segment("a ").contains(&NameProblem::TrailingSpaceOrDot));
        assert!(check_segment("a.").contains(&NameProblem::TrailingSpaceOrDot));
    }

    #[test]
    fn 予約名は基底名でだけ弾く() {
        assert!(check_segment("con").contains(&NameProblem::ReservedBaseName));
        assert!(check_file_name("CON.wav").contains(&NameProblem::ReservedBaseName));
        assert!(check_file_name("a.con").is_empty(), "拡張子は基底名でない");
    }

    /// Windows は上付き数字を数字と同一視する（`TR-PKG-16` (4)）。
    #[test]
    fn 上付き数字の変種も予約名() {
        assert!(check_segment("COM¹").contains(&NameProblem::ReservedBaseName));
    }

    #[test]
    fn 小文字化の衝突を見つける() {
        assert_eq!(case_collisions(&["A.wav", "a.wav", "b.wav"]), ["a.wav"]);
        assert!(case_collisions(&["a.wav", "b.wav"]).is_empty());
    }

    #[test]
    fn エイリアスの空白と_nfd_を弾く() {
        assert!(check_alias("あ").is_empty());
        assert!(check_alias(" あ").contains(&AliasProblem::EdgeSpace));
        assert!(check_alias("あ  い").contains(&AliasProblem::DoubleSpace));
        assert!(check_alias("あ\u{3000}い").contains(&AliasProblem::IdeographicSpace));
        // か + 濁点（NFD）。NFC なら「が」1文字。
        assert!(check_alias("か\u{3099}").contains(&AliasProblem::NotNfc));
    }

    #[test]
    fn 配布名の既定値は_ascii_に落ちる() {
        assert_eq!(default_distribution_name("Koeru Voice 1"), "Koeru-Voice-1");
        assert_eq!(default_distribution_name("こえるちゃん"), "voice");
        assert_eq!(default_distribution_name(""), "voice");
        // ハイフンは通す文字なので、畳まれるのは落とした文字の並びだけ。
        assert_eq!(default_distribution_name("--a--b--"), "a--b");
        assert_eq!(default_distribution_name("a　／　b"), "a-b");
    }

    #[test]
    fn 配布名の既定値は予約名にならない() {
        assert_eq!(default_distribution_name("con"), "con-voice");
        assert!(check_segment(&default_distribution_name("con")).is_empty());
    }

    /// 既定値はそのまま通る名前でなければ意味が無い。
    #[test]
    fn 配布名の既定値は検査を通る() {
        for src in ["Koeru", "こえる", "a/b", "  ", "🎤", "LPT9"] {
            let name = default_distribution_name(src);
            assert!(
                check_segment(&name).is_empty(),
                "{src:?} から出た {name:?} が通らない"
            );
        }
    }
}
