//! 書き出し前の検証と自動修復（`TR-ALN-20`）。
//!
//! **修復できるものは直し、直せないものは書き出しを止める。**
//!
//! > 全エントリに対して書き出し前に次を検証し、違反があれば規約の範囲内で自動修復する。
//! > 修復できない場合はそのエントリを確認キューへ回し書き出しをブロックする。
//!
//! # 検証する6点
//!
//! | | 条件 | 直せるか |
//! |---|---|---|
//! | (1) | 右ブランクの位置と子音部の間隔が 1ms 以上 | 子音部を縮めて直す |
//! | (2) | オフセット・先行発声・子音部が 0 以上 | 0 へクリップして直す |
//! | (3) | 先行発声 ≦ 子音部 | 子音部を先行発声まで伸ばして直す |
//! | (4) | 子音部の位置 < 右ブランクの位置 | (1) と同じ |
//! | (5) | 切り出し範囲が WAV 長を超えない | 右ブランクを WAV 末尾へ寄せて直す |
//! | (6) | 同一 WAV 内でエイリアスが重複しない | **直せない** |
//!
//! # なぜ (6) だけ直せないのか
//!
//! **どちらのエントリを残すか、機械には決められない。** 名前を機械的に付け替えると
//! 「同名で意味の違う成果物」を作ることになり（`TR-PKG-46`）、
//! 外部ツールで開いたときに何が起きたか説明できなくなる。
//!
//! # 直したことは黙らない
//!
//! [`Repair`] が「何をどう直したか」を返す。**直った事実を持っておかないと、
//! 上級モードで「なぜこの値になっているのか」が説明できない。**

use koeru_core::oto::Oto;

/// 右ブランクの位置と子音部の間隔の下限（`TR-ALN-20` (1)）。
pub const MIN_GAP_MS: f64 = 1.0;

/// 検証で見つかった問題（`TR-ALN-20`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Issue {
    /// (1)(4) 子音部が右ブランクに近すぎる、または追い越している。
    ConsonantTooCloseToCutoff,
    /// (2) オフセット・先行発声・子音部のどれかが負。
    NegativeValue,
    /// (3) 先行発声が子音部より右にある。
    PreutteranceBeyondConsonant,
    /// (5) 切り出し範囲が WAV 長を超えている。
    RegionBeyondFile,
    /// (6) 同一 WAV 内でエイリアスが重複している。**直せない。**
    DuplicateAlias,
}

impl Issue {
    /// 送信してよい固定文字列。
    #[must_use]
    pub const fn kind(self) -> &'static str {
        match self {
            Self::ConsonantTooCloseToCutoff => "validate.consonant_too_close_to_cutoff",
            Self::NegativeValue => "validate.negative_value",
            Self::PreutteranceBeyondConsonant => "validate.preutterance_beyond_consonant",
            Self::RegionBeyondFile => "validate.region_beyond_file",
            Self::DuplicateAlias => "validate.duplicate_alias",
        }
    }

    /// 規約の範囲内で自動修復できるか（`TR-ALN-20`）。
    #[must_use]
    pub const fn is_repairable(self) -> bool {
        !matches!(self, Self::DuplicateAlias)
    }
}

/// 修復の結果。
#[derive(Debug, Clone, PartialEq)]
pub struct Repair {
    /// 直したあとの5値。
    pub oto: Oto,
    /// 直した項目。**何もなければ空。**
    pub fixed: Vec<Issue>,
    /// 直せなかった項目。**あれば書き出しを止める。**
    pub unrepairable: Vec<Issue>,
}

impl Repair {
    /// 書き出してよいか。
    #[must_use]
    pub fn may_export(&self) -> bool {
        self.unrepairable.is_empty()
    }
}

