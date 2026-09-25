//! 取り込んだ `oto.ini` を1バイトも変えずに書き戻す（`TR-EDT-39`、`TR-EDT-38`、`TR-EDT-41`）。
//!
//! 原本はバイト列で書く。 ファイルに置くと `.gitattributes` の `eol=lf` が CRLF を
//! LF に揃えてしまい、CRLF の原本を試せなくなる。
//!
//! CP932 の字は符号で書く。 あ `82 A0`、い `82 A2`、表 `95 5C`、ソ `83 5C`。

use koeru_failure::Failure as _;
use koeru_formats::oto_ini::{self, Document, Field, IniEntry, IniError, LineEnding, LineKind};
use koeru_formats::text::{self, TextEncoding};
use koeru_model::oto::Oto;

use TextEncoding::{Cp932, Utf8};

/// 取り込みで出会う形。 名前・原文・符号化。
const FIXTURES: &[(&str, &[u8], TextEncoding)] = &[
    (
        "CP932・CRLF・コメントと空行",
        b"#comment \x82\xa0\r\n\r\na001.wav=\x82\xa0,80,100,-520,70,23.333\r\n",
        Cp932,
    ),
    (
        "LF だけ",
        b"a.wav=\x82\xa0,1,2,3,4,5\nb.wav=\x82\xa2,1,2,3,4,5\n",
        Cp932,
    ),
    (
        "改行の混在（CRLF・LF・CR）",
        b"a.wav=\x82\xa0,1,2,3,4,5\r\nb.wav=\x82\xa2,1,2,3,4,5\nc.wav=x,1,2,3,4,5\rd.wav=y,1,2,3,4,5\r\n",
        Cp932,
    ),
    (
        "最後の行に改行が無い",
        b"a.wav=\x82\xa0,1,2,3,4,5\r\nb.wav=\x82\xa2,1,2,3,4,5",
        Cp932,
    ),
    (
        "解釈できない行",
        b"[a.wav]\r\n\x82\xa0\x82\xa2\r\na.wav=\x82\xa0,1,2,3\r\na.wav=\x82\xa0,1,2,x,4,5\r\na.wav=\x82\xa0,1,2,3,4,5,6\r\n=\x82\xa0,1,2,3,4,5\r\na.wav=\x82\xa0,nan,0,0,0,0\r\n",
        Cp932,
    ),
    (
        "空欄",
        b"a.wav=,,,,,\r\nb.wav=\x82\xa0,80,,,,\r\nc.wav=\x82\xa2, , ,0,,\r\n",
        Cp932,
    ),
    (
        "負の右ブランクとオーバーラップ",
        b"a.wav=\x82\xa0,80,100,-520.123456,70,-12.5\r\nb.wav=\x82\xa2,0,0,-0,0,-0.000\r\n",
        Cp932,
    ),
    (
        "丸めない綴り",
        b"a.wav=\x82\xa0,80,80.0,80.00000001,+5,1e2\r\nb.wav=\x82\xa2,.5,5.,  7 ,0080,-0  \r\n",
        Cp932,
    ),
    (
        "2バイト目が `\\` の字（ソ.wav=表）",
        b"\x83\x5c.wav=\x95\x5c,1,2,3,4,5\r\n",
        Cp932,
    ),
    (
        "UTF-8 と #Charset:UTF-8",
        b"#Charset:UTF-8\na.wav=\xe3\x81\x82,80,100,-520,70,23.333\nb.wav=\xf0\x9f\x8e\xa4,1,2,3,4,5\n",
        Utf8,
    ),
    (
        "UTF-8 の BOM",
        b"\xef\xbb\xbf#Charset:UTF-8\r\na.wav=\xe3\x81\x82,1,2,3,4,5\r\n",
        Utf8,
    ),
    ("空のファイル", b"", Cp932),
    ("改行だけ", b"\r\n\n\r", Cp932),
    ("空白だけの行", b"  \t\r\na.wav=x,1,2,3,4,5\r\n", Cp932),
];

