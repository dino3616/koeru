//! 指紋（`DEC-PLT-053`）。
//!
//! 「同じ中身か」を言うための値。 中身の SHA-256 で持ち、パスも更新時刻も混ぜない。
//! 同じ中身が別の場所にあれば同じ指紋、同じ場所で中身が変われば別の指紋になる。
//!
//! 3つの層がある。
//!
//! - [`Digest`] — 値そのもの。 バイト列の SHA-256
//! - [`Canonical`] — 複数の欄を、曖昧さの無い並べ方で1つの [`Digest`] へ畳む
//! - [`InputFingerprint`] と [`OutputFingerprint`] — 計算が読んだものと、計算が出したもの
//!
//! 入出力を持たない。 ファイルを読むのは呼び出し側で、[`ContentHasher`] へ少しずつ渡す。
//!
//! **同じプロセスの中でだけ使う鍵（`std::hash::DefaultHasher`）とは別のもの。** あちらは
//! 版や起動で値が変わりうるので、永続化や受領証には使えない。 ここの値は、どの環境でも
//! 同じ入力から同じバイト列になる（`TR-ALN-29`）。

use std::collections::BTreeMap;
use std::fmt;

use sha2::{Digest as _, Sha256};

use crate::oto::{Boundary, Oto};

/// バイト列の SHA-256。
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest([u8; Digest::LEN]);

impl Digest {
    /// 値のバイト数。
    pub const LEN: usize = 32;

    /// バイト列の指紋。
    #[must_use]
    pub fn of(bytes: &[u8]) -> Self {
        let mut hasher = ContentHasher::new();
        hasher.update(bytes);
        hasher.finish()
    }

    /// 生のバイト列。
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; Self::LEN] {
        &self.0
    }

    /// 小文字の16進（64字）。 台帳へ置く形。
    #[must_use]
    pub fn to_hex(&self) -> String {
        use std::fmt::Write as _;
        self.0.iter().fold(String::with_capacity(64), |mut out, b| {
            let _ = write!(out, "{b:02x}");
            out
        })
    }

    /// [`Self::to_hex`] の逆。 64字の16進でなければ `None`。
    ///
    /// 大文字は受け取らない。 同じ値に2つの表記があると、文字列の比べで外れる。
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != Self::LEN * 2 {
            return None;
        }
        let mut out = [0_u8; Self::LEN];
        // 長さは上で確かめてある（64字）ので、余りは出ない。
        let (pairs, _) = bytes.as_chunks::<2>();
        for (slot, [hi, lo]) in out.iter_mut().zip(pairs) {
            *slot = (nibble(*hi)? << 4) | nibble(*lo)?;
        }
        Some(Self(out))
    }
}

fn nibble(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        _ => None,
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Digest({})", self.to_hex())
    }
}

/// 大きなファイルを少しずつ食わせる [`Digest`] の作り手。
///
/// 区切り方で値が変わらない。 何バイトずつ渡しても、全部をつなげたものと同じ値になる。
#[derive(Clone, Default)]
pub struct ContentHasher(Sha256);

impl ContentHasher {
    #[must_use]
    pub fn new() -> Self {
        Self(Sha256::new())
    }

    pub fn update(&mut self, bytes: &[u8]) {
        self.0.update(bytes);
    }

    #[must_use]
    pub fn finish(self) -> Digest {
        Digest(self.0.finalize().into())
    }
}

impl fmt::Debug for ContentHasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ContentHasher { .. }")
    }
}

/// 複数の欄を、曖昧さの無い並べ方で1つの [`Digest`] へ畳む。
///
/// 欄は「名前の長さ・名前・型の印・値の長さ・値」で並べる。 区切り文字で繋ぐと、
/// `("ab", "c")` と `("a", "bc")` が同じ並びになる。
///
/// 先頭に用途の名前（`domain`）を入れる。 別の用途が同じ欄を同じ値で並べても、
/// 別の指紋になる。
#[derive(Clone)]
pub struct Canonical(Sha256);

