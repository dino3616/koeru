//! 同梱する伝承曲（`TR-RCL-12`）。
//!
//! UST / USTX の構文は `koeru_formats::ust` へ移した（`DEC-PLT-034`）。 下の再輸出は
//! 既存の `koeru_core::ust` の経路を通すためだけにある。 **移行中。**
//! 同梱曲は構文ではなく曲のデータなので、ここに残っている。

pub use koeru_formats::ust::{Format, UstError, parse_file, parse_ust, title_hint};

use crate::song::{Note, Provenance, Song};

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
    use super::*;
    use crate::alias::Method;
    use crate::inventory::UnitSet;

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
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

    /// 同梱曲はどれも1音符1モーラ。 取り込みの検査で同梱曲が落ちないこと。
    #[test]
    fn 同梱曲は一音符一モーラ() {
        for s in bundled_songs() {
            assert_eq!(s.note_not_one_mora(UnitSet::Core), None, "{}", s.title);
        }
    }

    /// 内部形式はテンポと既定ピッチベンドを持つ（`TR-SYN-30`）。
    ///
    /// UST / USTX は読み込みの入口であって内部形式ではない。 テンポを
    /// 落とすと、読み込んだ曲がどれも同じ速さで鳴る。
    #[test]
    fn 内部形式はテンポを持つ() {
        let s = bundled_songs();
        assert_eq!(s.len(), 1, "同梱は最小限（`TR-RCL-12`）");
        assert!(s[0].tempo_bpm > 0.0);
        assert!(s[0].default_portamento_ms >= 0.0);
    }

    /// 同梱曲はパブリックドメイン（`TR-SYN-30`）。
    ///
    /// > 同梱する課題曲は、権利処理済みのオリジナル、またはパブリックドメインの
    /// > 旋律に限り、最小限（1〜2曲）にとどめる
    #[test]
    fn 同梱曲は出典と許諾を持つ() {
        for s in bundled_songs() {
            assert!(!s.provenance.source.trim().is_empty(), "{}", s.title);
            assert!(!s.provenance.license.trim().is_empty(), "{}", s.title);
        }
    }
}
