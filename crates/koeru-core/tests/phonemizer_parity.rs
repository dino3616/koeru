//! phonemizer の綴りを固定する（`TR-SYN-12`, `DEC-SYN-010` の層A）。
//!
//! # 何を固定するのか
//!
//! KOERU が書き出す `presamp.ini` と、その表から KOERU 自身が組み立てる綴り。
//! **方式ごとに1つの音源へ書き出す**——`fixtures/phonemizer-parity/<方式>/`。
//!
//! OpenUtau 側の半分は `.github/scripts/openutau-parity.ps1`。 あちらが同じ音源を
//! OpenUtau へ食わせて綴りを出し、CI が突き合わせる
//! （`.github/workflows/ci.yml` の `phonemizer parity`）。**2つで1つの検査。**
//!
//! 向こうは `.github/` に置く。 走らせるのが CI だけだから——
//! `cargo test` は触らないので、`tests/` に置くとテスト対象に見える。
//!
//! # なぜ方式ごとに音源を分けるのか
//!
//! **phonemizer に方式を渡す口が無い。** presamp phonemizer は1つで
//! 単独音・連続音・CVVC を賄い、**どの綴りを出すかは音源の中身が決める**
//! ——「その綴りの oto があるか」で候補を落としていく。だから、
//! 音符1つに返る綴りも1つしかない。
//!
//! **1つの音源に3方式ぶんの綴りを入れて、3方式ぶんの答えを期待していた。**
//! 同じ音符を3度食わせて3つの違う答えを待つ形で、**原理的に埋まらない。**
//! 方式ごとに音源を分けて、その方式の oto だけを置けば、あちらの候補落としが
//! その方式の綴りへ収束する（`DEC-SYN-010`）。
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

/// 突き合わせる方式と、その音源のディレクトリ名。
const METHODS: [(Method, &str); 3] = [
    (Method::Single, "single"),
    (Method::Sequential, "sequential"),
    (Method::Cvvc, "cvvc"),
];

fn fixture_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/phonemizer-parity")
}

/// その方式の期待値（`TR-SYN-12`）。
///
/// タブ区切り。 直前の歌詞・歌詞・綴り。 空欄は `-`——
/// 空文字のままだと、列がずれているのか空なのかが目で読めない。
///
/// 方式の欄を持たない。 音源1つが1方式なので、ディレクトリ名が方式。
fn expected(rules: &presamp::Rules, method: Method) -> String {
    let mut out = String::from("# 直前\t歌詞\t綴り\n");
    for (prev, kana) in CASES {
        // 直前の母音は、直前の歌詞から引く。 語頭は `None`。
        let previous_vowel = prev.map(|p| rules.vowel_of(p).to_owned());
        let alias = rules
            .candidates(method, kana, previous_vowel.as_deref())
            .first()
            .cloned()
            .unwrap_or_default();
        out.push_str(&format!("{}\t{kana}\t{alias}\n", prev.unwrap_or("-")));
    }
    out
}

/// 書き出したものと突き合わせる。 `KOERU_WRITE_PARITY=1` で作り直す。
fn fixed(rel: &str, body: &str) {
    fixed_bytes(rel, body.as_bytes());
}

/// [`fixed`] のバイト列版。WAV のように文字列でないものを置く。
fn fixed_bytes(rel: &str, body: &[u8]) {
    let path = fixture_dir().join(rel);
    if std::env::var_os("KOERU_WRITE_PARITY").is_some() {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).expect("作れる");
        }
        std::fs::write(&path, body).expect("書ける");
        return;
    }
    let have = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{rel} を読めない: {e}。`KOERU_WRITE_PARITY=1` で作り直す"));
    assert!(
        have == body,
        "{rel} が実装とずれている。\
         \n配布した音源の互換性が変わる差分。意図した変更なら \
         `KOERU_WRITE_PARITY=1 cargo test -p koeru-core --test phonemizer_parity` で作り直す"
    );
}

/// oto が指す WAV（`DEC-SYN-010` の層B）。
///
/// **無いと oto が1件も残らない。** OpenUtau は音源を読むときに各 oto の
/// WAV の在処を確かめ、無いものを「Sound file missing」として落とす
/// ——102 件書いても全部落ち、phonemizer は候補を1つも見つけられない。
/// **踏んだ。**
///
/// 中身は見られない。 綴りを決めるのに波形は要らないので、
/// 44100 Hz / 16 bit / 1ch（`TR-PKG-20`）の最小の1本を置く。
fn parity_wav() -> Vec<u8> {
    // 1標本だけ。無音。
    const DATA: [u8; 2] = [0, 0];
    let rate: u32 = 44_100;
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + DATA.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16_u32.to_le_bytes()); // fmt チャンクの長さ
    out.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1_u16.to_le_bytes()); // 1ch
    out.extend_from_slice(&rate.to_le_bytes());
    out.extend_from_slice(&(rate * 2).to_le_bytes()); // バイト毎秒
    out.extend_from_slice(&2_u16.to_le_bytes()); // ブロック境界
    out.extend_from_slice(&16_u16.to_le_bytes()); // 量子化ビット数
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(DATA.len() as u32).to_le_bytes());
    out.extend_from_slice(&DATA);
    out
}

