//! `readme.txt`（`TR-PKG-28`, `TR-PKG-06`, `TR-PKG-31`, `TR-PKG-32`）。
//!
//! KOERU が持っている情報（音源名・収録方式・収録音高・同梱物一覧・周波数表の扱い）は
//! 必ず出し、本人が書く節は書いたものだけを出す（`DEC-PKG-011`）。
//!
//! # KOERU の名前をここへ書かない
//!
//! `TR-PKG-31` が禁じている。 バージョン・プロジェクト ID・生成ログ・宣伝文・
//! 免責文のどれも入れない。免責の節に出るのは音源制作者の免責で、
//! 作った道具の免責ではない。

use crate::bank::VoiceBank;
use crate::profile::NEWLINE;
use koeru_core::alias::Method;

/// 収録方式の表示名。 作り方だけで、多音階かどうかは含めない（`DEC-PKG-015`）。
///
/// **「多音階連続音」を名乗らせていた。** 多音階の単独音と CVVC は
/// 「単独音」「CVVC」とだけ書かれ、多音階の連続音から降りた単独音は
/// 名乗る名前が無かった。高さは「収録音高」の節が持つ。
const fn method_label(m: Method) -> &'static str {
    match m {
        Method::Single => "単独音",
        Method::Sequential => "連続音",
        Method::Cvvc => "CVVC",
    }
}

/// 収録音高の表示（`DEC-PKG-015`）。 音名を低い順に並べる。
///
/// 単音階でも出す。 何の高さで録ったかは、受け取った人が音域を決める手がかりになる。
fn tones_label(tones: &[i32]) -> String {
    let mut sorted = tones.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .map(|t| koeru_core::tone::name(*t))
        .collect::<Vec<_>>()
        .join(" / ")
}

/// 周波数表について受け手に伝えること（`TR-PKG-06`）。
///
/// 同梱するのは `.frq` だけ。 `.frc` / `.llsm` / `.pmk` / `.vs4ufrq` /
/// `desc.mrq` を使うリサンプラーは、初回再生時に自分で作る。
const FRQ_NOTE: &str = "\
周波数表は .frq のみを同梱しています。
これ以外の形式（.frc / .llsm / .pmk / .vs4ufrq / desc.mrq）を使うリサンプラーでは、
初回再生時に受け手のリサンプラーが自動生成します。最初の一度だけ時間がかかります。";

/// `readme.txt` を組み立てる（`TR-PKG-28`）。
#[must_use]
pub fn readme_txt(bank: &VoiceBank, contents: &[String]) -> String {
    let r = &bank.readme;
    let mut out = Sections::default();

    out.always("音源名", &bank.character.name);
    out.written("制作者名義", bank.character.author.as_deref());
    out.written("バージョン", bank.character.version.as_deref());
    out.always("収録方式", method_label(bank.method));
    out.always("収録音高", &tones_label(&bank.tones));
    out.written("推奨音域", r.tone_range_note.as_deref());
    out.written("キャラクター設定", r.character_note.as_deref());
    out.always("同梱物一覧", &contents.join(NEWLINE));
    out.written("利用規約", r.terms.as_deref());
    out.written("クレジット表記例", r.credit_example.as_deref());
    out.written("連絡先", r.contact.as_deref());
    out.written("免責", r.disclaimer.as_deref());
    out.always("周波数表の扱い", FRQ_NOTE);
    out.always("綴りを確かめた OpenUtau", &verified_note());

    out.finish()
}

/// 綴りを突き合わせた OpenUtau の版（`DEC-SYN-010` の層B）。
///
/// **受け取った側に届かないと意味が無い。** 「どの KOERU がどの OpenUtau と
/// 突き合わされたか」はリポジトリを見れば分かるが、配布物を受け取った人は
/// リポジトリを見ない。同梱の `presamp.ini` がどの版で確かめられたかを、
/// 音源の中に書いておく。
///
/// 保証とは言わない。 確かめた版を並べるだけで、他の版で動くとも
/// 動かないとも言わない。
fn verified_note() -> String {
    let versions: Vec<String> = koeru_core::presamp::verified_openutau()
        .into_iter()
        .map(|v| format!("  {} {}", v.channel, v.version))
        .collect();
    format!(
        "同梱の presamp.ini は、次の OpenUtau で綴りが一致することを確かめています。\
{NEWLINE}{}{NEWLINE}\
これ以外の版で動かないという意味ではありません。",
        versions.join(NEWLINE)
    )
}

/// 節を積む。見出しの形をここ1箇所に閉じる。
#[derive(Debug, Default)]
struct Sections {
    body: String,
    titles: Vec<&'static str>,
}

