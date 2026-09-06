//! 課題曲（`TR-RCL-12`, `TR-RCL-17`, `TR-RCL-19`, `TR-SYN-17`, `TR-SYN-18`）。
//!
//! 曲バンクを持たない（`TR-RCL-12`）。同梱するのは初回のとっかかりに要る
//! 最小限に限り、パブリックドメインの伝承曲だけ。
//! 主経路は本人が持ち込む UST / USTX。
//!
//! # なぜ曲を置くのか
//!
//! 「あと N 項目録ると『さくらさくら』が歌えるようになる」は、
//! 録り始めのとっかかりとして最も効く指標（`TR-RCL-19`）。
//! ただし唯一の指標ではない。 曲を1本も入れていないプロジェクトでも進捗は読める。
//!
//! # 出さないもの
//!
//! 品質スコア、良し悪しの判定、他音源との比較、上達度（`TR-SYN-20`）。
//! 不足は「エイリアス名の一覧」ではなく「あと N 項目で『曲名』が歌える」の形で出す。

use std::collections::BTreeSet;

use crate::alias::{self, Method};
use crate::inventory::UnitSet;
use crate::mora::{self, Mora};

/// 曲の1ノート（`TR-RCL-12` (a)(b)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// 歌詞（1モーラぶん）。
    pub lyric: String,
    pub midi: i32,
    /// 長さ（ティック）。UST の 480 ティック = 4分音符。
    pub ticks: u32,
}

/// 曲の出典と許諾（`TR-RCL-12` (f)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    /// どこから来たか。
    pub source: String,
    pub license: String,
}

/// 課題曲（`TR-RCL-12`）。
///
/// 持ち込んだ曲データは配布パッケージに含めない（`TR-RCL-12`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Song {
    pub title: String,
    pub notes: Vec<Note>,
    /// 出典と許諾。
    pub provenance: Provenance,
}

impl Song {
    /// 曲全体の最低音・最高音（`TR-RCL-12` (c)）。
    #[must_use]
    pub fn range(&self) -> Option<(i32, i32)> {
        let lo = self.notes.iter().map(|n| n.midi).min()?;
        let hi = self.notes.iter().map(|n| n.midi).max()?;
        Some((lo, hi))
    }

    /// 総モーラ数（`TR-RCL-12` (d)）。
    ///
    /// 長音と促音も数に入る。 収録単位は要求しないが、拍としては存在する。
    #[must_use]
    pub fn total_moras(&self, set: UnitSet) -> usize {
        self.moras(set).map_or(0, |m| m.len())
    }

    /// 歌詞のモーラ列。
    ///
    /// 読めない歌詞があれば `None`。 一部だけ読めた形で先へ進めない。
    #[must_use]
    pub fn moras(&self, set: UnitSet) -> Option<Vec<Mora>> {
        let text: String = self.notes.iter().map(|n| n.lyric.as_str()).collect();
        mora::parse(&text, set).ok()
    }

    /// 方式ごとの必要エイリアス集合（`TR-RCL-12` (e), `TR-RCL-15`, `TR-SYN-17`）。
    ///
    /// 「録音済みサンプルが1件も無い状態」で走らせて事前に算出する（`TR-SYN-17`）。
    #[must_use]
    pub fn required_aliases(&self, method: Method, set: UnitSet) -> BTreeSet<String> {
        self.moras(set)
            .map(|m| alias::required_aliases(method, &m, set))
            .unwrap_or_default()
    }
}

/// 曲がいまどう鳴るか（`TR-RCL-19`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Singability {
    /// 必要単位がすべて収録済み。
    Complete,
    /// 一部が未収録だが、フォールバックで解決すれば全ノートが鳴る。
    ///
    /// 音のつながりが粗くなることを画面で1行説明する。
    WithFallback,
    /// フォールバックでも解決できない音符がある。
    Unavailable,
}

impl Singability {
    /// 画面と IPC へ渡す識別子。
    ///
    /// `Debug` を wire 形式にしない。 variant を改名すると、
    /// TypeScript 側のリテラル union が黙って外れる。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Complete => "Complete",
            Self::WithFallback => "WithFallback",
            Self::Unavailable => "Unavailable",
        }
    }

    /// 「歌える」に含めてよいか（`TR-RCL-19`）。
    #[must_use]
    pub const fn is_singable(self) -> bool {
        matches!(self, Self::Complete | Self::WithFallback)
    }
}