/// 1エントリを検証して、直せるものを直す（`TR-ALN-20`）。
///
/// `duplicate_alias` は、呼び出し側が同一 WAV 内の重複を見て渡す。
/// **ここは1エントリしか見ないので、重複は判定できない**（[`find_duplicate_aliases`] を使う）。
#[must_use]
pub fn repair(oto: &Oto, file_len_ms: f64, duplicate_alias: bool) -> Repair {
    let mut o = *oto;
    let mut fixed = Vec::new();
    let mut unrepairable = Vec::new();

    if duplicate_alias {
        unrepairable.push(Issue::DuplicateAlias);
    }

    // (2) 負の値を 0 へクリップする。**オーバーラップだけは負を許す**（TR-ALN-16）。
    if o.offset_ms < 0.0 || o.preutterance_ms < 0.0 || o.consonant_ms < 0.0 {
        o.offset_ms = o.offset_ms.max(0.0);
        o.preutterance_ms = o.preutterance_ms.max(0.0);
        o.consonant_ms = o.consonant_ms.max(0.0);
        fixed.push(Issue::NegativeValue);
    }

    // (5) 切り出し範囲が WAV 長を超えないようにする。
    if o.offset_ms > file_len_ms {
        o.offset_ms = file_len_ms;
        fixed.push(Issue::RegionBeyondFile);
    }
    let available = (file_len_ms - o.offset_ms).max(0.0);
    if o.usable_ms(file_len_ms) > available {
        // **負値表現へ寄せる。** 正値表現のまま縮めると、
        // ファイル末尾からの距離という意味が変わってしまう。
        o.cutoff_ms = -available;
        if !fixed.contains(&Issue::RegionBeyondFile) {
            fixed.push(Issue::RegionBeyondFile);
        }
    }
    let usable = o.usable_ms(file_len_ms);

    // (3) 先行発声 ≦ 子音部。**子音部を伸ばして合わせる。**
    // 先行発声を縮めると、母音の開始位置という意味が壊れる。
    if o.preutterance_ms > o.consonant_ms {
        o.consonant_ms = o.preutterance_ms;
        fixed.push(Issue::PreutteranceBeyondConsonant);
    }

    // (1)(4) 子音部は右ブランクより 1ms 以上手前。**子音部を縮めて直す。**
    if o.consonant_ms > usable - MIN_GAP_MS {
        o.consonant_ms = (usable - MIN_GAP_MS).max(0.0);
        fixed.push(Issue::ConsonantTooCloseToCutoff);
        // 縮めた結果 (3) が崩れることがある。**先行発声も一緒に引く。**
        if o.preutterance_ms > o.consonant_ms {
            o.preutterance_ms = o.consonant_ms;
            if !fixed.contains(&Issue::PreutteranceBeyondConsonant) {
                fixed.push(Issue::PreutteranceBeyondConsonant);
            }
        }
    }

    // **直したのに使える区間が残らなかったら、それは直せていない。**
    if o.usable_ms(file_len_ms) <= 0.0 {
        unrepairable.push(Issue::RegionBeyondFile);
    }

    Repair {
        oto: o,
        fixed,
        unrepairable,
    }
}

