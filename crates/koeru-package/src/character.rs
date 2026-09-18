//! `character.txt` / `character.yaml` / `prefix.map`（`TR-PKG-02`〜`04`, `TR-PKG-29`）。
//!
//! 3つとも同じ割り当てを別の形で書く。 ここ1箇所が持つのは、
//! `prefix.map` と `subbanks` が食い違わないようにするため（`TR-PKG-04`）。

use crate::bank::{Character, Subbank, VoiceBank};
use crate::profile::{NEWLINE, Profile};
use crate::tone;

/// `character.txt` に書いてよいキー（`TR-PKG-02`）。
///
/// 並びもここで固定する。 書くたびに順序が変わると、
/// 同じ内容でも差分が出て、外部ツールとの突き合わせが濁る。
const TXT_KEYS: [&str; 7] = [
    "name", "image", "author", "voice", "sample", "web", "version",
];

/// 音源アイコンのファイル名（`TR-PKG-07`）。
pub const ICON_FILE: &str = "icon.bmp";
/// 立ち絵のファイル名（`TR-PKG-07`）。
pub const PORTRAIT_FILE: &str = "portrait.png";

/// `character.txt` を組み立てる（`TR-PKG-02`）。
///
/// 値の無いキーは行ごと出さない。 空の `author=` は「名義が無い」ではなく
/// 「書き忘れ」に見える。`name` だけは必ず出す（`TR-PKG-01`）。
#[must_use]
pub fn character_txt(c: &Character, has_icon: bool) -> String {
    let icon = has_icon.then(|| ICON_FILE.to_owned());
    let values: [Option<&str>; 7] = [
        Some(c.name.as_str()),
        icon.as_deref(),
        c.author.as_deref(),
        c.voice.as_deref(),
        c.sample.as_deref(),
        c.web.as_deref(),
        c.version.as_deref(),
    ];
    let mut out = String::new();
    for (key, value) in TXT_KEYS.iter().zip(values) {
        let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) else {
            continue;
        };
        out.push_str(key);
        out.push('=');
        out.push_str(v);
        out.push_str(NEWLINE);
    }
    out
}

/// `character.yaml` を組み立てる（`TR-PKG-03`, `TR-PKG-29`）。
///
/// # 書かないキーがある
///
/// `default_phonemizer` / `symbol_set` / `use_filename_as_alias` は出さない。
/// OpenUtau が受け付ける値の一次情報を持っていないので、推測で書かない——
/// 知らない値を書くと、読み手が既定へ倒すのか失敗するのかも分からない。
/// 書かなければ OpenUtau 自身の既定が効く。
#[must_use]
pub fn character_yaml(bank: &VoiceBank, profile: Profile, has_icon: bool) -> String {
    let c = &bank.character;
    let mut y = Yaml::default();

    y.field("name", &c.name);
    if !c.localized_names.is_empty() {
        y.line("localized_names:");
        for (tag, name) in &c.localized_names {
            y.indented(1, &format!("{}: {}", quote(tag), quote(name)));
        }
    }
    // classic UTAU 形式の音源であることの宣言。 KOERU が出すのはこれだけ。
    y.raw("singer_type", "utau");
    y.raw("text_file_encoding", profile.declared_encoding());
    if has_icon {
        y.field("image", ICON_FILE);
    }
    if let Some(p) = &c.portrait {
        y.field("portrait", PORTRAIT_FILE);
        y.raw("portrait_opacity", &format_opacity(p.opacity));
        y.raw("portrait_height", &p.height.to_string());
    }
    for (key, value) in [
        ("author", c.author.as_deref()),
        ("voice", c.voice.as_deref()),
        ("web", c.web.as_deref()),
        ("version", c.version.as_deref()),
        ("sample", c.sample.as_deref()),
    ] {
        if let Some(v) = value.map(str::trim).filter(|v| !v.is_empty()) {
            y.field(key, v);
        }
    }

    // 単一音階では subbanks を書かない（`TR-PKG-29`）。
    // 書いても意味のある音域を宣言できず、推奨音域は readme.txt が持つ。
    if bank.is_multi_pitch() {
        y.line("subbanks:");
        for s in &bank.subbanks {
            y.indented(1, &format!("- color: {}", quote(&s.color)));
            y.indented(2, &format!("prefix: {}", quote(&s.prefix)));
            y.indented(2, &format!("suffix: {}", quote(&s.suffix)));
            let ranges = tone::ranges(&s.tones);
            if !ranges.is_empty() {
                y.indented(2, "tone_ranges:");
                for r in ranges {
                    y.indented(3, &format!("- {}", quote(&r)));
                }
            }
        }
    }
    y.finish()
}

