//! UST / USTX の取り込み（`TR-RCL-12`）。
//!
//! 主経路は本人が持ち込む UST / USTX。 曲バンクを持たないので、
//! 「歌えるか」を測る対象は本人が決める。
//!
//! ファイル全体だけでなく、任意のノート群を選んで目標にできる（サビだけ、など）。
//! ここが返すのはノート列なので、切り出しは呼び出し側が行う。
//!
//! # 符号化
//!
//! UST は CP932 が既定（UTAU 本体がそう書く）。判定は [`crate::text`] に任せる。
//! USTX は YAML で UTF-8。
//!
//! # 取り込まないもの
//!
//! テンポ、フラグ、エンベロープ、ピッチベンド。要るのは歌詞・音高・長さだけ
//! （`TR-RCL-12` (a)(b)）。カバレッジの計算にそれ以外は効かない。

use crate::song::{Note, Provenance, Song};
use crate::text::{self, TextEncoding};

/// UST を読めなかった理由。
#[derive(Debug, thiserror::Error)]
pub enum UstError {
    /// 符号化を判定できなかった。文字化けした状態で読み込まない（`TR-PLT-08`）。
    #[error("符号化を判定できなかった")]
    Encoding(#[from] text::TextError),

    /// ノートが1つも無い。
    #[error("ノートが1つも無い")]
    NoNotes,

    /// 書式が想定と違う。
    #[error("UST として読めない")]
    Malformed,
}

impl UstError {
    /// 送信してよい種別文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Encoding(e) => e.kind(),
            Self::NoNotes => "ust.no_notes",
            Self::Malformed => "ust.malformed",
        }
    }
}

/// UTAU の休符を表す歌詞。
const REST_LYRICS: [&str; 4] = ["R", "r", "休", "-"];

/// UST を読む（CP932 / UTF-8 のどちらでも）。
///
/// 符号化は宣言を見てから決める。 `#Charset:` があればそれ、無ければ CP932
/// （`TR-PKG-48` と同じ順序）。
#[tracing::instrument(skip(bytes, title), fields(len = bytes.len()), err)]
pub fn parse_ust(bytes: &[u8], title: &str) -> Result<Song, UstError> {
    let declared = text::oto_charset_declaration(bytes);
    let encoding = declared
        .as_deref()
        .and_then(TextEncoding::parse)
        .unwrap_or(TextEncoding::Cp932);

    // 宣言どおりに読めなければ、もう一方も試す。
    // UTAU 以外が書いた UST は UTF-8 のことがある。読めないまま止めるより試す。
    let body = text::decode(bytes, encoding).or_else(|first| {
        let other = match encoding {
            TextEncoding::Cp932 => TextEncoding::Utf8,
            TextEncoding::Utf8 => TextEncoding::Cp932,
        };
        text::decode(bytes, other).map_err(|_| first)
    })?;

    let mut notes = Vec::new();
    let mut lyric: Option<String> = None;
    let mut midi: Option<i32> = None;
    let mut ticks: Option<u32> = None;
    let mut in_note = false;

    let flush = |notes: &mut Vec<Note>,
                 lyric: &mut Option<String>,
                 midi: &mut Option<i32>,
                 ticks: &mut Option<u32>| {
        if let (Some(l), Some(m), Some(t)) = (lyric.take(), midi.take(), ticks.take())
            && !REST_LYRICS.contains(&l.as_str())
        {
            notes.push(Note {
                lyric: l,
                midi: m,
                ticks: t,
            });
        }
        *lyric = None;
        *midi = None;
        *ticks = None;
    };

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_note {
                flush(&mut notes, &mut lyric, &mut midi, &mut ticks);
            }
            // `[#0000]` のような節がノート。 `[#SETTING]` などは飛ばす。
            in_note = line
                .trim_start_matches("[#")
                .trim_end_matches(']')
                .chars()
                .all(|c| c.is_ascii_digit())
                && line.len() > 3;
            continue;
        }
        if !in_note {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "Lyric" => lyric = Some(value.trim().to_owned()),
            "NoteNum" => midi = value.trim().parse().ok(),
            "Length" => ticks = value.trim().parse().ok(),
            _ => {}
        }
    }
    if in_note {
        flush(&mut notes, &mut lyric, &mut midi, &mut ticks);
    }

    if notes.is_empty() {
        return Err(UstError::NoNotes);
    }

    Ok(Song {
        title: title.to_owned(),
        notes,
        provenance: Provenance {
            // 持ち込んだ曲は配布パッケージに含めない（`TR-RCL-12`）。
            source: "本人が持ち込んだ UST".to_owned(),
            license: "不明（配布物には含めない）".to_owned(),
        },
    })
}

