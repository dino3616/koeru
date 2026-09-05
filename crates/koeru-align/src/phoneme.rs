//! 音素と、仮名からの写像（`TR-ALN-07`）。
//!
//! 実行時に g2p を持ち込まない。
//!
//! > 仮名から音素への写像は辞書ファイル（例: `ka → k a`）としてリソースに置き、
//! > コードに埋め込まない。
//!
//! 辞書は [`resources/kana-phonemes.tsv`] に、音素セットは
//! [`resources/mfa-japanese-phones.tsv`] に置いてある。**どちらもコンパイル時に
//! 埋め込む**（`include_str!`）ので、実行時のダウンロードも外部ファイルの探索も無い
//! （`TR-PLT-19`, `TR-PLT-20`）。写像を直すのに Rust を触らなくてよい形は保っている。
//!
//! # 音素は音素セットの中からしか作れない
//!
//! [`Phoneme`] は `&'static str` を包んでいて、[`phone_set`] に載っている記号からしか
//! 作れない。モデルが知らない音素をアライナへ渡せない。 渡してしまうと、
//! Kaldi 側で番号が引けずに落ちるか、黙って `<eps>` に潰れる。
//!
//! [`resources/kana-phonemes.tsv`]: https://github.com/dino3616/koeru/blob/main/crates/koeru-align/resources/kana-phonemes.tsv
//! [`resources/mfa-japanese-phones.tsv`]: https://github.com/dino3616/koeru/blob/main/crates/koeru-align/resources/mfa-japanese-phones.tsv

use std::collections::BTreeMap;
use std::sync::OnceLock;

/// MFA 日本語音響モデルの音素セット（原文）。
const PHONES_TSV: &str = include_str!("../resources/mfa-japanese-phones.tsv");

/// 仮名 → 音素列の辞書（`TR-ALN-07`）。
const KANA_TSV: &str = include_str!("../resources/kana-phonemes.tsv");

/// 無音の音素。モデルの `optional_silence_phone`。
pub const SILENCE: &str = "sil";

/// 未知語の音素。モデルの `oov_phone`。
pub const UNKNOWN: &str = "spn";

/// 音素1つ。
///
/// 音素セットに載っている記号からしか作れない（[`Phoneme::new`]）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Phoneme(&'static str);

impl Phoneme {
    /// 記号から作る。音素セットに無ければ `None`。
    #[must_use]
    pub fn new(symbol: &str) -> Option<Self> {
        phone_set().get_key_value(symbol).map(|(k, _)| Self(k))
    }

    /// IPA の記号。
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        self.0
    }

    /// モデル内の番号。Kaldi へ渡すときの整数。
    #[must_use]
    pub fn id(&self) -> u32 {
        phone_set()[self.0]
    }
}

impl std::fmt::Display for Phoneme {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// 辞書の読み込みに失敗した。
///
/// リソースは同梱物なので、これが出るのはビルドの取り違えだけ。
/// それでも `unwrap` にしないのは、落ちる場所が読めなくなるため。
#[derive(Debug, thiserror::Error)]
pub enum PhonemeError {
    /// 音素セットに無い記号が辞書に書いてある。
    #[error("辞書に音素セット外の記号がある")]
    UnknownSymbol,

    /// その読みが辞書に無い。
    #[error("辞書にその読みが無い")]
    UnknownReading,
}

impl PhonemeError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::UnknownSymbol => "phoneme.unknown_symbol",
            Self::UnknownReading => "phoneme.unknown_reading",
        }
    }
}

/// 音素セット。記号 → モデル内の番号。
fn phone_set() -> &'static BTreeMap<&'static str, u32> {
    static SET: OnceLock<BTreeMap<&'static str, u32>> = OnceLock::new();
    SET.get_or_init(|| {
        PHONES_TSV
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| l.split_once('\t'))
            .filter_map(|(sym, id)| id.trim().parse().ok().map(|n| (sym, n)))
            .collect()
    })
}

fn dictionary() -> &'static BTreeMap<&'static str, Vec<Phoneme>> {
    static DICT: OnceLock<BTreeMap<&'static str, Vec<Phoneme>>> = OnceLock::new();
    DICT.get_or_init(|| {
        KANA_TSV
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
            .filter_map(|l| l.split_once('\t'))
            .map(|(kana, phones)| {
                let v = phones
                    .split_whitespace()
                    .filter_map(Phoneme::new)
                    .collect::<Vec<_>>();
                (kana, v)
            })
            .collect()
    })
}

/// 音素セットに載っている記号の数。`<eps>` と `sil` と `spn` を含む。
#[must_use]
pub fn phone_count() -> usize {
    phone_set().len()
}