/// CP932 で同じ字に2つの符号があるもの。 NEC 特殊文字の `≒`（`87 90`）と、
/// NEC 選定 IBM 拡張の `ⅰ`（`EE EF`）。
const DUPLICATE_CODES: &[u8] = b"a.wav=\x87\x90\xee\xef,1,2,3,4,5\r\n";

fn parse(bytes: &[u8], enc: TextEncoding) -> Document {
    Document::parse(bytes, enc).expect("読めること")
}

/// 行の終わりを含めて切る。 文書の切り方とは別に書き、それと突き合わせる。
fn lines_of(bytes: &[u8]) -> Vec<&[u8]> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let end = match bytes[i] {
            b'\r' if bytes.get(i + 1) == Some(&b'\n') => Some(i + 2),
            b'\r' | b'\n' => Some(i + 1),
            _ => None,
        };
        if let Some(end) = end {
            out.push(&bytes[start..end]);
            start = end;
            i = end;
        } else {
            i += 1;
        }
    }
    if start < bytes.len() {
        out.push(&bytes[start..]);
    }
    out
}

/// 取り込んですぐ書き出したら1バイトも変わらない（`TR-EDT-39`）。
#[test]
fn 取り込んですぐ書き出すと同じバイト列になる() {
    for (name, bytes, enc) in FIXTURES {
        let doc = parse(bytes, *enc);
        assert_eq!(
            doc.to_bytes().expect("書けること"),
            *bytes,
            "{name}: 取り込んで書き戻すとバイトが変わった"
        );
    }
}

/// 文字列から作り直さない。 字は同じでも、CP932 の符号が別のものに入れ替わる。
#[test]
fn 同じ字の別の符号を入れ替えない() {
    let reencoded = text::encode(
        &text::decode(DUPLICATE_CODES, Cp932).expect("読める"),
        Cp932,
    )
    .expect("書ける");
    assert_ne!(
        reencoded, DUPLICATE_CODES,
        "前提: 読んで書き直すと符号が変わる（変わらないならこの試験は何も見ていない）"
    );

    let mut doc = parse(DUPLICATE_CODES, Cp932);
    assert_eq!(doc.to_bytes().expect("書ける"), DUPLICATE_CODES);

    // ほかの行を編集しても、この行は原文のまま。
    let mut two = DUPLICATE_CODES.to_vec();
    two.extend_from_slice(b"b.wav=x,1,2,3,4,5\r\n");
    let mut doc2 = parse(&two, Cp932);
    doc2.entry_mut(1)
        .expect("エントリ")
        .set(Field::Offset, Some(9.0))
        .expect("変えられる");
    let out = doc2.to_bytes().expect("書ける");
    assert!(out.starts_with(DUPLICATE_CODES), "{out:?}");

    // 編集した行そのものは作り直すので、この字は正規の符号になる。 それはその行だけ。
    doc.entry_mut(0)
        .expect("エントリ")
        .set(Field::Offset, Some(9.0))
        .expect("変えられる");
    assert_eq!(
        doc.to_bytes().expect("書ける"),
        b"a.wav=\x81\xe0\xfa\x40,9.000,2,3,4,5\r\n"
    );
}

/// 何度書き出しても同じバイト列（`TR-EDT-41`）。 読み戻しても同じ文書になる。
#[test]
fn 書き出しを繰り返しても変わらない() {
    for (name, bytes, enc) in FIXTURES {
        let mut doc = parse(bytes, *enc);
        let first = doc.entries().next().map(|(i, _)| i);
        if let Some(i) = first {
            doc.entry_mut(i)
                .expect("エントリ")
                .set(Field::Cutoff, Some(-123.456_789))
                .expect("変えられる");
        }
        let once = doc.to_bytes().expect("書ける");
        assert_eq!(once, doc.to_bytes().expect("書ける"), "{name}");
        let again = parse(&once, *enc);
        assert_eq!(again.to_bytes().expect("書ける"), once, "{name}");
    }
}

