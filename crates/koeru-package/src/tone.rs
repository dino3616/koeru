//! 音階の名前（`TR-PKG-04`, `TR-PKG-29`）。
//!
//! `prefix.map` は1行1音階で音階名を書き、`character.yaml` の
//! `subbanks.tone_ranges` は `"C3-C4"` か `"C5"` の形で書く。
//! どちらも同じ割り当てを表すので、名前の作り方はここ1箇所が持つ。
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

#[cfg(test)]
mod tests {
    use super::*;

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
