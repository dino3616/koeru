//! 録る対象（`DEC-RCL-017`）。
//!
//! 録る対象の identity は、音高・方式の上の役割・音素の文脈の組で決まる。 綴りの規則
//! （`presamp.ini`、`TR-SYN-36`）を通さずに、方式と行の並びだけから導ける。
//!
//! エイリアスは、この組を作成時に固定した rules（`DEC-SYN-013`）で描いた表現。
//! **鍵にしない。** M6 ではエイリアスを一括で書き換えられる（`TR-EDT-27` の (2)）ので、
//! 文字列を鍵にすると改名した時点で被覆も固定した値も外れる。
//!
//! rules が2つの組を同じ綴りに畳むとき（CVVC の2音目以降の CV は直前の母音を問わず
//! 素の CV になる、など）は、被覆の上で同値として数える。 同値かどうかは [`Catalog`] が
//! 決め、文字列の比較に任せない。
//!
//! 移行中。 台帳はまだ（音高, 綴り）で持っていて、鍵を差し替えるのは移行の段。

use std::collections::BTreeMap;

use crate::alias::Method;
use crate::inventory::{Unit, UnitSet, consonants, units};
use crate::presamp::Rules;
use crate::reclist::Slot;

/// CV の前に何があるか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Context {
    /// 文脈を持たない。 単独音の CV。
    Isolated,
    /// 無音から読み始める（語頭形、`TR-RCL-21`）。
    Head,
    /// 直前のモーラの母音クラス。
    After(&'static str),
}

/// 録る対象の、音高を除いた部分（`TR-RCL-05`）。
///
/// 綴りはどこにも持たない。 描くのは [`render`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// モーラ1つの CV。
    Cv {
        /// 仮名表記（`inventory::Unit::kana`）。
        kana: &'static str,
        /// 前に何があるか。
        context: Context,
    },
    /// 隣り合う2モーラのあいだの渡り。
    Vc {
        /// 前のモーラの母音クラス。
        vowel: &'static str,
        /// 次のモーラの子音記号。空にならない。
        consonant: &'static str,
    },
    /// 行末の母音が消えていく区間。
    Ending {
        /// 最後のモーラの母音クラス。
        vowel: &'static str,
    },
}

/// 録る対象（`DEC-RCL-017`）。 被覆は音高ごとに独立（`TR-RCL-26`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TargetKey {
    /// 収録音高（MIDI）。
    pub tone: i32,
    /// 音高を除いた部分。
    pub target: Target,
}

/// 行が生む対象を、並びのまま全部（`TR-RCL-05`, `TR-RCL-18`）。
///
/// **重ねを除かない。** 同じ CV が2度出れば2度返す。 どのモーラから導くかは
/// [`Slot`] が持つ。 綴りの重ねを除いた形は [`crate::reclist::row_entries`]。
#[must_use]
pub fn of_row(method: Method, line: &[Unit]) -> Vec<(Target, Slot)> {
    let mut out = Vec::new();
    let context = |i: usize| match (method, i.checked_sub(1)) {
        (Method::Single, _) => Context::Isolated,
        (_, None) => Context::Head,
        (_, Some(p)) => Context::After(line[p].vowel),
    };
    for (i, unit) in line.iter().enumerate() {
        out.push((
            Target::Cv {
                kana: unit.kana,
                context: context(i),
            },
            Slot::Cv { mora: i },
        ));
        // 隣接から渡り（`TR-RCL-05`）。 母音始まりの次へは渡らない。
        if method == Method::Cvvc
            && let Some(next) = line.get(i + 1)
            && !next.consonant.is_empty()
        {
            out.push((
                Target::Vc {
                    vowel: unit.vowel,
                    consonant: next.consonant,
                },
                Slot::Vc {
                    prev: i,
                    next: i + 1,
                },
            ));
        }
    }
    if method == Method::Cvvc
        && let Some(last) = line.last()
    {
        out.push((
            Target::Ending { vowel: last.vowel },
            Slot::Ending {
                mora: line.len() - 1,
            },
        ));
    }
    out
}