/// 1件を編集したら、変わるのはその1行だけ。 行の終わりも保つ。
#[test]
fn 一件を編集すると一行だけが変わる() {
    for (name, bytes, enc) in FIXTURES {
        let doc = parse(bytes, *enc);
        let targets: Vec<usize> = doc.entries().map(|(i, _)| i).collect();
        for at in targets {
            let mut edited = doc.clone();
            edited
                .entry_mut(at)
                .expect("エントリ")
                .set(Field::Offset, Some(12.5))
                .expect("変えられる");
            let out = edited.to_bytes().expect("書ける");
            let (before, after) = (lines_of(bytes), lines_of(&out));
            assert_eq!(before.len(), after.len(), "{name}: 行の数が変わった");
            let changed: Vec<usize> = (0..before.len())
                .filter(|i| before[*i] != after[*i])
                .collect();
            assert_eq!(changed, [at], "{name}: 変わった行");
            let ending = |l: &[u8]| {
                l.iter()
                    .rev()
                    .take_while(|b| **b == b'\r' || **b == b'\n')
                    .count()
            };
            assert_eq!(
                ending(before[at]),
                ending(after[at]),
                "{name}: 行の終わりが変わった"
            );
        }
    }
}

/// 編集した行でも、触っていない欄とエイリアスとファイル名は原文の綴りのまま。
#[test]
fn 編集した行も触っていない欄は原文のまま() {
    let src = b"\x83\x5c.wav=\x95\x5c,80,,-520,70,23.3333333\r\n";
    let mut doc = parse(src, Cp932);
    doc.entry_mut(0)
        .expect("エントリ")
        .set(Field::Preutterance, Some(71.25))
        .expect("変えられる");
    assert_eq!(
        doc.to_bytes().expect("書ける"),
        b"\x83\x5c.wav=\x95\x5c,80,,-520,71.250,23.3333333\r\n"
    );
}

/// 空欄と `0` の欄を区別して保つ（`TR-EDT-39`）。
#[test]
fn 空欄と_0_の欄を区別する() {
    let doc = parse(b"a.wav=x,0,,0.000, ,-0\r\n", Cp932);
    let (_, e) = doc.entries().next().expect("エントリ");
    assert_eq!(e.get(Field::Offset), Some(0.0));
    assert_eq!(e.get(Field::Consonant), None);
    assert_eq!(e.get(Field::Cutoff), Some(0.0));
    assert_eq!(e.get(Field::Preutterance), None);
    assert_eq!(e.raw(Field::Overlap), "-0");

    // 空欄を空欄のまま、ほかの欄だけを変える。
    let mut doc = doc;
    doc.entry_mut(0)
        .expect("エントリ")
        .set(Field::Overlap, Some(5.0))
        .expect("変えられる");
    assert_eq!(
        doc.to_bytes().expect("書ける"),
        b"a.wav=x,0,,0.000, ,5.000\r\n"
    );

    // 空欄へ戻すこともできる。 0 で埋めない。
    doc.entry_mut(0)
        .expect("エントリ")
        .set(Field::Cutoff, None)
        .expect("変えられる");
    assert_eq!(doc.to_bytes().expect("書ける"), b"a.wav=x,0,,, ,5.000\r\n");
}

/// 解釈できない行は同じ位置に原文のまま残る（`TR-EDT-39`）。
#[test]
fn 解釈できない行を同じ位置に残す() {
    let src = b"a.wav=x,1,2,3,4,5\r\n[setting]\r\nb.wav=y,1,2,3\r\nc.wav=z,1,2,3,4,5\r\n";
    let mut doc = parse(src, Cp932);
    let kinds: Vec<&str> = doc
        .lines()
        .iter()
        .map(|l| match l.kind() {
            LineKind::Blank => "空行",
            LineKind::Comment(_) => "コメント",
            LineKind::Entry(_) => "エントリ",
            LineKind::Uninterpretable(_) => "解釈できない",
        })
        .collect();
    assert_eq!(
        kinds,
        ["エントリ", "解釈できない", "解釈できない", "エントリ"]
    );

    doc.entry_mut(0)
        .expect("エントリ")
        .set_alias("w")
        .expect("変えられる");
    doc.entry_mut(3)
        .expect("エントリ")
        .set(Field::Consonant, Some(20.0))
        .expect("変えられる");
    assert!(doc.entry_mut(1).is_none(), "解釈できない行は編集させない");
    assert_eq!(
        doc.to_bytes().expect("書ける"),
        b"a.wav=w,1,2,3,4,5\r\n[setting]\r\nb.wav=y,1,2,3\r\nc.wav=z,1,20.000,3,4,5\r\n"
    );
}

