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
//! UST は CP932 が既定（UTAU 本体がそう書く）が、**既定を判定に使わない。**
//! 宣言が無ければ UTF-8 として読めるかで決める。 復号は [`crate::text`]。
//! USTX は YAML で UTF-8。
//!
//! # 取り込まないもの
//!
//! フラグ、エンベロープ、ピッチベンド、表情。要るのは歌詞・音高・長さと
//! テンポだけ（`TR-RCL-12` (a)(b)）。カバレッジの計算にそれ以外は効かない。
//!
//! **テンポは落とさない。** 落とすと試唱が常に既定の速さで鳴り、
//! 持ち込んだ曲が別の曲に聞こえる（`TR-SYN-30`）。

use std::collections::BTreeMap;

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

    /// ノートの節に `Lyric`・`NoteNum`・`Length` のどれかが欠けている。
    ///
    /// `index` は何番目のノート節か（1 始まり）。 歌詞は載せない——
    /// この表示は画面にもトレースにも出る（`AGENTS.md` #3）。
    #[error("{index} 番目のノートに、歌詞・音高・長さのどれかが欠けている")]
    IncompleteNote { index: usize },

    /// USTX（YAML）として読めない。
    #[error("USTX として読めない")]
    MalformedUstx,
}

impl koeru_failure::Failure for UstError {
    fn code(&self) -> &'static str {
        match self {
            Self::Encoding(e) => e.code(),
            Self::NoNotes => "ust.no_notes",
            Self::Malformed => "ust.malformed",
            Self::IncompleteNote { .. } => "ust.incomplete_note",
            Self::MalformedUstx => "ust.malformed_ustx",
        }
    }

    /// 読むのは本人が選んだ曲のファイル。読めないのは渡されたものの問題。
    fn class(&self) -> koeru_failure::Class {
        koeru_failure::Class::InvalidInput
    }
}

/// UTAU の休符を表す歌詞。
const REST_LYRICS: [&str; 4] = ["R", "r", "休", "-"];

/// もう一方の符号化。
const fn other(enc: TextEncoding) -> TextEncoding {
    match enc {
        TextEncoding::Cp932 => TextEncoding::Utf8,
        TextEncoding::Utf8 => TextEncoding::Cp932,
    }
}

