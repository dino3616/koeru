//! 素材の解決（`DEC-PLT-052`）。
//!
//! 試唱と書き出しは、同じ事実（どの行がどの対象を生み、どのテイクを採用し、そのテイクが
//! どの5値を持つか）から、同じ候補を得る。 用途で変わるところは [`UseContext`] だけが持つ。
//! 候補ごとに、使うか・使わないならなぜかを返す（[`Disposition`]）。 各所が候補を
//! 自分で絞り直さない——絞り方が2つに割れると、試唱で鳴る綴りと書き出しに入る綴りが
//! 黙ってずれる。
//!
//! # 誰が選ばれるか
//!
//! - 録った方式のまま使う用途（試唱と、そのままの書き出し）は、綴りを持つ行を
//!   [`selection::owners`]（`DEC-RCL-016`）で決める。 持ち主の採用テイクのうち、5値を持つものだけ使う
//! - 下位方式へ書き出す用途は、行の素材から作り直した対象を、行の頭に近いモーラから採る
//!   （`DEC-PKG-014`）。 同じ深さなら素材の名前で決める
//!
//! 台帳の鍵は移行中なので（音高, 綴り）。 鍵は型引数で受け、ここは綴りを知らない
//! （`DEC-RCL-017`）。 綴りを鍵にした型をここに足さない。
//!
//! 並びに依らない。 同じ事実を別の順で渡しても同じ候補を返す。

use std::collections::{BTreeMap, BTreeSet};

use crate::alias::Method;
use crate::id::{RowId, TakeId};
use crate::selection;
use crate::target::{Catalog, Target};

/// 対象の役割。 用途によって、鳴らすか配るかが変わる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Role {
    /// モーラ1つの CV。
    Cv,
    /// 隣り合う2モーラのあいだの渡り。
    Vc,
    /// 行末の母音が消えていく区間。
    Ending,
}

impl Role {
    /// 綴りの役割。
    ///
    /// 綴りを描いた対象の組（[`Catalog::class`]）が全部同じ役割のときだけその役割。
    /// 知らない綴りや、役割が割れる綴りは CV として扱う——**鳴らさない側へ倒すと、
    /// 録ったはずの音が黙って鳴らなくなる。**
    #[must_use]
    pub fn of(catalog: &Catalog, alias: &str) -> Self {
        let class = catalog.class(alias);
        let all = |want: fn(&Target) -> bool| !class.is_empty() && class.iter().all(want);
        if all(|t| matches!(t, Target::Ending { .. })) {
            Self::Ending
        } else if all(|t| matches!(t, Target::Vc { .. })) {
            Self::Vc
        } else {
            Self::Cv
        }
    }
}

/// 書き出しの届け方。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Delivery {
    /// 録った方式のまま配る。
    AsRecorded,
    /// 下位方式へ降りて配る（`TR-PKG-24`）。 `target` の綴りを、行の素材から作り直す。
    Downgrade {
        /// 降りた先の方式。
        target: Method,
    },
}

/// 素材を何に使うか（`DEC-PLT-052`）。
///
/// 用途の違いはここに足す。 呼び出し側が候補を後から絞り直すのではなく、解く前に用途を渡す。
///
/// 書き出しのプロファイル（`classic` / `openutau` / `both`）は持たない。 あれが変えるのは
/// 符号化と配置で、どの素材を選ぶかは変えない。 選び方を変えるものが出たときに足す。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UseContext {
    /// 試唱。 鳴らす対象だけを使う。 語尾（`a -`）は、OpenUtau の presamp phonemizer が
    /// 挟まないのに揃えて（`TR-SYN-11`）鳴らさないので、使わない。
    Preview,
    /// 書き出し。 鳴らさない対象も配る——録った語尾は配布物の素材として入る。
    Export {
        /// どう届けるか。
        delivery: Delivery,
    },
}

impl UseContext {
    /// トレースに載せてよい語。 固定の語彙。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Preview => "preview",
            Self::Export {
                delivery: Delivery::AsRecorded,
            } => "export",
            Self::Export {
                delivery: Delivery::Downgrade { .. },
            } => "export_downgrade",
        }
    }
}