/// 対象を綴りに描く（`TR-SYN-36`）。
///
/// 候補が複数あるときは先頭を採る。 規則が何も返さなければ仮名のまま。
#[must_use]
pub fn render(rules: &Rules, method: Method, target: &Target) -> String {
    match *target {
        Target::Cv { kana, context } => {
            let prev = match context {
                Context::Isolated | Context::Head => None,
                Context::After(v) => Some(v),
            };
            rules
                .candidates(method, kana, prev)
                .into_iter()
                .next()
                .unwrap_or_else(|| kana.to_owned())
        }
        Target::Vc { vowel, consonant } => rules.vc(vowel, consonant),
        Target::Ending { vowel } => rules.ending(vowel),
    }
}

/// 行の中で、その綴りに描かれる最初の対象（`DEC-RCL-017`）。
///
/// 台帳の（テイク, 綴り）の行を対象へ写すときに使う。 綴りだけから引かずに行の並びを
/// 通すので、同値の組のどれだったかが分かる。 最初のものを採るのは
/// [`crate::reclist::row_entries`] が重ねを除くときと同じ。
#[must_use]
pub fn in_row(rules: &Rules, method: Method, line: &[Unit], alias: &str) -> Option<(Target, Slot)> {
    of_row(method, line)
        .into_iter()
        .find(|(t, _)| render(rules, method, t) == alias)
}

/// 方式と単位のセットが生みうる対象の全体と、その描き方（`DEC-RCL-017`）。
///
/// 同値の判定はここが持つ。 rules は作成時に固定されている（`DEC-SYN-013`）ので、
/// プロジェクトの中では同じ `Catalog` が同じ答えを返す。
#[derive(Debug, Clone)]
pub struct Catalog {
    method: Method,
    aliases: BTreeMap<Target, String>,
    classes: BTreeMap<String, Vec<Target>>,
}

impl Catalog {
    /// 対象の全体を並べて描く。
    #[must_use]
    pub fn new(rules: &Rules, method: Method, set: UnitSet) -> Self {
        let table = units(set);
        let mut vowels: Vec<&'static str> = Vec::new();
        for u in &table {
            if !vowels.contains(&u.vowel) {
                vowels.push(u.vowel);
            }
        }
        let mut all = Vec::new();
        for u in &table {
            if method == Method::Single {
                all.push(Target::Cv {
                    kana: u.kana,
                    context: Context::Isolated,
                });
                continue;
            }
            all.push(Target::Cv {
                kana: u.kana,
                context: Context::Head,
            });
            for &v in &vowels {
                all.push(Target::Cv {
                    kana: u.kana,
                    context: Context::After(v),
                });
            }
        }
        if method == Method::Cvvc {
            for &v in &vowels {
                for c in consonants(set) {
                    all.push(Target::Vc {
                        vowel: v,
                        consonant: c,
                    });
                }
                all.push(Target::Ending { vowel: v });
            }
        }
        let mut aliases = BTreeMap::new();
        let mut classes: BTreeMap<String, Vec<Target>> = BTreeMap::new();
        for t in all {
            let a = render(rules, method, &t);
            classes.entry(a.clone()).or_default().push(t);
            aliases.insert(t, a);
        }
        for members in classes.values_mut() {
            members.sort();
        }
        Self {
            method,
            aliases,
            classes,
        }
    }

    /// どの方式の対象か。
    #[must_use]
    pub const fn method(&self) -> Method {
        self.method
    }

    /// その対象を描いた綴り。 この方式とセットが生まない対象なら `None`。
    #[must_use]
    pub fn alias(&self, target: &Target) -> Option<&str> {
        self.aliases.get(target).map(String::as_str)
    }

    /// その綴りに描かれる対象の組。 並びは [`Target`] の順。知らない綴りなら空。
    ///
    /// 組の中はどれも同値。 どれだったかを知りたいときは、行の並びを通す（[`in_row`]）。
    #[must_use]
    pub fn class(&self, alias: &str) -> &[Target] {
        self.classes.get(alias).map_or(&[], Vec::as_slice)
    }

    /// 同値の組を代表する対象（組の中で最小のもの）。 被覆や持ち主を組ごとに数えるときの鍵。
    #[must_use]
    pub fn canonical(&self, target: &Target) -> Option<Target> {
        self.class(self.alias(target)?).first().copied()
    }