/// UST を読む（CP932 / UTF-8 のどちらでも）。
///
/// 宣言があればそれに従う。 `#Charset:` を先に見る。
///
/// **宣言が無いときに CP932 を既定にしない。** Shift_JIS はほとんどのバイト列を
/// 「読めた」ことにするので、UTF-8 で書かれた UST が化けたまま通る——
/// 「な」（`E3 81 AA`）が「縺ｪ」になった。**踏んだ。** 逆は起きにくい。
/// CP932 の日本語は、まず UTF-8 として不正になる。だから UTF-8 を先に試す。
///
/// ASCII だけの UST はどちらで読んでも同じ字になる。 順序は効かない。
#[tracing::instrument(skip(bytes, title), fields(len = bytes.len()))]
pub fn parse_ust(bytes: &[u8], title: &str) -> Result<Song, UstError> {
    let declared = text::oto_charset_declaration(bytes)
        .as_deref()
        .and_then(TextEncoding::parse);

    // 宣言どおりに読めなければ、もう一方も試す。
    // 宣言が間違っている UST を、読めないまま止めない。
    let body = match declared {
        Some(enc) => text::decode(bytes, enc)
            .or_else(|first| text::decode(bytes, other(enc)).map_err(|_| first))?,
        None => text::decode(bytes, TextEncoding::Utf8)
            .or_else(|_| text::decode(bytes, TextEncoding::Cp932))?,
    };

    let mut notes: Vec<Note> = Vec::new();
    let mut lyric: Option<String> = None;
    let mut midi: Option<i32> = None;
    let mut ticks: Option<u32> = None;
    let mut in_note = false;
    let mut tempo = None;
    // 休符の長さは次の音符へ持ち越す（`TR-RCL-12`）。
    // **捨てない。** 捨てると曲が詰まって、元と違うリズムで鳴る。
    let mut pending_rest = 0_u32;
    // 何番目のノート節か（1 始まり）。 欠けた節を名指すのに使う。
    let mut section = 0_usize;

    let flush = |notes: &mut Vec<Note>,
                 pending_rest: &mut u32,
                 lyric: &mut Option<String>,
                 midi: &mut Option<i32>,
                 ticks: &mut Option<u32>,
                 section: usize|
     -> Result<(), UstError> {
        let (l, m, t) = (lyric.take(), midi.take(), ticks.take());
        // 休符は長さだけ使う。 鳴らないので、音高が欠けていても曲は変わらない。
        if let (Some(rest), Some(t)) = (&l, t)
            && REST_LYRICS.contains(&rest.as_str())
        {
            *pending_rest = pending_rest.saturating_add(t);
            return Ok(());
        }
        // **黙って捨てていた。** 1つでも欠けた節を飛ばすと、歌う音符が消えるか、
        // 休符が消えてリズムが詰まる——読めたふりをして、別の曲を取り込む。
        let (Some(l), Some(m), Some(t)) = (l, m, t) else {
            return Err(UstError::IncompleteNote { index: section });
        };
        notes.push(Note {
            lyric: l,
            midi: m,
            ticks: t,
            rest_ticks: std::mem::take(pending_rest),
        });
        Ok(())
    };

    for line in body.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            if in_note {
                flush(
                    &mut notes,
                    &mut pending_rest,
                    &mut lyric,
                    &mut midi,
                    &mut ticks,
                    section,
                )?;
            }
            // `[#0000]` のような節がノート。 `[#SETTING]` などは飛ばす。
            in_note = line
                .trim_start_matches("[#")
                .trim_end_matches(']')
                .chars()
                .all(|c| c.is_ascii_digit())
                && line.len() > 3;
            if in_note {
                section += 1;
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !in_note {
            // `[#SETTING]` のテンポ。 ノートごとの `Tempo=` は採らない——
            // 曲に1つしか持てない（`Song::tempo_bpm`）ので、途中の変化は
            // どれを採っても曲全体が別の速さになる。
            if key.trim() == "Tempo" {
                tempo = tempo.or_else(|| value.trim().parse::<f64>().ok());
            }
            continue;
        }
        match key.trim() {
            "Lyric" => lyric = Some(value.trim().to_owned()),
            "NoteNum" => midi = value.trim().parse().ok(),
            "Length" => ticks = value.trim().parse().ok(),
            _ => {}
        }
    }
    if in_note {
        flush(
            &mut notes,
            &mut pending_rest,
            &mut lyric,
            &mut midi,
            &mut ticks,
            section,
        )?;
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
        tempo_bpm: usable_tempo(tempo),
        default_portamento_ms: 0.0,
        // 本人が指定するまで動かさない（`DEC-SYN-012`）。
        transpose: 0,
    })
}

/// 4分音符のティック数。UST も USTX も、KOERU の内部表現もこれで揃える。
const TICKS_PER_QUARTER: u32 = 480;

/// 曲の速さとして使える値か見て、駄目なら既定へ倒す。
///
/// 0 や負の BPM を通すと、試唱の長さがゼロ除算か負になる。 書式としては
/// 通ってしまうので、ここで止める。
fn usable_tempo(bpm: Option<f64>) -> f64 {
    match bpm {
        Some(v) if v.is_finite() && v > 0.0 => v,
        _ => crate::guide::DEFAULT_TEMPO_BPM,
    }
}

/// 取り込めるファイルの形式（`TR-RCL-12`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// UTAU の UST。INI 風の平文。
    Ust,
    /// OpenUtau の USTX。YAML。
    Ustx,
}

impl Format {
    /// 拡張子で決める。 拡張子が当てにならなければ中身の先頭を見る。
    ///
    /// 拡張子を先に見る。 UST も USTX も本人が名前を付け替えられるが、
    /// 付け替えたのは本人なので、そちらの意思を先に採る。
    #[must_use]
    pub fn of(file_name: &str, bytes: &[u8]) -> Self {
        let lower = file_name.to_ascii_lowercase();
        if lower.ends_with(".ustx") {
            return Self::Ustx;
        }
        if lower.ends_with(".ust") {
            return Self::Ust;
        }
        // UST は必ず `[#…]` の節から始まる。 YAML にその形は無い。
        let head = String::from_utf8_lossy(&bytes[..bytes.len().min(64)]);
        // BOM は空白ではないので `trim_start` が落とさない。
        // 先に剥がさないと、BOM 付きの UST が USTX に見える。
        let head = head.trim_start_matches('\u{feff}').trim_start();
        if head.starts_with('[') {
            Self::Ust
        } else {
            Self::Ustx
        }
    }
}

