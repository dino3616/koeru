//! 規約プリセットの外部化（`TR-ALN-23`）。
//!
//! > 5値の規約（マージン、比率、子音クラス別係数、方式別分岐）をコードから分離した
//! > 宣言的なプリセットデータとして保持する。既定プリセットを方式ごとに1つ用意し、
//! > 上級モードで編集・保存・プロジェクトへ適用できる。
//! > プリセットの変更は再計算だけで反映され、再アライメントを要求しない
//!
//! # 再アライメントを要求しない、が効いている
//!
//! 導出（[`crate::derive`]）はアライナが出した境界だけを入力に取る。プリセットは
//! 境界から5値を作る式のパラメータでしかない。 だからマージンを 20ms から 25ms へ
//! 変えても、推論はやり直さなくてよい（`TR-ALN-13` の三分法がこれを可能にしている）。
//!
//! # 既定は方式ごとに1つ
//!
//! [`resources/presets.toml`] に置いてある。コードに定数を書かないのが
//! `TR-ALN-23` の要求で、[`Preset::default_for`] はそこから読む。
//!
//! # 版を持つ
//!
//! [`Preset::version`] が変わったら再計算の対象になる（`TR-ALN-29` の決定性）。
//! 上級モードで編集したプリセットも版を持つ——持たないと、
//! 「値が変わったのに再計算されない」が起きる。
//!
//! [`resources/presets.toml`]: https://github.com/dino3616/koeru/blob/main/crates/koeru-align/resources/presets.toml

use std::collections::BTreeMap;

use koeru_core::alias::Method;

/// 既定プリセット（方式ごとに1つ）。
const PRESETS_TOML: &str = include_str!("../resources/presets.toml");

/// 子音のクラス（`TR-ALN-17` の子音クラス別係数）。
///
/// オーバーラップと子音部の扱いがクラスで分かれる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ConsonantClass {
    /// 母音始まり。子音を持たない。
    None,
    /// 無声破裂音（k / t / p 系）。オーバーラップを 0 にする（`TR-ALN-16`）。
    UnvoicedPlosive,
    /// 破擦音（ts / ch 系）。閉鎖を持つが破裂音そのものではない。
    Affricate,
    /// 摩擦音（s / sh / h / f 系）。
    Fricative,
    /// 鼻音（m / n 系）。
    Nasal,
    /// はじき音（r 系）。短いので子音部を詰める。
    Flap,
    /// 上のどれでもない有声子音（b / d / g / w / y / v / z 系）。
    Voiced,
}

impl ConsonantClass {
    /// プリセットの表で使う名前。
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::UnvoicedPlosive => "unvoiced_plosive",
            Self::Affricate => "affricate",
            Self::Fricative => "fricative",
            Self::Nasal => "nasal",
            Self::Flap => "flap",
            Self::Voiced => "voiced",
        }
    }

    /// UTAU 式の子音記号から引く。
    ///
    /// `unvoiced_plosive` の判定は `koeru_core::inventory::Unit` が正本
    /// （`TR-ALN-16` が挙げる k / t / p / ky / ty / py）。ここはそれ以外の
    /// クラス分けを足しているだけ。
    #[must_use]
    pub fn of(consonant: &str) -> Self {
        match consonant {
            "" => Self::None,
            "k" | "ky" | "t" | "ty" | "p" | "py" => Self::UnvoicedPlosive,
            "ts" | "ch" => Self::Affricate,
            "s" | "sh" | "h" | "hy" | "f" => Self::Fricative,
            "m" | "my" | "n" | "ny" => Self::Nasal,
            "r" | "ry" => Self::Flap,
            _ => Self::Voiced,
        }
    }
}

/// 子音クラスごとの係数（`TR-ALN-17`, `TR-ALN-23`）。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ClassCoefficients {
    /// オーバーラップ比。オフセットから先行発声までの区間に掛ける（`TR-ALN-16`）。
    pub overlap_ratio: f64,
    /// 母音定常マージン。子音部の下限に効く（`TR-ALN-17`）。
    pub vowel_steady_margin_ms: f64,
}