/// `prefix.map` を組み立てる（`TR-PKG-04`）。
///
/// 1行1音階で「音階名 TAB prefix TAB suffix」。単一音階では `None`。
///
/// 音階の並びは MIDI 番号順。 区画の並び順に出すと、区画を足したときに
/// 全体の並びが動いて差分が読めなくなる。
#[must_use]
pub fn prefix_map(bank: &VoiceBank) -> Option<String> {
    if !bank.is_multi_pitch() {
        return None;
    }
    let mut rows: Vec<(i32, &Subbank)> = bank
        .subbanks
        .iter()
        .flat_map(|s| s.tones.iter().map(move |t| (*t, s)))
        .collect();
    rows.sort_by_key(|(t, _)| *t);

    let mut out = String::new();
    for (midi, s) in rows {
        out.push_str(&format!(
            "{}\t{}\t{}{NEWLINE}",
            tone::name(midi),
            s.prefix,
            s.suffix
        ));
    }
    Some(out)
}

/// YAML を素直に組み立てる小さな道具。
///
/// serde を引かない。 書くキーは `TR-PKG-03` が17種に固定していて、
/// 構造も入れ子2段までしかない。読み込みは持たないので、
/// 直列化器を1つ抱えるほどの面が無い。
#[derive(Debug, Default)]
struct Yaml(String);

impl Yaml {
    fn line(&mut self, s: &str) {
        self.0.push_str(s);
        self.0.push_str(NEWLINE);
    }

    fn indented(&mut self, depth: usize, s: &str) {
        self.0.push_str(&"  ".repeat(depth));
        self.line(s);
    }

    /// 値を引用して書く。 名前に `:` や `#` が入っても壊れない。
    fn field(&mut self, key: &str, value: &str) {
        let quoted = quote(value);
        self.line(&format!("{key}: {quoted}"));
    }

    /// 引用しないで書く。真偽値・数値・KOERU が決めた語だけに使う。
    fn raw(&mut self, key: &str, value: &str) {
        self.line(&format!("{key}: {value}"));
    }

    fn finish(self) -> String {
        self.0
    }
}

/// YAML の二重引用符スカラーにする。
///
/// 常に引用する。 引用が要るかを判定すると、`はい` や `1.0` のような
/// 名前が真偽値や数値として読まれる経路が残る。
fn quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