impl Sections {
    /// KOERU が持っている情報。必ず出す。
    fn always(&mut self, title: &'static str, body: &str) {
        self.push(title, body);
    }

    /// 本人が書く節。書いていなければ節ごと出さない（`DEC-PKG-011`）。
    fn written(&mut self, title: &'static str, body: Option<&str>) {
        let Some(b) = body.map(str::trim).filter(|b| !b.is_empty()) else {
            return;
        };
        self.push(title, b);
    }

    fn push(&mut self, title: &'static str, body: &str) {
        if self.titles.contains(&title) {
            return;
        }
        if !self.body.is_empty() {
            self.body.push_str(NEWLINE);
        }
        self.body.push_str(&format!("【{title}】{NEWLINE}"));
        for line in body.lines() {
            self.body.push_str(line);
            self.body.push_str(NEWLINE);
        }
        self.titles.push(title);
    }

    fn finish(self) -> String {
        self.body
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bank::{Character, Readme};

    fn bank() -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character {
                name: "こえる".to_owned(),
                ..Character::default()
            },
            readme: Readme::default(),
            method: Method::Single,
            tones: vec![57],
            subbanks: Vec::new(),
            rules: koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
        }
    }

    fn contents() -> Vec<String> {
        vec!["character.txt".to_owned(), "oto.ini".to_owned()]
    }

    #[test]
    fn 持っている情報は必ず出る() {
        let txt = readme_txt(&bank(), &contents());
        for t in [
            "音源名",
            "収録方式",
            "収録音高",
            "同梱物一覧",
            "周波数表の扱い",
        ] {
            assert!(txt.contains(&format!("【{t}】")), "{t} が無い");
        }
        assert!(txt.contains("単独音"));
        assert!(txt.contains("character.txt"));
    }

    /// 方式と高さを別の節に書く（`DEC-PKG-015`）。 単音階でも高さを出す。
    #[test]
    fn 収録音高は方式と別の節に出る() {
        let single = readme_txt(&bank(), &contents());
        assert!(single.contains("【収録音高】\r\nA3\r\n"), "{single}");

        // 多音階の連続音から単独音へ降ろした配布物。 名乗るのは降りた先の方式。
        let mut b = bank();
        b.tones = vec![69, 55, 62];
        let multi = readme_txt(&b, &contents());
        assert!(multi.contains("【収録方式】\r\n単独音\r\n"), "{multi}");
        assert!(
            multi.contains("【収録音高】\r\nG3 / D4 / A4\r\n"),
            "{multi}"
        );
        assert!(!multi.contains("多音階"), "方式の名前に高さを混ぜない");
    }

    /// 書いていない節を空の見出しで置かない（`DEC-PKG-011`）。
    #[test]
    fn 書いていない節は出さない() {
        let txt = readme_txt(&bank(), &contents());
        for t in ["利用規約", "連絡先", "免責", "クレジット表記例"] {
            assert!(!txt.contains(&format!("【{t}】")), "{t} が出ている");
        }
    }

    #[test]
    fn 書いた節は出る() {
        let mut b = bank();
        b.readme.terms = Some("自由に使えます".to_owned());
        b.character.author = Some("しお".to_owned());
        let txt = readme_txt(&b, &contents());
        assert!(txt.contains("【利用規約】"));
        assert!(txt.contains("自由に使えます"));
        assert!(txt.contains("【制作者名義】"));
    }

    #[test]
    fn 空白だけの節は書いたことにしない() {
        let mut b = bank();
        b.readme.terms = Some("  \n ".to_owned());
        assert!(!readme_txt(&b, &contents()).contains("【利用規約】"));
    }

    /// `TR-PKG-31`。道具の名前もバージョンも配布物に出さない。
    #[test]
    fn koeru_の名前を書かない() {
        let mut b = bank();
        b.readme.terms = Some("自由に使えます".to_owned());
        let txt = readme_txt(&b, &contents());
        assert!(!txt.to_lowercase().contains("koeru"));
    }

    #[test]
    fn 周波数表の説明が入る() {
        let txt = readme_txt(&bank(), &contents());
        assert!(txt.contains(".frq"));
        assert!(txt.contains("自動生成"));
    }

    #[test]
    fn 改行は_crlf() {
        let txt = readme_txt(&bank(), &contents());
        assert!(txt.contains("\r\n"));
        // 素の LF を残さない。 CR を伴わない LF が1つでもあれば、
        // Windows のメモ帳で全部が1行に見える。
        assert!(
            !txt.match_indices('\n')
                .any(|(i, _)| i == 0 || !txt[..i].ends_with('\r')),
            "CR を伴わない LF がある"
        );
    }
}
