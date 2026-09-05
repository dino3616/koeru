//! 日本語の収録単位インベントリ（`TR-RCL-02`）。
//!
//! KOERU 自身が保持する1個のテーブル。 OREMO・NHP・その他の配布リストの
//! ファイルを同梱・再配布しない。録音リストはこのテーブルからアルゴリズムで生成する。
//!
//! ## 導出手順（`TR-RCL-02` が求める「導出根拠の欄」）
//!
//! OpenUtau の presamp 既定値（母音 7 種 / 子音 30 種、MIT）から機械的に導出する。
//! コードは取り込まず、体系だけを参照する（`DEC-PLT-011`）。
//!
//! 1. presamp の子音表から、各子音が担う仮名を集める（149 種）
//! 2. 現代日本語の拍として出現しないものを外す（下記の5件）
//! 3. 残りを、外来語専用の拍かどうかで中核セットと拡張セットに分ける
//!
//! ## 外した拍と、その音韻的な根拠
//!
//! | 拍 | 根拠 |
//! |---|---|
//! | ゐ / ゑ | 歴史的仮名遣い。現代日本語の拍として出現しない |
//! | うぅ / いぃ | 母音の重複表記。CV として独立した拍ではない |
//! | ヴぅ | 「ヴ」と同じ拍。表記の揺れ |
//!
//! ## セットの大きさ
//!
//! 中核 102 / 拡張 144。 `TR-RCL-03` は当初 141 / 168 と書いていたが、
//! その数の根拠が追えなかったので、導出手順のほうを正本にした（`DEC-RCL-004`）。

/// インベントリの版。プロジェクトは作成時の版を記録する（`TR-RCL-02`）。
/// インベントリの版。
///
/// 方式プリセットは「方式 × モーラ長 × 音階数 × 音素インベントリ」の単一データ
/// （`TR-RCL-01`）。ここはその最後の軸で、[`crate::reclist`] が残りを組み合わせる。
pub const INVENTORY_VERSION: u32 = 1;

/// 母音クラス（7種）。presamp の既定値と同じ。
pub const VOWEL_CLASSES: [&str; 7] = ["a", "i", "u", "e", "o", "n", "N"];

/// 収録単位（CV 1つ）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unit {
    /// 仮名表記。読み上げるテキストとしてはこのまま使う（`TR-RCL-08`）。
    pub kana: &'static str,
    /// 子音記号。母音始まりは空。同一行にまとめる軸（`TR-RCL-03`）。
    pub consonant: &'static str,
    /// 母音クラス。
    pub vowel: &'static str,
    /// 無声破裂音か。oto のオーバーラップを 0 にする軸（`TR-ALN-16`）。
    ///
    /// presamp の子音行の末尾フラグは「1文字目を伸ばすか」であって、これではない。
    /// `TR-ALN-16` が挙げる k / t / p / ky / ty / py から判定する。
    /// ts / ch は破擦音で、閉鎖を持つが破裂音そのものではないので含めない。
    pub unvoiced_plosive: bool,
}

const fn u(
    kana: &'static str,
    consonant: &'static str,
    vowel: &'static str,
    unvoiced_plosive: bool,
) -> Unit {
    Unit {
        kana,
        consonant,
        vowel,
        unvoiced_plosive,
    }
}

/// 母音始まりと撥音。子音を持たない6拍。
const VOWEL_ONLY: [Unit; 6] = [
    u("あ", "", "a", false),
    u("い", "", "i", false),
    u("う", "", "u", false),
    u("え", "", "e", false),
    u("お", "", "o", false),
    u("ん", "", "n", false),
];

