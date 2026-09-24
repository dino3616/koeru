//! 配布する音源の内容。
//!
//! DB から組み立ててここへ渡す。 この型より下は DB を知らない——
//! 生成規則だけを持つので、試験が SQLite を要さない。
//!
//! 文字列は全て内部モデル（UTF-8 / NFC、`TR-PKG-11`）。
//! ファイルシステムから読み戻した名前をここへ入れない（`TR-PKG-15`）。

use std::path::PathBuf;

use koeru_align::ini::IniEntry;
use koeru_core::alias::Method;

/// 配布する音源1本（`TR-PKG-01`）。
#[derive(Debug, Clone)]
pub struct VoiceBank {
    /// 音源ルートフォルダ名。ASCII 固定（`DEC-PKG-008`、`TR-PKG-16`）。
    pub distribution_name: String,
    /// `character.txt` と `character.yaml` に出る値。
    pub character: Character,
    /// `readme.txt` に出る値。
    pub readme: Readme,
    /// 配布物の作り方。readme の「収録方式」に出る（`DEC-PKG-015`）。
    ///
    /// 下位方式で書き出すときは降りた先の方式（`TR-PKG-24`）。
    /// **manifest の方式を入れていた。** 連続音から単独音へ降ろした配布物が
    /// 「連続音」を名乗り、配布の記録（降りた方式を書く）とも食い違っていた。
    ///
    /// 多音階かどうかは持たない。 それは [`tones`](Self::tones) の本数。
    pub method: Method,
    /// 収録音高（MIDI、低い順）。readme の「収録音高」に出る（`DEC-PKG-015`）。
    ///
    /// 単音階でも1つ入る。 区画（[`subbanks`](Self::subbanks)）は単音階で
    /// 音高を持たないので、そちらからは引けない。
    pub tones: Vec<i32>,
    /// 音階ごとの区画。単一音階では1つ（`TR-PKG-04`）。
    pub subbanks: Vec<Subbank>,
    /// 同梱する `presamp.ini` の中身（`TR-RCL-24`）。
    ///
    /// 受け取った側が同じ規則で解決するための表（`DEC-SYN-010`）。
    /// インベントリとフォールバック規則から作り、別定義を持たない。
    pub rules: koeru_core::presamp::Rules,
}

/// `character.txt` / `character.yaml` に出る値（`TR-PKG-02`, `TR-PKG-03`）。
///
/// キーを増やさない。 `TR-PKG-02` が7種、`TR-PKG-03` が17種に限っていて、
/// 独自キーを足すと読み手によって無視されたり壊れたりする。
/// キャラクター設定は `readme.txt` と `localized_names` が持つ（`TR-PKG-32`）。
#[derive(Debug, Clone, Default)]
pub struct Character {
    /// 表示名。`character.txt` の `name=`。
    pub name: String,
    /// 別名義。`character.yaml` の `localized_names`（言語タグ → 名前）。
    pub localized_names: Vec<(String, String)>,
    /// 制作者名義。
    pub author: Option<String>,
    /// 声の提供者。
    pub voice: Option<String>,
    /// サンプル音源のファイル名。音源ルートからの相対。
    pub sample: Option<String>,
    /// 配布元の URL。
    pub web: Option<String>,
    /// バージョン。
    pub version: Option<String>,
    /// 音源アイコンの元画像（`TR-PKG-07`、`DEC-PKG-012`）。
    ///
    /// PNG か JPEG のバイト列。100×100 の BMP へ変換して同梱する。
    /// 無ければ `image=` を書かない。
    pub icon: Option<Vec<u8>>,
    /// 立ち絵（`TR-PKG-07`）。`character.txt` には書かない。
    pub portrait: Option<Portrait>,
}

/// 立ち絵（`TR-PKG-07`）。
#[derive(Debug, Clone)]
pub struct Portrait {
    /// PNG のバイト列。そのまま複製する。
    pub png: Vec<u8>,
    /// 0.0〜1.0。`character.yaml` の `portrait_opacity`。
    pub opacity: f64,
    /// 表示の高さ（px）。`character.yaml` の `portrait_height`。
    pub height: u32,
}

