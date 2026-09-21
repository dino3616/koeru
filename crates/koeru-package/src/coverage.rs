//! どの方式を書き出せるか（`TR-PKG-23`, `TR-RCL-21`, `DEC-PKG-005`）。
//!
//! 上下関係のグラフを宣言しない。 方式が要求するエイリアス表を素材が
//! すべて含むかどうか、それだけが判定基準（`INV-PKG-105`）。逆方向
//! （単独音の素材から連続音）は、要求表を満たしようが無いので出てこない。
//!
//! 形式的な契約は `specs/requirements/method-coverage.fsl` が持つ。
//!
//! # M4 が出せるのは、そのまま満たしている方式だけ
//!
//! 連続音の素材から単独音を出すには、語頭 CV を切り直して5値を再導出する
//! 必要がある。M4 は [`downgradable`] で「構成上は出せる」と答えるところまでにした
//! （`PROFILE-M4` の excludes）。再導出そのものは `koeru-align` が持つ（`TR-ALN-34`）。

use std::collections::BTreeSet;

use koeru_core::alias::{self, Method, Request};
use koeru_core::inventory::{UnitSet, transition_vowels, units, vc_units};

/// その方式が要求するエイリアス表（`TR-PKG-23`）。
///
/// 綴りは [`alias::candidates`] から取る。 ここで組み立て直すと、
/// 解決側と綴りが分かれて「歌えると出たのに書き出せない」が起きる。
///
/// 先行母音は実在する単位が持つものだけを回す（`DEC-RCL-009`）。
/// presamp の体系が持つ `N` を混ぜると、どの単位も作れない `N か` を要求し、
/// 連続音が 100% 被覆に到達できなくなる。**踏んだ。**
///
/// いまはどの方式も表を持つので `None` を返す枝は無い。 返り値の形は残す
/// ——インベントリを持たない方式を足したときに、推測で並べたくない。
#[must_use]
pub fn required(method: Method, set: UnitSet) -> Option<BTreeSet<String>> {
    let table = units(set);
    let first = |req: &Request<'_>| alias::candidates(method, req).first().cloned();
    let mut out = BTreeSet::new();
    match method {
        Method::Single => {
            for u in &table {
                if let Some(a) = first(&Request {
                    lyric: u.kana,
                    previous_vowel: None,
                }) {
                    out.insert(a);
                }
            }
        }
        Method::Sequential => {
            for u in &table {
                // 語頭 CV。`TR-RCL-21` が全 CV について必ず含めることを求めている。
                if let Some(a) = first(&Request {
                    lyric: u.kana,
                    previous_vowel: None,
                }) {
                    out.insert(a);
                }
                for v in transition_vowels(set) {
                    if let Some(a) = first(&Request {
                        lyric: u.kana,
                        previous_vowel: Some(v),
                    }) {
                        out.insert(a);
                    }
                }
            }
        }
        // CV・VC・語尾の3種（`TR-RCL-05`）。CV の綴りは `candidates` から取り、
        // VC と語尾は音符に対応しないので専用の綴りを使う。
        //
        // CV は2綴り要る（`DEC-SYN-011`）。 語頭形と素の CV で立ち上がりが違い、
        // 候補順が文脈で分かれるので、どちらが欠けても第一候補を落とす。
        Method::Cvvc => {
            for u in &table {
                for prev in [None, Some("a")] {
                    if let Some(a) = first(&Request {
                        lyric: u.kana,
                        previous_vowel: prev,
                    }) {
                        out.insert(a);
                    }
                }
            }
            for vc in vc_units(set) {
                out.insert(alias::vc_alias(vc.vowel, vc.consonant));
            }
            for v in transition_vowels(set) {
                out.insert(alias::ending_alias(v));
            }
        }
    }
    Some(out)
}

/// 被覆の状態（`TR-PKG-23`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Coverage {
    /// 要求するエイリアスの数。
    pub required: usize,
    /// そのうち持っているものの数。
    pub provided: usize,
    /// 足りないエイリアス。
    pub missing: Vec<String>,
}