/// 中核セットの子音つき拍。presamp から機械的に導出した。
const CORE_CV: [Unit; 96] = [
    u("ち", "ch", "i", false),
    u("ちゃ", "ch", "a", false),
    u("ちゅ", "ch", "u", false),
    u("ちょ", "ch", "o", false),
    u("ぎ", "gy", "i", false),
    u("ぎゃ", "gy", "a", false),
    u("ぎゅ", "gy", "u", false),
    u("ぎょ", "gy", "o", false),
    u("つ", "ts", "u", false),
    u("ぴ", "py", "i", true),
    u("ぴゃ", "py", "a", true),
    u("ぴゅ", "py", "u", true),
    u("ぴょ", "py", "o", true),
    u("り", "ry", "i", false),
    u("りゃ", "ry", "a", false),
    u("りゅ", "ry", "u", false),
    u("りょ", "ry", "o", false),
    u("に", "ny", "i", false),
    u("にゃ", "ny", "a", false),
    u("にゅ", "ny", "u", false),
    u("にょ", "ny", "o", false),
    u("ら", "r", "a", false),
    u("る", "r", "u", false),
    u("れ", "r", "e", false),
    u("ろ", "r", "o", false),
    u("ひ", "hy", "i", false),
    u("ひゃ", "hy", "a", false),
    u("ひゅ", "hy", "u", false),
    u("ひょ", "hy", "o", false),
    u("び", "by", "i", false),
    u("びゃ", "by", "a", false),
    u("びゅ", "by", "u", false),
    u("びょ", "by", "o", false),
    u("ば", "b", "a", false),
    u("ぶ", "b", "u", false),
    u("べ", "b", "e", false),
    u("ぼ", "b", "o", false),
    u("だ", "d", "a", false),
    u("で", "d", "e", false),
    u("ど", "d", "o", false),
    u("が", "g", "a", false),
    u("ぐ", "g", "u", false),
    u("げ", "g", "e", false),
    u("ご", "g", "o", false),
    u("ふ", "f", "u", false),
    u("は", "h", "a", false),
    u("へ", "h", "e", false),
    u("ほ", "h", "o", false),
    u("か", "k", "a", true),
    u("く", "k", "u", true),
    u("け", "k", "e", true),
    u("こ", "k", "o", true),
    u("じ", "j", "i", false),
    u("じゃ", "j", "a", false),
    u("じゅ", "j", "u", false),
    u("じょ", "j", "o", false),
    u("ま", "m", "a", false),
    u("む", "m", "u", false),
    u("め", "m", "e", false),
    u("も", "m", "o", false),
    u("な", "n", "a", false),
    u("ぬ", "n", "u", false),
    u("ね", "n", "e", false),
    u("の", "n", "o", false),
    u("ぱ", "p", "a", true),
    u("ぷ", "p", "u", true),
    u("ぺ", "p", "e", true),
    u("ぽ", "p", "o", true),
    u("さ", "s", "a", false),
    u("す", "s", "u", false),
    u("せ", "s", "e", false),
    u("そ", "s", "o", false),
    u("し", "sh", "i", false),
    u("しゃ", "sh", "a", false),
    u("しゅ", "sh", "u", false),
    u("しょ", "sh", "o", false),
    u("た", "t", "a", true),
    u("て", "t", "e", true),
    u("と", "t", "o", true),
    u("わ", "w", "a", false),
    u("を", "w", "o", false),
    u("や", "y", "a", false),
    u("ゆ", "y", "u", false),
    u("よ", "y", "o", false),
    u("き", "ky", "i", true),
    u("きゃ", "ky", "a", true),
    u("きゅ", "ky", "u", true),
    u("きょ", "ky", "o", true),
    u("ざ", "z", "a", false),
    u("ず", "z", "u", false),
    u("ぜ", "z", "e", false),
    u("ぞ", "z", "o", false),
    u("み", "my", "i", false),
    u("みゃ", "my", "a", false),
    u("みゅ", "my", "u", false),
    u("みょ", "my", "o", false),
];

/// 拡張セットで足す拍。外来語で使う。
const EXTENDED_CV: [Unit; 42] = [
    u("ちぇ", "ch", "e", false),
    u("ぎぇ", "gy", "e", false),
    u("つぁ", "ts", "a", false),
    u("つぃ", "ts", "i", false),
    u("つぇ", "ts", "e", false),
    u("つぉ", "ts", "o", false),
    u("てぃ", "ty", "i", true),
    u("てぇ", "ty", "e", true),
    u("てゃ", "ty", "a", true),
    u("てゅ", "ty", "u", true),
    u("てょ", "ty", "o", true),
    u("ぴぇ", "py", "e", true),
    u("りぇ", "ry", "e", false),
    u("にぇ", "ny", "e", false),
    u("ひぇ", "hy", "e", false),
    u("でぃ", "dy", "i", false),
    u("でぇ", "dy", "e", false),
    u("でゃ", "dy", "a", false),
    u("でゅ", "dy", "u", false),
    u("でょ", "dy", "o", false),
    u("びぇ", "by", "e", false),
    u("どぅ", "d", "u", false),
    u("ふぁ", "f", "a", false),
    u("ふぃ", "f", "i", false),
    u("ふぇ", "f", "e", false),
    u("ふぉ", "f", "o", false),
    u("じぇ", "j", "e", false),
    u("すぃ", "s", "i", false),
    u("しぇ", "sh", "e", false),
    u("とぅ", "t", "u", true),
    u("うぃ", "w", "i", false),
    u("うぇ", "w", "e", false),
    u("うぉ", "w", "o", false),
    u("ヴ", "v", "u", false),
    u("ヴぁ", "v", "a", false),
    u("ヴぃ", "v", "i", false),
    u("ヴぇ", "v", "e", false),
    u("ヴぉ", "v", "o", false),
    u("いぇ", "y", "e", false),
    u("きぇ", "ky", "e", true),
    u("ずぃ", "z", "i", false),
    u("みぇ", "my", "e", false),
];

