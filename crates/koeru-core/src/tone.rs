//! 音階の名前と、多音階の割り当て（`TR-RCL-06`, `TR-REC-25`, `TR-REC-36`, `TR-PKG-04`, `TR-PKG-29`）。
//!
//! `prefix.map` は1行1音階で音階名を書き、`character.yaml` の
//! `subbanks.tone_ranges` は `"C3-C4"` か `"C5"` の形で書く。
//! 音高ごとのディレクトリ名も同じ（`TR-REC-36`）。どれも同じ割り当てを表すので、
//! 名前の作り方はここ1箇所が持つ。
//!
//! # 内部は MIDI、外は英語音名
//!
//! `TR-REC-25` がそう定めている。 表示・ディレクトリ名・`prefix.map` の tone は
//! 英語音名で、計算はすべて MIDI ノート番号で行う。
//!
//! # C4 = 60
//!
//! UTAU の表記に合わせる。 科学的音高表記（MIDI 60 = C4）と同じで、
//! Yamaha 式（60 = C3）ではない。ここを1オクターブずらすと、
//! prefix.map の割り当てが全部隣のオクターブへ移る。

/// MIDI 60 を `C4` と呼ぶ（UTAU の表記）。
const MIDI_C4: i32 = 60;

/// 半音の名前。異名同音は `#` 側だけを使う。
///
/// UTAU の prefix.map も OpenUtau の tone_ranges も `#` 表記。
/// `Db` を書くと、どちらも音階として解釈しない。
const SEMITONES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// MIDI 番号から音階名を作る。
#[must_use]
pub fn name(midi: i32) -> String {
    let index = midi.rem_euclid(12);
    let octave = (midi - MIDI_C4).div_euclid(12) + 4;
    #[allow(
        clippy::cast_sign_loss,
        reason = "rem_euclid(12) は 0..12 しか返さない"
    )]
    let semitone = SEMITONES[index as usize];
    format!("{semitone}{octave}")
}

/// 音階名から MIDI 番号を読む。読めなければ `None`。
///
/// `prefix.map` を読み戻すときに要る。書き出しだけなら使わないが、
/// [`name`] の逆であることを試験で確かめられるようにしておく。
#[must_use]
pub fn parse(s: &str) -> Option<i32> {
    let s = s.trim();
    let (semitone, octave) = s.split_at(s.find(|c: char| c == '-' || c.is_ascii_digit())?);
    let index = SEMITONES.iter().position(|n| *n == semitone)?;
    let octave: i32 = octave.parse().ok()?;
    #[allow(clippy::cast_possible_wrap, reason = "position は 0..12")]
    let index = index as i32;
    Some(MIDI_C4 + (octave - 4) * 12 + index)
}

/// 連続する音階を `"C3-C4"` へ、単独を `"C5"` へ畳む（`TR-PKG-04`）。
///
/// 並びが飛んでいたら区間を分ける。 飛びを潰して1区間にすると、
/// 録っていない音階まで「その prefix で鳴る」と宣言することになる。
#[must_use]
pub fn ranges(tones: &[i32]) -> Vec<String> {
    let mut sorted: Vec<i32> = tones.to_vec();
    sorted.sort_unstable();
    sorted.dedup();

    let mut out = Vec::new();
    let mut run: Option<(i32, i32)> = None;
    for t in sorted {
        match run {
            Some((start, last)) if t == last + 1 => run = Some((start, t)),
            Some((start, last)) => {
                out.push(render_range(start, last));
                run = Some((t, t));
            }
            None => run = Some((t, t)),
        }
    }
    if let Some((start, last)) = run {
        out.push(render_range(start, last));
    }
    out
}

fn render_range(start: i32, last: i32) -> String {
    if start == last {
        name(start)
    } else {
        format!("{}-{}", name(start), name(last))
    }
}

/// `prefix.map` が覆う最低音（C1、`TR-RCL-06`）。
pub const PREFIX_MAP_LOW: i32 = 24;
/// `prefix.map` が覆う最高音（B7）。
pub const PREFIX_MAP_HIGH: i32 = 107;