/// 使わない理由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Reason {
    /// 先に録った別の行が持つ（`DEC-RCL-016`）。
    NotOwner,
    /// 持ち主のテイクに、その綴りの5値が無い。 失敗ではない——解析が済んでいないだけで、
    /// 被覆には数える。
    NoOto,
    /// この用途では鳴らさない。
    NotSounded,
    /// 下位方式で、行の頭に近い別の素材が選ばれた（`DEC-PKG-014`）。
    Outranked,
}

impl Reason {
    /// トレースに載せてよい語。 固定の語彙。
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::NotOwner => "not_owner",
            Self::NoOto => "no_oto",
            Self::NotSounded => "not_sounded",
            Self::Outranked => "outranked",
        }
    }
}

/// 候補をどうするか。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Disposition {
    /// 使う。
    Use,
    /// 使わない。 理由を添える。
    Skip(Reason),
}

/// 行が鍵を生むという事実。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Production<K> {
    /// 生む鍵。
    pub key: K,
    /// 生む行。
    pub row: RowId,
    /// 行の中の、何モーラ目から取るか。 下位方式の選びだけが読む（`DEC-PKG-014`）。
    /// 同じ行が同じ鍵を重ねて名乗るときは、いちばん浅いほうを採る。
    pub depth: usize,
}

/// 行の有効な採用テイク。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Adoption {
    /// 採用しているテイク。
    pub take: TakeId,
    /// 素材の名前。 下位方式で深さが同じとき、これで決める。
    pub name: String,
}

/// 素材の解決に要る事実。
///
/// 台帳の1つの版から読んだもの。 `O` は5値の型で、ここは中身を見ない（有無だけ）。
#[derive(Debug, Clone)]
pub struct Facts<K, O = ()> {
    /// 行が生む鍵。 録っていない行のものが混じっていてよい。 解くときに落とす。
    pub produced: Vec<Production<K>>,
    /// 有効な採用テイクを持つ行。 採用が無い行と、採用したテイクが無効な行は入らない。
    pub adopted: BTreeMap<RowId, Adoption>,
    /// 行ごとの、最初に確定した有効なテイク（[`selection::owners`]）。
    pub first_valid: BTreeMap<RowId, TakeId>,
    /// テイクが持つ5値（テイク, 鍵）。
    pub otos: BTreeMap<(TakeId, K), O>,
}

impl<K: Ord, O> Facts<K, O> {
    /// 何も録っていない。
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            produced: Vec::new(),
            adopted: BTreeMap::new(),
            first_valid: BTreeMap::new(),
            otos: BTreeMap::new(),
        }
    }

    /// 候補の5値。 無ければ `None`。
    #[must_use]
    pub fn oto(&self, c: &Candidate<K>) -> Option<&O>
    where
        K: Clone,
    {
        self.otos.get(&(c.take, c.key.clone()))
    }
}

/// 候補1つ。 鍵を生む行と、そのテイク。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate<K> {
    /// 鍵。
    pub key: K,
    /// 生む行。
    pub row: RowId,
    /// その行の採用テイク。
    pub take: TakeId,
    /// 使うか、使わないならなぜか。
    pub disposition: Disposition,
}

/// 解いた結果。 並びは（鍵, 行）の順で、入力の並びに依らない。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution<K> {
    context: UseContext,
    candidates: Vec<Candidate<K>>,
}

impl<K: Ord> Resolution<K> {
    /// 解いた用途。
    #[must_use]
    pub const fn context(&self) -> UseContext {
        self.context
    }

    /// 候補を全部。 使わないものも理由つきで並ぶ。
    #[must_use]
    pub fn candidates(&self) -> &[Candidate<K>] {
        &self.candidates
    }

    /// 使う候補。 鍵ごとにちょうど1つ以下。
    pub fn used(&self) -> impl Iterator<Item = &Candidate<K>> {
        self.candidates
            .iter()
            .filter(|c| c.disposition == Disposition::Use)
    }

    /// 録ってある鍵。 有効な採用テイクを持つ行が1つでも生む鍵。
    ///
    /// 用途にも5値の有無にも依らない。 被覆（`TR-RCL-18`, `TR-PKG-23`）はこれで数える
    /// ——5値は解析のあとに付くので、解析が済む前の素材を「録っていない」とは言わない。
    #[must_use]
    pub fn covered(&self) -> BTreeSet<&K> {
        self.candidates.iter().map(|c| &c.key).collect()
    }