/// 改行の形と、最後の行に改行が無いことを保つ。
#[test]
fn 行の終わりを行ごとに保つ() {
    let src = b"a.wav=x,1,2,3,4,5\r\nb.wav=y,1,2,3,4,5\nc.wav=z,1,2,3,4,5\rd.wav=w,1,2,3,4,5";
    let doc = parse(src, Cp932);
    let endings: Vec<LineEnding> = doc.lines().iter().map(|l| l.ending()).collect();
    assert_eq!(
        endings,
        [
            LineEnding::CrLf,
            LineEnding::Lf,
            LineEnding::Cr,
            LineEnding::Missing
        ]
    );
    let mut doc = doc;
    doc.entry_mut(3)
        .expect("エントリ")
        .set(Field::Offset, Some(2.0))
        .expect("変えられる");
    assert!(
        doc.to_bytes()
            .expect("書ける")
            .ends_with(b"d.wav=w,2.000,2,3,4,5"),
        "最後の行に改行を足さない"
    );
}

/// `#Charset:` の宣言の行を読み、書き戻す（`TR-EDT-38`）。 BOM も落とさない。
#[test]
fn charset_の宣言と_bom_を保つ() {
    let src = b"\xef\xbb\xbf#Charset:UTF-8\r\na.wav=\xe3\x81\x82,1,2,3,4,5\r\n";
    let mut doc = parse(src, Utf8);
    assert_eq!(doc.charset_declaration(), Some("UTF-8"));
    assert!(doc.has_bom());
    assert_eq!(doc.encoding(), Utf8);
    let (_, e) = doc.entries().next().expect("エントリ");
    assert_eq!(e.file(), "a.wav", "BOM をファイル名に混ぜない");
    assert_eq!(e.alias(), "あ");

    doc.entry_mut(1)
        .expect("エントリ")
        .set_alias("🎤")
        .expect("変えられる");
    assert_eq!(
        doc.to_bytes().expect("書ける"),
        b"\xef\xbb\xbf#Charset:UTF-8\r\na.wav=\xf0\x9f\x8e\xa4,1,2,3,4,5\r\n"
    );

    // 宣言が1行目に無ければ宣言とみなさない。
    let late = parse(b"a.wav=x,1,2,3,4,5\r\n#Charset:UTF-8\r\n", Cp932);
    assert_eq!(late.charset_declaration(), None);
}

/// 指定した符号化で読めない行があれば、黙って読み進めない（`TR-EDT-38`）。
#[test]
fn 読めない符号化では取り込まない() {
    let utf8 = b"a.wav=\xe3\x81\x82,1,2,3,4,5\r\n";
    let e = Document::parse(utf8, Cp932).expect_err("拒むこと");
    assert_eq!(e.code(), "text.undecodable");

    let cp932 = b"a.wav=\x82\xa0,1,2,3,4,5\r\n";
    assert!(Document::parse(cp932, Utf8).is_err());

    // 行の途中で切れた2バイト文字。 置換文字にして進まない。
    assert!(Document::parse(b"a.wav=\x82\r\n", Cp932).is_err());
}