/// 辞書に載っている読みの数。
#[must_use]
pub fn reading_count() -> usize {
    dictionary().len()
}

/// 1モーラぶんの読みから音素列を引く（`TR-ALN-07`）。
///
/// # Errors
///
/// 辞書にその読みが無い。
pub fn phonemes_for(reading: &str) -> Result<&'static [Phoneme], PhonemeError> {
    dictionary()
        .get(reading)
        .map(Vec::as_slice)
        .ok_or(PhonemeError::UnknownReading)
}

/// 読み列から、アライナへ渡す音素列を組み立てる（`TR-ALN-07`）。
///
/// 前後の無音は足さない。 それはアライナ側の仕事で、
/// `TR-ALN-09` (a)(b) が「前後の無音区間の長さを自由にする」と定めている。
///
/// # Errors
///
/// いずれかの読みが辞書に無い。
pub fn phonemes_for_all(readings: &[&str]) -> Result<Vec<Phoneme>, PhonemeError> {
    let mut v = Vec::new();
    for r in readings {
        v.extend_from_slice(phonemes_for(r)?);
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 音素セットは 86 エントリ（`<eps>` / `sil` / `spn` ＋ 音素 83）。
    #[test]
    fn 音素セットの件数がモデルと一致する() {
        assert_eq!(phone_count(), 86);
    }

    /// 辞書は 144 行（拡張セットの収録単位と同数。`TR-RCL-02`）。
    #[test]
    fn 辞書は収録単位を全て覆う() {
        assert_eq!(reading_count(), 144);
        for u in koeru_core::inventory::units(koeru_core::inventory::UnitSet::Extended) {
            assert!(phonemes_for(u.kana).is_ok(), "{} が辞書に無い", u.kana);
        }
    }

    /// 辞書の記号は全て音素セットに載っている。
    /// 載っていない記号は `Phoneme::new` が落とすので、行が短くなって現れる。
    #[test]
    fn 辞書の記号は音素セットの中だけ() {
        for line in KANA_TSV
            .lines()
            .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        {
            let (kana, phones) = line.split_once('\t').expect("タブ区切り");
            let want = phones.split_whitespace().count();
            let got = phonemes_for(kana).expect("引ける").len();
            assert_eq!(got, want, "{kana} に音素セット外の記号がある: {phones}");
        }
    }

    #[test]
    fn 母音始まりは音素ひとつ() {
        assert_eq!(phonemes_for("あ").unwrap(), &[Phoneme::new("a").unwrap()]);
    }

    /// す / つ / ず の母音だけ中舌（MFA が歯茎音の後でそうしている）。
    #[test]
    fn 歯茎音の後の母音は中舌になる() {
        assert_eq!(phonemes_for("す").unwrap().last().unwrap().as_str(), "ɨ");
        assert_eq!(phonemes_for("つ").unwrap().last().unwrap().as_str(), "ɨ");
        assert_eq!(phonemes_for("ず").unwrap().last().unwrap().as_str(), "ɨ");
        assert_eq!(phonemes_for("く").unwrap().last().unwrap().as_str(), "ɯ");
    }

    /// き 行は口蓋化した別音素（MFA は `k` と `c` を分けている）。
    #[test]
    fn 口蓋化した子音は別の音素() {
        assert_eq!(phonemes_for("き").unwrap()[0].as_str(), "c");
        assert_eq!(phonemes_for("か").unwrap()[0].as_str(), "k");
        assert_eq!(phonemes_for("ぎ").unwrap()[0].as_str(), "ɟ");
        assert_eq!(phonemes_for("が").unwrap()[0].as_str(), "ɡ");
    }

    #[test]
    fn 撥音は独立した音素() {
        assert_eq!(phonemes_for("ん").unwrap(), &[Phoneme::new("ɴ").unwrap()]);
    }

    /// 音素セットに無い記号からは作れない。
    #[test]
    fn 音素セット外からは作れない() {
        assert!(Phoneme::new("ɸ").is_some());
        assert!(Phoneme::new("q").is_none());
        assert!(Phoneme::new("").is_none());
    }

    #[test]
    fn 無音と未知語はセットに載っている() {
        assert!(Phoneme::new(SILENCE).is_some());
        assert!(Phoneme::new(UNKNOWN).is_some());
    }

    #[test]
    fn 読み列から音素列を組み立てられる() {
        let v = phonemes_for_all(&["か", "き"]).expect("引ける");
        let s: Vec<&str> = v.iter().map(Phoneme::as_str).collect();
        assert_eq!(s, ["k", "a", "c", "i"]);
    }

    #[test]
    fn 辞書に無い読みは失敗する() {
        assert!(phonemes_for("ぢゃ").is_err());
    }
}