    /// その扱いの候補の数。
    #[must_use]
    pub fn count(&self, disposition: Disposition) -> usize {
        self.candidates
            .iter()
            .filter(|c| c.disposition == disposition)
            .count()
    }
}

/// 事実と用途から、候補を解く（`DEC-PLT-052`）。
///
/// `role_of` は鍵の役割。 用途が役割を見るか（試唱は語尾を鳴らさない）はここが決める。
/// 呼び出し側は役割を答えるだけで、使うかどうかは決めない。
///
/// 理由ごとの件数をトレースへ出す。 載せるのは用途の語・理由の語・件数だけで、
/// 鍵（綴り）や行や素材の名前は載せない。
#[must_use]
pub fn resolve<K, O>(
    facts: &Facts<K, O>,
    context: UseContext,
    role_of: impl Fn(&K) -> Role,
) -> Resolution<K>
where
    K: Ord + Clone,
{
    // 録っていない行の生む鍵は候補にならない。 同じ行が同じ鍵を重ねて名乗るなら、浅いほう。
    let mut depth: BTreeMap<(K, RowId), usize> = BTreeMap::new();
    for p in &facts.produced {
        if !facts.adopted.contains_key(&p.row) {
            continue;
        }
        depth
            .entry((p.key.clone(), p.row.clone()))
            .and_modify(|d| *d = (*d).min(p.depth))
            .or_insert(p.depth);
    }

    let candidates = match context {
        UseContext::Preview
        | UseContext::Export {
            delivery: Delivery::AsRecorded,
        } => by_owner(facts, context, &role_of, &depth),
        UseContext::Export {
            delivery: Delivery::Downgrade { .. },
        } => by_depth(facts, &depth),
    };
    let resolution = Resolution {
        context,
        candidates,
    };
    record(&resolution);
    resolution
}

/// 綴りの持ち主の行だけを使う（`DEC-RCL-016`）。
fn by_owner<K, O>(
    facts: &Facts<K, O>,
    context: UseContext,
    role_of: &impl Fn(&K) -> Role,
    depth: &BTreeMap<(K, RowId), usize>,
) -> Vec<Candidate<K>>
where
    K: Ord + Clone,
{
    let adopted_valid: BTreeSet<RowId> = facts.adopted.keys().cloned().collect();
    let owners = selection::owners(
        depth.keys().map(|(k, r)| (k.clone(), r.clone())),
        &adopted_valid,
        &facts.first_valid,
    );
    depth
        .keys()
        .filter_map(|(key, row)| {
            let take = facts.adopted.get(row)?.take;
            let disposition = if owners.get(key) != Some(row) {
                Disposition::Skip(Reason::NotOwner)
            } else if context == UseContext::Preview && role_of(key) == Role::Ending {
                Disposition::Skip(Reason::NotSounded)
            } else if !facts.otos.contains_key(&(take, key.clone())) {
                Disposition::Skip(Reason::NoOto)
            } else {
                Disposition::Use
            };
            Some(Candidate {
                key: key.clone(),
                row: row.clone(),
                take,
                disposition,
            })
        })
        .collect()
}

/// 行の頭に近いモーラから採る。 同じ深さなら素材の名前（`DEC-PKG-014`）。
///
/// **下位方式では、同じ綴りを複数の素材が出せる。** 連続音の行はどれも複数のモーラを
/// 含むので、単独音へ降りると `い` を出せる行が 30 以上並ぶ。 全部出すと `oto.ini` 横断で
/// エイリアスが重なり、`TR-PKG-19` に反する。
///
/// **行頭のモーラから採る。** 行頭は無音に続くので、単独音の録り方にいちばん近い。
/// 途中のモーラは先行母音から渡ってくるので、子音の立ち上がりが前の母音に埋もれる。
///
/// **持ち主を見ない。** 降りた先の綴りは録った綴りと別で、行はその綴りを
/// 持っていない。 どの行からも同じ綴りを作れるので、全部が候補になる。
///
/// **音高ごとに選ぶ。** 多音階は音高ごとに区画と `oto.ini` を分ける（`TR-ALN-22`）ので、
/// 音高を跨いで1つに絞ると、残りの区画からその綴りが消える。 鍵が音高を含むので、そのまま成り立つ。
///
/// 並びが実行ごとに変わると、同じ台帳から違う配布物が出る（`TR-RCL-27`）。
/// 名前も同じなら行の ID で決める。
fn by_depth<K, O>(facts: &Facts<K, O>, depth: &BTreeMap<(K, RowId), usize>) -> Vec<Candidate<K>>
where
    K: Ord + Clone,
{
    let rank = |key: &K, row: &RowId| {
        let name = facts.adopted.get(row).map_or("", |a| a.name.as_str());
        (depth[&(key.clone(), row.clone())], name, row.clone())
    };
    let mut best: BTreeMap<&K, (usize, &str, RowId)> = BTreeMap::new();
    for (key, row) in depth.keys() {
        let here = rank(key, row);
        match best.get_mut(key) {
            Some(held) if here < *held => *held = here,
            Some(_) => {}
            None => {
                best.insert(key, here);
            }
        }
    }
    depth
        .keys()
        .filter_map(|(key, row)| {
            let take = facts.adopted.get(row)?.take;
            let chosen = best.get(key).is_some_and(|(_, _, r)| r == row);
            Some(Candidate {
                key: key.clone(),
                row: row.clone(),
                take,
                disposition: if chosen {
                    Disposition::Use
                } else {
                    Disposition::Skip(Reason::Outranked)
                },
            })
        })
        .collect()
}