/// 多音階の推奨値、女声（G3 / D4 / A4、`TR-RCL-06`）。
pub const DEFAULT_TONES_FEMALE: [i32; 3] = [55, 62, 69];
/// 男声の推奨値（C3 / A3 / E4）。
pub const DEFAULT_TONES_MALE: [i32; 3] = [48, 57, 64];

/// 収録音高として受け取れる並びか見て、揃えて返す（`TR-RCL-01`）。
///
/// **本数も音高も本人が決める。** ここが弾くのは、そもそも鳴らせないものだけ
/// ——空、`prefix.map` が覆う範囲（C1〜B7）の外、重複。
///
/// 並べ替えて返す。 台帳もディレクトリも音高の昇順を前提にしている。
///
/// # Errors
///
/// 空、範囲の外、重複があるとき。
pub fn normalize(tones: &[i32]) -> Result<Vec<i32>, ToneError> {
    if tones.is_empty() {
        return Err(ToneError::Empty);
    }
    if let Some(midi) = tones
        .iter()
        .find(|m| !(PREFIX_MAP_LOW..=PREFIX_MAP_HIGH).contains(m))
    {
        return Err(ToneError::OutOfRange { midi: *midi });
    }
    let mut sorted = tones.to_vec();
    sorted.sort_unstable();
    let before = sorted.len();
    sorted.dedup();
    if sorted.len() != before {
        return Err(ToneError::Duplicate);
    }
    Ok(sorted)
}

/// 収録音高として受け取れない並び。
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ToneError {
    /// 1本も選ばれていない。
    #[error("収録音高が1つも無い")]
    Empty,
    /// `prefix.map` が覆う範囲の外。
    #[error("prefix.map が覆う範囲の外")]
    OutOfRange {
        /// 外れていた音高。
        midi: i32,
    },
    /// 同じ音高が2度ある。
    #[error("同じ音高が2度ある")]
    Duplicate,
}

impl ToneError {
    /// 送信層へ載せてよい固定文字列。
    #[must_use]
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Empty => "tone.empty",
            Self::OutOfRange { .. } => "tone.out_of_range",
            Self::Duplicate => "tone.duplicate",
        }
    }
}

/*
  収録音高の間隔に警告を出さない。

  以前は 5〜9 半音を外れたら警告していた。 **数字の出どころが無かった。**
  `reclist.toml` の領域リスクが「resampler ごとの劣化量を測定した公開データは
  見つからなかった」と書いているとおりで、確認できているのは既存音源の
  prefix.map が 7〜9 半音で切り替わっていることだけ。下限の 5 は何にも
  基づいていない。**根拠の無い数字で本人の選択を咎めない。**
*/

/// その音を鳴らすのに使う収録音高（floor 割り当て、`TR-RCL-06`）。
///
/// 「その音以下で最も高い収録音高」。 最低音高は自分より下の全音域も担当する
/// ——下に素材が無いので、上へ伸ばすしかない。
///
/// 収録音高が空なら `None`。
#[must_use]
pub fn floor_tone(tones: &[i32], midi: i32) -> Option<i32> {
    let mut sorted = tones.to_vec();
    sorted.sort_unstable();
    sorted
        .iter()
        .rev()
        .find(|t| **t <= midi)
        .copied()
        // 最低音高より下は、その最低音高が担う。
        .or_else(|| sorted.first().copied())
}

/// 鳴らすのに要るピッチシフト量（半音、`TR-RCL-22`）。
///
/// 正なら上へ、負なら下へ。floor 割り当ての帰結として、
/// 最低収録音高より下を鳴らすときだけ負になる。
#[must_use]
pub fn shift_for(tones: &[i32], midi: i32) -> Option<i32> {
    floor_tone(tones, midi).map(|t| midi - t)
}