/// 曲ファイルを読む（`TR-RCL-12`）。
///
/// **USTX は1トラックが1曲になる。** ハモリを主旋律と同じノート列へ混ぜると、
/// 同じ拍に複数の歌詞が並び、範囲を選ぶ画面で「サビだけ」を指せなくなる。
/// UST は単トラックなので常に1曲。
///
/// 題はファイル名から採った**候補**。 複数のトラックが出るときだけ
/// トラック名を足す。**確定させるのは呼び出し側**——本人が入力欄で
/// 直してから台帳へ入る（`TR-RCL-12`）。
///
/// # Errors
///
/// 符号化を判定できない、ノートが1つも無い、書式が想定と違う。
#[tracing::instrument(skip(bytes, file_name), fields(len = bytes.len()))]
pub fn parse_file(bytes: &[u8], file_name: &str) -> Result<Vec<Song>, UstError> {
    let stem = title_hint(file_name);
    match Format::of(file_name, bytes) {
        Format::Ust => Ok(vec![parse_ust(bytes, stem)?]),
        Format::Ustx => parse_ustx(bytes, stem),
    }
}

/// ファイル名から題の**候補**を作る。 ディレクトリと拡張子を落とす。
///
/// **空でも埋めない。** 以前は空になったら `曲` にしていたが、そうすると
/// 「曲」という題の行が黙って一覧に並ぶ。ここが返すのは入力欄に置く候補で、
/// 決めるのは本人（`TR-RCL-12`）。空なら空のまま返し、呼び出し側が
/// 題の入力を求める。
#[must_use]
pub fn title_hint(file_name: &str) -> &str {
    let base = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(file_name)
        .trim();
    base.rsplit_once('.').map_or(base, |(head, _)| head).trim()
}

/// USTX（YAML）を読む。
///
/// 読むのは `voice_parts` の中の歌詞・音高・長さと、曲の速さだけ。
/// `wave_parts`・表情・ピッチ曲線は見ない。
fn parse_ustx(bytes: &[u8], stem: &str) -> Result<Vec<Song>, UstError> {
    // USTX は UTF-8。 BOM は `encoding_rs` が落とす。
    let body = text::decode(bytes, TextEncoding::Utf8)?;
    let doc: yaml_serde::Value =
        yaml_serde::from_str(&body).map_err(|_| UstError::MalformedUstx)?;

    let resolution = doc
        .get("resolution")
        .and_then(yaml_serde::Value::as_i64)
        .and_then(|v| u32::try_from(v).ok())
        .filter(|v| *v > 0)
        .unwrap_or(TICKS_PER_QUARTER);
    let tempo = usable_tempo(ustx_tempo(&doc));
    let names = ustx_track_names(&doc);

    // トラック番号ごとに集める。 パートはトラックの中で並び直す——
    // 1つのトラックが、間を空けた複数のパートに分かれていることがある。
    let mut by_track: BTreeMap<i64, Vec<(i64, Note)>> = BTreeMap::new();
    for part in sequence(doc.get("voice_parts")) {
        let track = part
            .get("track_no")
            .and_then(yaml_serde::Value::as_i64)
            .unwrap_or(0);
        let at = part
            .get("position")
            .and_then(yaml_serde::Value::as_i64)
            .unwrap_or(0);
        for raw in sequence(part.get("notes")) {
            let position = at
                + raw
                    .get("position")
                    .and_then(yaml_serde::Value::as_i64)
                    .unwrap_or(0);
            if let Some(note) = ustx_note(raw, resolution)? {
                by_track.entry(track).or_default().push((position, note));
            }
        }
    }
    by_track.retain(|_, notes| !notes.is_empty());
    if by_track.is_empty() {
        return Err(UstError::NoNotes);
    }

    let multiple = by_track.len() > 1;
    let mut songs = Vec::with_capacity(by_track.len());
    for (track, mut notes) in by_track {
        // 安定ソート。 同じ位置に2つ並んでいたら、書かれていた順を保つ。
        notes.sort_by_key(|(position, _)| *position);

        /*
          音符のあいだの空きを休みとして持つ（`TR-RCL-12`）。

          **USTX は休符をノートとして持たない。** 空いている時間がそのまま休み。
          位置を捨てて長さだけ並べると、曲が詰まって元と違うリズムで鳴る。

          分解能を揃えたあとで測る。 位置も `resolution` の刻みなので、
          そのまま引くと 480 に揃えたティックと単位が合わない。
        */
        let scale = |t: i64| -> u32 {
            u32::try_from(t.max(0) * i64::from(TICKS_PER_QUARTER) / i64::from(resolution.max(1)))
                .unwrap_or(u32::MAX)
        };
        // 曲の原点から数える。 **最初の音符の位置から数えていた**ので、
        // 出だしに置かれた間が消え、取り込んだ曲がいきなり鳴り出していた。
        let mut end = 0_i64;
        for (position, note) in &mut notes {
            note.rest_ticks = scale(*position - end);
            // 時間軸は戻さない。 **毎回この音符の終わりを入れていた**ので、
            // 長い音符の中に短い音符が入っていると読み位置が巻き戻り、
            // 次の音符の手前に無い休符が生まれていた。
            end = end.max(
                *position
                    + i64::from(note.ticks) * i64::from(resolution) / i64::from(TICKS_PER_QUARTER),
            );
        }

        songs.push(Song {
            title: if multiple {
                format!("{stem} — {}", track_name(&names, track))
            } else {
                stem.to_owned()
            },
            notes: notes.into_iter().map(|(_, note)| note).collect(),
            provenance: Provenance {
                // 持ち込んだ曲は配布パッケージに含めない（`TR-RCL-12`）。
                source: "本人が持ち込んだ USTX".to_owned(),
                license: "不明（配布物には含めない）".to_owned(),
            },
            tempo_bpm: tempo,
            default_portamento_ms: 0.0,
            // 本人が指定するまで動かさない（`DEC-SYN-012`）。
            transpose: 0,
        });
    }
    Ok(songs)
}

