//! エイリアスの解決（`TR-SYN-12`, `TR-RCL-15`, `TR-RCL-20`）。
//!
//! **これが単一の定義。** カバレッジ判定・書き出し・試唱が同じコードパスを通る
//! （`TR-SYN-12`, `TR-RCL-20`）。3箇所で別々に解くと、
//! 「歌えると出たのに書き出したら鳴らない」が起きる。
//!
//! **OpenUtau の公開された振る舞いを仕様として参照し、自前実装する**（`TR-SYN-11`）。
//! コードは移植しない（`TR-PLT-10`）。
//!
//! # 抽象の形を保つ
//!
//! 「直前・直後の音符を参照して、音素列とエイリアス候補列を返す」という形を保つ
//! （`TR-SYN-11`）。同じ入力に同じ出力を返すことを検証できるようにするためであり、
//! **利用者が別の phonemizer を接続する差し替え点**にもなる（`TR-SYN-35`）。

use std::collections::BTreeSet;

use crate::inventory::{Unit, UnitSet, units};
use crate::mora::{Mora, MoraKind};

/// 収録方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// 単独音。
    Single,
    /// 連続音（VCV）。
    Sequential,
    /// CVVC。
    Cvvc,
}

/// 1つの音符に対する解決の要求。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request<'a> {
    /// この音符の歌詞（1モーラ）。
    pub lyric: &'a str,
    /// 直前の音符の末尾母音クラス。**無ければフレーズの先頭。**
    pub previous_vowel: Option<&'a str>,
}

/// 解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// 実際に使うエイリアス。
    pub alias: String,
    /// **候補の何番目で当たったか。** 0 が第一候補。
    ///
    /// 0 より大きければ「代替」として台帳に記録する（`TR-RCL-20`）。
    pub rank: usize,
}

/// 解決できなかった音符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    /// その歌詞。
    pub lyric: String,
    /// 試した候補。
    pub tried: Vec<String>,
}

/// 音符1つのエイリアス候補列を作る（`TR-SYN-12`）。
///
/// **順序がそのまま優先順位。** 先頭から順に、持っているものを探す。
#[must_use]
pub fn candidates(method: Method, req: &Request<'_>) -> Vec<String> {
    let lyric = req.lyric;
    match method {
        // 単独音: 「{歌詞}」だけ。見つからなければ欠損。
        Method::Single => vec![lyric.to_owned()],

        // 連続音(VCV)。
        Method::Sequential => match req.previous_vowel {
            Some(v) => vec![
                format!("{v} {lyric}"),
                format!("* {lyric}"),
                lyric.to_owned(),
                format!("- {lyric}"),
            ],
            None => vec![format!("- {lyric}"), lyric.to_owned()],
        },

        // CVVC の CV 部。**VC は該当が無ければ出力せず、CV だけで繋ぐ。**
        Method::Cvvc => vec![format!("- {lyric}"), lyric.to_owned()],
    }
}

/// 持っているエイリアスの集合から、1音符を解決する（`TR-SYN-12`）。
pub fn resolve(
    method: Method,
    req: &Request<'_>,
    available: &BTreeSet<String>,
) -> Result<Resolved, Missing> {
    let tried = candidates(method, req);
    for (rank, c) in tried.iter().enumerate() {
        if available.contains(c) {
            return Ok(Resolved {
                alias: c.clone(),
                rank,
            });
        }
    }
    Err(Missing {
        lyric: req.lyric.to_owned(),
        tried,
    })
}

/// モーラ列から、方式ごとの必要エイリアス集合を作る（`TR-RCL-15`）。
///
/// **第一候補だけを数える。** 「フォールバックすれば足りる」ものを必要集合に入れると、
/// 何を録れば完全になるのかが分からなくなる。
/// フォールバックで解決できるかは [`resolve`] が別に答える。
#[must_use]
pub fn required_aliases(method: Method, moras: &[Mora], set: UnitSet) -> BTreeSet<String> {
    let table = units(set);
    let mut out = BTreeSet::new();
    let mut prev_vowel: Option<String> = None;

    for m in moras {
        match m.kind {
            // **長音は直前母音の継続。** 新たなエイリアスを要求しない。
            MoraKind::LongVowel => continue,
            // **促音は単位を要求しない。** 直後の CV の子音部を要求するが、
            // その子音は次のモーラのエイリアスが持っている。
            MoraKind::Geminate => continue,
            MoraKind::Syllable | MoraKind::Moraic => {}
        }
        let Some(unit) = m.unit else { continue };

        let req = Request {
            lyric: unit,
            previous_vowel: prev_vowel.as_deref(),
        };
        if let Some(first) = candidates(method, &req).first() {
            out.insert(first.clone());
        }
        prev_vowel = vowel_of(&table, unit).map(str::to_owned);
    }
    out
}

/// その単位の母音クラス。
fn vowel_of<'a>(table: &'a [Unit], kana: &str) -> Option<&'a str> {
    table.iter().find(|u| u.kana == kana).map(|u| u.vowel)
}