/// 半音ごとの割り当て（`TR-RCL-06`）。
///
/// C1 から B7 の 84 組を、`(その半音, 担う収録音高)` で返す。
/// 綴りは持たない——`prefix.map` の prefix / suffix は区画が決める。
#[must_use]
pub fn prefix_map_rows(tones: &[i32]) -> Vec<(i32, i32)> {
    (PREFIX_MAP_LOW..=PREFIX_MAP_HIGH)
        .filter_map(|midi| floor_tone(tones, midi).map(|t| (midi, t)))
        .collect()
}

/// 収録音高に対する既定の綴り。prefix は空、suffix は音名（`TR-REC-25`）。
#[must_use]
pub fn default_affix(recorded: i32) -> (String, String) {
    (String::new(), name(recorded))
}

/// `prefix.map` の本文（`TR-RCL-06`）。
///
/// C1 から B7 の 84 半音すべてに1行を持つ。 行は `音名 \t prefix \t suffix` で、
/// 綴りは `affix` が収録音高から決める。
///
/// **抜けを作らない。** 担う音高が書かれていない半音があると、UTAU は
/// そこだけ素のエイリアスを探し、単音階の音源として鳴らそうとする。
///
/// **prefix を勝手に空にしない。** 区画が `↑` のような接頭辞を持っているのに
/// ここで落とすと、その音域のエイリアスがどれも引けなくなる。**踏んだ。**
pub fn prefix_map_body(
    tones: &[i32],
    affix: impl Fn(i32) -> (String, String),
    newline: &str,
) -> String {
    let mut out = String::new();
    for (midi, recorded) in prefix_map_rows(tones) {
        let (prefix, suffix) = affix(recorded);
        out.push_str(&format!("{}\t{prefix}\t{suffix}{newline}", name(midi)));
    }
    out
}