/// USTX の1ノート。 休符なら `None`。
///
/// **欠けている鍵があれば読めないものとして返す。** 落として進むと、
/// 歌詞の並びが元の曲と違うものになり、要求するエイリアスが変わる。
fn ustx_note(raw: &yaml_serde::Value, resolution: u32) -> Result<Option<Note>, UstError> {
    let lyric = raw
        .get("lyric")
        .and_then(yaml_serde::Value::as_str)
        .ok_or(UstError::MalformedUstx)?
        .trim();
    let midi = raw
        .get("tone")
        .and_then(yaml_serde::Value::as_i64)
        .and_then(|v| i32::try_from(v).ok())
        .ok_or(UstError::MalformedUstx)?;
    let duration = raw
        .get("duration")
        .and_then(yaml_serde::Value::as_i64)
        .and_then(|v| u32::try_from(v).ok())
        .ok_or(UstError::MalformedUstx)?;

    if lyric.is_empty() || REST_LYRICS.contains(&lyric) {
        return Ok(None);
    }

    /*
      `+` で始まる歌詞は直前のノートの続き（OpenUtau の伸ばし）。
      長音として持つ。 新たな収録単位は要求しないが、拍としては存在する
      （`TR-RCL-13` (c)）ので、落とすと総モーラ数が合わなくなる。
    */
    let lyric = if lyric.starts_with('+') {
        "ー".to_owned()
    } else {
        lyric.to_owned()
    };

    Ok(Some(Note {
        lyric,
        midi,
        // 分解能はファイルごとに違う。 KOERU の内部表現は 480 に揃える。
        ticks: u32::try_from(
            u64::from(duration) * u64::from(TICKS_PER_QUARTER) / u64::from(resolution),
        )
        .unwrap_or(u32::MAX),
        // 休みは並べ直したあとに埋める。 ここでは前の音符が分からない。
        rest_ticks: 0,
    }))
}

/// 曲の速さ。 新しい USTX は `tempos`、古いものは `bpm`。
fn ustx_tempo(doc: &yaml_serde::Value) -> Option<f64> {
    // テンポ変化は持たない（`Song::tempo_bpm`）ので、先頭だけを採る。
    sequence(doc.get("tempos"))
        .next()
        .and_then(|t| t.get("bpm"))
        .and_then(yaml_serde::Value::as_f64)
        .or_else(|| doc.get("bpm").and_then(yaml_serde::Value::as_f64))
}