/// フレーズ全体を解決する。
///
/// **解決できた音符と、できなかった音符の両方を返す。**
/// できなかったものを黙って飛ばすと、短縮版（`TR-SYN-18`）が作れない。
#[must_use]
pub fn resolve_phrase(
    method: Method,
    moras: &[Mora],
    available: &BTreeSet<String>,
    set: UnitSet,
) -> Vec<Result<Resolved, Missing>> {
    let table = units(set);
    let mut out = Vec::new();
    let mut prev_vowel: Option<String> = None;

    for m in moras {
        match m.kind {
            MoraKind::LongVowel | MoraKind::Geminate => continue,
            MoraKind::Syllable | MoraKind::Moraic => {}
        }
        let Some(unit) = m.unit else { continue };

        let req = Request {
            lyric: unit,
            previous_vowel: prev_vowel.as_deref(),
        };
        out.push(resolve(method, &req, available));
        prev_vowel = vowel_of(&table, unit).map(str::to_owned);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mora::parse;

    fn have(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    /// **単独音は「{歌詞}」だけ。** 見つからなければ欠損（TR-SYN-12）。
    #[test]
    fn 単独音の候補は一つだけ() {
        let req = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(candidates(Method::Single, &req), ["か"]);

        assert!(resolve(Method::Single, &req, &have(&["さ"])).is_err());
        assert_eq!(
            resolve(Method::Single, &req, &have(&["か"]))
                .expect("解決できる")
                .rank,
            0
        );
    }

    /// **連続音は直前の母音を見る**（TR-SYN-12）。
    #[test]
    fn 連続音の候補は要件どおりの順序() {
        let with_prev = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(
            candidates(Method::Sequential, &with_prev),
            ["a か", "* か", "か", "- か"]
        );

        let head = Request {
            lyric: "か",
            previous_vowel: None,
        };
        assert_eq!(candidates(Method::Sequential, &head), ["- か", "か"]);
    }

    /// **順に落ちる。** 何番目で当たったかを返す（TR-RCL-20 の「代替」）。
    #[test]
    fn 連続音は順に落ちる() {
        let req = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(
            resolve(Method::Sequential, &req, &have(&["a か"]))
                .expect("")
                .rank,
            0
        );
        assert_eq!(
            resolve(Method::Sequential, &req, &have(&["* か"]))
                .expect("")
                .rank,
            1
        );
        assert_eq!(
            resolve(Method::Sequential, &req, &have(&["か"]))
                .expect("")
                .rank,
            2,
            "単独音で録ったものが連続音の代替になる"
        );
        assert_eq!(
            resolve(Method::Sequential, &req, &have(&["- か"]))
                .expect("")
                .rank,
            3
        );
    }

    #[test]
    fn cvvc_の_cv_は語頭を先に見る() {
        let req = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(candidates(Method::Cvvc, &req), ["- か", "か"]);
    }

    /// **必要集合は第一候補だけ**（TR-RCL-15）。
    #[test]
    fn 必要集合は第一候補で作る() {
        let m = parse("さくら", UnitSet::Core).expect("読める");

        let single = required_aliases(Method::Single, &m, UnitSet::Core);
        assert_eq!(single, have(&["さ", "く", "ら"]));

        let seq = required_aliases(Method::Sequential, &m, UnitSet::Core);
        assert_eq!(seq, have(&["- さ", "a く", "u ら"]));
    }

    /// **長音と促音は必要集合に入らない**（TR-RCL-13 (b)(c)）。
    #[test]
    fn 長音と促音は必要集合に入らない() {
        let m = parse("かーきって", UnitSet::Core).expect("読める");
        let single = required_aliases(Method::Single, &m, UnitSet::Core);
        assert_eq!(single, have(&["か", "き", "て"]));
    }

    /// **解決できたものとできなかったものを両方返す**（TR-SYN-18 の短縮版に要る）。
    #[test]
    fn フレーズの欠損を数えられる() {
        let m = parse("さくら", UnitSet::Core).expect("読める");
        let got = resolve_phrase(Method::Single, &m, &have(&["さ", "ら"]), UnitSet::Core);
        assert_eq!(got.len(), 3);
        assert!(got[0].is_ok());
        assert!(got[1].is_err(), "く が無い");
        assert!(got[2].is_ok());

        let Err(missing) = &got[1] else { panic!() };
        assert_eq!(missing.lyric, "く");
        assert_eq!(missing.tried, ["く"]);
    }

    /// **カバレッジ判定と試唱が同じコードパスを通る**（TR-SYN-12, TR-RCL-20）。
    #[test]
    fn 必要集合を全部持っていればフレーズが解決する() {
        let m = parse("さくらさくら", UnitSet::Core).expect("読める");
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            let need = required_aliases(method, &m, UnitSet::Core);
            let got = resolve_phrase(method, &m, &need, UnitSet::Core);
            assert!(
                got.iter().all(Result::is_ok),
                "{method:?}: 必要集合を持てば全部解決すること"
            );
        }
    }
}