/// 欄の型の印。 値のバイト列が何の符号化かを混ぜる。
const TAG_BYTES: u8 = 1;
const TAG_TEXT: u8 = 2;
const TAG_DIGEST: u8 = 3;
const TAG_FLOAT: u8 = 4;
const TAG_ABSENT: u8 = 5;

impl Canonical {
    #[must_use]
    pub fn new(domain: &str) -> Self {
        let mut hasher = Sha256::new();
        put_len(&mut hasher, domain.len());
        hasher.update(domain.as_bytes());
        Self(hasher)
    }

    fn field(mut self, name: &str, tag: u8, value: &[u8]) -> Self {
        put_len(&mut self.0, name.len());
        self.0.update(name.as_bytes());
        self.0.update([tag]);
        put_len(&mut self.0, value.len());
        self.0.update(value);
        self
    }

    /// 生のバイト列。
    #[must_use]
    pub fn bytes(self, name: &str, value: &[u8]) -> Self {
        self.field(name, TAG_BYTES, value)
    }

    /// 文字列。 UTF-8 のバイト列をそのまま混ぜる。 正規化はしない。
    #[must_use]
    pub fn text(self, name: &str, value: &str) -> Self {
        self.field(name, TAG_TEXT, value.as_bytes())
    }

    /// 別の指紋。
    #[must_use]
    pub fn digest(self, name: &str, value: &Digest) -> Self {
        self.field(name, TAG_DIGEST, value.as_bytes())
    }

    /// あるかもしれない指紋。 **無いことも値として混ぜる**——省いて並べると、
    /// 欄の数が違うだけで偶然に同じ並びになりうる。
    #[must_use]
    pub fn optional_digest(self, name: &str, value: Option<&Digest>) -> Self {
        match value {
            Some(v) => self.digest(name, v),
            None => self.field(name, TAG_ABSENT, &[]),
        }
    }

    /// 浮動小数。 ビット列で混ぜる。 丸めで指紋が動かないように、`0.0` と `-0.0` は別の値。
    #[must_use]
    pub fn float(self, name: &str, value: f64) -> Self {
        self.field(name, TAG_FLOAT, &value.to_bits().to_le_bytes())
    }

    #[must_use]
    pub fn finish(self) -> Digest {
        Digest(self.0.finalize().into())
    }
}

impl fmt::Debug for Canonical {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Canonical { .. }")
    }
}

fn put_len(hasher: &mut Sha256, len: usize) {
    // `usize` の幅は環境で違う。 8バイトのリトルエンディアンへ揃える。
    hasher.update((len as u64).to_le_bytes());
}

/// 計算が読んだものの指紋（`DEC-PLT-053`）。
///
/// どれかが変われば別の指紋になる。 再計算するかどうかは、この比べだけで決まる。
///
/// `TR-ALN-29` の「入力のハッシュ・モデルバージョン・プリセットバージョン」に、マスターの
/// 内容・計算器の識別・パラメータ・綴りの表を足したもの。 **既に台帳へ置いてある指紋
/// （`koeru_align::determinism::Fingerprint`）とは別の値で、置き換えない。** あちらは
/// アライメントの入口の標本を見ており、こちらはマスターのファイルの中身を見る。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputFingerprint {
    /// マスターの内容の指紋。 パスや更新時刻ではない。
    pub master: Digest,
    /// 読み（録音リストがその行に持っている音素列の元）。
    pub reading: String,
    /// 規約プリセットの識別（`Preset::identity`）。 版を含む。
    pub preset: String,
    /// 計算器の識別。 モデルの版を含む（`Aligner::identity`）。
    pub engine: String,
    /// 計算器のパラメータ。 名前順に混ぜるので、並べた順は効かない。
    pub parameters: BTreeMap<String, String>,
    /// 綴りの表の版。 読まない計算には無い。
    pub rules: Option<Digest>,
}