/// トラック名の一覧。 書かれていない位置は空文字。
fn ustx_track_names(doc: &yaml_serde::Value) -> Vec<String> {
    sequence(doc.get("tracks"))
        .map(|t| {
            t.get("track_name")
                .and_then(yaml_serde::Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_owned()
        })
        .collect()
}

/// 番号からトラック名を引く。 名前が無ければ番号で呼ぶ。
fn track_name(names: &[String], track: i64) -> String {
    usize::try_from(track)
        .ok()
        .and_then(|i| names.get(i))
        .filter(|name| !name.is_empty())
        .map_or_else(|| format!("トラック {}", track + 1), Clone::clone)
}

/// 並びとして読む。 無いもの・並びでないものは空として扱う。
fn sequence(value: Option<&yaml_serde::Value>) -> impl Iterator<Item = &yaml_serde::Value> {
    value
        .and_then(yaml_serde::Value::as_sequence)
        .into_iter()
        .flatten()
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
    /*
      (歌詞, 半音, 拍数)。A3（57）を基準にした都節音階。

      **既定の収録音高で書く**（`preset::DEFAULT_TONE_MIDI`）。 KOERU は曲を
      勝手に移調しない（`DEC-SYN-012`）ので、A4 で書くと1本目を録り終えても
      「録った高さから遠い」まま出ることになる。**同梱曲の調はこちらが決める
      ものなので、移調ではなく記譜で合わせる。**

      別の音高で作った人には遠くなる。 そのときは曲の面がキーを出す。
    */
    const NOTES: [(&str, i32, u32); 14] = [
        ("さ", 57, 2),
        ("く", 57, 2),
        ("ら", 59, 4),
        ("さ", 57, 2),
        ("く", 57, 2),
        ("ら", 59, 4),
        ("や", 57, 2),
        ("よ", 59, 2),
        ("い", 60, 2),
        ("の", 62, 2),
        ("そ", 60, 2),
        ("ら", 59, 2),
        ("は", 57, 4),
        ("ー", 57, 4),
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
                // 切れ目なく続く。
                rest_ticks: 0,
            })
            .collect(),
        provenance: Provenance {
            source: "日本古謡".to_owned(),
            license: "パブリックドメイン".to_owned(),
        },
        tempo_bpm: crate::guide::DEFAULT_TEMPO_BPM,
        default_portamento_ms: 0.0,
        // 本人が指定するまで動かさない（`DEC-SYN-012`）。
        transpose: 0,
    }
}

#[cfg(test)]
mod tests {

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }
    use super::*;
    use crate::alias::Method;
    use crate::inventory::UnitSet;
    use koeru_failure::Failure;

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

    /// 欠けたノート節を黙って捨てない（`TR-RCL-12`）。
    ///
    /// **黙って捨てていた。** 1つ欠けた節を飛ばすと、歌う音符が消えるか休符が
    /// 消えてリズムが詰まる——読めたふりをして別の曲を取り込む。
    #[test]
    fn 欠けたノート節は取り込まない() {
        let missing_pitch = "[#0000]\nLength=480\nLyric=さ\nNoteNum=60\n[#0001]\nLength=480\nLyric=く\n[#TRACKEND]\n";
        let e = parse_ust(missing_pitch.as_bytes(), "x").expect_err("断る");
        assert!(
            matches!(e, UstError::IncompleteNote { index: 2 }),
            "2 番目の節を名指す: {e:?}"
        );
        assert_eq!(e.code(), "ust.incomplete_note");
        assert!(!e.to_string().contains('く'), "歌詞は載せない");

        // 読めない長さも欠けたのと同じ。
        let bad_length = "[#0000]\nLength=abc\nLyric=さ\nNoteNum=60\n[#TRACKEND]\n";
        assert!(matches!(
            parse_ust(bad_length.as_bytes(), "x"),
            Err(UstError::IncompleteNote { index: 1 })
        ));
    }

    /// 休符は長さだけ要る。 鳴らないので、音高が無くても曲は変わらない。
    #[test]
    fn 音高の無い休符は通す() {
        let rest = "[#0000]\nLength=480\nLyric=さ\nNoteNum=60\n[#0001]\nLength=240\nLyric=R\n[#0002]\nLength=480\nLyric=く\nNoteNum=62\n[#TRACKEND]\n";
        let s = parse_ust(rest.as_bytes(), "x").expect("読める");
        assert_eq!(s.notes.len(), 2);
        assert_eq!(s.notes[1].rest_ticks, 240, "休符の長さは次の音符へ持ち越す");
    }

    /// UST は CP932 が既定（UTAU 本体がそう書く）。
    #[test]
    fn cp932_の_ust_を読める() {
        let bytes = text::encode(SAMPLE, TextEncoding::Cp932).expect("書けること");
        assert_ne!(bytes, SAMPLE.as_bytes());
        let s = parse_ust(&bytes, "テスト").expect("読めること");
        assert_eq!(s.notes[0].lyric, "さ");
    }

    /// Shift_JIS は UTF-8 のバイト列も「読めた」ことにする。
    ///
    /// 「な」（`E3 81 AA`）がそう。 CP932 を既定にしていたので化けたまま通った。
    /// **踏んだ。**
    #[test]
    fn utf8_の_ust_を_cp932_として読まない() {
        let utf8 = "[#0000]\nLength=480\nLyric=な\nNoteNum=62\n[#TRACKEND]\n";
        let s = parse_ust(utf8.as_bytes(), "x").expect("読めること");
        assert_eq!(s.notes[0].lyric, "な", "縺ｪ にならない");

        // CP932 で書かれた UST は、これまでどおり CP932 として読む。
        let cp932 = text::encode(utf8, TextEncoding::Cp932).expect("書ける");
        let s = parse_ust(&cp932, "x").expect("読めること");
        assert_eq!(s.notes[0].lyric, "な");
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
        assert_eq!(e.code(), "ust.no_notes");
    }

    /// 休符だけの UST もノート無し。
    #[test]
    fn 休符だけならノート無し() {
        let only_rest = "[#0000]\nLength=480\nLyric=R\nNoteNum=60\n[#TRACKEND]\n";
        assert!(parse_ust(only_rest.as_bytes(), "x").is_err());
    }

    /// OpenUtau が書く USTX。 見ない枝（ピッチ曲線・ビブラート・表情）も入れてある。
    const USTX: &str = r#"name: テスト
