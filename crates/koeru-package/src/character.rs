//! `character.txt` / `character.yaml` / `prefix.map`（`TR-PKG-02`〜`04`, `TR-PKG-29`）。
//!
//! 3つとも同じ割り当てを別の形で書く。 ここ1箇所が持つのは、
//! `prefix.map` と `subbanks` が食い違わないようにするため（`TR-PKG-04`）。

use crate::bank::{Character, VoiceBank};
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

/// 受け取った側が使う phonemizer（`TR-PKG-03`, `DEC-SYN-010`）。
///
/// 同梱する `presamp.ini`（`TR-RCL-24`）を読む phonemizer を指す。 これを書かないと、
/// 受け取った側がどの規則で鳴らすかを自分で選べてしまい、KOERU の中での解決と
/// 一致しない。音素レベルの一致は、ここが構造的な保証になる。
///
/// **[Unknown] 完全修飾の綴りを一次資料で確かめていない。** クラス名が
/// `JapanesePresampPhonemizer` であることは確かめたが、`character.yaml` が
/// 名前空間付きを求めるかは未確認（`EVID-SYN-001`）。外れていても OpenUtau 側が
/// 既定へ倒すだけで、書かなかった場合と同じ状態に戻る。`DEC-SYN-010` の層B で確かめる。
pub const DEFAULT_PHONEMIZER: &str = "OpenUtau.Plugin.Builtin.JapanesePresampPhonemizer";

/// `character.yaml` を組み立てる（`TR-PKG-03`, `TR-PKG-29`）。
///
/// # 書かないキーがある
///
/// `symbol_set` / `use_filename_as_alias` は出さない。
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

    // 同梱した presamp.ini を読ませる（`DEC-SYN-010`）。
    y.field("default_phonemizer", DEFAULT_PHONEMIZER);

    // 単一音階では subbanks を書かない（`TR-PKG-29`）。
    // 書いても意味のある音域を宣言できず、推奨音域は readme.txt が持つ。
    if bank.is_multi_pitch() {
        // 担う範囲は `prefix.map` と同じ floor 割り当てから作る（`TR-PKG-04`）。
        // 区画が自分で範囲を持つと、2箇所が別々の割り当てを宣言する。
        let assigned = tone::assigned_ranges(&recorded_tones(bank));
        y.line("subbanks:");
        for s in &bank.subbanks {
            y.indented(1, &format!("- color: {}", quote(&s.color)));
            y.indented(2, &format!("prefix: {}", quote(&s.prefix)));
            y.indented(2, &format!("suffix: {}", quote(&s.suffix)));
            let owned = s
                .tone
                .and_then(|t| assigned.iter().find(|(r, _)| *r == t))
                .map(|(_, v)| tone::ranges(v))
                .unwrap_or_default();
            if !owned.is_empty() {
                y.indented(2, "tone_ranges:");
                for r in owned {
                    y.indented(3, &format!("- {}", quote(&r)));
                }
            }
        }
    }
    y.finish()
}

/// `prefix.map` を組み立てる（`TR-PKG-04`, `TR-RCL-06`）。
///
/// C1 から B7 の 84 半音すべてに1行を持つ。 1行は「音階名 TAB prefix TAB suffix」で、
/// 接尾辞はその半音を担う収録音高の音名。単一音階では `None`。
///
/// 割り当ては収録音高から導く（`koeru_core::tone::prefix_map_body`）。 区画が
/// 宣言している範囲を書き写さない——`character.yaml` の `tone_ranges` と
/// 同じ割り当てを2箇所で作ることになる。
#[must_use]
pub fn prefix_map(bank: &VoiceBank) -> Option<String> {
    if !bank.is_multi_pitch() {
        return None;
    }
    let tones = recorded_tones(bank);
    if tones.is_empty() {
        return None;
    }
    // 綴りは区画が持つ。 収録音高から引き直す——区画の並び順に出すと、
    // 区画を足したときに全体の並びが動いて差分が読めなくなる。
    let affix = |recorded: i32| {
        bank.subbanks
            .iter()
            .find(|s| s.tone == Some(recorded))
            .map_or_else(
                || tone::default_affix(recorded),
                |s| (s.prefix.clone(), s.suffix.clone()),
            )
    };
    Some(tone::prefix_map_body(&tones, affix, NEWLINE))
}