/// 編集した行が書き出しの符号化で書けなければ、書き出しで止める（`TR-PLT-08`）。
#[test]
fn 書けない字を入れた行は書き出しで止める() {
    let mut doc = parse(b"a.wav=\x82\xa0,1,2,3,4,5\r\n", Cp932);
    doc.entry_mut(0)
        .expect("エントリ")
        .set_alias("🎤")
        .expect("入れること自体はできる");
    let e = doc.to_bytes().expect_err("拒むこと");
    assert!(matches!(
        e,
        IniError::Text(text::TextError::Unencodable { .. })
    ));
}

/// 行の組み合わせを総当たりで作り、往復と編集の局所性を見る。
///
/// 性質の試験の道具は入れていない（`DEC-PLT-039`）。 小さな部品を全部組み合わせて代える。
#[test]
fn 行の組み合わせを総当たりで往復させる() {
    const BODIES: [&[u8]; 6] = [
        b"",
        b"  ",
        b"#c",
        b"a.wav=\x82\xa0,80,,-520,70,23.3333",
        b"[x]",
        b"b.wav=\x87\x90,1,2,3,4,5",
    ];
    const ENDINGS: [&[u8]; 3] = [b"\r\n", b"\n", b"\r"];
    const LAST: [&[u8]; 4] = [b"\r\n", b"\n", b"\r", b""];

    let mut docs = 0_usize;
    let mut edits = 0_usize;
    for a in BODIES {
        for b in BODIES {
            for c in BODIES {
                for ea in ENDINGS {
                    for eb in ENDINGS {
                        for ec in LAST {
                            let src = [a, ea, b, eb, c, ec].concat();
                            let doc = parse(&src, Cp932);
                            assert_eq!(doc.to_bytes().expect("書ける"), src, "{src:?}");
                            docs += 1;

                            for (at, _) in doc.entries() {
                                let mut edited = doc.clone();
                                edited
                                    .entry_mut(at)
                                    .expect("エントリ")
                                    .set(Field::Overlap, Some(1.0))
                                    .expect("変えられる");
                                let out = edited.to_bytes().expect("書ける");
                                let (before, after) = (lines_of(&src), lines_of(&out));
                                assert_eq!(before.len(), after.len(), "{src:?}");
                                let changed = (0..before.len())
                                    .filter(|i| before[*i] != after[*i])
                                    .count();
                                assert_eq!(changed, 1, "{src:?}");
                                edits += 1;
                            }
                        }
                    }
                }
            }
        }
    }
    assert_eq!(docs, 6 * 6 * 6 * 3 * 3 * 4);
    // 部品の3分の1がエントリで、1つの文書は3行。 編集を試した数は文書の数と同じになる。
    assert_eq!(edits, docs, "エントリを読み落としている");
}

/// KOERU の書き出しの口は変えていない（`TR-EDT-41`）。 同じエントリから同じバイト列。
#[test]
fn 書き出しの口のバイト列は変わらない() {
    let entries = [
        IniEntry {
            file: "a001.wav".to_owned(),
            alias: "あ".to_owned(),
            oto: Oto {
                offset_ms: 80.0,
                consonant_ms: 100.0,
                cutoff_ms: -520.0,
                preutterance_ms: 70.0,
                overlap_ms: 23.333_333,
            },
        },
        IniEntry {
            file: "a002.wav".to_owned(),
            alias: "い".to_owned(),
            oto: Oto {
                offset_ms: 1.234_567,
                consonant_ms: 0.0,
                cutoff_ms: -0.0,
                preutterance_ms: 0.000_6,
                overlap_ms: -12.5,
            },
        },
    ];
    let want: &[u8] = b"a001.wav=\x82\xa0,80.000,100.000,-520.000,70.000,23.333\r\n\
a002.wav=\x82\xa2,1.235,0.000,0.000,0.001,-12.500\r\n";
    let once = oto_ini::write(&entries, Cp932).expect("書ける");
    assert_eq!(once, want);
    assert_eq!(oto_ini::write(&entries, Cp932).expect("書ける"), once);

    // 書き出したものを取り込んでも、1バイトも変わらない。
    assert_eq!(parse(&once, Cp932).to_bytes().expect("書ける"), once);
}