/// 収録音高ごとに、その音高が担う半音の並び（`TR-PKG-04` の `tone_ranges`）。
///
/// `prefix.map` と同じ floor 割り当てから作る。 2箇所で別々に割り当てると、
/// `prefix.map` が指す音高と `character.yaml` が宣言する範囲がずれる。
#[must_use]
pub fn assigned_ranges(tones: &[i32]) -> Vec<(i32, Vec<i32>)> {
    let mut sorted = tones.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .iter()
        .map(|t| {
            let owned = (PREFIX_MAP_LOW..=PREFIX_MAP_HIGH)
                .filter(|m| floor_tone(&sorted, *m) == Some(*t))
                .collect();
            (*t, owned)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最低音高より下は、その最低音高が担う（`TR-RCL-06`）。
    #[test]
    fn 最低音高は自分より下も担う() {
        let tones = DEFAULT_TONES_FEMALE;
        assert_eq!(floor_tone(&tones, 55), Some(55));
        assert_eq!(floor_tone(&tones, 54), Some(55), "G3 より下も G3");
        assert_eq!(floor_tone(&tones, PREFIX_MAP_LOW), Some(55));
    }

    /// その音以下で最も高い収録音高（`TR-RCL-06`）。
    #[test]
    fn floor_割り当ては直下の収録音高を選ぶ() {
        let tones = DEFAULT_TONES_FEMALE;
        assert_eq!(floor_tone(&tones, 61), Some(55), "D4 の手前は G3");
        assert_eq!(floor_tone(&tones, 62), Some(62));
        assert_eq!(floor_tone(&tones, 68), Some(62));
        assert_eq!(floor_tone(&tones, 69), Some(69));
        assert_eq!(floor_tone(&tones, 100), Some(69), "A4 より上は A4");
        assert_eq!(floor_tone(&[], 60), None);
    }

    /// シフト量は floor 割り当ての差（`TR-RCL-22`）。
    #[test]
    fn シフト量は収録音高との差() {
        let tones = DEFAULT_TONES_FEMALE;
        assert_eq!(shift_for(&tones, 69), Some(0));
        assert_eq!(shift_for(&tones, 68), Some(6));
        assert_eq!(shift_for(&tones, 50), Some(-5), "最低音高より下は負");
    }

    /// prefix.map は 84 行あり、抜けが無い（`TR-RCL-06`）。
    #[test]
    fn prefix_map_は八十四行で抜けが無い() {
        let body = prefix_map_body(&DEFAULT_TONES_FEMALE, default_affix, "\n");
        let lines: Vec<&str> = body.lines().collect();
        assert_eq!(lines.len(), 84);
        assert!(lines[0].starts_with("C1\t"), "{}", lines[0]);
        assert!(lines[83].starts_with("B7\t"), "{}", lines[83]);
        for l in &lines {
            let cols: Vec<&str> = l.split('\t').collect();
            assert_eq!(cols.len(), 3, "{l}");
            assert!(!cols[2].is_empty(), "担う音高が空: {l}");
        }
    }

    /// 本数も音高も本人が決める（`TR-RCL-01`）。弾くのは鳴らせないものだけ。
    #[test]
    fn 収録音高は本人が決める() {
        // 間隔は問わない。 2 半音差でも 24 半音差でも通す。
        assert_eq!(normalize(&[60, 62]).expect("通る"), [60, 62]);
        assert_eq!(normalize(&[48, 72]).expect("通る"), [48, 72]);
        // 並べ替えて返す。台帳もディレクトリも昇順を前提にしている。
        assert_eq!(normalize(&[69, 55, 62]).expect("通る"), [55, 62, 69]);
        // 1本でよい。
        assert_eq!(normalize(&[57]).expect("通る"), [57]);
    }

    #[test]
    fn 鳴らせない音高は弾く() {
        assert_eq!(normalize(&[]).expect_err("弾く").kind(), "tone.empty");
        assert_eq!(
            normalize(&[PREFIX_MAP_LOW - 1]).expect_err("弾く").kind(),
            "tone.out_of_range"
        );
        assert_eq!(
            normalize(&[PREFIX_MAP_HIGH + 1]).expect_err("弾く").kind(),
            "tone.out_of_range"
        );
        assert_eq!(
            normalize(&[60, 60]).expect_err("弾く").kind(),
            "tone.duplicate"
        );
    }

    /// 割り当てた範囲を合わせると 84 半音になる（`TR-PKG-04`）。
    #[test]
    fn 割り当ては重ならず抜けない() {
        let ranges = assigned_ranges(&DEFAULT_TONES_FEMALE);
        assert_eq!(ranges.len(), 3);
        let total: usize = ranges.iter().map(|(_, v)| v.len()).sum();
        assert_eq!(total, 84);
        let mut all: Vec<i32> = ranges.iter().flat_map(|(_, v)| v.clone()).collect();
        all.sort_unstable();
        all.dedup();
        assert_eq!(all.len(), 84, "重なりが無い");
    }

    #[test]
    fn c4_は_60() {
        assert_eq!(name(60), "C4");
        assert_eq!(name(61), "C#4");
        assert_eq!(name(59), "B3");
        assert_eq!(name(24), "C1");
    }

    #[test]
    fn 名前を往復できる() {
        for midi in 12..=108 {
            assert_eq!(parse(&name(midi)), Some(midi), "{midi}");
        }
        assert_eq!(parse("なんだこれ"), None);
        assert_eq!(parse("H4"), None);
    }

    #[test]
    fn 単独の音階は範囲にしない() {
        assert_eq!(ranges(&[72]), ["C5"]);
    }

    #[test]
    fn 連続は畳む() {
        assert_eq!(ranges(&[48, 49, 50]), ["C3-D3"]);
    }

    /// 飛びを潰さない。潰すと、録っていない音階を宣言してしまう。
    #[test]
    fn 飛びは区間を分ける() {
        assert_eq!(ranges(&[48, 49, 60]), ["C3-C#3", "C4"]);
    }

    #[test]
    fn 並びと重複に依存しない() {
        assert_eq!(ranges(&[50, 48, 49, 50]), ["C3-D3"]);
        assert!(ranges(&[]).is_empty());
    }
}