comment: ''
output_dir: Vocal
cache_dir: UCache
ustx_version: '0.6'
resolution: 480
bpm: 120
tempos:
- position: 0
  bpm: 135.5
time_signatures:
- bar_position: 0
  beat_per_bar: 4
  beat_unit: 4
tracks:
- singer: ''
  phonemizer: OpenUtau.Core.DefaultPhonemizer
  track_name: 主旋律
  mute: false
  solo: false
- singer: ''
  track_name: ハモリ
voice_parts:
- name: Part1
  comment: ''
  track_no: 0
  position: 0
  notes:
  - position: 480
    duration: 240
    tone: 62
    lyric: く
  - position: 0
    duration: 480
    tone: 60
    lyric: さ
    pitch:
      data:
      - {x: -40, y: 0, shape: io}
      - {x: 40, y: 0, shape: io}
      snap_first: true
    vibrato: {length: 0, period: 175, depth: 25, in: 10, out: 10, shift: 0, drift: 0, vol_link: 0}
    phoneme_expressions: []
    phoneme_overrides: []
  - position: 720
    duration: 240
    tone: 62
    lyric: R
  curves: []
- name: Part2
  track_no: 1
  position: 960
  notes:
  - position: 0
    duration: 480
    tone: 55
    lyric: ら
  - position: 480
    duration: 480
    tone: 55
    lyric: '+'