/// 理由ごとの件数を出す。 綴りも行も載せない（`AGENTS.md` の禁止事項3）。
fn record<K: Ord>(r: &Resolution<K>) {
    let mut by: BTreeMap<Disposition, usize> = BTreeMap::new();
    for c in &r.candidates {
        *by.entry(c.disposition).or_default() += 1;
    }
    for (d, count) in by {
        let reason = match d {
            Disposition::Use => "used",
            Disposition::Skip(why) => why.code(),
        };
        tracing::debug!(kind = r.context.code(), reason, count, "素材を解いた");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::{UnitSet, units};
    use crate::presamp::Rules;
    use crate::target;

    type Key = (i32, &'static str);

    fn row(s: &str) -> RowId {
        RowId::new(s)
    }

    fn make(
        produced: &[(Key, &str)],
        adopted: &[(&str, i32)],
        first_valid: &[(&str, i32)],
        otos: &[(i32, Key)],
    ) -> Facts<Key> {
        Facts {
            produced: produced
                .iter()
                .map(|(k, r)| Production {
                    key: *k,
                    row: row(r),
                    depth: 0,
                })
                .collect(),
            adopted: adopted
                .iter()
                .map(|(r, t)| {
                    (
                        row(r),
                        Adoption {
                            take: TakeId::new(*t),
                            name: (*r).to_owned(),
                        },
                    )
                })
                .collect(),
            first_valid: first_valid
                .iter()
                .map(|(r, t)| (row(r), TakeId::new(*t)))
                .collect(),
            otos: otos
                .iter()
                .map(|(t, k)| ((TakeId::new(*t), *k), ()))
                .collect(),
        }
    }

    fn cv(_: &Key) -> Role {
        Role::Cv
    }

    const EXPORT: UseContext = UseContext::Export {
        delivery: Delivery::AsRecorded,
    };

    fn disposition_of(r: &Resolution<Key>, key: Key, row_id: &str) -> Disposition {
        r.candidates()
            .iter()
            .find(|c| c.key == key && c.row == row(row_id))
            .unwrap_or_else(|| panic!("候補が無い: {key:?} {row_id}"))
            .disposition
    }

    /// 全部の綴りは `フルリストの行 s001（テイク 7）` と `詰め直した行 p1（テイク 3）` が生む。
    /// 先に録った p1 が持つ。 5値は持ち主のテイクにだけある。
    fn full_and_packed(order_of_rows: bool) -> Facts<Key> {
        let mut produced = vec![
            ((60, "さ"), "s001"),
            ((60, "し"), "s001"),
            ((60, "さ"), "p1"),
        ];
        if order_of_rows {
            produced.reverse();
        }
        make(
            &produced,
            &[("s001", 7), ("p1", 3)],
            &[("s001", 7), ("p1", 3)],
            &[(7, (60, "し")), (3, (60, "さ"))],
        )
    }

    /// 並びの入れ替えでは、持ち主も使う候補も変わらない（試唱が持ち主ではない行のテイクを
    /// 引いて、綴りが鳴らなかった）。
    #[test]
    fn 録った順に持ち主が決まり_行の並びに依らない() {
        for reversed in [false, true] {
            let r = resolve(&full_and_packed(reversed), UseContext::Preview, cv);
            assert_eq!(
                disposition_of(&r, (60, "さ"), "p1"),
                Disposition::Use,
                "先に録った行が持つ"
            );
            assert_eq!(
                disposition_of(&r, (60, "さ"), "s001"),
                Disposition::Skip(Reason::NotOwner)
            );
            assert_eq!(disposition_of(&r, (60, "し"), "s001"), Disposition::Use);
        }
        assert_eq!(
            resolve(&full_and_packed(false), UseContext::Preview, cv),
            resolve(&full_and_packed(true), UseContext::Preview, cv)
        );
    }

    /// 持ち主のテイクに5値が無ければ、別の行のテイクへ落とさない。 被覆には数える。
    #[test]
    fn 持ち主に5値が無ければ使わず_被覆には数える() {
        let mut f = full_and_packed(false);
        f.otos.remove(&(TakeId::new(3), (60, "さ")));
        let r = resolve(&f, UseContext::Preview, cv);
        assert_eq!(
            disposition_of(&r, (60, "さ"), "p1"),
            Disposition::Skip(Reason::NoOto)
        );
        assert_eq!(
            disposition_of(&r, (60, "さ"), "s001"),
            Disposition::Skip(Reason::NotOwner),
            "持ち主でない行が代わりに鳴らない"
        );
        assert!(r.covered().contains(&(60, "さ")));
        assert!(r.used().all(|c| c.key != (60, "さ")));
    }

    /// 同じ綴りでも音高が違えば別の鍵。 持ち主も別々に決まる（`TR-RCL-26`）。
    #[test]
    fn 音高が違う同じ綴りは別々に決まる() {
        let f = make(
            &[
                ((60, "さ"), "low1"),
                ((60, "さ"), "low2"),
                ((72, "さ"), "high1"),
                ((72, "さ"), "high2"),
            ],
            &[("low1", 5), ("low2", 2), ("high1", 3), ("high2", 9)],
            &[("low1", 5), ("low2", 2), ("high1", 3), ("high2", 9)],
            &[
                (5, (60, "さ")),
                (2, (60, "さ")),
                (3, (72, "さ")),
                (9, (72, "さ")),
            ],
        );
        let r = resolve(&f, UseContext::Preview, cv);
        let used: Vec<(Key, &str)> = r.used().map(|c| (c.key, c.row.as_str())).collect();
        assert_eq!(used, [((60, "さ"), "low2"), ((72, "さ"), "high1")]);
    }

    /// 持ち主の採用が無効になれば、次に録った行へ移る（`DEC-RCL-016`）。
    #[test]
    fn 持ち主の採用が無効になれば次に録った行へ移る() {
        let mut f = full_and_packed(false);
        f.otos.insert((TakeId::new(7), (60, "さ")), ());
        // p1 の採用が無効になる。 採用も最初の有効なテイクも持たなくなる。
        f.adopted.remove(&row("p1"));
        f.first_valid.remove(&row("p1"));
        let r = resolve(&f, UseContext::Preview, cv);
        assert_eq!(disposition_of(&r, (60, "さ"), "s001"), Disposition::Use);
        assert!(
            r.candidates().iter().all(|c| c.row != row("p1")),
            "採用が無効な行は候補にならない"
        );
    }

    /// 録っていない行は候補にならない。
    #[test]
    fn 録っていない行は候補にならない() {
        let f = make(&[((60, "さ"), "s002")], &[], &[], &[]);
        let r = resolve(&f, EXPORT, cv);
        assert!(r.candidates().is_empty());
        assert!(r.covered().is_empty());
    }

    fn ending(k: &Key) -> Role {
        if k.1 == "a -" { Role::Ending } else { Role::Cv }
    }

    /// 試唱は語尾を鳴らさず、書き出しは配る。 違いは用途だけが持つ。
    #[test]
    fn 語尾は試唱で使わず書き出しで使う() {
        let f = make(
            &[((60, "a -"), "r1"), ((60, "か"), "r1")],
            &[("r1", 1)],
            &[("r1", 1)],
            &[(1, (60, "a -")), (1, (60, "か"))],
        );
        let p = resolve(&f, UseContext::Preview, ending);
        let e = resolve(&f, EXPORT, ending);
        assert_eq!(
            disposition_of(&p, (60, "a -"), "r1"),
            Disposition::Skip(Reason::NotSounded)
        );
        assert_eq!(disposition_of(&e, (60, "a -"), "r1"), Disposition::Use);
        assert_eq!(disposition_of(&p, (60, "か"), "r1"), Disposition::Use);
        assert_eq!(
            p.covered(),
            e.covered(),
            "鳴らさなくても録ってあることは変わらない"
        );
    }

    /// 下位方式は持ち主を見ない。 行の頭に近い素材を採り、同じ深さなら名前で決める（`DEC-PKG-014`）。
    #[test]
    fn 下位方式は浅い素材を採り_同じ深さなら名前で決める() {
        let produced = vec![
            Production {
                key: (60, "い"),
                row: row("b"),
                depth: 0,
            },
            Production {
                key: (60, "い"),
                row: row("a"),
                depth: 0,
            },
            Production {
                key: (60, "い"),
                row: row("c"),
                depth: 2,
            },
            Production {
                key: (60, "う"),
                row: row("c"),
                depth: 2,
            },
            Production {
                key: (60, "う"),
                row: row("a"),
                depth: 3,
            },
        ];
        let f: Facts<Key> = Facts {
            produced,
            adopted: [("a", 1), ("b", 2), ("c", 3)]
                .into_iter()
                .map(|(r, t)| {
                    (
                        row(r),
                        Adoption {
                            take: TakeId::new(t),
                            name: r.to_owned(),
                        },
                    )
                })
                .collect(),
            first_valid: BTreeMap::new(),
            otos: BTreeMap::new(),
        };
        let r = resolve(
            &f,
            UseContext::Export {
                delivery: Delivery::Downgrade {
                    target: Method::Single,
                },
            },
            cv,
        );
        let used: Vec<(Key, &str)> = r.used().map(|c| (c.key, c.row.as_str())).collect();
        assert_eq!(used, [((60, "い"), "a"), ((60, "う"), "c")]);
        assert_eq!(
            disposition_of(&r, (60, "い"), "b"),
            Disposition::Skip(Reason::Outranked)
        );
    }

    /// 下位方式の選びも音高ごと。 音高を跨いで1つに絞らない。
    #[test]
    fn 下位方式は音高ごとに選ぶ() {
        let f: Facts<Key> = Facts {
            produced: vec![
                Production {
                    key: (55, "あ"),
                    row: row("x"),
                    depth: 1,
                },
                Production {
                    key: (62, "あ"),
                    row: row("y"),
                    depth: 1,
                },
            ],
            adopted: [("x", 1), ("y", 2)]
                .into_iter()
                .map(|(r, t)| {
                    (
                        row(r),
                        Adoption {
                            take: TakeId::new(t),
                            name: r.to_owned(),
                        },
                    )
                })
                .collect(),
            first_valid: BTreeMap::new(),
            otos: BTreeMap::new(),
        };
        let r = resolve(
            &f,
            UseContext::Export {
                delivery: Delivery::Downgrade {
                    target: Method::Single,
                },
            },
            cv,
        );
        assert_eq!(r.used().count(), 2);
    }

    /// 同じ行が同じ鍵を重ねて名乗っても、候補は1つ。
    #[test]
    fn 同じ行が同じ鍵を重ねても候補は1つ() {
        let mut f = make(
            &[((60, "か"), "r1")],
            &[("r1", 1)],
            &[("r1", 1)],
            &[(1, (60, "か"))],
        );
        f.produced.push(f.produced[0].clone());
        let r = resolve(&f, EXPORT, cv);
        assert_eq!(r.candidates().len(), 1);
    }

    /// 理由の件数が候補の数と合う。
    #[test]
    fn 件数は理由ごとに数えられる() {
        let r = resolve(&full_and_packed(false), UseContext::Preview, cv);
        assert_eq!(r.count(Disposition::Use), 2);
        assert_eq!(r.count(Disposition::Skip(Reason::NotOwner)), 1);
        assert_eq!(r.candidates().len(), 3);
    }

    // ## 性質の試験

    /// 小さな疑似乱数。 試験を決定的にする。
    struct Rng(u64);

    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }

        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    const SPELLINGS: [&str; 5] = ["あ", "か", "a か", "a k", "a -"];

    fn random_facts(rng: &mut Rng) -> Facts<Key> {
        let rows: Vec<String> = (0..5).map(|i| format!("r{i}")).collect();
        let mut f: Facts<Key> = Facts::empty();
        let mut next_take = 1;
        for r in &rows {
            // 録っていない行・採用が無効な行も混ぜる。
            if rng.below(4) == 0 {
                continue;
            }
            let take = TakeId::new(next_take);
            next_take += 1 + i32::try_from(rng.below(3)).unwrap_or(0);
            f.adopted.insert(
                row(r),
                Adoption {
                    take,
                    name: format!("n{}", rng.below(3)),
                },
            );
            f.first_valid.insert(row(r), take);
        }
        for r in &rows {
            for _ in 0..rng.below(5) {
                let tone = if rng.below(2) == 0 { 60 } else { 72 };
                let key = (tone, SPELLINGS[usize::try_from(rng.below(5)).unwrap_or(0)]);
                f.produced.push(Production {
                    key,
                    row: row(r),
                    depth: usize::try_from(rng.below(4)).unwrap_or(0),
                });
                if let Some(a) = f.adopted.get(&row(r))
                    && rng.below(3) != 0
                {
                    f.otos.insert((a.take, key), ());
                }
            }
        }
        f
    }

    fn shuffled(f: &Facts<Key>, rng: &mut Rng) -> Facts<Key> {
        let mut g = f.clone();
        for i in (1..g.produced.len()).rev() {
            let j = usize::try_from(rng.below(u64::try_from(i + 1).unwrap_or(1))).unwrap_or(0);
            g.produced.swap(i, j);
        }
        g
    }

    /// 試唱と書き出しは同じ候補を同じ並びで得る。 扱いが割れるのは、用途が決めた
    /// 語尾の1点だけで、使う候補の差は語尾だけ。 事実の並びを替えても結果は変わらず、
    /// 鍵ごとの持ち主は1つ。
    #[test]
    fn 試唱と書き出しは用途が決めたところだけ違う() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for _ in 0..500 {
            let f = random_facts(&mut rng);
            let p = resolve(&f, UseContext::Preview, ending);
            let e = resolve(&f, EXPORT, ending);

            let ids = |r: &Resolution<Key>| -> Vec<(Key, RowId, TakeId)> {
                r.candidates()
                    .iter()
                    .map(|c| (c.key, c.row.clone(), c.take))
                    .collect()
            };
            assert_eq!(ids(&p), ids(&e), "同じ候補が同じ並びで出る");
            assert_eq!(p.covered(), e.covered());

            for (cp, ce) in p.candidates().iter().zip(e.candidates()) {
                if cp.disposition == ce.disposition {
                    continue;
                }
                assert_eq!(ending(&cp.key), Role::Ending, "割れるのは語尾だけ");
                assert_eq!(
                    cp.disposition,
                    Disposition::Skip(Reason::NotSounded),
                    "試唱の側が鳴らさない"
                );
                assert_ne!(ce.disposition, Disposition::Skip(Reason::NotOwner));
            }
            let sounded: BTreeSet<(Key, RowId)> = e
                .used()
                .filter(|c| ending(&c.key) != Role::Ending)
                .map(|c| (c.key, c.row.clone()))
                .collect();
            let previewed: BTreeSet<(Key, RowId)> =
                p.used().map(|c| (c.key, c.row.clone())).collect();
            assert_eq!(previewed, sounded, "鳴らす候補は書き出しと揃う");

            // 鍵ごとに使う候補は高々1つ。
            let mut seen = BTreeSet::new();
            for c in e.used() {
                assert!(seen.insert(c.key), "{:?} の候補が2つ", c.key);
            }

            // 並びに依らない。
            let g = shuffled(&f, &mut rng);
            assert_eq!(resolve(&g, UseContext::Preview, ending), p);
            assert_eq!(resolve(&g, EXPORT, ending), e);
        }
    }

    /// 持ち主は、録ってある行のうち最初に確定したテイクを持つ行。 使う候補は
    /// 持ち主のテイクが5値を持つものだけ。
    #[test]
    fn 使う候補は持ち主のテイクが5値を持つものだけ() {
        let mut rng = Rng(0x1234_5678_9abc_def1);
        for _ in 0..500 {
            let f = random_facts(&mut rng);
            let r = resolve(&f, EXPORT, cv);
            for c in r.used() {
                assert!(f.otos.contains_key(&(c.take, c.key)));
                // 同じ鍵を生む、録ってある行のどれより先に録った行。
                let at = f.first_valid[&c.row];
                for p in &f.produced {
                    if p.key == c.key && f.adopted.contains_key(&p.row) {
                        assert!(at <= f.first_valid[&p.row]);
                    }
                }
            }
        }
    }

    // ## 同値の組

    /// 同じ綴りに描かれる2つの対象は、鍵を綴りで持っても同値の組の代表で持っても
    /// 1つに数える（`DEC-RCL-017`）。 CVVC の2音目以降の CV は直前の母音を問わず素の CV。
    #[test]
    fn 同値の対象は1つに数える() {
        let rules = Rules::builtin(UnitSet::Core);
        let catalog = Catalog::new(&rules, Method::Cvvc, UnitSet::Core);
        let table = units(UnitSet::Core);
        let pick = |kana: &str| {
            table
                .iter()
                .find(|u| u.kana == kana)
                .cloned()
                .expect("単位")
        };
        // 「さ か」と「い か」。 か は直前の母音が違うが、同じ綴りに描かれる。
        let lines = [
            ("r1", vec![pick("さ"), pick("か")]),
            ("r2", vec![pick("い"), pick("か")]),
        ];
        let mut by_spelling: Facts<(i32, String)> = Facts::empty();
        let mut by_class: Facts<(i32, Target)> = Facts::empty();
        for (n, (id, line)) in lines.iter().enumerate() {
            let take = TakeId::new(i32::try_from(n + 1).expect("小さい"));
            let adoption = Adoption {
                take,
                name: (*id).to_owned(),
            };
            by_spelling.adopted.insert(row(id), adoption.clone());
            by_class.adopted.insert(row(id), adoption);
            by_spelling.first_valid.insert(row(id), take);
            by_class.first_valid.insert(row(id), take);
            for (t, slot) in target::of_row(Method::Cvvc, line) {
                by_spelling.produced.push(Production {
                    key: (60, target::render(&rules, Method::Cvvc, &t)),
                    row: row(id),
                    depth: slot.mora(),
                });
                by_class.produced.push(Production {
                    key: (60, catalog.canonical(&t).expect("一覧にある")),
                    row: row(id),
                    depth: slot.mora(),
                });
            }
        }
        let x = resolve(&by_spelling, EXPORT, |_| Role::Cv);
        let y = resolve(&by_class, EXPORT, |_| Role::Cv);
        // か は2つの行から生まれるが、持ち主は1つ。 5値は置いていないので `NoOto` が持ち主。
        let holders = |r: &[Disposition]| {
            r.iter()
                .filter(|d| **d != Disposition::Skip(Reason::NotOwner))
                .count()
        };
        let ka: Vec<Disposition> = x
            .candidates()
            .iter()
            .filter(|c| c.key.1 == "か")
            .map(|c| c.disposition)
            .collect();
        assert_eq!(ka.len(), 2, "2つの行が名乗る");
        assert_eq!(holders(&ka), 1);
        // 鍵を同値の組の代表で持っても、数も持ち主も変わらない。
        assert_eq!(x.covered().len(), y.covered().len());
        fn dispositions<K>(r: &[Candidate<K>]) -> Vec<(RowId, Disposition)> {
            let mut v: Vec<_> = r.iter().map(|c| (c.row.clone(), c.disposition)).collect();
            v.sort();
            v
        }
        assert_eq!(dispositions(x.candidates()), dispositions(y.candidates()));
    }

    /// 語尾を役割で引く。
    #[test]
    fn 綴りの役割は一覧から引く() {
        let rules = Rules::builtin(UnitSet::Core);
        let catalog = Catalog::new(&rules, Method::Cvvc, UnitSet::Core);
        assert_eq!(Role::of(&catalog, &rules.ending("a")), Role::Ending);
        assert_eq!(Role::of(&catalog, &rules.vc("a", "k")), Role::Vc);
        assert_eq!(Role::of(&catalog, "か"), Role::Cv);
        assert_eq!(
            Role::of(&catalog, "知らない綴り"),
            Role::Cv,
            "知らない綴りは鳴らす側へ倒す"
        );
    }
}
