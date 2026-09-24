//! エイリアスの解決（`TR-SYN-12`, `TR-RCL-15`, `TR-RCL-20`）。
//!
//! これが単一の定義。 カバレッジ判定・書き出し・試唱が同じコードパスを通る
//! （`TR-SYN-12`, `TR-RCL-20`）。3箇所で別々に解くと、
//! 「歌えると出たのに書き出したら鳴らない」が起きる。
//!
//! OpenUtau の公開された振る舞いを仕様として参照し、自前実装する（`TR-SYN-11`）。
//! コードは移植しない（`TR-PLT-10`）。
//!
//! # 抽象の形を保つ
//!
//! 「直前・直後の音符を参照して、音素列とエイリアス候補列を返す」という形を保つ
//! （`TR-SYN-11`）。同じ入力に同じ出力を返すことを検証できるようにするためであり、
//! 利用者が別の phonemizer を接続する差し替え点にもなる（`TR-SYN-35`）。
//!
//! # 綴りは持たない
//!
//! 綴りの表は [`crate::presamp::Rules`] が持つ（`TR-SYN-36`、`DEC-SYN-010`）。
//! ここが持つのは、どの文脈でどの節を引くか・どこに渡りを挟むかという
//! 構造だけ。**綴りをここにも書いていた。** 利用者が音源に置いた
//! `presamp.ini` は `Rules` にしか入らないので、解決は既定の綴りのまま走り、
//! 差し替え点が名前だけのものになっていた。

use std::collections::BTreeSet;

use crate::inventory::{Unit, UnitSet, units};
use crate::mora::{Mora, MoraKind};
use crate::presamp::Rules;

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
    /// 直前の音符の末尾母音クラス。無ければフレーズの先頭。
    pub previous_vowel: Option<&'a str>,
}

/// 解決の結果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolved {
    /// 実際に使うエイリアス。
    pub alias: String,
    /// 候補の何番目で当たったか。 0 が第一候補。
    ///
    /// 0 より大きければ「代替」として台帳に記録する（`TR-RCL-20`）。
    pub rank: usize,
    /// どの収録音高の素材で当たったか。 区画を分けない解決では `None`。
    pub tone: Option<i32>,
    /// floor の収録音高から何段降りたか。 0 が floor（`DEC-SYN-014`）。
    ///
    /// 0 より大きければ、第一候補でも「代替」（`TR-RCL-20`）。
    pub step: usize,
}

impl Resolved {
    /// 本来の素材か。 floor の区画の第一候補だけが真（`DEC-SYN-014`）。
    #[must_use]
    pub const fn is_exact(&self) -> bool {
        self.rank == 0 && self.step == 0
    }
}

/// 1音符が素材を探す区画の並び（`DEC-SYN-014`）。
///
/// 先頭が floor の区画で、そこから下へ降りる。 区画を分けない解決では
/// `(None, 持っている集合)` の1つだけ。
pub type Chain<'a> = Vec<(Option<i32>, &'a BTreeSet<String>)>;

/// 解決できなかった音符。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Missing {
    pub lyric: String,
    pub tried: Vec<String>,
}

/// 音符1つのエイリアス候補列を作る（`TR-SYN-12`）。
///
/// 順序がそのまま優先順位。 先頭から順に、持っているものを探す。
///
/// 綴りは `rules` が決める（`TR-SYN-36`）。 ここが決めるのは、
/// 直前の母音の有無でどの節をどの順に引くかだけ。
#[must_use]
pub fn candidates(rules: &Rules, method: Method, req: &Request<'_>) -> Vec<String> {
    rules.candidates(method, req.lyric, req.previous_vowel)
}

/// 持っているエイリアスの集合から、1音符を解決する（`TR-SYN-12`）。
pub fn resolve(
    rules: &Rules,
    method: Method,
    req: &Request<'_>,
    available: &BTreeSet<String>,
) -> Result<Resolved, Missing> {
    resolve_in(rules, method, req, &vec![(None, available)])
}