wave_parts: []
"#;

    #[test]
    fn ustx_を読める() {
        let songs = parse_file(USTX.as_bytes(), "/tmp/テスト.ustx").expect("読めること");
        assert_eq!(songs.len(), 2, "1トラックが1曲");

        let lead = &songs[0];
        assert_eq!(lead.title, "テスト — 主旋律");
        // 書かれた順ではなく位置の順。休符は落とす。
        assert_eq!(lead.notes.len(), 2);
        assert_eq!(lead.notes[0].lyric, "さ");
        assert_eq!(lead.notes[0].midi, 60);
        assert_eq!(lead.notes[0].ticks, 480);
        assert_eq!(lead.notes[1].lyric, "く");
        assert_eq!(lead.notes[1].ticks, 240);
    }

    /// `tempos` を `bpm` より先に見る。 古い USTX は `bpm` しか持たない。
    /// 休符を落とさない（`TR-RCL-12`）。 落とすと曲が詰まって鳴る。
    #[test]
    fn ust_の休符を次の音符の休みにする() {
        // さ(480) / R(240) / く(480)
        let with_rest = "[#0000]\nLength=480\nLyric=さ\nNoteNum=60\n[#0001]\nLength=240\nLyric=R\nNoteNum=60\n[#0002]\nLength=480\nLyric=く\nNoteNum=62\n[#TRACKEND]\n";
        let s = parse_ust(with_rest.as_bytes(), "x").expect("読める");
        assert_eq!(s.notes.len(), 2, "休符は音符として並べない");
        assert_eq!(s.notes[0].rest_ticks, 0);
        assert_eq!(s.notes[1].rest_ticks, 240, "休みは次の音符が持つ");
    }

    /// USTX は休符をノートに持たない。空いている時間がそのまま休み。
    #[test]
    fn ustx_のノート間の空きを休みにする() {
        // Part1 は 0..480（さ）、480..720（く）、720..960（R）で隙間なし。
        let songs = parse_file(USTX.as_bytes(), "テスト.ustx").expect("読める");
        assert_eq!(songs[0].notes[1].rest_ticks, 0, "隙間が無ければ 0");

        // 「く」を後ろへずらして 240 ティック空ける。
        let gapped = USTX.replace(
            "  - position: 480\n    duration: 240\n    tone: 62\n    lyric: く",
            "  - position: 720\n    duration: 240\n    tone: 62\n    lyric: く",
        );
        let songs = parse_file(gapped.as_bytes(), "テスト.ustx").expect("読める");
        assert_eq!(songs[0].notes[1].rest_ticks, 240, "空きが休みになる");
    }

    /// 分解能が違っても、休みも 480 に揃う。
    #[test]
    fn 休みも分解能を揃える() {
        let doubled = USTX
            .replace("resolution: 480", "resolution: 960")
            .replace(
                "  - position: 480\n    duration: 240\n    tone: 62\n    lyric: く",
                "  - position: 1440\n    duration: 480\n    tone: 62\n    lyric: く",
            )
            .replace("duration: 480\n    tone: 60", "duration: 960\n    tone: 60");
        let songs = parse_file(doubled.as_bytes(), "テスト.ustx").expect("読める");
        assert_eq!(songs[0].notes[0].ticks, 480);
        assert_eq!(
            songs[0].notes[1].rest_ticks, 240,
            "960 の 480 は 480 の 240"
        );
    }

    #[test]
    fn ustx_のテンポを落とさない() {
        let songs = parse_file(USTX.as_bytes(), "テスト.ustx").expect("読めること");
        for s in &songs {
            assert!(
                (s.tempo_bpm - 135.5).abs() < f64::EPSILON,
                "{}",
                s.tempo_bpm
            );
        }

        let legacy = USTX.replace("tempos:\n- position: 0\n  bpm: 135.5\n", "");
        let songs = parse_file(legacy.as_bytes(), "テスト.ustx").expect("読めること");
        assert!((songs[0].tempo_bpm - 120.0).abs() < f64::EPSILON);
    }

    /// `+` で始まる歌詞は直前の続き（OpenUtau の伸ばし）。長音として持つ。
    #[test]
    fn ustx_の伸ばしは長音になる() {
        let songs = parse_file(USTX.as_bytes(), "テスト.ustx").expect("読めること");
        let harmony = &songs[1];
        assert_eq!(harmony.title, "テスト — ハモリ");
        assert_eq!(harmony.notes[1].lyric, "ー");

        // 拍としては数えるが、収録単位は要求しない（`TR-RCL-13` (c)）。
        assert_eq!(harmony.total_moras(UnitSet::Core), 2);
        assert_eq!(
            harmony
                .required_aliases(&builtin_rules(), Method::Single, UnitSet::Core)
                .len(),
            1
        );
    }

    /// トラックが1本なら、題にトラック名を足さない。
    #[test]
    fn ustx_が単トラックなら題はファイル名だけ() {
        let one = USTX.split("- name: Part2").next().expect("前半").to_owned() + "wave_parts: []\n";
        let songs = parse_file(one.as_bytes(), "テスト.ustx").expect("読めること");
        assert_eq!(songs.len(), 1);
        assert_eq!(songs[0].title, "テスト");
    }

    /// 分解能はファイルごとに違う。内部表現は 480 に揃える。
    #[test]
    fn ustx_の分解能を揃える() {
        let doubled = USTX
            .replace("resolution: 480", "resolution: 960")
            .replace("duration: 480", "duration: 960")
            .replace("duration: 240", "duration: 480");
        let songs = parse_file(doubled.as_bytes(), "テスト.ustx").expect("読めること");
        assert_eq!(songs[0].notes[0].ticks, 480);
        assert_eq!(songs[0].notes[1].ticks, 240);
    }

    /// 鍵が欠けたノートは落とさず拒む。落とすと歌詞の並びが元の曲と変わる。
    #[test]
    fn ustx_の壊れたノートを落とさない() {
        let broken = USTX.replace("    tone: 60\n", "");
        let e = parse_file(broken.as_bytes(), "テスト.ustx").expect_err("拒むこと");
        assert_eq!(e.code(), "ust.malformed_ustx");
    }

    /// .NET が書くと BOM が付くことがある。 復号の時点で落とす。
    ///
    /// 残したまま渡すと、最初の鍵が `\u{feff}name` になって
    /// 曲の速さも分解能も既定へ落ちる。数は合っているので気づけない。
    #[test]
    fn bom_付きの_ustx_を読める() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(USTX.as_bytes());
        let songs = parse_file(&bytes, "テスト.ustx").expect("読めること");
        assert_eq!(songs.len(), 2);
        assert!(
            (songs[0].tempo_bpm - 135.5).abs() < f64::EPSILON,
            "速さも読める"
        );
    }

    #[test]
    fn ustx_として読めなければ拒む() {
        let e = parse_file(b"\tname: x\n  - broken", "x.ustx").expect_err("拒むこと");
        assert_eq!(e.code(), "ust.malformed_ustx");
    }

    #[test]
    fn 歌うノートが無い_ustx_を拒む() {
        let empty = "name: x\nresolution: 480\nvoice_parts: []\n";
        let e = parse_file(empty.as_bytes(), "x.ustx").expect_err("拒むこと");
        assert_eq!(e.code(), "ust.no_notes");
    }

    /// 拡張子が当てにならないときは中身で決める。
    #[test]
    fn 拡張子が無くても形式を見分ける() {
        assert_eq!(Format::of("曲", SAMPLE.as_bytes()), Format::Ust);
        assert_eq!(Format::of("曲", USTX.as_bytes()), Format::Ustx);
        // 拡張子があれば、そちらを先に採る。
        assert_eq!(Format::of("曲.ust", USTX.as_bytes()), Format::Ust);

        // BOM は空白ではないので、先に剥がさないと UST が USTX に見える。
        let mut bom = vec![0xEF, 0xBB, 0xBF];
        bom.extend_from_slice(SAMPLE.as_bytes());
        assert_eq!(Format::of("曲", &bom), Format::Ust);
    }

    /// 題の候補はファイル名から。 **空でも埋めない**（`TR-RCL-12`）。
    #[test]
    fn 題の候補はファイル名から採る() {
        assert_eq!(title_hint("/a/b/さくら.ust"), "さくら");
        assert_eq!(title_hint(r"C:\songs\さくら.USTX"), "さくら");
        assert_eq!(title_hint("さくら"), "さくら");
        // 「曲」で埋めない。 埋めると、題を決めないまま一覧に並ぶ。
        assert_eq!(title_hint(".ust"), "");
        assert_eq!(title_hint("  "), "");
    }

    /// UST のテンポも落とさない（`TR-SYN-30`）。
    #[test]
    fn ust_のテンポを落とさない() {
        let s = parse_ust(SAMPLE.as_bytes(), "テスト").expect("読めること");
        assert!((s.tempo_bpm - 120.0).abs() < f64::EPSILON);

        let faster = SAMPLE.replace("Tempo=120.00", "Tempo=180.00");
        let s = parse_ust(faster.as_bytes(), "テスト").expect("読めること");
        assert!((s.tempo_bpm - 180.0).abs() < f64::EPSILON);
    }

    /// 0 や負の BPM は書式としては通る。倒さないと試唱の長さが壊れる。
    #[test]
    fn 使えないテンポは既定へ倒す() {
        let zero = SAMPLE.replace("Tempo=120.00", "Tempo=0");
        let s = parse_ust(zero.as_bytes(), "テスト").expect("読めること");
        assert!((s.tempo_bpm - crate::guide::DEFAULT_TEMPO_BPM).abs() < f64::EPSILON);
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

        let need = s.required_aliases(&builtin_rules(), Method::Single, UnitSet::Core);
        // さ く ら や よ い の そ は。長音は単位を要求しない。
        assert_eq!(need.len(), 9, "{need:?}");
        assert!(need.contains("さ"));
        assert!(!need.contains("ー"));

        // 既定の収録音高（A3 = 57）のまま届く（`DEC-SYN-012`）。
        let (lo, hi) = s.range().expect("音域");
        assert_eq!(lo, 57, "1本目の収録音高から始まる");
        assert!(
            hi <= 57 + crate::song::MAX_SHIFT_UP,
            "上へ伸ばす幅に収まる: {hi}"
        );
    }
}