/// 同一 WAV 内で重複しているエイリアスを返す（`TR-ALN-20` (6)）。
///
/// **大文字小文字を無視する**（`TR-REC-34` が oto セット内のファイル名に課しているのと同じ規律。
/// 大小だけ違う名前は、Windows と macOS で別物になったり同じ物になったりする）。
#[must_use]
pub fn find_duplicate_aliases<'a>(aliases: &[&'a str]) -> Vec<&'a str> {
    let mut seen = std::collections::BTreeMap::<String, usize>::new();
    for a in aliases {
        *seen.entry(a.to_lowercase()).or_default() += 1;
    }
    let mut dup: Vec<&str> = aliases
        .iter()
        .filter(|a| seen[&a.to_lowercase()] > 1)
        .copied()
        .collect();
    dup.sort_unstable();
    dup.dedup();
    dup
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> Oto {
        Oto {
            offset_ms: 80.0,
            consonant_ms: 100.0,
            cutoff_ms: -520.0,
            preutterance_ms: 70.0,
            overlap_ms: 23.0,
        }
    }

    #[test]
    fn 正しい五値は何も直さない() {
        let r = repair(&base(), 1000.0, false);
        assert!(r.fixed.is_empty(), "{:?}", r.fixed);
        assert!(r.may_export());
        assert_eq!(r.oto, base());
    }

    /// (2) 負の値は 0 へ倒す。
    #[test]
    fn 負の値を直す() {
        let o = Oto {
            offset_ms: -5.0,
            preutterance_ms: -1.0,
            consonant_ms: -2.0,
            ..base()
        };
        let r = repair(&o, 1000.0, false);
        assert!(r.fixed.contains(&Issue::NegativeValue));
        assert_eq!(r.oto.offset_ms, 0.0);
        assert_eq!(r.oto.preutterance_ms, 0.0);
        assert!(r.may_export());
    }

    /// **オーバーラップの負は違反ではない**（`TR-ALN-16`）。
    #[test]
    fn 負のオーバーラップは直さない() {
        let o = Oto {
            overlap_ms: -12.0,
            ..base()
        };
        let r = repair(&o, 1000.0, false);
        assert!(r.fixed.is_empty());
        assert_eq!(r.oto.overlap_ms, -12.0);
    }

    /// (3) 先行発声が子音部を追い越したら、**子音部を伸ばす。**
    #[test]
    fn 先行発声が子音部を超えたら子音部を伸ばす() {
        let o = Oto {
            preutterance_ms: 200.0,
            consonant_ms: 100.0,
            ..base()
        };
        let r = repair(&o, 1000.0, false);
        assert!(r.fixed.contains(&Issue::PreutteranceBeyondConsonant));
        assert_eq!(r.oto.preutterance_ms, 200.0, "先行発声は動かさない");
        assert_eq!(r.oto.consonant_ms, 200.0);
    }

    /// (1)(4) 子音部は右ブランクより 1ms 以上手前。
    #[test]
    fn 子音部が右ブランクに近すぎたら縮める() {
        let o = Oto {
            consonant_ms: 520.0,
            cutoff_ms: -520.0,
            preutterance_ms: 10.0,
            ..base()
        };
        let r = repair(&o, 1000.0, false);
        assert!(r.fixed.contains(&Issue::ConsonantTooCloseToCutoff));
        assert_eq!(r.oto.consonant_ms, 519.0);
        assert!(r.may_export());
    }

    /// 縮めた結果 (3) が崩れたら、**先行発声も一緒に引く。**
    #[test]
    fn 縮めた結果の破れも直す() {
        let o = Oto {
            offset_ms: 0.0,
            consonant_ms: 90.0,
            preutterance_ms: 90.0,
            cutoff_ms: -50.0,
            overlap_ms: 0.0,
        };
        let r = repair(&o, 1000.0, false);
        assert_eq!(r.oto.consonant_ms, 49.0);
        assert_eq!(r.oto.preutterance_ms, 49.0);
        assert!(r.oto.violations(1000.0).is_empty());
        assert!(r.may_export());
    }

    /// (5) WAV 長を超える切り出しは末尾へ寄せる。
    #[test]
    fn ファイル長を超える範囲を詰める() {
        let o = Oto {
            offset_ms: 900.0,
            cutoff_ms: -500.0,
            consonant_ms: 10.0,
            preutterance_ms: 5.0,
            overlap_ms: 0.0,
        };
        let r = repair(&o, 1000.0, false);
        assert!(r.fixed.contains(&Issue::RegionBeyondFile));
        assert_eq!(r.oto.usable_ms(1000.0), 100.0, "残り 100ms へ詰める");
        assert!(r.may_export());
    }

    /// **オフセットがファイル末尾を越えていたら、使える区間が残らない。**
    #[test]
    fn 使える区間が残らないのは直せない() {
        let o = Oto {
            offset_ms: 1200.0,
            cutoff_ms: -10.0,
            consonant_ms: 0.0,
            preutterance_ms: 0.0,
            overlap_ms: 0.0,
        };
        let r = repair(&o, 1000.0, false);
        assert!(!r.may_export());
        assert!(r.unrepairable.contains(&Issue::RegionBeyondFile));
    }

    /// (6) エイリアスの重複は**直せない**。
    #[test]
    fn エイリアスの重複は直せない() {
        let r = repair(&base(), 1000.0, true);
        assert!(!r.may_export());
        assert_eq!(r.unrepairable, [Issue::DuplicateAlias]);
        assert!(!Issue::DuplicateAlias.is_repairable());
    }

    #[test]
    fn 重複するエイリアスを見つけられる() {
        assert_eq!(find_duplicate_aliases(&["a", "i", "u"]), Vec::<&str>::new());
        assert_eq!(find_duplicate_aliases(&["a", "i", "a"]), ["a"]);
    }

    /// **大文字小文字だけ違う名前も重複として扱う**（`TR-REC-34` と同じ規律）。
    #[test]
    fn 大文字小文字だけ違うのも重複() {
        assert_eq!(find_duplicate_aliases(&["Ka", "ka"]), ["Ka", "ka"]);
    }

    /// **直したあとは制約を満たしている。** ここが破れると、
    /// 修復が「別の違反を作る」だけになる。
    #[test]
    fn 直したあとは制約を満たす() {
        let cases = [
            Oto {
                offset_ms: -5.0,
                consonant_ms: 900.0,
                cutoff_ms: -100.0,
                preutterance_ms: 800.0,
                overlap_ms: -3.0,
            },
            Oto {
                offset_ms: 0.0,
                consonant_ms: 0.0,
                cutoff_ms: -1.0,
                preutterance_ms: 0.0,
                overlap_ms: 0.0,
            },
            Oto {
                offset_ms: 500.0,
                consonant_ms: 400.0,
                cutoff_ms: -800.0,
                preutterance_ms: 350.0,
                overlap_ms: 10.0,
            },
        ];
        for (i, o) in cases.iter().enumerate() {
            let r = repair(o, 1000.0, false);
            if r.may_export() {
                assert!(
                    r.oto.violations(1000.0).is_empty(),
                    "{i} 件目が直しきれていない: {:?}",
                    r.oto.violations(1000.0)
                );
                assert!(r.oto.consonant_ms <= r.oto.usable_ms(1000.0) - MIN_GAP_MS + 1e-9);
                assert!(r.oto.preutterance_ms <= r.oto.consonant_ms + 1e-9);
            }
        }
    }

    #[test]
    fn 問題の種別は固定文字列() {
        for i in [
            Issue::ConsonantTooCloseToCutoff,
            Issue::NegativeValue,
            Issue::PreutteranceBeyondConsonant,
            Issue::RegionBeyondFile,
            Issue::DuplicateAlias,
        ] {
            assert!(i.kind().starts_with("validate."));
        }
    }
}