impl Coverage {
    /// 100% 被覆しているか（`REQ-PKG-102`）。
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// 素材が方式をどれだけ覆っているか（`TR-PKG-23`）。
///
/// 要求表を持たない方式には `None`。
#[must_use]
pub fn coverage(method: Method, set: UnitSet, provided: &BTreeSet<String>) -> Option<Coverage> {
    let required = required(method, set)?;
    let missing: Vec<String> = required.difference(provided).cloned().collect();
    Some(Coverage {
        required: required.len(),
        provided: required.len() - missing.len(),
        missing,
    })
}

/// そのまま書き出せる方式（`INV-PKG-105`）。
///
/// 素材が要求表を全部持っているものだけ。M4 が実際に出せるのはこれ。
#[must_use]
pub fn exportable(set: UnitSet, provided: &BTreeSet<String>) -> Vec<Method> {
    [Method::Single, Method::Sequential, Method::Cvvc]
        .into_iter()
        .filter(|m| coverage(*m, set, provided).is_some_and(|c| c.is_complete()))
        .collect()
}

/// 構成上は出せるが、5値の再導出が要る方式（`TR-RCL-21`）。
///
/// 連続音の素材は語頭 CV（`- CV`）を全 CV について持つので、切り直せば
/// 単独音になる。**値の流用はしない。** `- CV` を素の `CV` として複製すると、
/// 語頭の子音区間を持ったままの oto が単独音として配られる。
///
/// **ここは経路を用意しない。** 返すのは方式の一覧だけで、
/// 5値の再導出は `koeru-align` が持つ（`TR-ALN-34`）。
#[must_use]
pub fn downgradable(set: UnitSet, provided: &BTreeSet<String>) -> Vec<Method> {
    if missing_head_cv(set, provided).is_empty()
        && !exportable(set, provided).contains(&Method::Single)
    {
        vec![Method::Single]
    } else {
        Vec::new()
    }
}

/// 足りない語頭 CV（`TR-RCL-21`）。
///
/// 連続音のリストが「単独音を出せる構成」になっているかは、これが空かどうか。
#[must_use]
pub fn missing_head_cv(set: UnitSet, provided: &BTreeSet<String>) -> Vec<String> {
    units(set)
        .iter()
        .filter_map(|u| {
            let head = format!("- {}", u.kana);
            (!provided.contains(&head)).then_some(head)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn provided(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    fn single_full() -> BTreeSet<String> {
        units(UnitSet::Core)
            .iter()
            .map(|u| u.kana.to_owned())
            .collect()
    }

    fn sequential_full() -> BTreeSet<String> {
        required(Method::Sequential, UnitSet::Core).expect("表があること")
    }

    #[test]
    fn 単独音の要求表は収録単位そのもの() {
        let r = required(Method::Single, UnitSet::Core).expect("表があること");
        assert_eq!(r.len(), units(UnitSet::Core).len());
        assert!(r.contains("あ"));
    }

    /// `TR-RCL-21`。連続音の要求表は語頭 CV を全 CV について含む。
    #[test]
    fn 連続音の要求表は語頭_cv_を全部含む() {
        let r = sequential_full();
        for u in units(UnitSet::Core) {
            assert!(r.contains(&format!("- {}", u.kana)), "{}", u.kana);
        }
        assert!(r.contains("a か"));
    }

    /// `TR-RCL-05`。CVVC は CV・VC・語尾の3種。拡張セットで 144 + 180 + 6 = 330。
    #[test]
    fn cvvc_の要求表は三種の合計() {
        let r = required(Method::Cvvc, UnitSet::Extended).expect("表があること");
        let units = units(UnitSet::Extended).len();
        let vc = koeru_core::inventory::vc_units(UnitSet::Extended).len();
        let ending = transition_vowels(UnitSet::Extended).len();
        assert_eq!((units, vc, ending), (144, 180, 6));
        // CV は語頭形と素の2綴り（`DEC-SYN-011`）。
        assert_eq!(r.len(), units * 2 + vc + ending);
        assert!(r.contains("- か"), "語頭 CV");
        assert!(r.contains("か"), "素の CV");
        assert!(r.contains("a k"), "VC");
        assert!(r.contains("a -"), "語尾");
    }

    /// `DEC-RCL-009`。要求表に、録音では作れないエイリアスを入れない。
    ///
    /// 母音クラス `N` はどの単位も持たない。 要求表に `N か` が入っていると、
    /// 全部録っても連続音が 100% に届かず、書き出せる方式に入らない。**踏んだ。**
    #[test]
    fn 要求表は録音で埋められるものだけを並べる() {
        // 録音でできることを、要求表を経由せずに作る。
        // required() から provided を作ると、表が自分自身を満たしてしまう。
        let recorded = recordable(UnitSet::Extended);
        for m in [Method::Single, Method::Sequential, Method::Cvvc] {
            let r = required(m, UnitSet::Extended).expect("表があること");
            let unreachable: Vec<&String> = r.difference(&recorded).collect();
            assert!(
                unreachable.is_empty(),
                "{m:?} が作れない要求を持つ: {unreachable:?}"
            );
        }
    }

    /// 録音で実際に作れるエイリアスの全体。
    ///
    /// 行を読み上げれば得られるものを、インベントリから直に組む。
    /// 語頭 CV は行の先頭、素の CV は行の途中、VCV は隣接、VC は渡り、語尾は行末。
    fn recordable(set: UnitSet) -> BTreeSet<String> {
        let table = units(set);
        let mut out = BTreeSet::new();
        for u in &table {
            out.insert(u.kana.to_owned());
            out.insert(format!("- {}", u.kana));
            for v in transition_vowels(set) {
                out.insert(format!("{v} {}", u.kana));
            }
        }
        for vc in koeru_core::inventory::vc_units(set) {
            out.insert(alias::vc_alias(vc.vowel, vc.consonant));
        }
        for v in transition_vowels(set) {
            out.insert(alias::ending_alias(v));
        }
        out
    }

    #[test]
    fn 揃っていれば書き出せる() {
        assert_eq!(exportable(UnitSet::Core, &single_full()), [Method::Single]);
    }

    #[test]
    fn 足りなければ書き出せない() {
        let mut p = single_full();
        p.remove("あ");
        assert!(exportable(UnitSet::Core, &p).is_empty());
        let c = coverage(Method::Single, UnitSet::Core, &p).expect("表があること");
        assert!(!c.is_complete());
        assert_eq!(c.missing, ["あ"]);
    }

    /// `ASSUME-3`。逆方向は禁止を宣言しなくても出てこない。
    #[test]
    fn 単独音の素材から連続音は出てこない() {
        let out = exportable(UnitSet::Core, &single_full());
        assert!(!out.contains(&Method::Sequential));
        assert!(downgradable(UnitSet::Core, &single_full()).is_empty());
    }

    /// `TR-RCL-21`。連続音の素材は、構成上は単独音を出せる。
    #[test]
    fn 連続音の素材は単独音へ降りられる() {
        let p = sequential_full();
        assert_eq!(exportable(UnitSet::Core, &p), [Method::Sequential]);
        assert!(missing_head_cv(UnitSet::Core, &p).is_empty());
        assert_eq!(downgradable(UnitSet::Core, &p), [Method::Single]);
    }

    #[test]
    fn 語頭_cv_が欠けていたら降りられない() {
        let mut p = sequential_full();
        p.remove("- あ");
        assert_eq!(missing_head_cv(UnitSet::Core, &p), ["- あ"]);
        assert!(downgradable(UnitSet::Core, &p).is_empty());
    }

    #[test]
    fn 何も無ければ何も出せない() {
        assert!(exportable(UnitSet::Core, &provided(&[])).is_empty());
    }
}