/// 曲ごとの状態（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
#[derive(Debug, Clone, PartialEq)]
pub struct SongStatus {
    /// バンクの中でこの曲を指す識別子。
    ///
    /// 並び順は指定の手段にしない。 ここが返す並びは「手が届く順」で、
    /// バンクの保持順とは違う。位置で指すと、別の曲を指す。
    pub id: String,
    pub title: String,
    /// いまどう鳴るか。
    pub singability: Singability,
    /// 必要単位のうち収録済みの数。
    pub covered: usize,
    /// 必要単位の数。
    pub required: usize,
    /// あと何項目録れば完全になるか（`TR-SYN-20`）。
    ///
    /// エイリアス名の一覧ではなく、この数で出す。
    pub missing_units: usize,
    /// あと何行録れば完全になるか（`TR-RCL-16`, `TR-RCL-17`）。
    ///
    /// フルリストの行の部分集合として数える。詰め直さない。
    pub missing_rows: usize,
    /// その行を録るのに掛かる推定時間（秒、`TR-RCL-09`）。
    pub seconds: f64,
    /// 総モーラ数。同数のときの並べ替えに使う（`TR-RCL-17`）。
    pub total_moras: usize,
}

/// すべての曲の状態を出す（`TR-RCL-17`）。
///
/// 追加項目数が同じ曲は、総モーラ数の少ない順に並べる（`TR-RCL-17`）。
/// 短い曲のほうが、最初の1曲としては手が届く。
#[must_use]
pub fn status_of(
    songs: &[(String, Song)],
    method: Method,
    recorded: &BTreeSet<String>,
    set: UnitSet,
    full_list: &[crate::reclist::Row],
) -> Vec<SongStatus> {
    let mut out: Vec<SongStatus> = songs
        .iter()
        .map(|(id, song)| {
            let required = song.required_aliases(method, set);
            let covered = required.intersection(recorded).count();
            let missing = required.len().saturating_sub(covered);

            // フォールバックで全ノートが鳴るか（`TR-RCL-19` の「代替あり」）。
            let singability = if missing == 0 {
                Singability::Complete
            } else {
                let resolvable = song.moras(set).is_some_and(|m| {
                    alias::resolve_phrase(method, &m, recorded, set)
                        .iter()
                        .all(alias::PhraseUnit::is_playable)
                });
                if resolvable {
                    Singability::WithFallback
                } else {
                    Singability::Unavailable
                }
            };

            // あと何行かを、フルリストの部分集合として数える（`TR-RCL-16`）。
            let still: BTreeSet<String> = required.difference(recorded).cloned().collect();
            let plan = crate::plan::rows_to_cover(&still, full_list);

            SongStatus {
                id: id.clone(),
                title: song.title.clone(),
                singability,
                covered,
                required: required.len(),
                missing_units: missing,
                missing_rows: plan.rows.len(),
                seconds: plan.seconds,
                total_moras: song.total_moras(set),
            }
        })
        .collect();

    // 手が届く順。追加項目が少ない順、同数なら短い順。
    out.sort_by(|a, b| {
        a.missing_units
            .cmp(&b.missing_units)
            .then(a.total_moras.cmp(&b.total_moras))
            .then(a.title.cmp(&b.title))
    });
    out
}

