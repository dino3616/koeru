//! phonemizer の綴りを固定する（`TR-SYN-12`, `DEC-SYN-010` の層A）。
//!
//! # 何を固定するのか
//!
//! KOERU が書き出す `presamp.ini` と、その表から KOERU 自身が組み立てる綴り。
//! **両方を1つのディレクトリへ書き出す**——`fixtures/phonemizer-parity/`。
//!
//! OpenUtau 側の半分は `.github/scripts/openutau-parity.ps1`。 あちらが同じ音源を
//! OpenUtau へ食わせて綴りを出し、CI が突き合わせる
//! （`.github/workflows/ci.yml` の `phonemizer parity`）。**2つで1つの検査。**
//!
//! 向こうは `.github/` に置く。 走らせるのが CI だけだから——
//! `cargo test` は触らないので、`tests/` に置くとテスト対象に見える。
//!
//! # なぜ固定するのか
//!
//! 綴りは3箇所を通る——カバレッジ判定、書き出し、試唱（`TR-SYN-36`）。
//! 定義が1つでも、出る綴りが変わったことに気づける形が別に要る。
//! **ここが変わる差分は、配布した音源の互換性が変わる差分。**

use std::path::PathBuf;

use koeru_core::alias::Method;
use koeru_core::inventory::UnitSet;
use koeru_core::presamp;

/// 固定する音符の並び（`TR-SYN-12`）。
///
/// 網羅ではない。 層A の全数検査は `presamp` の単体試験が持つ。
/// ここは**層B が実際に OpenUtau へ食わせる入力**なので、音素クラスを
/// 1つずつ踏む短い列に絞る——165 MB を引いて回す検査を長くしない。
///
/// `(直前の歌詞, 歌詞)`。 直前が `None` なら語頭。
const CASES: [(Option<&str>, &str); 14] = [
    (None, "あ"),       // 母音始まり
    (Some("あ"), "か"), // 無声破裂音
    (Some("か"), "が"), // 有声破裂音
    (Some("が"), "さ"), // 摩擦音
    (Some("さ"), "し"), // 口蓋化した摩擦音
    (Some("し"), "た"), // 破裂音
    (Some("た"), "ち"), // 破擦音
    (Some("ち"), "な"), // 鼻音
    (Some("な"), "は"), // 声門摩擦音
    (Some("は"), "ま"), // 両唇鼻音
    (Some("ま"), "や"), // 半母音
    (Some("や"), "ら"), // 流音
    (Some("ら"), "わ"), // 両唇半母音
    (Some("わ"), "ん"), // 撥音
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/phonemizer-parity")
}

/// 期待値の本文を組み立てる。
///
/// タブ区切り。 方式・直前の歌詞・歌詞・綴り。 空欄は `-`——
/// 空文字のままだと、列がずれているのか空なのかが目で読めない。
fn expected() -> String {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    let mut out = String::from("# 方式\t直前\t歌詞\t綴り\n");
    for method in [Method::Single, Method::Sequential, Method::Cvvc] {
        let name = match method {
            Method::Single => "single",
            Method::Sequential => "sequential",
            Method::Cvvc => "cvvc",
        };
        for (prev, kana) in CASES {
            // 直前の母音は、直前の歌詞から引く。 語頭は `None`。
            let previous_vowel = prev.map(|p| rules.vowel_of(p).to_owned());
            let alias = rules
                .candidates(method, kana, previous_vowel.as_deref())
                .first()
                .cloned()
                .unwrap_or_default();
            out.push_str(&format!(
                "{name}\t{}\t{kana}\t{alias}\n",
                prev.unwrap_or("-")
            ));
        }
    }
    out
}

/// 書き出したものと突き合わせる。 `KOERU_WRITE_PARITY=1` で作り直す。
fn fixed(name: &str, body: &str) {
    let path = fixture_dir().join(name);
    if std::env::var_os("KOERU_WRITE_PARITY").is_some() {
        std::fs::create_dir_all(fixture_dir()).expect("作れる");
        std::fs::write(&path, body).expect("書ける");
        return;
    }
    let have = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("{name} を読めない: {e}。`KOERU_WRITE_PARITY=1` で作り直す"));
    assert_eq!(
        have, body,
        "{name} が実装とずれている。\
         \n配布した音源の互換性が変わる差分。意図した変更なら \
         `KOERU_WRITE_PARITY=1 cargo test -p koeru-core --test phonemizer_parity` で作り直す"
    );
}

/// 突合に使う音源一式を書き出す（`DEC-SYN-010` の層B）。
///
/// **OpenUtau は「その綴りの oto があるか」で候補を選ぶ。** 候補の一部しか
/// 置いていない音源を渡すと、あちらは最後の候補まで落ちて、こちらと違う綴りを出す。
/// **食い違いの原因が音源の不足なのか実装の差なのか、分からなくなる。**
/// だから満たされた音源を渡す——全単位・全直前母音の候補と、VC と語尾。
fn oto_ini() -> String {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    let units = koeru_core::inventory::units(UnitSet::Core);
    let vowels = koeru_core::inventory::transition_vowels(UnitSet::Core);

    let mut aliases: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for u in &units {
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            aliases.extend(rules.candidates(method, u.kana, None));
            for v in &vowels {
                aliases.extend(rules.candidates(method, u.kana, Some(v)));
            }
        }
    }
    for c in koeru_core::inventory::consonants(UnitSet::Core) {
        for v in &vowels {
            aliases.insert(rules.vc(v, c));
        }
    }
    for v in &vowels {
        aliases.insert(rules.ending(v));
    }

    // 5値は突合に効かない。 見るのは綴りだけなので、同じ WAV を全部が指す。
    let mut out = String::new();
    for a in aliases {
        out.push_str(&format!("_parity.wav={a},0,0,0,0,0\n"));
    }
    out
}

/// 突合に使う音源一式（`DEC-SYN-010` の層B）。
///
/// CI がこのディレクトリをそのまま OpenUtau へ渡す。 `character.txt` は
/// `VoicebankLoader` が音源として認めるために要る最小限。
#[test]
fn 突合用の音源を固定する() {
    fixed("character.txt", "name=KOERU parity\n");
    fixed("oto.ini", &oto_ini());
}

/// 書き出す `presamp.ini` を固定する（`TR-RCL-24`, `DEC-SYN-010`）。
///
/// **層B が読むのはこのファイル。** OpenUtau に同じ表を渡さなければ、
/// 綴りが一致しても一致の意味が無い。
#[test]
fn presamp_ini_を固定する() {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    // 改行は LF で固定する。 CRLF にすると OS で指紋が変わる。
    fixed("presamp.ini", &presamp::write(&rules, "\n"));
}

/// KOERU が組み立てる綴りを固定する（`TR-SYN-12`）。
#[test]
fn 綴りを固定する() {
    fixed("expected.tsv", &expected());
}

/// 書き出した `presamp.ini` を読み戻すと、同じ綴りが出る（`DEC-SYN-010` の層A）。
///
/// **往復を見る。** 表を書けても、その表から同じ綴りが出なければ、
/// 受け取った側（OpenUtau）は別の綴りを作る。
#[test]
fn 書き出した表から同じ綴りが出る() {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    let round = presamp::parse(&presamp::write(&rules, "\n"));
    for method in [Method::Single, Method::Sequential, Method::Cvvc] {
        for (prev, kana) in CASES {
            let pv = prev.map(|p| rules.vowel_of(p).to_owned());
            assert_eq!(
                rules.candidates(method, kana, pv.as_deref()),
                round.candidates(method, kana, pv.as_deref()),
                "{kana}（直前 {prev:?}）"
            );
        }
    }
}