/// 導出の規約（`TR-ALN-23`）。
#[derive(Debug, Clone, PartialEq)]
pub struct Preset {
    /// 識別子。再計算の鍵に混ぜる（`TR-ALN-29`）。
    pub id: String,
    /// 版。上げたら再計算の対象になる。
    pub version: u32,
    /// 対象の方式（`TR-ALN-23` の方式別分岐）。
    pub method: Method,
    /// オフセットの前に残す余白（`TR-ALN-14`）。
    pub leading_margin_ms: f64,
    /// 子音クラスごとの係数。
    pub classes: BTreeMap<String, ClassCoefficients>,
}

/// プリセットの読み書きで起きる失敗。
#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    /// TOML として読めない。
    #[error("プリセットを読めない")]
    Malformed,

    /// その方式の既定プリセットが無い。
    #[error("その方式の既定プリセットが無い")]
    NoDefaultForMethod,

    /// 必要な欄が無い。
    #[error("プリセットに必要な欄が無い")]
    MissingField,
}

impl PresetError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Malformed => "preset.malformed",
            Self::NoDefaultForMethod => "preset.no_default_for_method",
            Self::MissingField => "preset.missing_field",
        }
    }
}

type Result<T> = std::result::Result<T, PresetError>;

const fn method_key(m: Method) -> &'static str {
    match m {
        Method::Single => "single",
        Method::Sequential => "sequential",
        Method::Cvvc => "cvvc",
    }
}

impl Preset {
    /// 方式ごとの既定プリセット（`TR-ALN-23`）。
    ///
    /// # Errors
    ///
    /// 同梱のプリセットが壊れている、その方式の既定が無い。
    pub fn default_for(method: Method) -> Result<Self> {
        let doc: toml_edit::DocumentMut =
            PRESETS_TOML.parse().map_err(|_| PresetError::Malformed)?;
        let table = doc
            .get(method_key(method))
            .and_then(toml_edit::Item::as_table)
            .ok_or(PresetError::NoDefaultForMethod)?;
        Self::from_table(method, table)
    }

    fn from_table(method: Method, t: &toml_edit::Table) -> Result<Self> {
        let s = |k: &str| {
            t.get(k)
                .and_then(|v| v.as_str())
                .ok_or(PresetError::MissingField)
        };
        let f = |k: &str| {
            t.get(k)
                .and_then(toml_edit::Item::as_float)
                .ok_or(PresetError::MissingField)
        };
        let i = |k: &str| {
            t.get(k)
                .and_then(toml_edit::Item::as_integer)
                .ok_or(PresetError::MissingField)
        };

        let mut classes = BTreeMap::new();
        let ct = t
            .get("class")
            .and_then(toml_edit::Item::as_table)
            .ok_or(PresetError::MissingField)?;
        for (name, item) in ct {
            let c = item.as_table().ok_or(PresetError::MissingField)?;
            let g = |k: &str| {
                c.get(k)
                    .and_then(toml_edit::Item::as_float)
                    .ok_or(PresetError::MissingField)
            };
            classes.insert(
                name.to_owned(),
                ClassCoefficients {
                    overlap_ratio: g("overlap_ratio")?,
                    vowel_steady_margin_ms: g("vowel_steady_margin_ms")?,
                },
            );
        }

        Ok(Self {
            id: s("id")?.to_owned(),
            version: u32::try_from(i("version")?).map_err(|_| PresetError::MissingField)?,
            method,
            leading_margin_ms: f("leading_margin_ms")?,
            classes,
        })
    }

    /// 子音クラスの係数を引く。無ければ `Voiced` へ落とす。
    #[must_use]
    pub fn coefficients(&self, class: ConsonantClass) -> ClassCoefficients {
        self.classes
            .get(class.key())
            .or_else(|| self.classes.get(ConsonantClass::Voiced.key()))
            .copied()
            .unwrap_or(ClassCoefficients {
                overlap_ratio: 1.0 / 3.0,
                vowel_steady_margin_ms: 30.0,
            })
    }

    /// 再計算の鍵に混ぜる文字列（`TR-ALN-29`）。
    #[must_use]
    pub fn identity(&self) -> String {
        format!("{}@{}", self.id, self.version)
    }

    /// TOML へ書き出す（上級モードでの保存。`TR-ALN-23`）。
    #[must_use]
    pub fn to_toml(&self) -> String {
        let mut s = format!(
            "id = {:?}\nversion = {}\nleading_margin_ms = {:?}\n",
            self.id, self.version, self.leading_margin_ms
        );
        for (name, c) in &self.classes {
            s.push_str(&format!(
                "\n[class.{name}]\noverlap_ratio = {:?}\nvowel_steady_margin_ms = {:?}\n",
                c.overlap_ratio, c.vowel_steady_margin_ms
            ));
        }
        s
    }