/// `readme.txt` に出る値（`TR-PKG-28`）。
///
/// 本人が書く節は [`Option`]。 書いていない節は出さない（`DEC-PKG-011`）。
/// 空の見出しだけを置くと、受け手には「書き忘れ」に見える。
#[derive(Debug, Clone, Default)]
pub struct Readme {
    /// 推奨音域。多音階では `subbanks.tone_ranges` にも出る（`TR-PKG-29`）。
    pub tone_range_note: Option<String>,
    /// 利用規約の本文（`DEC-PKG-011`）。
    pub terms: Option<String>,
    /// クレジット表記例。
    pub credit_example: Option<String>,
    /// 連絡先。
    pub contact: Option<String>,
    /// 免責。音源制作者のもので、KOERU のものではない（`TR-PKG-31`）。
    pub disclaimer: Option<String>,
    /// キャラクター設定の自由文（`TR-PKG-32`）。
    pub character_note: Option<String>,
}

/// 音階ごとの区画（`TR-PKG-04`）。
///
/// 単一音階では1つだけを持ち、[`folder`](Self::folder) は `None`。
/// そのとき `prefix.map` も `subbanks` も書かない。
#[derive(Debug, Clone)]
pub struct Subbank {
    /// サブフォルダ名。音源ルート直下なら `None`（`TR-PKG-08`）。
    pub folder: Option<String>,
    /// `character.yaml` の `subbanks.color`。
    pub color: String,
    /// エイリアスの前に付ける（`TR-PKG-19`）。
    pub prefix: String,
    /// エイリアスの後ろに付ける（`TR-PKG-19`）。
    pub suffix: String,
    /// この区画の収録音高（MIDI、`TR-REC-25`）。単一音階では `None`。
    ///
    /// **担う範囲（[`tones`](Self::tones)）と別に持つ。** 収録音高は「実際に録った音」で、
    /// 担う範囲は floor 割り当ての結果（`TR-RCL-06`）。同じ値ではない——
    /// G3 で録った区画は G3 から C#4 までを担う。
    pub tone: Option<i32>,
    /// この区画の素材。
    pub samples: Vec<Sample>,
}

impl Subbank {
    /// 音源ルートから見たフォルダの接頭辞。ルート直下なら空。
    #[must_use]
    pub fn path_prefix(&self) -> String {
        self.folder
            .as_ref()
            .map_or_else(String::new, |f| format!("{f}/"))
    }
}

/// 素材1本（`TR-PKG-08`, `TR-PKG-20`）。
#[derive(Debug, Clone)]
pub struct Sample {
    /// WAV のファイル名。拡張子を含み、同一フォルダ内で一意（`TR-PKG-08`）。
    pub file: String,
    /// マスター WAV の在り処。書き出しで 16 bit へ落とす（`TR-PKG-20`）。
    ///
    /// **44100 Hz で録れていることは検証で確かめる。** ここを信じて
    /// 変換だけすると、44100 と名乗る別のレートの音になる（`TR-REC-02`）。
    pub master: PathBuf,
    /// 周波数表（`TR-PKG-05`）。録音時に作ったものをそのまま入れる。
    pub frq: Option<Vec<u8>>,
    /// この WAV が持つ oto のエントリ。
    pub entries: Vec<IniEntry>,
}

impl VoiceBank {
    /// 多音階か（`TR-PKG-04`）。
    ///
    /// サブフォルダを持つ区画が1つでもあれば多音階。区画の数で判定しない
    /// ——単一音階でも区画は1つあるので、数では区別できない。
    #[must_use]
    pub fn is_multi_pitch(&self) -> bool {
        self.subbanks.iter().any(|s| s.folder.is_some())
    }

    /// 全区画のエイリアスを、出てくる順に並べる（`TR-PKG-19`）。
    ///
    /// prefix / suffix を付けたあとの形。一意性はこの形で見る。
    #[must_use]
    pub fn aliases(&self) -> Vec<String> {
        self.subbanks
            .iter()
            .flat_map(|s| {
                s.samples
                    .iter()
                    .flat_map(move |m| m.entries.iter().map(|e| decorate(s, &e.alias)))
            })
            .collect()
    }

