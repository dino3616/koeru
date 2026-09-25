//! 録り直したテイクへ、固定した値を写す（`REQ-ALN-007`, `DEC-ALN-019`）。
//!
//! 5値のうち左ブランクだけがファイル先頭からの絶対位置で、残りは左ブランクからの
//! 相対（[`Oto`]）。 発声が始まる時刻はテイクごとに違うので、固定した左ブランクを
//! そのまま写すと、新しい録音の別の場所を指す。
//!
//! 左ブランクは「発声の始まりからの距離」として写す。 相対の4値は写した左ブランクに
//! 付いていくので、固定していればそのまま写す。 正の右ブランク（ファイル末尾から）も
//! 末尾からの距離としてそのまま写す。
//!
//! どちらかのテイクで発声の始まりが分からなければ、絶対位置のまま写す。
//! 黙って当て直したことにしない——呼び出し側は [`Anchor::Absolute`] を見て確認へ戻す。

use std::collections::BTreeMap;

use crate::oto::{Boundary, Oto};
use crate::reclist::Slot;

/// 前の世代で固定していた値（`REQ-ALN-007`）。
///
/// 添字で持たない。 並びを取り違えると、固定した子音部が右ブランクへ写る。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "5値のそれぞれが独立に固定される。状態機械ではない"
)]
pub struct Pins {
    /// 左ブランク。
    pub offset: bool,
    /// 子音部（固定範囲）。
    pub consonant: bool,
    /// 右ブランク。
    pub cutoff: bool,
    /// 先行発声。
    pub preutterance: bool,
    /// オーバーラップ。
    pub overlap: bool,
}

/// 2つのテイクで、その綴りが乗るモーラの発声の始まり（ミリ秒）。
///
/// 境界を残していないテイク（`TR-ALN-34` より前に推定したもの、揃えられなかったもの）は
/// `None`。
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Onsets {
    /// 固定した値を持っていた前の世代。
    pub previous: Option<f64>,
    /// 新しく録ったテイク。
    pub next: Option<f64>,
}

/// 左ブランクをどう写したか。
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Anchor {
    /// 左ブランクは固定していない。 新しいテイクの値のまま。
    NotPinned,
    /// 発声の始まりの差だけ動かした。
    Shifted {
        /// 動かした量（ミリ秒）。 後ろへ動かしたら正。
        by_ms: f64,
    },
    /// 揃えられなかったので、絶対位置のまま写した。 確認へ戻す。
    Absolute(Unanchored),
}

/// 当て直せなかった理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unanchored {
    /// 前の世代の発声の始まりが分からない。
    PreviousOnsetUnknown,
    /// 新しいテイクの発声の始まりが分からない。
    NextOnsetUnknown,
    /// 当て直すとファイルの先頭より前を指す。 左ブランクは 0 以上（`TR-ALN-14`）。
    BeforeFileStart,
}

impl Unanchored {
    /// トレースへ載せてよい固定文字列。
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PreviousOnsetUnknown => "previous_onset_unknown",
            Self::NextOnsetUnknown => "next_onset_unknown",
            Self::BeforeFileStart => "before_file_start",
        }
    }
}

/// 写した5値と、左ブランクの写し方。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Carried {
    /// 新しいテイクへ置く5値。
    pub oto: Oto,
    /// 左ブランクの写し方。
    pub anchor: Anchor,
}

/// 固定した値を新しいテイクへ写す（`DEC-ALN-019`）。
///
/// 固定していない値は `fresh`（新しい録音から導いたもの）のまま。
#[must_use]
pub fn carry(previous: &Oto, fresh: &Oto, pins: Pins, onsets: Onsets) -> Carried {
    let pick = |pinned: bool, prev: f64, now: f64| if pinned { prev } else { now };
    let (offset_ms, anchor) = if pins.offset {
        match (onsets.previous, onsets.next) {
            (None, _) => (
                previous.offset_ms,
                Anchor::Absolute(Unanchored::PreviousOnsetUnknown),
            ),
            (_, None) => (
                previous.offset_ms,
                Anchor::Absolute(Unanchored::NextOnsetUnknown),
            ),
            (Some(was), Some(now)) => {
                let by_ms = now - was;
                let moved = previous.offset_ms + by_ms;
                if moved < 0.0 {
                    (
                        previous.offset_ms,
                        Anchor::Absolute(Unanchored::BeforeFileStart),
                    )
                } else {
                    (moved, Anchor::Shifted { by_ms })
                }
            }
        }
    } else {
        (fresh.offset_ms, Anchor::NotPinned)
    };
    Carried {
        oto: Oto {
            offset_ms,
            consonant_ms: pick(pins.consonant, previous.consonant_ms, fresh.consonant_ms),
            cutoff_ms: pick(pins.cutoff, previous.cutoff_ms, fresh.cutoff_ms),
            preutterance_ms: pick(
                pins.preutterance,
                previous.preutterance_ms,
                fresh.preutterance_ms,
            ),
            overlap_ms: pick(pins.overlap, previous.overlap_ms, fresh.overlap_ms),
        },
        anchor,
    }
}