/// [`InputFingerprint`] を畳むときの用途の名前。 欄の意味を変えたら版を上げる。
const INPUT_DOMAIN: &str = "koeru.fingerprint.input.v1";

impl InputFingerprint {
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut c = Canonical::new(INPUT_DOMAIN)
            .digest("master", &self.master)
            .text("reading", &self.reading)
            .text("preset", &self.preset)
            .text("engine", &self.engine)
            .optional_digest("rules", self.rules.as_ref());
        // 件数を先に入れる。 欄の並びだけだと、パラメータの後ろに別の欄を足したときに
        // 境目が曖昧になる。
        c = c.bytes("parameters", &(self.parameters.len() as u64).to_le_bytes());
        for (k, v) in &self.parameters {
            c = c.text(k, v);
        }
        c.finish()
    }
}

/// 計算が出したものの指紋（`DEC-PLT-053`）。
///
/// oto の5値・境界・`.frq` を、綴りの順に並べ直してから畳む。 計算が出した順や、
/// 台帳から読み出した順で値が動かない。 `TR-ALN-29` の「ビット単位で同一」を、
/// 1つの値の比べで言うために使う。
#[derive(Debug, Clone, Copy)]
pub struct OutputFingerprint<'a> {
    /// 綴りごとの5値。
    pub otos: &'a [(String, Oto)],
    /// 綴りごとの境界。
    pub boundaries: &'a [(String, Boundary)],
    /// 書き出す `.frq` のバイト列（`koeru_formats::frq::Frq::to_bytes`）。
    pub frq: &'a [u8],
}

const OUTPUT_DOMAIN: &str = "koeru.fingerprint.output.v1";