/// 区画の並びから、1音符を解決する（`TR-RCL-20`, `DEC-SYN-014`）。
///
/// **1つの区画の中で候補を尽くしてから、下の区画へ降りる。** OpenUtau は
/// ノートの音高を含む区画の中で候補を試し、下の区画へは降りない。
/// 降りるのは KOERU が足した動きなので、区画の中で尽きたときだけにする。
///
/// **和集合で綴りを選んでいた。** 選んだ綴りがそのノートの区画に無く、
/// 素材を引く段で落ちて黙って鳴らなかった。曲の状態は「完全」と出ていた。
pub fn resolve_in(
    rules: &Rules,
    method: Method,
    req: &Request<'_>,
    chain: &Chain<'_>,
) -> Result<Resolved, Missing> {
    let tried = candidates(rules, method, req);
    for (step, (tone, available)) in chain.iter().enumerate() {
        for (rank, c) in tried.iter().enumerate() {
            if available.contains(c) {
                return Ok(Resolved {
                    alias: c.clone(),
                    rank,
                    tone: *tone,
                    step,
                });
            }
        }
    }
    Err(Missing {
        lyric: req.lyric.to_owned(),
        tried,
    })
}

/// モーラ列から、方式ごとの必要エイリアス集合を作る（`TR-RCL-15`）。
///
/// 第一候補だけを数える。 「フォールバックすれば足りる」ものを必要集合に入れると、
/// 何を録れば完全になるのかが分からなくなる。
/// フォールバックで解決できるかは [`resolve`] が別に答える。
#[must_use]
pub fn required_aliases(
    rules: &Rules,
    method: Method,
    moras: &[Mora],
    breaks: &BTreeSet<usize>,
) -> BTreeSet<String> {
    required_entries(rules, method, moras, breaks)
        .into_iter()
        .map(|(_, a)| a)
        .collect()
}

/// 必要エイリアスを、それを鳴らすモーラと組にして返す（`TR-RCL-15`）。
///
/// 多音階では必要単位が（エイリアス, 収録音高）の組になる。 音高はモーラの
/// ノートから決まるので、どのモーラが要求したかを残す。
///
/// 渡り（CVVC の VC）は直前のモーラが持つ。 その尾に乗って鳴るので
/// （[`resolve_phrase`] の [`Role::Transition`]）、音高も直前のノートのもの。
#[must_use]
pub fn required_entries(
    rules: &Rules,
    method: Method,
    moras: &[Mora],
    breaks: &BTreeSet<usize>,
) -> BTreeSet<(usize, String)> {
    let mut out = BTreeSet::new();
    let mut prev_vowel: Option<String> = None;

    for (i, m) in moras.iter().enumerate() {
        // 休符で綴りの文脈が切れる（`TR-RCL-12`）。 直前の母音を落とすと、
        // ここからは語頭形を要求する。
        if breaks.contains(&i) {
            prev_vowel = None;
        }
        match m.kind {
            // 長音は直前母音の継続。 新たなエイリアスを要求しない。
            MoraKind::LongVowel => continue,
            // 促音は単位を要求しない。 直後の CV の子音部を要求するが、
            // その子音は次のモーラのエイリアスが持っている。
            MoraKind::Geminate => continue,
            MoraKind::Syllable | MoraKind::Moraic => {}
        }
        let Some(unit) = m.unit else { continue };

        let req = Request {
            lyric: unit,
            previous_vowel: prev_vowel.as_deref(),
        };
        if let Some(first) = candidates(rules, method, &req).first() {
            out.insert((i, first.clone()));
        }
        /*
          CVVC は渡りも要る（`TR-RCL-05`）。

          **CV だけを数えていた。** そうすると、曲から詰め直したリスト
          （`TR-RCL-16`）に VC が1つも入らず、**録っても渡りが鳴らない音源**が
          できる。CVVC を選んだ意味が無くなる。
        */
        if method == Method::Cvvc
            && let Some(prev) = prev_vowel.as_deref()
        {
            let c = rules.consonant_of(unit);
            if !c.is_empty() {
                // 直前の母音があるなら、直前のモーラがある。
                out.insert((i.saturating_sub(1), rules.vc(prev, c)));
            }
        }
        prev_vowel = non_empty(rules.vowel_of(unit)).map(str::to_owned);
    }
    out
}