    /// TOML から読む（上級モードで編集したものを戻す。`TR-ALN-23`）。
    ///
    /// # Errors
    ///
    /// TOML として読めない、必要な欄が無い。
    pub fn from_toml(method: Method, src: &str) -> Result<Self> {
        let doc: toml_edit::DocumentMut = src.parse().map_err(|_| PresetError::Malformed)?;
        Self::from_table(method, doc.as_table())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 方式ごとに既定がある（`TR-ALN-23`）。
    #[test]
    fn 方式ごとの既定プリセットがある() {
        for m in [Method::Single, Method::Sequential, Method::Cvvc] {
            let p = Preset::default_for(m).expect("既定がある");
            assert_eq!(p.method, m);
            assert!(!p.id.is_empty());
            assert!(p.version >= 1);
        }
    }

    /// 定数をコードに書かない（`TR-ALN-23`）。リソース側の値が使われている。
    #[test]
    fn 既定の値はリソースから来る() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        assert!((p.leading_margin_ms - 20.0).abs() < 1e-9);
        let c = p.coefficients(ConsonantClass::None);
        assert!((c.overlap_ratio - 1.0 / 3.0).abs() < 1e-9);
    }

    /// 無声破裂音はオーバーラップを取らない（`TR-ALN-16`）。
    #[test]
    fn 無声破裂音の係数はゼロ() {
        let p = Preset::default_for(Method::Single).expect("既定がある");
        assert_eq!(
            p.coefficients(ConsonantClass::UnvoicedPlosive)
                .overlap_ratio,
            0.0
        );
    }

    #[test]
    fn 子音記号からクラスを引ける() {
        assert_eq!(ConsonantClass::of(""), ConsonantClass::None);
        assert_eq!(ConsonantClass::of("k"), ConsonantClass::UnvoicedPlosive);
        assert_eq!(ConsonantClass::of("py"), ConsonantClass::UnvoicedPlosive);
        assert_eq!(ConsonantClass::of("ts"), ConsonantClass::Affricate);
        assert_eq!(ConsonantClass::of("sh"), ConsonantClass::Fricative);
        assert_eq!(ConsonantClass::of("ny"), ConsonantClass::Nasal);
        assert_eq!(ConsonantClass::of("r"), ConsonantClass::Flap);
        assert_eq!(ConsonantClass::of("b"), ConsonantClass::Voiced);
    }

    /// 知らないクラスは有声子音へ落とす。 プリセットに欄が増えても落ちない。
    #[test]
    fn 知らないクラスは有声へ落ちる() {
        let mut p = Preset::default_for(Method::Single).expect("既定がある");
        p.classes.remove(ConsonantClass::Nasal.key());
        let voiced = p.coefficients(ConsonantClass::Voiced);
        assert_eq!(p.coefficients(ConsonantClass::Nasal), voiced);
    }

    /// 上級モードで編集して保存し、読み戻せる（`TR-ALN-23`）。
    #[test]
    fn 編集して保存して読み戻せる() {
        let mut p = Preset::default_for(Method::Single).expect("既定がある");
        p.leading_margin_ms = 25.0;
        p.version += 1;
        let back = Preset::from_toml(Method::Single, &p.to_toml()).expect("読める");
        assert_eq!(back, p);
    }

    /// 版が変われば鍵が変わる（`TR-ALN-29`）。
    #[test]
    fn 版が変われば鍵が変わる() {
        let mut p = Preset::default_for(Method::Single).expect("既定がある");
        let before = p.identity();
        p.version += 1;
        assert_ne!(before, p.identity());
    }

    #[test]
    fn 壊れたプリセットは読めない() {
        assert!(matches!(
            Preset::from_toml(Method::Single, "これは TOML ではない ["),
            Err(PresetError::Malformed)
        ));
        assert!(matches!(
            Preset::from_toml(Method::Single, "id = \"x\"\n"),
            Err(PresetError::MissingField)
        ));
    }

    #[test]
    fn 失敗の種別は固定文字列() {
        for e in [
            PresetError::Malformed,
            PresetError::NoDefaultForMethod,
            PresetError::MissingField,
        ] {
            assert!(e.kind().starts_with("preset."));
        }
    }
}