impl OutputFingerprint<'_> {
    #[must_use]
    pub fn digest(&self) -> Digest {
        let mut otos: Vec<&(String, Oto)> = self.otos.iter().collect();
        otos.sort_by(|a, b| a.0.cmp(&b.0));
        let mut boundaries: Vec<&(String, Boundary)> = self.boundaries.iter().collect();
        boundaries.sort_by(|a, b| a.0.cmp(&b.0));

        let mut c = Canonical::new(OUTPUT_DOMAIN).bytes("otos", &(otos.len() as u64).to_le_bytes());
        for (alias, oto) in otos {
            c = c.text("alias", alias);
            for (name, v) in [
                ("offset", oto.offset_ms),
                ("consonant", oto.consonant_ms),
                ("cutoff", oto.cutoff_ms),
                ("preutterance", oto.preutterance_ms),
                ("overlap", oto.overlap_ms),
            ] {
                c = c.float(name, v);
            }
        }
        c = c.bytes("boundaries", &(boundaries.len() as u64).to_le_bytes());
        for (alias, b) in boundaries {
            c = c.text("alias", alias);
            for (name, v) in [
                ("voice_start", b.voice_start_ms),
                ("vowel_start", b.vowel_start_ms),
                ("vowel_end", b.vowel_end_ms),
            ] {
                c = c.float(name, v);
            }
        }
        c.bytes("frq", self.frq).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn oto(offset: f64) -> Oto {
        Oto {
            offset_ms: offset,
            consonant_ms: 60.0,
            cutoff_ms: -300.0,
            preutterance_ms: 40.0,
            overlap_ms: 20.0,
        }
    }

    fn boundary(start: f64) -> Boundary {
        Boundary {
            voice_start_ms: start,
            vowel_start_ms: start + 30.0,
            vowel_end_ms: start + 200.0,
        }
    }

    fn input() -> InputFingerprint {
        InputFingerprint {
            master: Digest::of(b"master bytes"),
            reading: "か".to_owned(),
            preset: "single@1".to_owned(),
            engine: "mfa-japanese@3.3.0".to_owned(),
            parameters: BTreeMap::from([("first_pass".to_owned(), "final.alimdl".to_owned())]),
            rules: Some(Digest::of(b"rules")),
        }
    }

    /// 公開されている SHA-256 の既知の値と突き合わせる。 実装の取り違えを固定する。
    #[test]
    fn 既知の値と一致する() {
        assert_eq!(
            Digest::of(b"abc").to_hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(
            Digest::of(b"").to_hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn 少しずつ食わせても同じ値になる() {
        let whole = Digest::of(b"0123456789abcdef");
        let mut h = ContentHasher::new();
        for chunk in b"0123456789abcdef".chunks(3) {
            h.update(chunk);
        }
        assert_eq!(h.finish(), whole);
    }

    #[test]
    fn 十六進は往復し_大文字と長さ違いは断る() {
        let d = Digest::of(b"x");
        assert_eq!(Digest::from_hex(&d.to_hex()), Some(d));
        assert_eq!(Digest::from_hex(&d.to_hex().to_uppercase()), None);
        assert_eq!(Digest::from_hex(&d.to_hex()[..62]), None);
        assert_eq!(Digest::from_hex(&"g".repeat(64)), None);
        assert_eq!(d.to_string(), d.to_hex());
    }

    /// 区切りで繋ぐと同じになる並びが、別の指紋になる。
    #[test]
    fn 欄の境目で曖昧にならない() {
        let a = Canonical::new("t").text("x", "ab").text("y", "c").finish();
        let b = Canonical::new("t").text("x", "a").text("y", "bc").finish();
        assert_ne!(a, b);
        let c = Canonical::new("t").text("xy", "").finish();
        let d = Canonical::new("t").text("x", "y").finish();
        assert_ne!(c, d);
    }

    #[test]
    fn 用途が違えば同じ欄でも別の指紋になる() {
        let a = Canonical::new("a").text("x", "v").finish();
        let b = Canonical::new("b").text("x", "v").finish();
        assert_ne!(a, b);
    }

    /// 無いことも値。 無い欄と空の欄を取り違えない。
    #[test]
    fn 無い指紋と空の文字列は別の値() {
        let absent = Canonical::new("t").optional_digest("r", None).finish();
        let empty = Canonical::new("t").text("r", "").finish();
        assert_ne!(absent, empty);
        let present = Canonical::new("t")
            .optional_digest("r", Some(&Digest::of(b"")))
            .finish();
        assert_ne!(absent, present);
    }

    #[test]
    fn 入力の指紋は同じ入力から同じ値になる() {
        assert_eq!(input().digest(), input().digest());
    }

    /// 並べ方そのものを固定する。 ここが動くと、台帳や受領証へ置いた指紋が全部別の値になる
    /// ——欄の意味を変えるときは用途の名前の版（`INPUT_DOMAIN`）を上げて、この値も更新する。
    #[test]
    fn 指紋の並べ方は版を上げない限り変わらない() {
        assert_eq!(
            Canonical::new("t").text("a", "b").finish().to_hex(),
            "f327f43011a1194499e1da9ab97eb6ff9f6517164361d6b104649df5cf6b2035"
        );
        assert_eq!(
            input().digest().to_hex(),
            "8a19eac3a85213f90eb21ab0db3feacdc7f6dc27ced966df4d69005047302618"
        );
        let otos = [("か".to_owned(), oto(10.0))];
        let bounds = [("か".to_owned(), boundary(10.0))];
        assert_eq!(
            output(&otos, &bounds, b"frq").to_hex(),
            "276bc58e858edead8ee4dc4a8d20c69b4ebc56a2ceab6281ce023cb2666ff250"
        );
    }

    /// どの成分が変わっても別の指紋になる（`TR-ALN-29`）。
    #[test]
    fn 入力のどの成分が変わっても指紋が変わる() {
        let base = input().digest();
        let changed: [(&str, InputFingerprint); 7] = [
            (
                "マスターの中身",
                InputFingerprint {
                    master: Digest::of(b"other"),
                    ..input()
                },
            ),
            (
                "読み",
                InputFingerprint {
                    reading: "き".to_owned(),
                    ..input()
                },
            ),
            (
                "プリセット",
                InputFingerprint {
                    preset: "single@2".to_owned(),
                    ..input()
                },
            ),
            (
                "モデルの版",
                InputFingerprint {
                    engine: "mfa-japanese@3.4.0".to_owned(),
                    ..input()
                },
            ),
            (
                "パラメータの値",
                InputFingerprint {
                    parameters: BTreeMap::from([("first_pass".to_owned(), "final.mdl".to_owned())]),
                    ..input()
                },
            ),
            (
                "パラメータの有無",
                InputFingerprint {
                    parameters: BTreeMap::new(),
                    ..input()
                },
            ),
            (
                "綴りの表",
                InputFingerprint {
                    rules: Some(Digest::of(b"rules 2")),
                    ..input()
                },
            ),
        ];
        for (what, fp) in changed {
            assert_ne!(fp.digest(), base, "{what} を変えても指紋が動かない");
        }
        let without = InputFingerprint {
            rules: None,
            ..input()
        };
        assert_ne!(without.digest(), base, "綴りの表の有無");
    }

    /// 並べた順は効かない（`BTreeMap` が名前順に揃える）。
    #[test]
    fn パラメータの並べ順は指紋に効かない() {
        let a = InputFingerprint {
            parameters: BTreeMap::from([
                ("a".to_owned(), "1".to_owned()),
                ("b".to_owned(), "2".to_owned()),
            ]),
            ..input()
        };
        let b = InputFingerprint {
            parameters: BTreeMap::from([
                ("b".to_owned(), "2".to_owned()),
                ("a".to_owned(), "1".to_owned()),
            ]),
            ..input()
        };
        assert_eq!(a.digest(), b.digest());
    }

    fn output<'a>(
        otos: &'a [(String, Oto)],
        boundaries: &'a [(String, Boundary)],
        frq: &'a [u8],
    ) -> Digest {
        OutputFingerprint {
            otos,
            boundaries,
            frq,
        }
        .digest()
    }

    #[test]
    fn 出力の指紋は出した順に依らない() {
        let a = [("か".to_owned(), oto(10.0)), ("き".to_owned(), oto(20.0))];
        let b = [("き".to_owned(), oto(20.0)), ("か".to_owned(), oto(10.0))];
        let bounds = [("か".to_owned(), boundary(10.0))];
        assert_eq!(output(&a, &bounds, b"frq"), output(&b, &bounds, b"frq"));
    }

    /// 出力の3つの成分のどれが変わっても別の指紋になる。
    #[test]
    fn 出力のどの成分が変わっても指紋が変わる() {
        let otos = [("か".to_owned(), oto(10.0))];
        let bounds = [("か".to_owned(), boundary(10.0))];
        let base = output(&otos, &bounds, b"frq");
        assert_ne!(
            output(&[("か".to_owned(), oto(10.5))], &bounds, b"frq"),
            base,
            "oto の5値"
        );
        assert_ne!(
            output(&otos, &[("か".to_owned(), boundary(11.0))], b"frq"),
            base,
            "境界"
        );
        assert_ne!(output(&otos, &bounds, b"frq2"), base, ".frq");
        assert_ne!(
            output(&[("が".to_owned(), oto(10.0))], &bounds, b"frq"),
            base,
            "綴り"
        );
    }

    /// 1ビットの違いも拾う。 丸めた値で比べていない。
    #[test]
    fn 浮動小数の1ビットの違いを拾う() {
        let next = f64::from_bits(10.0_f64.to_bits() + 1);
        let a = [("か".to_owned(), oto(10.0))];
        let b = [("か".to_owned(), oto(next))];
        assert_ne!(output(&a, &[], b""), output(&b, &[], b""));
    }
}