/// 区画が名乗っている収録音高（`TR-REC-25`）。
fn recorded_tones(bank: &VoiceBank) -> Vec<i32> {
    bank.subbanks.iter().filter_map(|s| s.tone).collect()
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
///
/// **制御文字は逃がす。** 表示名は敵対的な値を受け取る前提で（`TR-PKG-37`）、
/// U+0007 のような字がそのまま来る。二重引用符スカラーに生の制御文字は
/// 書けないので、素通しにすると `character.yaml` が YAML として壊れる。
/// **読み戻し検証はバイト列の一致と UTF-8 の復号しか見ないので通ってしまい、
/// OpenUtau が読めない配布物ができる。**
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
            // C0 / DEL / C1。YAML は `\xNN` と `\uNNNN` を持っている。
            c if (c as u32) < 0x20 || (c as u32) == 0x7F => {
                out.push_str(&format!("\\x{:02x}", c as u32));
            }
            c if (0x80..=0x9F).contains(&(c as u32)) => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
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
            tone: tones.first().copied(),
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
            rules: koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
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

    /// `prefix.map` と `subbanks` は同じ割り当てを表す（`TR-PKG-04`, `TR-RCL-06`）。
    ///
    /// どちらも収録音高から floor 割り当てで導く。 片方だけを直せないようにしてある。
    #[test]
    fn 多音階では両方を同じ割り当てを出す() {
        let b = bank(vec![
            subbank(Some("G4"), "↑", &[67]),
            subbank(Some("C4"), "", &[60]),
        ]);
        let y = character_yaml(&b, Profile::Both, false);
        assert!(y.contains("tone_ranges:"));
        // C4 は C1 から F#4 まで、G4 は G4 から B7 まで担う。
        assert!(y.contains("\"C1-F#4\""), "{y}");
        assert!(y.contains("\"G4-B7\""), "{y}");

        let map = prefix_map(&b).expect("多音階なら出る");
        let lines: Vec<&str> = map.lines().collect();
        assert_eq!(lines.len(), 84, "C1 から B7 まで抜けが無い");
        assert_eq!(lines[0], "C1\t\t", "最低音高が下の全音域も担う");
        assert_eq!(lines[43], "G4\t↑\t", "区画の prefix を落とさない");
        assert_eq!(lines[83], "B7\t↑\t");
    }

    /// 区画の `prefix` を落とさない。 落とすとその音域のエイリアスが引けない。
    #[test]
    fn 区画の接頭辞が_prefix_map_に出る() {
        let b = bank(vec![subbank(Some("G4"), "↑", &[67])]);
        let map = prefix_map(&b).expect("多音階なら出る");
        assert!(map.lines().all(|l| l.contains('↑')), "{map}");
    }

    #[test]
    fn yaml_の値は常に引用する() {
        let mut b = bank(vec![subbank(None, "", &[])]);
        b.character.name = "こえる: \"の\" 音源".to_owned();
        let y = character_yaml(&b, Profile::Both, false);
        assert!(y.contains(r#"name: "こえる: \"の\" 音源""#));
    }

    /// 制御文字を素通しにすると YAML が壊れる（`TR-PKG-37` の敵対的な表示名）。
    #[test]
    fn 制御文字を逃がす() {
        let mut b = bank(vec![subbank(None, "", &[])]);
        b.character.name = "こえる\u{7}ちゃん".to_owned();
        let y = character_yaml(&b, Profile::OpenUtau, false);
        assert!(y.contains(r"\x07"), "{y}");
        assert!(
            !y.chars()
                .any(|c| (c as u32) < 0x20 && c != '\r' && c != '\n'),
            "生の制御文字が残っている"
        );
    }

    #[test]
    fn 不透明度は_0_と_1_の間に収める() {
        assert_eq!(format_opacity(0.5), "0.50");
        assert_eq!(format_opacity(2.0), "1.00");
        assert_eq!(format_opacity(f64::NAN), "1.00");
    }
}