/// いま歌える曲の数（`TR-RCL-19`）。
///
/// カバレッジと常に両方出す。どちらかを隠さない。
#[must_use]
pub fn singable_count(status: &[SongStatus]) -> usize {
    status
        .iter()
        .filter(|s| s.singability.is_singable())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(lyric: &str, midi: i32) -> Note {
        Note {
            lyric: lyric.to_owned(),
            midi,
            ticks: 480,
        }
    }

    fn song(title: &str, lyrics: &[&str]) -> Song {
        Song {
            title: title.to_owned(),
            notes: lyrics
                .iter()
                .enumerate()
                .map(|(i, l)| note(l, 60 + i32::try_from(i % 5).unwrap_or(0)))
                .collect(),
            provenance: Provenance {
                source: "テスト".to_owned(),
                license: "PD".to_owned(),
            },
        }
    }

    fn have(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    fn full_list() -> Vec<crate::reclist::Row> {
        crate::reclist::generate_single(UnitSet::Core, 5).expect("生成できる")
    }

    #[test]
    fn 音域と総モーラ数を出す() {
        let s = song("test", &["さ", "く", "ら"]);
        assert_eq!(s.range(), Some((60, 62)));
        assert_eq!(s.total_moras(UnitSet::Core), 3);
    }

    #[test]
    fn 長音は総モーラ数に入るが単位を要求しない() {
        let s = song("test", &["か", "ー"]);
        assert_eq!(s.total_moras(UnitSet::Core), 2, "拍としては2つ");
        assert_eq!(
            s.required_aliases(Method::Single, UnitSet::Core),
            have(&["か"]),
            "単位は1つ"
        );
    }

    /// 全部持っていれば完全（`TR-RCL-19` (1)）。
    #[test]
    fn 全部揃えば完全() {
        let s = song("さくら", &["さ", "く", "ら"]);
        let got = status_of(
            std::slice::from_ref(&("s1".to_owned(), s.clone())),
            Method::Single,
            &have(&["さ", "く", "ら"]),
            UnitSet::Core,
            &full_list(),
        );
        assert_eq!(got[0].singability, Singability::Complete);
        assert_eq!(got[0].missing_units, 0);
        assert_eq!(singable_count(&got), 1);
    }

    /// 足りなければ不可（単独音にはフォールバックが無い。`TR-SYN-12`）。
    #[test]
    fn 単独音で足りなければ不可() {
        let s = song("さくら", &["さ", "く", "ら"]);
        let got = status_of(
            std::slice::from_ref(&("s1".to_owned(), s.clone())),
            Method::Single,
            &have(&["さ", "ら"]),
            UnitSet::Core,
            &full_list(),
        );
        assert_eq!(got[0].singability, Singability::Unavailable);
        assert_eq!(got[0].missing_units, 1);
        assert_eq!(singable_count(&got), 0);
    }

    /// 連続音は単独音で録ったもので代替できる（`TR-SYN-12` の第3候補）。
    #[test]
    fn 連続音は代替ありになる() {
        let s = song("さくら", &["さ", "く", "ら"]);
        // 連続音の第一候補（`- さ` / `a く` / `u ら`）は持っていないが、
        // 素の `さ` `く` `ら` は持っている。
        let got = status_of(
            std::slice::from_ref(&("s1".to_owned(), s.clone())),
            Method::Sequential,
            &have(&["さ", "く", "ら"]),
            UnitSet::Core,
            &full_list(),
        );
        assert_eq!(got[0].singability, Singability::WithFallback);
        assert!(got[0].missing_units > 0, "必要集合は満たしていない");
        assert!(got[0].singability.is_singable(), "それでも歌える");
    }

    /// 追加項目が少ない順、同数なら短い順（`TR-RCL-17`）。
    #[test]
    fn 手が届く順に並ぶ() {
        let near = song("近い", &["さ", "く"]);
        let far = song("遠い", &["な", "に", "ぬ", "ね"]);
        let short_tie = song("短い", &["は"]);

        let got = status_of(
            &[
                ("far".to_owned(), far),
                ("near".to_owned(), near),
                ("short".to_owned(), short_tie),
            ],
            Method::Single,
            &have(&["さ", "く"]),
            UnitSet::Core,
            &full_list(),
        );
        assert_eq!(got[0].title, "近い", "0項目で歌える");
        assert_eq!(got[1].title, "短い", "1項目。同数なら短い順");
        assert_eq!(got[2].title, "遠い", "4項目");
    }

    /// エイリアス名の一覧ではなく件数で出す（`TR-SYN-20`）。
    #[test]
    fn 不足は件数で出す() {
        let s = song("さくら", &["さ", "く", "ら"]);
        let got = status_of(
            std::slice::from_ref(&("s1".to_owned(), s.clone())),
            Method::Single,
            &BTreeSet::new(),
            UnitSet::Core,
            &full_list(),
        );
        assert_eq!(got[0].missing_units, 3);
        assert_eq!(got[0].covered, 0);
        assert_eq!(got[0].required, 3);
        // 行数でも出せる（`TR-RCL-16`, `TR-RCL-17`）。
        assert!(got[0].missing_rows > 0, "あと何行かも数えること");
        assert!(got[0].seconds > 0.0, "所要時間も出すこと");
    }

    /// 読めない歌詞の曲は必要集合が空になる。 一部だけ読めた形で先へ進めない。
    #[test]
    fn 読めない歌詞は先へ進めない() {
        let s = song("読めない", &["さ", "X"]);
        assert_eq!(s.moras(UnitSet::Core), None);
        assert!(s.required_aliases(Method::Single, UnitSet::Core).is_empty());
    }
}