/// その方式の音源が持つ `oto.ini`（`DEC-SYN-010` の層B）。
///
/// **その方式が出しうる候補を全部置く。** OpenUtau は「その綴りの oto があるか」で
/// 候補を落としていくので、先頭の候補だけを置くと、あちらは選ぶ余地を持たない
/// ——一致しても、候補落としの順が同じだと確かめたことにならない。
///
/// **他の方式の綴りは置かない。** 置くと候補落としが別の方式へ逸れる。
/// 連続音の音源に素の `か` があっても `a か` が先に当たるので害は無いが、
/// 単独音の音源に `a か` があると、あちらは `a か` を返す
/// ——**方式ごとに分けた意味が消える。**
fn oto_ini(rules: &presamp::Rules, method: Method) -> String {
    let units = koeru_core::inventory::units(UnitSet::Core);
    let vowels = koeru_core::inventory::transition_vowels(UnitSet::Core);

    let mut aliases: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for u in &units {
        aliases.extend(rules.candidates(method, u.kana, None));
        for v in &vowels {
            aliases.extend(rules.candidates(method, u.kana, Some(v)));
        }
    }
    // 渡りと語尾は CVVC だけが持つ（`TR-RCL-05`）。
    if method == Method::Cvvc {
        for c in koeru_core::inventory::consonants(UnitSet::Core) {
            for v in &vowels {
                aliases.insert(rules.vc(v, c));
            }
        }
        for v in &vowels {
            aliases.insert(rules.ending(v));
        }
    }

    // 5値は突合に効かない。 見るのは綴りだけなので、同じ WAV を全部が指す。
    let mut out = String::new();
    for a in aliases {
        out.push_str(&format!("_parity.wav={a},0,0,0,0,0\n"));
    }
    out
}

/// 方式ごとに音源一式を固定する（`DEC-SYN-010` の層B）。
///
/// CI がこのディレクトリをそのまま OpenUtau へ渡す。 `character.txt` は
/// `VoicebankLoader` が音源として認めるために要る最小限。
///
/// `presamp.ini` は3つとも同じ中身。 表は方式で変わらない（`TR-RCL-24`）が、
/// **音源はそれぞれ自分の表を持っていなければならない**——OpenUtau が読むのは
/// 開いた音源の中の1枚で、隣の音源を見には行かない。
///
/// `character.yaml` は文字コードを名乗るために要る（`TR-PKG-13`）。
/// **無いと classic の既定（Shift-JIS）で読まれる。** ここの綴りは UTF-8 で
/// 置いてあるので、名乗らないと全部の綴りが化けて1つも当たらない。
#[test]
fn 方式ごとの突合用音源を固定する() {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    for (method, dir) in METHODS {
        fixed(
            &format!("{dir}/character.txt"),
            &format!("name=KOERU parity {dir}\n"),
        );
        fixed(
            &format!("{dir}/character.yaml"),
            &format!("name: KOERU parity {dir}\ntext_file_encoding: utf-8\n"),
        );
        // 改行は LF で固定する。 CRLF にすると OS で指紋が変わる。
        fixed(&format!("{dir}/presamp.ini"), &presamp::write(&rules, "\n"));
        fixed(&format!("{dir}/oto.ini"), &oto_ini(&rules, method));
        fixed(&format!("{dir}/expected.tsv"), &expected(&rules, method));
        fixed_bytes(&format!("{dir}/_parity.wav"), &parity_wav());
    }
}

/// 方式ごとの音源が、綴りを混ぜていない（`DEC-SYN-010`）。
///
/// **混ぜていた。** 1つの音源に3方式ぶんの綴りを入れて、同じ音符から
/// 3つの違う答えを待っていた。ここが崩れると、層B は「何を確かめているのか」を
/// 失ったまま緑になる。
#[test]
fn 方式ごとの音源は他の方式の綴りを持たない() {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    let mine = |m: Method| -> std::collections::BTreeSet<String> {
        oto_ini(&rules, m)
            .lines()
            .filter_map(|l| l.split_once('=')?.1.split(',').next().map(str::to_owned))
            .collect()
    };
    let single = mine(Method::Single);
    let sequential = mine(Method::Sequential);

    // 単独音の音源に連続音の綴りがあると、あちらは `a か` を返す。
    assert!(
        !single.contains("a か"),
        "単独音の音源が連続音の綴りを持っている"
    );
    assert!(single.contains("か"), "単独音の音源が素の仮名を持つ");
    // 連続音の音源は、語頭と渡り先の両方を持つ。
    assert!(sequential.contains("- か"));
    assert!(sequential.contains("a か"));
    // 渡りと語尾は CVVC だけ（`TR-RCL-05`）。
    assert!(
        !sequential.iter().any(|a| a == "a k"),
        "連続音に VC は要らない"
    );
    assert!(mine(Method::Cvvc).contains("a k"), "CVVC は VC を持つ");
}

/// 書き出した表から同じ綴りが出る（`DEC-SYN-010` の層A）。
///
/// **往復を見る。** 表を書けても、その表から同じ綴りが出なければ、
/// 受け取った側（OpenUtau）は別の綴りを作る。
#[test]
fn 書き出した表から同じ綴りが出る() {
    let rules = presamp::Rules::builtin(UnitSet::Core);
    let round = presamp::parse(&presamp::write(&rules, "\n"));
    for (method, _) in METHODS {
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