    /// 2つの対象が同値か。 音高が違えば同値ではない（`TR-RCL-26`）。
    #[must_use]
    pub fn equivalent(&self, a: &TargetKey, b: &TargetKey) -> bool {
        a.tone == b.tone
            && matches!(
                (self.alias(&a.target), self.alias(&b.target)),
                (Some(x), Some(y)) if x == y
            )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preset;

    /// 書き換える前の `row_entries`。 綴りを直接作っていた。 対象を描く形に変えても
    /// 同じ綴りが同じ並びで出ることを、これと比べて確かめる。
    fn legacy_row_entries(rules: &Rules, method: Method, line: &[Unit]) -> Vec<(String, Slot)> {
        let mut out: Vec<(String, Slot)> = Vec::new();
        let mut push = |a: String, slot: Slot| {
            if !out.iter().any(|(x, _)| *x == a) {
                out.push((a, slot));
            }
        };
        let cv = |i: usize, prev: Option<&str>| {
            rules
                .candidates(method, line[i].kana, prev)
                .first()
                .cloned()
                .unwrap_or_else(|| line[i].kana.to_owned())
        };
        match method {
            Method::Single => {
                for i in 0..line.len() {
                    push(cv(i, None), Slot::Cv { mora: i });
                }
            }
            Method::Sequential => {
                for i in 0..line.len() {
                    let prev = i.checked_sub(1).map(|p| line[p].vowel);
                    push(cv(i, prev), Slot::Cv { mora: i });
                }
            }
            Method::Cvvc => {
                for i in 0..line.len() {
                    let prev = i.checked_sub(1).map(|p| line[p].vowel);
                    push(cv(i, prev), Slot::Cv { mora: i });
                    if let Some(next) = line.get(i + 1)
                        && !next.consonant.is_empty()
                    {
                        push(
                            rules.vc(line[i].vowel, next.consonant),
                            Slot::Vc {
                                prev: i,
                                next: i + 1,
                            },
                        );
                    }
                }
                if let Some(last) = line.last() {
                    push(
                        rules.ending(last.vowel),
                        Slot::Ending {
                            mora: line.len() - 1,
                        },
                    );
                }
            }
        }
        out
    }

    /// 綴りの節をすべて書き換えた規則。 既定の綴りと偶然一致して通るのを避ける。
    fn custom_rules(set: UnitSet) -> Rules {
        let mut r = Rules::builtin(set);
        for (section, template) in [
            ("VCV", "%v%_%CV%"),
            ("BEGINING_CV", "#%CV%"),
            ("CROSS_CV", "*%CV%"),
            ("VC", "%v%>%c%"),
            ("CV", "[%CV%]"),
            ("ENDING", "%v%R"),
        ] {
            r.templates.insert(section.to_owned(), template.to_owned());
        }
        r
    }

    /// 同梱のプリセットすべての行で、書き換える前と同じ綴りが同じ並びで出る。
    #[test]
    fn 今の行の綴りと一致する() {
        let mut rows = 0_usize;
        for p in preset::builtin() {
            for rules in [Rules::builtin(p.set), custom_rules(p.set)] {
                for row in p.reclist(&rules).expect("生成できる") {
                    assert_eq!(
                        crate::reclist::row_entries(&rules, p.method, &row.units),
                        legacy_row_entries(&rules, p.method, &row.units),
                        "{} の {}",
                        p.id,
                        row.id
                    );
                    rows += 1;
                }
            }
        }
        assert!(rows > 200, "比べた行が少なすぎる: {rows}");
    }

    /// 詰め直した行（`DEC-RCL-011`）でも同じ。
    #[test]
    fn 詰め直した行の綴りも一致する() {
        for p in preset::builtin() {
            let rules = Rules::builtin(p.set);
            let full = p.reclist(&rules).expect("生成できる");
            let want: std::collections::BTreeSet<String> = full
                .iter()
                .take(3)
                .flat_map(|r| crate::reclist::row_aliases(&rules, p.method, &r.units))
                .collect();
            for row in p.reclist_for(&rules, &want).expect("詰め直せる") {
                assert_eq!(
                    crate::reclist::row_entries(&rules, p.method, &row.units),
                    legacy_row_entries(&rules, p.method, &row.units),
                    "{} の {}",
                    p.id,
                    row.id
                );
            }
        }
    }

    /// 行が生む対象はどれも一覧に入っていて、同じ綴りに描かれる。
    #[test]
    fn 行の対象はどれも一覧にある() {
        for p in preset::builtin() {
            let rules = custom_rules(p.set);
            let catalog = Catalog::new(&rules, p.method, p.set);
            for row in p.reclist(&rules).expect("生成できる") {
                for (t, _) in of_row(p.method, &row.units) {
                    assert_eq!(
                        catalog.alias(&t),
                        Some(render(&rules, p.method, &t).as_str()),
                        "{} の {t:?}",
                        p.id
                    );
                }
            }
        }
    }

    fn unit(kana: &str) -> Unit {
        units(UnitSet::Core)
            .into_iter()
            .find(|u| u.kana == kana)
            .expect("中核の単位")
    }

    /// 単独音は文脈を持たない。 連続音と CVVC は語頭と直前の母音で分ける。
    #[test]
    fn 方式ごとに文脈を分ける() {
        let line = [unit("か"), unit("さ")];
        let cv = |m| {
            of_row(m, &line)
                .into_iter()
                .filter_map(|(t, _)| match t {
                    Target::Cv { context, .. } => Some(context),
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(cv(Method::Single), [Context::Isolated, Context::Isolated]);
        assert_eq!(cv(Method::Sequential), [Context::Head, Context::After("a")]);
        assert_eq!(cv(Method::Cvvc), [Context::Head, Context::After("a")]);
    }

    /// CVVC は渡りと語尾を持つ。 母音始まりの次へは渡らない。
    #[test]
    fn cvvc_は渡りと語尾を持つ() {
        let got: Vec<Target> = of_row(Method::Cvvc, &[unit("か"), unit("あ"), unit("さ")])
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        assert_eq!(
            got,
            [
                Target::Cv {
                    kana: "か",
                    context: Context::Head
                },
                Target::Cv {
                    kana: "あ",
                    context: Context::After("a")
                },
                Target::Vc {
                    vowel: "a",
                    consonant: "s"
                },
                Target::Cv {
                    kana: "さ",
                    context: Context::After("a")
                },
                Target::Ending { vowel: "a" },
            ]
        );
    }

    /// 同じ CV が2度出れば2度返す。 綴りの重ねを除くのは `row_entries` のほう。
    #[test]
    fn 重ねを除かない() {
        let got = of_row(Method::Single, &[unit("か"), unit("か")]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].1, Slot::Cv { mora: 1 });
    }

    /// CVVC の既定の規則は、2音目以降の CV を直前の母音を問わず素の CV に畳む。
    /// 畳まれた組は同値で、語頭形とは同値でない。
    #[test]
    fn 畳まれた組は同値になる() {
        let rules = Rules::builtin(UnitSet::Core);
        let catalog = Catalog::new(&rules, Method::Cvvc, UnitSet::Core);
        let key = |context| TargetKey {
            tone: 60,
            target: Target::Cv {
                kana: "か",
                context,
            },
        };
        assert!(catalog.equivalent(&key(Context::After("a")), &key(Context::After("i"))));
        assert!(!catalog.equivalent(&key(Context::Head), &key(Context::After("a"))));
        let other_tone = TargetKey {
            tone: 62,
            ..key(Context::After("a"))
        };
        assert!(
            !catalog.equivalent(&key(Context::After("a")), &other_tone),
            "音高が違えば同値ではない（`TR-RCL-26`）"
        );
        let class = catalog.class("か");
        assert!(class.len() > 1, "{class:?}");
        assert_eq!(
            catalog.canonical(&key(Context::After("i")).target),
            class.first().copied(),
            "代表は組の中で最小のもの"
        );
    }

    /// 連続音は直前の母音を綴りに織り込むので、組は1つだけ。
    #[test]
    fn 連続音の綴りは対象を1つに決める() {
        let rules = Rules::builtin(UnitSet::Core);
        let catalog = Catalog::new(&rules, Method::Sequential, UnitSet::Core);
        assert_eq!(
            catalog.class("a か"),
            [Target::Cv {
                kana: "か",
                context: Context::After("a")
            }]
        );
        assert!(catalog.class("知らない綴り").is_empty());
    }

    /// 台帳の綴りを行の並びを通して写すと、同値の組のどれだったかが分かる。
    #[test]
    fn 行の中の綴りから対象を引く() {
        let rules = Rules::builtin(UnitSet::Core);
        let line = [unit("い"), unit("か"), unit("あ"), unit("か")];
        let (t, slot) = in_row(&rules, Method::Cvvc, &line, "か").expect("行にある");
        assert_eq!(
            t,
            Target::Cv {
                kana: "か",
                context: Context::After("i")
            },
            "最初に出たものを採る"
        );
        assert_eq!(slot, Slot::Cv { mora: 1 });
        assert_eq!(in_row(&rules, Method::Cvvc, &line, "さ"), None);
    }
}
