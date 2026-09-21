//! `presamp.ini` の読み書きと、phonemizer の差し替え点（`TR-RCL-24`, `TR-SYN-36`）。
//!
//! 書き出しと読み込みが同じ1枚の表を通る（`DEC-SYN-010`）。 KOERU が書いた
//! `presamp.ini` を KOERU が読み戻せるし、利用者が音源に置いた `presamp.ini` は
//! そのままエイリアス解決の規則になる。
//!
//! 配布物にも入る。 受け取った側が presamp 系の phonemizer で開けば、
//! 同じ表で解決される（`TR-PKG-03` の `default_phonemizer`）。
//!
//! # 差し替えられるのは綴りまで
//!
//! 音素の時間位置は持たない（`DEC-SYN-010`）。 CVVC の VC をいつ差し込むかは
//! KOERU の規約プリセットが決める（`TR-ALN-19`）。
//!
//! # 行の書式
//!
//! `[CONSONANT]` は実物で確かめてある——`r=r,ら,る,れ,ろ=1` の形で、
//! 3列目は「1文字目を伸ばすか」（`DEC-RCL-004` で読み違えた欄）。
//!
//! **[Unknown] `[VOWEL]` の列の形は一次資料で確かめていない。** `[CONSONANT]` と
//! 同じ形として書いている。読み書きが往復することは試験が見ているが、
//! OpenUtau が同じ解釈をするかは別で、`DEC-SYN-010` の層B で確かめる。

use std::collections::BTreeMap;

use crate::alias::Method;
use crate::inventory::{UnitSet, VOWEL_CLASSES, consonants, units};

/// エイリアスの綴りを決める規則（`TR-SYN-36` の差し替え点）。
///
/// テンプレートの置き換え子は4つ。
///
/// | 置き換え子 | 中身 |
/// |---|---|
/// | `%CV%` | モーラの仮名（`か`） |
/// | `%v%` | 直前モーラの母音クラス（`a`） |
/// | `%c%` | そのモーラの子音記号（`k`） |
/// | `%V%` | そのモーラの母音クラス |
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rules {
    /// 母音クラスと、その所属仮名。
    pub vowels: BTreeMap<String, Vec<String>>,
    /// 子音記号と、その所属仮名。
    pub consonants: BTreeMap<String, Vec<String>>,
    /// エイリアス規則のテンプレート。節の名前で引く。
    pub templates: BTreeMap<String, String>,
}

/// `presamp.ini` が持つエイリアス規則の節（`TR-RCL-24`）。
pub const TEMPLATE_SECTIONS: [&str; 7] = [
    "VCV",
    "BEGINING_CV",
    "CROSS_CV",
    "VC",
    "CV",
    "LONG_V",
    "ENDING",
];