/// 不透明度を YAML の数値として書く。
///
/// 範囲外を書かない。 1 を超える値を OpenUtau がどう扱うかは決まっていない。
fn format_opacity(v: f64) -> String {
    let clamped = if v.is_finite() {
        v.clamp(0.0, 1.0)
    } else {
        1.0
    };
    format!("{clamped:.2}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Portrait, Readme, Sample, Subbank};
    use koeru_core::project::Method;
    use std::path::PathBuf;

    fn character() -> Character {
        Character {
            name: "こえる".to_owned(),
            author: Some("しお".to_owned()),
            version: Some("1.0".to_owned()),
            ..Character::default()
        }
    }

    fn subbank(folder: Option<&str>, prefix: &str, tones: &[i32]) -> Subbank {
        Subbank {
            folder: folder.map(str::to_owned),
            color: folder.unwrap_or_default().to_owned(),
            prefix: prefix.to_owned(),
            suffix: String::new(),
            tones: tones.to_vec(),
            samples: vec![Sample {
                file: "s001.wav".to_owned(),
                master: PathBuf::from("s001.wav"),
                frq: None,
                entries: Vec::new(),
            }],
        }
    }

    fn bank(subbanks: Vec<Subbank>) -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: character(),
            readme: Readme::default(),
            method: Method::Single,
            subbanks,
        }
    }

    /// `TR-PKG-02` の7種以外を書かない。
    #[test]
    fn character_txt_は7種のキーしか書かない() {
        let txt = character_txt(&character(), true);
        let keys: Vec<&str> = txt
            .lines()
            .filter_map(|l| l.split_once('=').map(|(k, _)| k))
            .collect();
        assert_eq!(keys, ["name", "image", "author", "version"]);
        for k in &keys {
            assert!(TXT_KEYS.contains(k), "{k} は許したキーではない");
        }
    }

    #[test]
    fn 値の無いキーは行ごと出さない() {
        let mut c = character();
        c.author = Some("   ".to_owned());
        let txt = character_txt(&c, false);
        assert!(!txt.contains("author="), "空白だけの値も出さない");
        assert!(!txt.contains("image="), "アイコンが無ければ書かない");
        assert!(txt.starts_with("name=こえる"));
    }

    #[test]
    fn 改行は_crlf() {
        assert!(character_txt(&character(), false).contains("\r\n"));
    }

    #[test]
    fn character_yaml_は宣言した符号化を書く() {
        let b = bank(vec![subbank(None, "", &[])]);
        let y = character_yaml(&b, Profile::Both, false);
        assert!(y.contains("text_file_encoding: shift_jis"));
        let y = character_yaml(&b, Profile::OpenUtau, false);
        assert!(y.contains("text_file_encoding: utf-8"));
    }

    /// `TR-PKG-03` が挙げた17種の外を書かない。
    #[test]
    fn character_yaml_は未知のキーを書かない() {
        const ALLOWED: [&str; 17] = [
            "name",
            "localized_names",
            "singer_type",
            "text_file_encoding",
            "image",
            "portrait",
            "portrait_opacity",
            "portrait_height",
            "author",
            "voice",
            "web",
            "version",
            "sample",
            "default_phonemizer",
            "symbol_set",
            "subbanks",
            "use_filename_as_alias",
        ];
        let mut b = bank(vec![
            subbank(Some("C4"), "", &[60, 61]),
            subbank(Some("G4"), "↑", &[67]),
        ]);
        b.character.localized_names = vec![("ja-JP".to_owned(), "こえる".to_owned())];
        b.character.portrait = Some(Portrait {
            png: Vec::new(),
            opacity: 0.8,
            height: 800,
        });
        let y = character_yaml(&b, Profile::Both, true);
        for line in y.lines() {
            // 入れ子の中は subbanks の下なので、字下げのある行は見ない。
            if line.starts_with(' ') || line.starts_with('-') {
                continue;
            }
            let key = line.split_once(':').map_or(line, |(k, _)| k);
            assert!(ALLOWED.contains(&key), "{key} は許したキーではない");
        }
    }

    #[test]
    fn 単一音階では_subbanks_を書かない() {
        let b = bank(vec![subbank(None, "", &[])]);
        let y = character_yaml(&b, Profile::Both, false);
        assert!(!y.contains("subbanks"));
        assert_eq!(prefix_map(&b), None);
    }

    /// `prefix.map` と `subbanks` は同じ割り当てを表す（`TR-PKG-04`）。
    #[test]
    fn 多音階では両方を同じ内容で出す() {
        let b = bank(vec![
            subbank(Some("G4"), "↑", &[67, 68]),
            subbank(Some("C4"), "", &[60, 61]),
        ]);
        let y = character_yaml(&b, Profile::Both, false);
        assert!(y.contains("tone_ranges:"));
        assert!(y.contains("\"C4-C#4\""));
        assert!(y.contains("\"G4-G#4\""));

        let map = prefix_map(&b).expect("多音階なら出る");
        assert_eq!(
            map.lines().collect::<Vec<_>>(),
            ["C4\t\t", "C#4\t\t", "G4\t↑\t", "G#4\t↑\t"],
            "MIDI 番号順で、区画の並びに依存しない"
        );
    }

    #[test]
    fn yaml_の値は常に引用する() {
        let mut b = bank(vec![subbank(None, "", &[])]);
        b.character.name = "こえる: \"の\" 音源".to_owned();
        let y = character_yaml(&b, Profile::Both, false);
        assert!(y.contains(r#"name: "こえる: \"の\" 音源""#));
    }

    #[test]
    fn 不透明度は_0_と_1_の間に収める() {
        assert_eq!(format_opacity(0.5), "0.50");
        assert_eq!(format_opacity(2.0), "1.00");
        assert_eq!(format_opacity(f64::NAN), "1.00");
    }
}