/// 行の綴りごとに、乗るモーラの発声の始まり（`DEC-ALN-019`）。
///
/// 境界はモーラの CV の綴りで残っている（`TR-ALN-34`）。 同じ CV が行に2度出ると、
/// 綴りは先に出たモーラの1つしか残らない（[`crate::reclist::row_entries`] が重ねない）。
/// そのモーラに乗る綴りは、別のモーラの始まりを借りずに外す——外したものは
/// 絶対位置のまま写って確認へ戻る。
#[must_use]
pub fn onsets(entries: &[(String, Slot)], saved: &[(String, Boundary)]) -> BTreeMap<String, f64> {
    let by_alias: BTreeMap<&str, &Boundary> = saved.iter().map(|(a, b)| (a.as_str(), b)).collect();
    let by_mora: BTreeMap<usize, f64> = entries
        .iter()
        .filter_map(|(a, slot)| match *slot {
            Slot::Cv { mora } => Some((mora, by_alias.get(a.as_str())?.voice_start_ms)),
            _ => None,
        })
        .collect();
    entries
        .iter()
        .filter_map(|(a, slot)| Some((a.clone(), *by_mora.get(&slot.mora())?)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PREV: Oto = Oto {
        offset_ms: 123.0,
        consonant_ms: 80.0,
        cutoff_ms: -250.0,
        preutterance_ms: 45.0,
        overlap_ms: 15.0,
    };
    const FRESH: Oto = Oto {
        offset_ms: 50.0,
        consonant_ms: 60.0,
        cutoff_ms: -300.0,
        preutterance_ms: 40.0,
        overlap_ms: 20.0,
    };

    fn all() -> Pins {
        Pins {
            offset: true,
            consonant: true,
            cutoff: true,
            preutterance: true,
            overlap: true,
        }
    }

    /// 発声が 100ms 遅れたテイクでは、固定した左ブランクも 100ms 後ろを指す。
    /// 相対の4値は左ブランクに付いていくので、そのまま写る。
    #[test]
    fn 発声の始まりに合わせて左ブランクを動かす() {
        let got = carry(
            &PREV,
            &FRESH,
            all(),
            Onsets {
                previous: Some(130.0),
                next: Some(230.0),
            },
        );
        assert_eq!(got.anchor, Anchor::Shifted { by_ms: 100.0 });
        assert_eq!(
            got.oto,
            Oto {
                offset_ms: 223.0,
                ..PREV
            }
        );
    }

    /// 固定していない値は新しい録音のもの。 2つの録音の値が混ざるのは、
    /// 固定した値の分だけ。
    #[test]
    fn 固定していない値は新しい録音から取る() {
        let got = carry(
            &PREV,
            &FRESH,
            Pins {
                offset: true,
                preutterance: true,
                ..Pins::default()
            },
            Onsets {
                previous: Some(130.0),
                next: Some(110.0),
            },
        );
        assert_eq!(
            got.oto,
            Oto {
                offset_ms: 103.0,
                preutterance_ms: PREV.preutterance_ms,
                ..FRESH
            }
        );
    }

    /// 左ブランクを固定していなければ、当て直すものが無い。 発声の始まりが
    /// 分からなくても確認へ戻す理由にしない。
    #[test]
    fn 左ブランクを固定していなければ当て直さない() {
        let got = carry(
            &PREV,
            &FRESH,
            Pins {
                consonant: true,
                ..Pins::default()
            },
            Onsets::default(),
        );
        assert_eq!(got.anchor, Anchor::NotPinned);
        assert_eq!(
            got.oto,
            Oto {
                consonant_ms: PREV.consonant_ms,
                ..FRESH
            }
        );
    }

    /// 揃えられなければ絶対位置のまま写し、なぜかを返す。
    #[test]
    fn 発声の始まりが分からなければ絶対位置のまま写す() {
        for (onsets, why) in [
            (
                Onsets {
                    previous: None,
                    next: Some(230.0),
                },
                Unanchored::PreviousOnsetUnknown,
            ),
            (
                Onsets {
                    previous: Some(130.0),
                    next: None,
                },
                Unanchored::NextOnsetUnknown,
            ),
        ] {
            let got = carry(&PREV, &FRESH, all(), onsets);
            assert_eq!(got.anchor, Anchor::Absolute(why));
            assert_eq!(got.oto, PREV, "{why:?}");
        }
    }

    /// 当て直すとファイルの先頭より前を指すなら、当て直さない。
    /// 0 へ寄せると、固定した値ではない位置を固定したことになる。
    #[test]
    fn 先頭より前を指すなら絶対位置のまま写す() {
        let got = carry(
            &PREV,
            &FRESH,
            all(),
            Onsets {
                previous: Some(300.0),
                next: Some(100.0),
            },
        );
        assert_eq!(got.anchor, Anchor::Absolute(Unanchored::BeforeFileStart));
        assert_eq!(got.oto.offset_ms, PREV.offset_ms);
    }

    /// 正の右ブランクはファイル末尾からの距離。 左ブランクを動かしても変えない。
    #[test]
    fn 末尾からの右ブランクはそのまま写す() {
        let prev = Oto {
            cutoff_ms: 200.0,
            ..PREV
        };
        let got = carry(
            &prev,
            &FRESH,
            all(),
            Onsets {
                previous: Some(130.0),
                next: Some(230.0),
            },
        );
        assert!((got.oto.cutoff_ms - 200.0).abs() < f64::EPSILON);
    }

    fn at(voice_start_ms: f64) -> Boundary {
        Boundary {
            voice_start_ms,
            vowel_start_ms: voice_start_ms + 20.0,
            vowel_end_ms: voice_start_ms + 120.0,
        }
    }

    /// 渡りは直前のモーラの始まり、語尾はそのモーラの始まりに合わせる。
    #[test]
    fn 綴りが乗るモーラの始まりを引く() {
        let entries = vec![
            ("- か".to_owned(), Slot::Cv { mora: 0 }),
            ("a さ".to_owned(), Slot::Cv { mora: 1 }),
            ("a s".to_owned(), Slot::Vc { prev: 0, next: 1 }),
            ("a R".to_owned(), Slot::Ending { mora: 1 }),
        ];
        let saved = vec![
            ("- か".to_owned(), at(100.0)),
            ("a さ".to_owned(), at(400.0)),
        ];
        let got = onsets(&entries, &saved);
        assert_eq!(got.get("- か"), Some(&100.0));
        assert_eq!(got.get("a s"), Some(&100.0), "渡りは直前のモーラに乗る");
        assert_eq!(got.get("a R"), Some(&400.0));
    }

    /// 同じ CV が2度出た行では、後ろのモーラの始まりが残っていない。
    /// 先のモーラの始まりを借りると、別の場所へ当て直す。
    #[test]
    fn 重なって消えたモーラの始まりを借りない() {
        let entries = vec![
            ("- か".to_owned(), Slot::Cv { mora: 0 }),
            ("a か".to_owned(), Slot::Cv { mora: 1 }),
            // 3モーラ目の `a か` は重ねないので、並びに出ない。
            ("a R".to_owned(), Slot::Ending { mora: 2 }),
        ];
        let saved = vec![
            ("- か".to_owned(), at(100.0)),
            ("a か".to_owned(), at(400.0)),
        ];
        let got = onsets(&entries, &saved);
        assert_eq!(got.get("a R"), None);
        assert_eq!(got.len(), 2);
    }

    /// 境界を残していないテイクは、どの綴りも始まりを持たない。
    #[test]
    fn 境界が無ければ始まりも無い() {
        let entries = vec![("か".to_owned(), Slot::Cv { mora: 0 })];
        assert!(onsets(&entries, &[]).is_empty());
    }
}