/// 同梱する伝承曲（`TR-RCL-12`）。
///
/// 同梱はパブリックドメインの伝承曲に限る。 第三者の楽曲の旋律・歌詞は含めない。
/// これは初回のとっかかりで、曲バンクではない。 本人が外せる。
#[must_use]
pub fn bundled_songs() -> Vec<Song> {
    vec![sakura_sakura()]
}

/// 「さくらさくら」（日本古謡、パブリックドメイン）。
///
/// 旋律も歌詞も江戸時代の作で、著作権は存在しない。
fn sakura_sakura() -> Song {
    // (歌詞, 半音, 拍数)。A（69）を基準にした都節音階。
    const NOTES: [(&str, i32, u32); 14] = [
        ("さ", 69, 2),
        ("く", 69, 2),
        ("ら", 71, 4),
        ("さ", 69, 2),
        ("く", 69, 2),
        ("ら", 71, 4),
        ("や", 69, 2),
        ("よ", 71, 2),
        ("い", 72, 2),
        ("の", 74, 2),
        ("そ", 72, 2),
        ("ら", 71, 2),
        ("は", 69, 4),
        ("ー", 69, 4),
    ];
    Song {
        title: "さくらさくら".to_owned(),
        notes: NOTES
            .iter()
            .map(|(l, m, beats)| Note {
                lyric: (*l).to_owned(),
                midi: *m,
                // UST の 480 ティック = 4分音符。
                ticks: beats * 240,
            })
            .collect(),
        provenance: Provenance {
            source: "日本古謡".to_owned(),
            license: "パブリックドメイン".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::alias::Method;
    use crate::inventory::UnitSet;

    const SAMPLE: &str = "[#VERSION]\nUST Version1.2\n[#SETTING]\nTempo=120.00\n[#0000]\nLength=480\nLyric=さ\nNoteNum=60\n[#0001]\nLength=480\nLyric=く\nNoteNum=62\n[#0002]\nLength=240\nLyric=R\nNoteNum=60\n[#TRACKEND]\n";

    #[test]
    fn utf8_の_ust_を読める() {
        let s = parse_ust(SAMPLE.as_bytes(), "テスト").expect("読めること");
        assert_eq!(s.notes.len(), 2, "休符は落とす");
        assert_eq!(s.notes[0].lyric, "さ");
        assert_eq!(s.notes[0].midi, 60);
        assert_eq!(s.notes[0].ticks, 480);
        assert_eq!(s.notes[1].lyric, "く");
    }

    /// UST は CP932 が既定（UTAU 本体がそう書く）。
    #[test]
    fn cp932_の_ust_を読める() {
        let bytes = text::encode(SAMPLE, TextEncoding::Cp932).expect("書けること");
        assert_ne!(bytes, SAMPLE.as_bytes());
        let s = parse_ust(&bytes, "テスト").expect("読めること");
        assert_eq!(s.notes[0].lyric, "さ");
    }

    /// 宣言があればそれに従う。
    #[test]
    fn charset_の宣言に従う() {
        let declared = format!("#Charset:UTF-8\n{SAMPLE}");
        let s = parse_ust(declared.as_bytes(), "テスト").expect("読めること");
        assert_eq!(s.notes[0].lyric, "さ");
    }

    #[test]
    fn ノートが無ければ拒む() {
        let e = parse_ust(b"[#VERSION]\nUST Version1.2\n[#TRACKEND]\n", "x").expect_err("拒むこと");
        assert_eq!(e.kind(), "ust.no_notes");
    }

    /// 休符だけの UST もノート無し。
    #[test]
    fn 休符だけならノート無し() {
        let only_rest = "[#0000]\nLength=480\nLyric=R\nNoteNum=60\n[#TRACKEND]\n";
        assert!(parse_ust(only_rest.as_bytes(), "x").is_err());
    }

    /// 同梱はパブリックドメインの伝承曲だけ（`TR-RCL-12`）。
    #[test]
    fn 同梱曲はパブリックドメイン() {
        let songs = bundled_songs();
        assert_eq!(songs.len(), 1, "曲バンクを持たない");
        for s in &songs {
            assert_eq!(s.provenance.license, "パブリックドメイン");
        }
    }

    #[test]
    fn 同梱曲の歌詞を読める() {
        let s = &bundled_songs()[0];
        let m = s.moras(UnitSet::Core).expect("読めること");
        assert_eq!(m.len(), s.notes.len());

        let need = s.required_aliases(Method::Single, UnitSet::Core);
        // さ く ら や よ い の そ は。長音は単位を要求しない。
        assert_eq!(need.len(), 9, "{need:?}");
        assert!(need.contains("さ"));
        assert!(!need.contains("ー"));

        let (lo, hi) = s.range().expect("音域");
        assert!(lo >= 60 && hi <= 84, "歌える範囲にあること: {lo}..{hi}");
    }
}