/// 空を「持っていない」に畳む。
///
/// `Rules` の所属表は、知らない仮名に空文字を返す。 そのまま持ち回ると、
/// 母音クラスが空のまま次のモーラの `%v%` に入り、`" か"` のような
/// 先頭が空白の綴りができる。
fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

/// フレーズの1拍（`TR-SYN-18`）。
///
/// モーラと1対1で並ぶ。 呼び出し側は結果の添字で音符を引くので、
/// 拍を落とすと、それ以降の音符の音高と長さが1つずつずれる。
/// 長音と促音を落としていて、実際にずれた（`DEC-SYN-009`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PhraseUnit {
    /// 素材を鳴らす。
    Sound(Resolved),
    /// 鳴らさない拍。 促音は閉鎖であって、素材を持たない
    /// （`MoraKind::Geminate`。子音部は次のモーラのエイリアスが持っている）。
    Rest,
    /// 素材が無い。
    Missing(Missing),
}

impl PhraseUnit {
    /// 鳴らせるか。休符も「鳴らせる」に数える——素材の不足ではない。
    #[must_use]
    pub const fn is_playable(&self) -> bool {
        !matches!(self, Self::Missing(_))
    }
}

impl PhraseEntry {
    /// 鳴らせるか。
    #[must_use]
    pub const fn is_playable(&self) -> bool {
        self.unit.is_playable()
    }

    /// 音符の本体か。 渡りは偽。
    #[must_use]
    pub const fn is_main(&self) -> bool {
        matches!(self.role, Role::Main)
    }
}

/// フレーズの1要素が、音符の本体か渡りか（`TR-SYN-12`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// その音符そのもの。
    Main,
    /// 次の音符へ入る渡り（CVVC の VC）。
    ///
    /// **音符ではない。** 置くのは、属している音符の尾
    /// （OpenUtau の `position = totalDuration - vcLength`）。
    /// 長さは**次の音符の CV の先行発声**から決まる——presamp の
    /// `[VCLENGTH] 0`（既定）と同じ。決めるのは呼び出し側で、
    /// ここは「どこに何を挟むか」だけを返す。
    Transition,
}

/// フレーズの1要素（`TR-SYN-12`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhraseEntry {
    pub unit: PhraseUnit,
    /// どのモーラのものか。 渡りは、その尾に乗る側のモーラを指す。
    pub mora: usize,
    pub role: Role,
}

/// フレーズ全体を解決する。
///
/// 解決できた音符と、できなかった音符の両方を返す。
/// できなかったものを黙って飛ばすと、短縮版（`TR-SYN-18`）が作れない。
///
/// # モーラと1対1ではない
///
/// 長音も促音も、拍として1つ返す。 **加えて CVVC は渡り（VC）を挟む**ので、
/// 要素の数はモーラの数より多くなりうる。だから添字では音符を引けない
/// ——どのモーラのものかは [`PhraseEntry::mora`] が持つ。
///
/// 添字で引いていた。 長音と促音を落として音高と長さがずれ（`DEC-SYN-009`）、
/// そのとき「モーラの数だけ返す」に直した。**今度は渡りで壊れる形だった**ので、
/// 数を合わせるのではなく、要素が自分の出どころを名乗るようにした。
///
/// - 長音（`ー`）は 直前モーラの末尾母音の継続（`MoraKind::LongVowel`）。
///   その母音の単独単位（あ/い/う/え/お）へ解決する。母音を伸ばすのが長音。
/// - 促音（`っ`）は [`PhraseUnit::Rest`]。閉鎖なので素材が無い。
///
/// # 渡りを挟む条件（`TR-SYN-12`）
///
/// CVVC で、直前に母音があり、その音符が子音を持ち、VC の綴りを持っているとき。
/// **無ければ挟まない**——「VC は該当エイリアスが無ければ VC 音素を出力せず
/// CV のみで繋ぐ」。
///
/// 語尾（`a -`）は挟まない。 OpenUtau の presamp phonemizer が挿さないので
/// 揃える（`TR-SYN-11`）。録った語尾は配布物の素材として入り、
/// 受け取った側の presamp が使う。
#[must_use]
pub fn resolve_phrase(
    rules: &Rules,
    method: Method,
    moras: &[Mora],
    available: &BTreeSet<String>,
    set: UnitSet,
    breaks: &BTreeSet<usize>,
) -> Vec<PhraseEntry> {
    resolve_phrase_in(
        rules,
        method,
        moras,
        |_| vec![(None, available)],
        set,
        breaks,
    )
}