impl Rules {
    /// 同梱の既定規則（`TR-SYN-11`）。
    ///
    /// いまの [`crate::alias::candidates`] と同じ綴りを作る。 差し替え点を
    /// 足しても既定の振る舞いが変わらないことを、試験が見ている。
    #[must_use]
    pub fn builtin(set: UnitSet) -> Self {
        let table = units(set);
        let mut vowels: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for v in VOWEL_CLASSES {
            vowels.insert(
                v.to_owned(),
                table
                    .iter()
                    .filter(|u| u.vowel == v)
                    .map(|u| u.kana.to_owned())
                    .collect(),
            );
        }
        let mut cons: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for c in consonants(set) {
            cons.insert(
                c.to_owned(),
                table
                    .iter()
                    .filter(|u| u.consonant == c)
                    .map(|u| u.kana.to_owned())
                    .collect(),
            );
        }
        let templates = [
            ("VCV", "%v% %CV%"),
            ("BEGINING_CV", "- %CV%"),
            ("CROSS_CV", "* %CV%"),
            ("VC", "%v% %c%"),
            ("CV", "%CV%"),
            ("LONG_V", "%V%ー"),
            ("ENDING", "%v% -"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        Self {
            vowels,
            consonants: cons,
            templates,
        }
    }

    /// 節のテンプレートを引く。無ければ既定の綴り。
    ///
    /// **落とさない。** 利用者の `presamp.ini` に節が欠けていても、
    /// その節だけ既定へ戻す——音源全体が読めなくなるより軽い。
    #[must_use]
    pub fn template(&self, section: &str) -> &str {
        self.templates
            .get(section)
            .map_or_else(|| default_template(section), String::as_str)
    }

    /// テンプレートを当てて綴りを作る。
    #[must_use]
    pub fn render(
        &self,
        section: &str,
        kana: &str,
        previous_vowel: &str,
        consonant: &str,
        vowel: &str,
    ) -> String {
        self.template(section)
            .replace("%CV%", kana)
            .replace("%v%", previous_vowel)
            .replace("%c%", consonant)
            .replace("%V%", vowel)
    }

    /// その仮名の子音記号。表に無ければ空。
    #[must_use]
    pub fn consonant_of(&self, kana: &str) -> &str {
        self.consonants
            .iter()
            .find(|(_, ks)| ks.iter().any(|k| k == kana))
            .map_or("", |(c, _)| c.as_str())
    }

    /// その仮名の母音クラス。表に無ければ空。
    #[must_use]
    pub fn vowel_of(&self, kana: &str) -> &str {
        self.vowels
            .iter()
            .find(|(_, ks)| ks.iter().any(|k| k == kana))
            .map_or("", |(v, _)| v.as_str())
    }

    /// 1音符のエイリアス候補列（`TR-SYN-12`）。
    ///
    /// 順序がそのまま優先順位。 カバレッジ判定も試唱も、ここを通る
    /// （`TR-SYN-36` の「同じ差し替え結果を通る」）。
    #[must_use]
    pub fn candidates(
        &self,
        method: Method,
        kana: &str,
        previous_vowel: Option<&str>,
    ) -> Vec<String> {
        let c = self.consonant_of(kana);
        let v = self.vowel_of(kana);
        let r = |section: &str, prev: &str| self.render(section, kana, prev, c, v);
        match method {
            Method::Single => vec![r("CV", "")],
            Method::Sequential => match previous_vowel {
                Some(p) => vec![
                    r("VCV", p),
                    r("CROSS_CV", p),
                    r("CV", p),
                    r("BEGINING_CV", p),
                ],
                None => vec![r("BEGINING_CV", ""), r("CV", "")],
            },
            Method::Cvvc => match previous_vowel {
                Some(p) => vec![r("CV", p), r("BEGINING_CV", p)],
                None => vec![r("BEGINING_CV", ""), r("CV", "")],
            },
        }
    }

    /// VC の綴り（`TR-RCL-05`）。音符に対応しないので候補列と別に持つ。
    #[must_use]
    pub fn vc(&self, previous_vowel: &str, consonant: &str) -> String {
        self.render("VC", "", previous_vowel, consonant, "")
    }

    /// 語尾の綴り（`TR-RCL-05`）。
    #[must_use]
    pub fn ending(&self, vowel: &str) -> String {
        self.render("ENDING", "", vowel, "", vowel)
    }
}

/// 節ごとの既定テンプレート。
fn default_template(section: &str) -> &'static str {
    match section {
        "VCV" => "%v% %CV%",
        "BEGINING_CV" => "- %CV%",
        "CROSS_CV" => "* %CV%",
        "VC" => "%v% %c%",
        "LONG_V" => "%V%ー",
        "ENDING" => "%v% -",
        // CV を含め、知らない節は素の仮名。
        _ => "%CV%",
    }
}

/// `presamp.ini` を組み立てる（`TR-RCL-24`）。
///
/// 並びは常に同じ。 書くたびに順序が変わると、同じ内容でも差分が出る。
#[must_use]
pub fn write(rules: &Rules, newline: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!("[VERSION]{newline}1.0{newline}{newline}"));

    out.push_str(&format!("[VOWEL]{newline}"));
    for (v, kana) in &rules.vowels {
        out.push_str(&format!("{v}={}={v}{newline}", kana.join(",")));
    }
    out.push_str(newline);

    out.push_str(&format!("[CONSONANT]{newline}"));
    for (c, kana) in &rules.consonants {
        // 3列目は「1文字目を伸ばすか」（`DEC-RCL-004`）。伸ばさない。
        out.push_str(&format!("{c}={}=0{newline}", kana.join(",")));
    }
    out.push_str(newline);

    for section in TEMPLATE_SECTIONS {
        out.push_str(&format!(
            "[{section}]{newline}{}{newline}{newline}",
            rules.template(section)
        ));
    }
    out
}