    /// 区画が生むパスを全部並べる（`TR-PKG-26`）。
    ///
    /// WAV と `.frq` と `oto.ini`。 衝突はこの全体で見る——**WAV だけを
    /// 見ると、フォルダ名が重なった区画どうしの `oto.ini` が漏れる。**
    /// ZIP は同名のエントリを許すので、包むところでは気づけない。
    #[must_use]
    pub fn generated_paths(&self) -> Vec<String> {
        let mut out = Vec::new();
        for s in &self.subbanks {
            let prefix = s.path_prefix();
            for m in &s.samples {
                out.push(format!("{prefix}{}", m.file));
                if m.frq.is_some() {
                    out.push(format!("{prefix}{}", frq_name(&m.file)));
                }
            }
            if !s.samples.is_empty() {
                out.push(format!("{prefix}oto.ini"));
            }
        }
        out
    }

    /// 音源ルートから見た WAV のパスを、出てくる順に並べる。
    #[must_use]
    pub fn wav_paths(&self) -> Vec<String> {
        self.subbanks
            .iter()
            .flat_map(|s| {
                let prefix = s.path_prefix();
                s.samples.iter().map(move |m| format!("{prefix}{}", m.file))
            })
            .collect()
    }
}

/// WAV 名から `.frq` 名を作る（`TR-PKG-05`）。
///
/// 拡張子のドットをアンダースコアに置き換えて `.frq` を付ける。
/// `s001.wav` → `s001_wav.frq`。この規則を外すと UTAU 側が表を見つけられない。
#[must_use]
pub fn frq_name(wav: &str) -> String {
    let stem = wav.strip_suffix(".wav").unwrap_or(wav);
    format!("{stem}_wav.frq")
}

/// 区画の prefix / suffix を付けたエイリアス（`TR-PKG-19`）。
///
/// 多音階でサブフォルダを分けても、フォルダ間で同名エイリアスが生じない
/// ようにする。ここを飛ばすと、音源全体での一意性が崩れる。
#[must_use]
pub fn decorate(subbank: &Subbank, alias: &str) -> String {
    format!("{}{alias}{}", subbank.prefix, subbank.suffix)
}

#[cfg(test)]
mod tests {
    use super::*;
    use koeru_core::oto::Oto;

    fn oto() -> Oto {
        Oto {
            offset_ms: 100.0,
            consonant_ms: 60.0,
            cutoff_ms: 400.0,
            preutterance_ms: 40.0,
            overlap_ms: 20.0,
        }
    }

    fn sample(file: &str, aliases: &[&str]) -> Sample {
        Sample {
            file: file.to_owned(),
            master: PathBuf::from(file),
            frq: None,
            entries: aliases
                .iter()
                .map(|a| IniEntry {
                    file: file.to_owned(),
                    alias: (*a).to_owned(),
                    oto: oto(),
                })
                .collect(),
        }
    }

    fn subbank(folder: Option<&str>, prefix: &str, files: &[(&str, &[&str])]) -> Subbank {
        Subbank {
            folder: folder.map(str::to_owned),
            color: folder.unwrap_or_default().to_owned(),
            prefix: prefix.to_owned(),
            suffix: String::new(),
            tone: None,
            samples: files.iter().map(|(f, a)| sample(f, a)).collect(),
        }
    }

    fn bank(subbanks: Vec<Subbank>) -> VoiceBank {
        VoiceBank {
            distribution_name: "koeru".to_owned(),
            character: Character::default(),
            readme: Readme::default(),
            method: Method::Single,
            tones: vec![57],
            subbanks,
            rules: koeru_core::presamp::Rules::builtin(koeru_core::inventory::UnitSet::Core),
        }
    }

    #[test]
    fn 単一音階はサブフォルダを持たない() {
        let b = bank(vec![subbank(None, "", &[("s001.wav", &["あ"])])]);
        assert!(!b.is_multi_pitch());
        assert_eq!(b.wav_paths(), ["s001.wav"]);
    }

    /// 区画の prefix を付けないと、フォルダ間で同名エイリアスが生じる（`TR-PKG-19`）。
    #[test]
    fn 多音階のエイリアスには接頭辞が付く() {
        let b = bank(vec![
            subbank(Some("C4"), "", &[("s001.wav", &["あ"])]),
            subbank(Some("G4"), "↑", &[("s001.wav", &["あ"])]),
        ]);
        assert!(b.is_multi_pitch());
        assert_eq!(b.aliases(), ["あ", "↑あ"]);
        assert_eq!(b.wav_paths(), ["C4/s001.wav", "G4/s001.wav"]);
    }
}