/// フレーズ全体を、モーラごとの区画の並びで解決する（`TR-RCL-20`, `DEC-SYN-014`）。
///
/// `chain_of(i)` はモーラ `i` のノートが使える区画を、floor から下へ並べたもの。
/// 候補の順は [`resolve_in`]。
///
/// 渡りは、その尾に乗る直前のモーラの区画から引く。 同じ順で探し、
/// どこにも無ければ挟まない（`TR-SYN-12`）。
#[must_use]
pub fn resolve_phrase_in<'a, F>(
    rules: &Rules,
    method: Method,
    moras: &[Mora],
    chain_of: F,
    set: UnitSet,
    breaks: &BTreeSet<usize>,
) -> Vec<PhraseEntry>
where
    F: Fn(usize) -> Chain<'a>,
{
    let table = units(set);
    let mut out: Vec<PhraseEntry> = Vec::new();
    let mut prev_vowel: Option<String> = None;

    for (i, m) in moras.iter().enumerate() {
        // 休符で綴りの文脈が切れる（`TR-RCL-12`）。 渡りも挟まない
        // ——間があるのだから、繋ぐ音は要らない。
        if breaks.contains(&i) {
            prev_vowel = None;
        }
        let main = |unit| PhraseEntry {
            unit,
            mora: i,
            role: Role::Main,
        };
        match m.kind {
            // 直前母音の継続。 母音の単独単位で伸ばす。
            // prev_vowel は据え置く——長音のあとも、母音は変わらない。
            MoraKind::LongVowel => {
                let unit = prev_vowel
                    .as_deref()
                    .and_then(|v| vowel_unit(&table, v))
                    .map(|kana| {
                        let req = Request {
                            lyric: kana,
                            previous_vowel: prev_vowel.as_deref(),
                        };
                        resolve_in(rules, method, &req, &chain_of(i))
                    });
                out.push(main(match unit {
                    Some(Ok(r)) => PhraseUnit::Sound(r),
                    Some(Err(e)) => PhraseUnit::Missing(e),
                    // 直前に母音が無い（曲の頭が長音など）。**黙って飛ばさない。**
                    None => PhraseUnit::Missing(Missing {
                        lyric: "ー".to_owned(),
                        tried: Vec::new(),
                    }),
                }));
                continue;
            }
            // 素材を持たない拍。 落とすと、以降の音符がずれる。
            MoraKind::Geminate => {
                out.push(main(PhraseUnit::Rest));
                continue;
            }
            MoraKind::Syllable | MoraKind::Moraic => {}
        }
        let Some(unit) = m.unit else {
            out.push(main(PhraseUnit::Rest));
            continue;
        };

        /*
          渡り（CVVC の VC）を先に挟む。

          時間順では「直前の音符の尾 → この音符」なので、この音符の手前へ置く。
          属するのは直前の要素——その尾に乗るから（OpenUtau の
          `position = totalDuration - vcLength`）。

          綴りが無ければ挟まない（`TR-SYN-12`）。
        */
        if method == Method::Cvvc
            && let Some(prev) = prev_vowel.as_deref()
            && let Some(consonant) = non_empty(rules.consonant_of(unit))
            && let Some(owner) = out.last().map(|e| e.mora)
        {
            let alias = rules.vc(prev, consonant);
            let hit = chain_of(owner)
                .into_iter()
                .enumerate()
                .find(|(_, (_, available))| available.contains(&alias));
            if let Some((step, (tone, _))) = hit {
                out.push(PhraseEntry {
                    unit: PhraseUnit::Sound(Resolved {
                        alias,
                        rank: 0,
                        tone,
                        step,
                    }),
                    mora: owner,
                    role: Role::Transition,
                });
            }
        }

        let req = Request {
            lyric: unit,
            previous_vowel: prev_vowel.as_deref(),
        };
        out.push(main(match resolve_in(rules, method, &req, &chain_of(i)) {
            Ok(r) => PhraseUnit::Sound(r),
            Err(e) => PhraseUnit::Missing(e),
        }));
        prev_vowel = non_empty(rules.vowel_of(unit)).map(str::to_owned);
    }
    out
}