/// `presamp.ini` を読む（`TR-SYN-36`）。
///
/// 読めない行は飛ばす。 1行が壊れているだけで音源全体を読めなくしない。
/// 節が欠けていれば既定のテンプレートへ戻る（[`Rules::template`]）。
#[must_use]
pub fn parse(text: &str) -> Rules {
    let mut vowels: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut cons: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut templates: BTreeMap<String, String> = BTreeMap::new();
    let mut section = String::new();

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_ascii_uppercase();
            continue;
        }
        match section.as_str() {
            "VOWEL" | "CONSONANT" => {
                let mut cols = line.split('=');
                let (Some(symbol), Some(kana)) = (cols.next(), cols.next()) else {
                    continue;
                };
                if symbol.is_empty() {
                    continue;
                }
                let list: Vec<String> = kana
                    .split(',')
                    .map(str::trim)
                    .filter(|k| !k.is_empty())
                    .map(str::to_owned)
                    .collect();
                if section == "VOWEL" {
                    vowels.insert(symbol.to_owned(), list);
                } else {
                    cons.insert(symbol.to_owned(), list);
                }
            }
            "VERSION" => {}
            s if TEMPLATE_SECTIONS.contains(&s) => {
                // 最初の行だけ。複数行の節は先頭を採る。
                templates
                    .entry(s.to_owned())
                    .or_insert_with(|| line.to_owned());
            }
            _ => {}
        }
    }
    Rules {
        vowels,
        consonants: cons,
        templates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 書いたものを読み戻せる（`TR-RCL-24`, `DEC-SYN-010`）。
    ///
    /// 往復しないと、配布物に入れた表を自分で解釈できない。
    #[test]
    fn 書いて読み戻せる() {
        let a = Rules::builtin(UnitSet::Extended);
        let b = parse(&write(&a, "\n"));
        assert_eq!(a, b);
    }

    /// 節が要件どおり揃う（`TR-RCL-24`）。
    #[test]
    fn 要件が挙げる節をすべて書く() {
        let text = write(&Rules::builtin(UnitSet::Extended), "\n");
        for s in TEMPLATE_SECTIONS {
            assert!(text.contains(&format!("[{s}]")), "[{s}] が無い");
        }
        assert!(text.contains("[VOWEL]"));
        assert!(text.contains("[CONSONANT]"));
        // 母音 7 種・子音 30 種（`TR-RCL-02`）。
        let r = Rules::builtin(UnitSet::Extended);
        assert_eq!(r.vowels.len(), 7);
        assert_eq!(r.consonants.len(), 30);
    }

    /// 既定規則は今の解決と同じ綴りを作る（`TR-SYN-11`）。
    ///
    /// 差し替え点を足しても既定の振る舞いが変わらない。
    #[test]
    fn 既定規則は既存の候補順と一致する() {
        let r = Rules::builtin(UnitSet::Core);
        for (method, prev, want) in [
            (Method::Single, None, vec!["か"]),
            (
                Method::Sequential,
                Some("a"),
                vec!["a か", "* か", "か", "- か"],
            ),
            (Method::Sequential, None, vec!["- か", "か"]),
            (Method::Cvvc, Some("a"), vec!["か", "- か"]),
            (Method::Cvvc, None, vec!["- か", "か"]),
        ] {
            assert_eq!(
                r.candidates(method, "か", prev),
                want,
                "{method:?} {prev:?}"
            );
        }
        assert_eq!(r.vc("a", "k"), "a k");
        assert_eq!(r.ending("a"), "a -");
    }

    /// 差し替えると綴りが変わる（`TR-SYN-36`）。
    #[test]
    fn 規則を差し替えると綴りが変わる() {
        let mut r = Rules::builtin(UnitSet::Core);
        r.templates.insert("VC".to_owned(), "%v%_%c%".to_owned());
        r.templates
            .insert("BEGINING_CV".to_owned(), "-%CV%".to_owned());
        assert_eq!(r.vc("a", "k"), "a_k");
        assert_eq!(r.candidates(Method::Cvvc, "か", None)[0], "-か");
    }

    /// 節が欠けても音源全体を読めなくしない。
    #[test]
    fn 欠けた節は既定へ戻る() {
        let r = parse("[VOWEL]\na=あ,か=a\n");
        assert_eq!(r.template("VC"), "%v% %c%");
        assert_eq!(r.vc("a", "k"), "a k");
        assert_eq!(r.vowel_of("か"), "a");
    }

    /// 壊れた行は飛ばす。
    #[test]
    fn 壊れた行は飛ばす() {
        let r = parse("[CONSONANT]\nk=か,き=0\nこれは壊れている\n=だめ=0\n");
        assert_eq!(r.consonants.len(), 1);
        assert_eq!(r.consonant_of("か"), "k");
    }

    /// 仮名から子音と母音を引ける。
    #[test]
    fn 仮名から子音と母音を引ける() {
        let r = Rules::builtin(UnitSet::Core);
        assert_eq!(r.consonant_of("か"), "k");
        assert_eq!(r.vowel_of("か"), "a");
        assert_eq!(r.consonant_of("あ"), "", "母音始まりは子音を持たない");
        assert_eq!(r.consonant_of("なにこれ"), "");
    }
}