/// 収録単位の集合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnitSet {
    /// 現代日本語で実際に出現する拍。
    Core,
    /// 外来語の拍を足す。
    Extended,
}

/// インベントリを引く。
///
/// 並びは常に同じ（`TR-RCL-27` の決定性）。母音始まりが先、続いて子音行が揃う順。
#[must_use]
pub fn units(set: UnitSet) -> Vec<Unit> {
    let mut v: Vec<Unit> = VOWEL_ONLY.to_vec();
    v.extend(CORE_CV.iter().cloned());
    if set == UnitSet::Extended {
        v.extend(EXTENDED_CV.iter().cloned());
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 中核セットは 102 件（`DEC-RCL-004` の導出手順から）。
    #[test]
    fn 中核セットの件数が導出どおり() {
        assert_eq!(units(UnitSet::Core).len(), 102);
    }

    /// 拡張セットは 144 件。
    #[test]
    fn 拡張セットの件数が導出どおり() {
        assert_eq!(units(UnitSet::Extended).len(), 144);
    }

    #[test]
    fn 母音クラスは七種() {
        assert_eq!(VOWEL_CLASSES.len(), 7);
    }

    /// 子音は 30 種（presamp の既定値と一致）。
    #[test]
    fn 子音は三十種() {
        let mut s: Vec<&str> = units(UnitSet::Extended)
            .iter()
            .map(|u| u.consonant)
            .filter(|c| !c.is_empty())
            .collect();
        s.sort_unstable();
        s.dedup();
        assert_eq!(s.len(), 30, "{s:?}");
    }

    /// 仮名が重複しない。 重複すると録音リストに同じ行が2度出る。
    #[test]
    fn 仮名は一意() {
        for set in [UnitSet::Core, UnitSet::Extended] {
            let mut seen = std::collections::BTreeSet::new();
            for u in units(set) {
                assert!(seen.insert(u.kana), "{} が重複している", u.kana);
            }
        }
    }

    /// 母音クラスは 7 種のいずれか。
    #[test]
    fn 母音クラスは定義済みのものだけ() {
        for u in units(UnitSet::Extended) {
            assert!(
                VOWEL_CLASSES.contains(&u.vowel),
                "{} の母音 {} が未定義",
                u.kana,
                u.vowel
            );
        }
    }

    /// 外した拍が入っていない（`DEC-RCL-004`）。
    #[test]
    fn 現代日本語で出現しない拍は入らない() {
        let v = units(UnitSet::Extended);
        for k in ["ゐ", "ゑ", "うぅ", "いぃ", "ヴぅ"] {
            assert!(!v.iter().any(|u| u.kana == k), "{k} が残っている");
        }
    }

    /// 生成が決定的（`TR-RCL-27`）。
    #[test]
    fn 何度引いても同じ並びになる() {
        for set in [UnitSet::Core, UnitSet::Extended] {
            assert_eq!(units(set), units(set));
        }
    }

    /// 無声破裂音が印されている（`TR-ALN-16` のオーバーラップ分岐に使う）。
    #[test]
    fn 無声破裂音が印されている() {
        let v = units(UnitSet::Core);
        let ka = v.iter().find(|u| u.kana == "か").expect("か がある");
        assert!(ka.unvoiced_plosive);
        let na = v.iter().find(|u| u.kana == "な").expect("な がある");
        assert!(!na.unvoiced_plosive);
    }
}