/// その母音クラスの単独母音（あ/い/う/え/お）。
fn vowel_unit<'a>(table: &'a [Unit], vowel: &str) -> Option<&'a str> {
    table
        .iter()
        .find(|u| u.consonant.is_empty() && u.vowel == vowel)
        .map(|u| u.kana)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 既定の綴り（`TR-SYN-36`）。 差し替えていない音源はこれを通る。
    fn builtin_rules() -> Rules {
        Rules::builtin(UnitSet::Core)
    }

    use crate::mora::parse;

    fn have(xs: &[&str]) -> BTreeSet<String> {
        xs.iter().map(|s| (*s).to_owned()).collect()
    }

    /// 単独音は「{歌詞}」だけ。 見つからなければ欠損（`TR-SYN-12`）。
    #[test]
    fn 単独音の候補は一つだけ() {
        let req = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(candidates(&builtin_rules(), Method::Single, &req), ["か"]);

        assert!(resolve(&builtin_rules(), Method::Single, &req, &have(&["さ"])).is_err());
        assert_eq!(
            resolve(&builtin_rules(), Method::Single, &req, &have(&["か"]))
                .expect("解決できる")
                .rank,
            0
        );
    }

    /// 連続音は直前の母音を見る（`TR-SYN-12`）。
    #[test]
    fn 連続音の候補は要件どおりの順序() {
        let with_prev = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(
            candidates(&builtin_rules(), Method::Sequential, &with_prev),
            ["a か", "* か", "か", "- か"]
        );

        let head = Request {
            lyric: "か",
            previous_vowel: None,
        };
        assert_eq!(
            candidates(&builtin_rules(), Method::Sequential, &head),
            ["- か", "か"]
        );
    }

    /// 順に落ちる。 何番目で当たったかを返す（`TR-RCL-20` の「代替」）。
    #[test]
    fn 連続音は順に落ちる() {
        let req = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(
            resolve(&builtin_rules(), Method::Sequential, &req, &have(&["a か"]))
                .expect("")
                .rank,
            0
        );
        assert_eq!(
            resolve(&builtin_rules(), Method::Sequential, &req, &have(&["* か"]))
                .expect("")
                .rank,
            1
        );
        assert_eq!(
            resolve(&builtin_rules(), Method::Sequential, &req, &have(&["か"]))
                .expect("")
                .rank,
            2,
            "単独音で録ったものが連続音の代替になる"
        );
        assert_eq!(
            resolve(&builtin_rules(), Method::Sequential, &req, &have(&["- か"]))
                .expect("")
                .rank,
            3
        );
    }

    /// CVVC も直前を見る（`DEC-SYN-011`）。 語頭形は語頭でだけ先に来る。
    #[test]
    fn cvvc_の_cv_は文脈で順序が変わる() {
        let mid = Request {
            lyric: "か",
            previous_vowel: Some("a"),
        };
        assert_eq!(
            candidates(&builtin_rules(), Method::Cvvc, &mid),
            ["か", "- か"]
        );

        let head = Request {
            lyric: "か",
            previous_vowel: None,
        };
        assert_eq!(
            candidates(&builtin_rules(), Method::Cvvc, &head),
            ["- か", "か"]
        );
    }

    /// VC と語尾の綴りは1箇所（`TR-RCL-05`）。
    #[test]
    fn vc_と語尾の綴り() {
        assert_eq!(builtin_rules().vc("a", "k"), "a k");
        assert_eq!(builtin_rules().ending("a"), "a -");
    }

    /// 必要集合は第一候補だけ（`TR-RCL-15`）。
    #[test]
    fn 必要集合は第一候補で作る() {
        let m = parse("さくら", UnitSet::Core).expect("読める");

        let single = required_aliases(&builtin_rules(), Method::Single, &m, &BTreeSet::new());
        assert_eq!(single, have(&["さ", "く", "ら"]));

        let seq = required_aliases(&builtin_rules(), Method::Sequential, &m, &BTreeSet::new());
        assert_eq!(seq, have(&["- さ", "a く", "u ら"]));
    }

    /// 長音と促音は必要集合に入らない（`TR-RCL-13` (b)(c)）。
    #[test]
    fn 長音と促音は必要集合に入らない() {
        let m = parse("かーきって", UnitSet::Core).expect("読める");
        let single = required_aliases(&builtin_rules(), Method::Single, &m, &BTreeSet::new());
        assert_eq!(single, have(&["か", "き", "て"]));
    }

    /// 解決できたものとできなかったものを両方返す（`TR-SYN-18` の短縮版に要る）。
    #[test]
    fn フレーズの欠損を数えられる() {
        let m = parse("さくら", UnitSet::Core).expect("読める");
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Single,
            &m,
            &have(&["さ", "ら"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert_eq!(got.len(), 3);
        assert!(got[0].is_playable());
        assert!(!got[1].is_playable(), "く が無い");
        assert!(got[2].is_playable());

        let PhraseUnit::Missing(missing) = &got[1].unit else {
            panic!()
        };
        assert_eq!(missing.lyric, "く");
        assert_eq!(missing.tried, ["く"]);
    }

    /// 長音は直前母音を伸ばす。落とさない（`DEC-SYN-009`）。
    ///
    /// 落とすと、呼び出し側が結果の添字で音符を引くので、
    /// それ以降の音高と長さが1つずつずれる。
    #[test]
    fn 長音は直前母音として鳴る() {
        let m = parse("かーさ", UnitSet::Core).expect("読める");
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Single,
            &m,
            &have(&["か", "あ", "さ"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert_eq!(got.len(), 3, "音符の数と揃うこと");
        let alias = |i: usize| match &got[i].unit {
            PhraseUnit::Sound(r) => r.alias.clone(),
            other => panic!("{other:?}"),
        };
        assert_eq!(alias(0), "か");
        assert_eq!(alias(1), "あ", "ー は直前母音 a の単独単位で伸ばす");
        assert_eq!(alias(2), "さ");
        // 添字ではなく、どのモーラのものかを名乗る。
        assert_eq!([got[0].mora, got[1].mora, got[2].mora], [0, 1, 2]);
    }

    /// 長音のあとも母音は変わらない。 連続音の前母音がずれないこと。
    #[test]
    fn 長音のあとも直前母音は変わらない() {
        let m = parse("きーい", UnitSet::Core).expect("読める");
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Single,
            &m,
            &have(&["き", "い"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert_eq!(got.len(), 3);
        assert_eq!(
            got[1].unit,
            PhraseUnit::Sound(Resolved {
                alias: "い".to_owned(),
                rank: 0,
                tone: None,
                step: 0,
            })
        );
    }

    /// 促音は鳴らさない拍として残る（`MoraKind::Geminate`）。
    /// 素材の不足ではないので、鳴らせないとは数えない。
    #[test]
    fn 促音は休符として残る() {
        let m = parse("きって", UnitSet::Core).expect("読める");
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Single,
            &m,
            &have(&["き", "て"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert_eq!(got.len(), 3, "っ も1拍として並ぶこと");
        assert_eq!(got[1].unit, PhraseUnit::Rest);
        assert!(got.iter().all(PhraseEntry::is_playable));
    }

    /// 伸ばす母音が無い長音は、欠損として返す。黙って飛ばさない。
    ///
    /// `parse` は先頭の長音を `DanglingModifier` で弾くので、
    /// この経路は通常は通らない。それでも落とさない——
    /// 落とすと、以降の音符が1つずれる形に戻る。
    #[test]
    fn 伸ばす母音が無ければ欠損として返す() {
        let m = vec![
            Mora {
                text: "ー".to_owned(),
                unit: None,
                kind: MoraKind::LongVowel,
            },
            Mora {
                text: "あ".to_owned(),
                unit: Some("あ"),
                kind: MoraKind::Syllable,
            },
        ];
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Single,
            &m,
            &have(&["あ"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert_eq!(got.len(), 2, "拍の数は減らさない");
        assert!(!got[0].is_playable());
        assert!(got[1].is_playable());
    }

    /// カバレッジ判定と試唱が同じコードパスを通る（`TR-SYN-12`, `TR-RCL-20`）。
    #[test]
    fn 必要集合を全部持っていればフレーズが解決する() {
        let m = parse("さくらさくら", UnitSet::Core).expect("読める");
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            let need = required_aliases(&builtin_rules(), method, &m, &BTreeSet::new());
            let got = resolve_phrase(
                &builtin_rules(),
                method,
                &m,
                &need,
                UnitSet::Core,
                &BTreeSet::new(),
            );
            assert!(
                got.iter().all(PhraseEntry::is_playable),
                "{method:?}: 必要集合を持てば全部解決すること"
            );
        }
    }

    /// CVVC は音符のあいだに渡り（VC）を挟む（`TR-SYN-12`, `TR-RCL-05`）。
    ///
    /// **挟んでいなかった。** VC を 180 個録っても歌唱で一度も鳴らず、
    /// 単独音と同じ鳴り方になっていた。
    #[test]
    fn cvvc_は渡りを挟む() {
        let m = parse("さか", UnitSet::Core).expect("読める");
        let need = required_aliases(&builtin_rules(), Method::Cvvc, &m, &BTreeSet::new());
        assert!(need.contains("a k"), "必要集合に渡りが入る: {need:?}");

        let got = resolve_phrase(
            &builtin_rules(),
            Method::Cvvc,
            &m,
            &need,
            UnitSet::Core,
            &BTreeSet::new(),
        );
        let seen: Vec<(&str, Role, usize)> = got
            .iter()
            .map(|e| match &e.unit {
                PhraseUnit::Sound(r) => (r.alias.as_str(), e.role, e.mora),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(
            seen,
            [
                ("- さ", Role::Main, 0),
                // 渡りは直前の音符の尾に乗る。属するのは 0 番目。
                ("a k", Role::Transition, 0),
                ("か", Role::Main, 1),
            ]
        );
    }

    /// 渡りの綴りが無ければ挟まない（`TR-SYN-12`）。
    ///
    /// > VC は該当エイリアスが無ければ VC 音素を出力せず CV のみで繋ぐ
    #[test]
    fn 渡りが無ければ_cv_だけで繋ぐ() {
        let m = parse("さか", UnitSet::Core).expect("読める");
        let got = resolve_phrase(
            &builtin_rules(),
            Method::Cvvc,
            &m,
            &have(&["- さ", "か"]),
            UnitSet::Core,
            &BTreeSet::new(),
        );
        assert!(
            got.iter().all(|e| e.is_main()),
            "渡りを作らない: {:?}",
            got.iter().map(|e| e.role).collect::<Vec<_>>()
        );
        assert_eq!(got.len(), 2);
    }

    /// ノートが使えない区画の綴りを選ばない（`DEC-SYN-014`）。
    ///
    /// G3 と D4 の2音階で、D4 だけに `a か` がある。A3 のノートは G3 以下しか
    /// 使えないので、G3 の代用の `か` で鳴る。**和集合で選ぶと `a か` に決まり、
    /// G3 に無いので鳴らなかった。**
    #[test]
    fn ノートが使えない区画の綴りを選ばない() {
        let m = parse("あか", UnitSet::Core).expect("読める");
        let g3 = have(&["- あ", "か"]);
        let d4 = have(&["- あ", "a か"]);
        let got = resolve_phrase_in(
            &builtin_rules(),
            Method::Sequential,
            &m,
            // どちらのノートも A3。 使えるのは floor の G3 だけ。
            |_| vec![(Some(55), &g3)],
            UnitSet::Core,
            &BTreeSet::new(),
        );
        let PhraseUnit::Sound(ka) = &got[1].unit else {
            panic!("鳴る: {got:?}")
        };
        assert_eq!(ka.alias, "か");
        assert_eq!(ka.tone, Some(55));
        assert!(!ka.is_exact(), "代用なので代替");
        assert!(d4.contains("a か"), "D4 には本来の綴りがあるが使わない");
    }

    /// 同じ区画の中の代用を、下の区画の本来の綴りより先に試す（`DEC-SYN-014`）。
    ///
    /// E4 のノート。floor の D4 には代用の `か` しか無く、G3 には `a か` がある。
    /// 高さを守る——OpenUtau は区画の中で候補を試し、下の区画へは降りない。
    #[test]
    fn 区画の中の代用を下の区画より先に試す() {
        let m = parse("あか", UnitSet::Core).expect("読める");
        let d4 = have(&["- あ", "か"]);
        let g3 = have(&["- あ", "a か"]);
        let got = resolve_phrase_in(
            &builtin_rules(),
            Method::Sequential,
            &m,
            |_| vec![(Some(62), &d4), (Some(55), &g3)],
            UnitSet::Core,
            &BTreeSet::new(),
        );
        let PhraseUnit::Sound(ka) = &got[1].unit else {
            panic!("鳴る: {got:?}")
        };
        assert_eq!((ka.alias.as_str(), ka.tone, ka.step), ("か", Some(62), 0));

        // floor の区画に何も無ければ、下の区画へ降りる。
        let empty = BTreeSet::new();
        let got = resolve_phrase_in(
            &builtin_rules(),
            Method::Sequential,
            &m,
            |_| vec![(Some(62), &empty), (Some(55), &g3)],
            UnitSet::Core,
            &BTreeSet::new(),
        );
        let PhraseUnit::Sound(ka) = &got[1].unit else {
            panic!("鳴る: {got:?}")
        };
        assert_eq!((ka.alias.as_str(), ka.tone, ka.step), ("a か", Some(55), 1));
        assert!(!ka.is_exact(), "下の区画で鳴るのは代替");
    }

    /// 単独音と連続音は渡りを挟まない。 つなぎ目は素材の中にある。
    #[test]
    fn 単独音と連続音は渡りを挟まない() {
        let m = parse("さか", UnitSet::Core).expect("読める");
        for method in [Method::Single, Method::Sequential] {
            let need = required_aliases(&builtin_rules(), method, &m, &BTreeSet::new());
            let got = resolve_phrase(
                &builtin_rules(),
                method,
                &m,
                &need,
                UnitSet::Core,
                &BTreeSet::new(),
            );
            assert!(got.iter().all(|e| e.is_main()), "{method:?}");
            assert_eq!(got.len(), 2, "{method:?}");
        }
    }
}
