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
use crate::presamp::Rules;
use crate::tone;

/// 曲の1ノート（`TR-RCL-12` (a)(b)）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Note {
    /// 歌詞（1モーラぶん）。
    pub lyric: String,
    pub midi: i32,
    /// 長さ（ティック）。UST の 480 ティック = 4分音符。
    pub ticks: u32,
    /// この音符の前に置く休み（ティック、`TR-RCL-12`）。
    ///
    /// **休符を落とさない。** 一度は落としていた——UST の `R` も USTX の
    /// ノート間の空きも捨てていたので、取り込んだ曲が詰まって鳴り、
    /// **元の曲と違うリズムになった。**
    ///
    /// 休符を音符として持たない。 休みは歌詞もモーラも持たないので、
    /// 音符の並びに混ぜると被覆の計算に入ってしまう。
    pub rest_ticks: u32,
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
#[derive(Debug, Clone, PartialEq)]
pub struct Song {
    pub title: String,
    pub notes: Vec<Note>,
    /// 出典と許諾。
    pub provenance: Provenance,
    /// テンポ（`TR-SYN-30`）。
    ///
    /// 曲に1つ。 テンポ変化は持たない——課題曲は試唱のための短いフレーズで、
    /// 途中で速さが変わるものを想定していない。
    pub tempo_bpm: f64,
    /// 既定のポルタメント長（ミリ秒、`TR-SYN-30` の「既定ピッチベンド」）。
    ///
    /// 音符ごとのベンドは持たない。 UTAU の PBS/PBW/PBY/PBM は音符ごとの形だが、
    /// 試唱はフラグを既定に固定する（`TR-SYN-09`）ので、曲に1つで足りる。
    pub default_portamento_ms: f64,
    /// 本人が指定した移調量（半音、`TR-SYN-15`）。既定は 0。
    ///
    /// **自動では動かさない。** 以前は収録音高に近づくよう曲全体を勝手に
    /// 移調していた（`DEC-SYN-012`）。本人が「この曲でこの声はどう聴こえるか」を
    /// 確かめるために入れた曲を黙って別の調にすると、確かめた結果が別物になる。
    ///
    /// 推奨値は [`recommended_transpose`] が出す。 出すだけで、当てない。
    pub transpose: i32,
}

impl Song {
    /// 曲全体の最低音・最高音（`TR-RCL-12` (c)）。
    #[must_use]
    pub fn range(&self) -> Option<(i32, i32)> {
        let lo = self.notes.iter().map(|n| n.midi).min()?;
        let hi = self.notes.iter().map(|n| n.midi).max()?;
        Some((lo, hi))
    }

    /// 選んだノート群だけを取り出す（`TR-RCL-12`）。
    ///
    /// > ファイル全体だけでなく、任意のノート群を選んで目標にできる（サビだけ、など）
    ///
    /// `ranges` は `[開始, 終了)` の並び。 重なっていても、順が入れ替わっていても
    /// よい——**曲の中の位置は保つ。** 選んだ順に並べ替えると、歌詞のモーラ列が
    /// 元の曲と違うものになり、要求するエイリアスが変わる。
    ///
    /// 範囲の外は落とす。 落ちた境界で前後がつながるので、連続音の遷移は
    /// 選んだ範囲の中だけで数えられる。
    #[must_use]
    pub fn select(&self, ranges: &[(usize, usize)]) -> Self {
        let mut keep: BTreeSet<usize> = BTreeSet::new();
        for (from, to) in ranges {
            for i in *from..(*to).min(self.notes.len()) {
                keep.insert(i);
            }
        }
        Self {
            title: self.title.clone(),
            notes: keep
                .iter()
                .filter_map(|i| self.notes.get(*i).cloned())
                .collect(),
            provenance: self.provenance.clone(),
            tempo_bpm: self.tempo_bpm,
            default_portamento_ms: self.default_portamento_ms,
            transpose: self.transpose,
        }
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

    /// 音符ごとのモーラ数。 読めなくなった最初の音符（0 始まり）を `Err` で返す。
    ///
    /// 前から繋げて読み、1つ前との差で数える。 **音符ごとに切って読むと
    /// 数えられない**——長音（`ー`）は直前のモーラに付くので、単独では
    /// `DanglingModifier` になる。同梱の「さくらさくら」も `ー` の音符を持つ。
    ///
    /// 前の音符と字が結びつくと差が 0 になる（`き` + `ゃ` → `きゃ`）。
    /// 音符の切れ目がモーラの途中にあるということなので、そのまま 0 と数える。
    fn note_moras(&self, set: UnitSet) -> Result<Vec<usize>, usize> {
        let mut text = String::new();
        let mut before = 0_usize;
        let mut out = Vec::with_capacity(self.notes.len());
        for (i, n) in self.notes.iter().enumerate() {
            text.push_str(&n.lyric);
            let now = mora::parse(&text, set).map_err(|_| i)?.len();
            out.push(now.saturating_sub(before));
            before = now;
        }
        Ok(out)
    }

    /// 1モーラでない最初の音符（0 始まり）。 どれも1モーラなら `None`。
    ///
    /// 解決は音符とモーラが1対1で並ぶ前提で進む（`alias::resolve_phrase` は
    /// モーラの添字で音符を引く、`DEC-SYN-009`）。 **全体を繋げて読めるかしか
    /// 見ていなかった**ので、`さく` のような音符が取り込めてしまい、そこから
    /// 後ろの音高と長さが1つずつずれ、末尾は既定値で鳴っていた。
    ///
    /// 読めない音符もここで拾う。 0 モーラも2モーラも、並びを崩す点で同じ。
    #[must_use]
    pub fn note_not_one_mora(&self, set: UnitSet) -> Option<usize> {
        match self.note_moras(set) {
            Ok(counts) => counts.iter().position(|c| *c != 1),
            Err(i) => Some(i),
        }
    }

    /// フレーズの切れ目（`TR-RCL-12` の休符）。 モーラの添字で返す。
    ///
    /// 休符の手前で綴りの文脈が切れる。 **繋げて解決していた**ので、
    /// 休符のあとの音符が語頭形（`- か`）ではなく継続（`a か`）に
    /// 解決され、試唱は別の立ち上がりで鳴り、被覆も別の綴りを要求していた。
    ///
    /// 数え方は [`note_moras`](Self::note_moras) と同じ。 **音符ごとに切って
    /// 読んでいた**ので、`ー` の音符を1つでも持つ曲では数えられず、黙って
    /// 切れ目なしに倒れていた。読めない曲も切れ目なしに倒す——
    /// **誤った位置で切るより、切らないほうがよい。**
    #[must_use]
    pub fn phrase_breaks(&self, set: UnitSet) -> BTreeSet<usize> {
        let Ok(counts) = self.note_moras(set) else {
            return BTreeSet::new();
        };
        let mut breaks = BTreeSet::new();
        let mut at = 0_usize;
        for (n, c) in self.notes.iter().zip(counts) {
            if n.rest_ticks > 0 && at > 0 {
                breaks.insert(at);
            }
            at += c;
        }
        breaks
    }

    /// 方式ごとの必要エイリアス集合（`TR-RCL-12` (e), `TR-RCL-15`, `TR-SYN-17`）。
    ///
    /// 「録音済みサンプルが1件も無い状態」で走らせて事前に算出する（`TR-SYN-17`）。
    #[must_use]
    pub fn required_aliases(
        &self,
        rules: &Rules,
        method: Method,
        set: UnitSet,
    ) -> BTreeSet<String> {
        self.moras(set)
            .map(|m| alias::required_aliases(rules, method, &m, &self.phrase_breaks(set)))
            .unwrap_or_default()
    }
}

/// 上へ許すシフト量（半音、`TR-RCL-22`）。
pub const MAX_SHIFT_UP: i32 = 7;
/// 下へ許すシフト量（半音、`TR-RCL-22`）。負値。
pub const MAX_SHIFT_DOWN: i32 = -3;
/// 許容範囲を超えるノートの割合が、これを超えたら「音域外」（`TR-RCL-22`）。
pub const OUT_OF_RANGE_RATIO: f64 = 0.10;

/// 試唱で許す最大シフト量（半音、`TR-SYN-15`）。
pub const PREVIEW_MAX_SHIFT: i32 = 7;
/// 試唱で許す二乗平均シフト量（半音、`TR-SYN-15`）。
pub const PREVIEW_MAX_RMS_SHIFT: f64 = 4.0;

/// 勧めるキーの探索範囲（半音、`TR-SYN-15`）。
///
/// 1オクターブの上下まで。 `TR-SYN-15` の「本人が半音単位で 1 オクターブの
/// 上下まで指定できる」がそのまま上限で、[`crate::db`] 側の口
/// （`set_song_transpose`）も同じ範囲しか受け取らない。
///
/// **一度これを広げた。** 2オクターブ離れた曲にも勧め先を出そうとしたが、
/// 出しても本人が選べない値になる——押すと `song.transpose_out_of_range`
/// で断られる。届かないほど離れた曲に出すのは、キーではなく収録音高
/// （[`rescuing_tone`]。`TR-SYN-15` の (c)）。
const TRANSPOSE_SEARCH: std::ops::RangeInclusive<i32> = -12..=12;

/// 収録音高と曲の音域の整合（`TR-RCL-22`, `TR-SYN-15`）。
///
/// **移調したうえで判定する。** `TR-SYN-15` が「課題曲全体を半音単位で自動移調する。
/// ユーザーにキーを選ばせない」と定めている以上、移調前の絶対音高で音域を判定すると、
/// 実際には歌える曲を落とす。A3 で録った音源に A4 の曲を当てて「+12 半音だから
/// 音域外」と言ってしまう。**踏んだ。**
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeFit {
    /// 選んだ移調量（半音）。曲全体に一律で掛かる。
    pub transpose: i32,
    /// 許容シフト量を超えるノートの数。
    pub strained: usize,
    /// ノートの総数。
    pub notes: usize,
    /// 最大シフト量の絶対値（半音）。
    pub max_shift: i32,
    /// 二乗平均シフト量（半音）。
    pub rms_shift: f64,
}

impl RangeFit {
    /// 「音域外」か（`TR-RCL-22`、`TR-RCL-19` の (3) へ落ちる）。
    ///
    /// 1音でも外れたら音域外、にはしない。 端の1音だけが届かない曲を
    /// 「歌えない」と言うと、実際には歌える曲が消える。
    #[must_use]
    pub fn is_out_of_range(&self) -> bool {
        self.notes > 0 && self.ratio() > OUT_OF_RANGE_RATIO
    }

    /// 許容範囲を超えるノートの割合。
    #[must_use]
    pub fn ratio(&self) -> f64 {
        if self.notes == 0 {
            return 0.0;
        }
        self.strained as f64 / self.notes as f64
    }

    /// 試唱の選択肢に出してよいか（`TR-SYN-15`）。
    ///
    /// > 移調してもこの条件を満たせない課題曲は、試唱の選択肢に出さない
    #[must_use]
    pub fn is_previewable(&self) -> bool {
        self.notes > 0
            && self.max_shift <= PREVIEW_MAX_SHIFT
            && self.rms_shift <= PREVIEW_MAX_RMS_SHIFT
    }
}

/// オクターブ単位なら 0、そうでなければ 1（`TR-SYN-15`）。
///
/// 12 半音の移調は調を保つ。 それ以外は転調になる。
const fn octave_rank(transpose: i32) -> u8 {
    if transpose % 12 == 0 { 0 } else { 1 }
}

/// ある移調量での整合（`TR-RCL-22`, `TR-SYN-15`）。
#[must_use]
pub fn range_fit_at(song: &Song, tones: &[i32], transpose: i32) -> RangeFit {
    let shifts: Vec<Option<i32>> = song
        .notes
        .iter()
        .map(|n| tone::shift_for(tones, n.midi + transpose))
        .collect();
    let strained = shifts
        .iter()
        .filter(|s| s.is_none_or(|v| !(MAX_SHIFT_DOWN..=MAX_SHIFT_UP).contains(&v)))
        .count();
    let known: Vec<i32> = shifts.iter().flatten().copied().collect();
    let max_shift = known.iter().map(|s| s.abs()).max().unwrap_or(i32::MAX);
    let rms_shift = if known.is_empty() {
        f64::INFINITY
    } else {
        (known.iter().map(|s| f64::from(*s).powi(2)).sum::<f64>() / known.len() as f64).sqrt()
    };
    RangeFit {
        transpose,
        strained,
        notes: song.notes.len(),
        max_shift,
        rms_shift,
    }
}

/// 曲の音域と収録音高の整合を、**本人が指定したキーで**見る（`TR-RCL-22`, `TR-SYN-15`）。
///
/// **自動で移調しない**（`DEC-SYN-012`）。 曲は本人が入れた調で鳴る。
/// 届かないときに何をするかは本人が決める——キーを動かすか、音高を足すか、
/// そのまま聴くか。推奨は [`recommended_transpose`] と [`rescuing_tone`] が出す。
#[must_use]
pub fn range_fit(song: &Song, tones: &[i32]) -> RangeFit {
    if song.notes.is_empty() || tones.is_empty() {
        return RangeFit {
            transpose: song.transpose,
            strained: song.notes.len(),
            notes: song.notes.len(),
            max_shift: i32::MAX,
            rms_shift: f64::INFINITY,
        };
    }
    range_fit_at(song, tones, song.transpose)
}

/// 勧めるキー（半音、`TR-SYN-15`）。**当てない。提示するだけ。**
///
/// 選び方は5段。 (1) 外れるノートが最も少ないもの、(2) 試唱の条件を満たすもの、
/// (3) オクターブ単位のもの、(4) 移調量が小さいもの、(5) 二乗平均シフト量が小さいもの。
///
/// **オクターブを他の移調より先に採る。** 12 半音の移調は調を変えないが、
/// 11 半音は長7度下——曲が別の調になる。勧める先が転調では困る。
///
/// **二乗平均を先に見ない。** 条件を満たしているのに、平均を数半音下げるためだけに
/// 調を動かすことを勧めることになる。曲は書かれた調で鳴るのが既定。
#[must_use]
pub fn recommended_transpose(song: &Song, tones: &[i32]) -> i32 {
    if song.notes.is_empty() || tones.is_empty() {
        return 0;
    }
    TRANSPOSE_SEARCH
        .map(|t| range_fit_at(song, tones, t))
        .min_by(|a, b| {
            a.strained
                .cmp(&b.strained)
                .then(b.is_previewable().cmp(&a.is_previewable()))
                .then(octave_rank(a.transpose).cmp(&octave_rank(b.transpose)))
                .then(a.transpose.abs().cmp(&b.transpose.abs()))
                .then(a.rms_shift.total_cmp(&b.rms_shift))
        })
        .map_or(0, |f| f.transpose)
}

/// 足すと、この曲がいまのキーのまま届くようになる収録音高（`TR-RCL-22`）。
///
/// **キーを動かしたくない人のための道。** 移調を勧めるだけだと、
/// 「この曲をこの調で歌わせたい」という目的そのものを諦めさせることになる。
///
/// 候補は `prefix.map` が覆う範囲。 同じ効果なら低いほうを採る——
/// 低く録ったほうが上へ伸ばせる幅が広い。
#[must_use]
pub fn rescuing_tone(song: &Song, tones: &[i32]) -> Option<i32> {
    if song.notes.is_empty() || !range_fit(song, tones).is_out_of_range() {
        return None;
    }
    (tone::PREFIX_MAP_LOW..=tone::PREFIX_MAP_HIGH).find(|t| {
        if tones.contains(t) {
            return false;
        }
        let mut with = tones.to_vec();
        with.push(*t);
        with.sort_unstable();
        !range_fit(song, &with).is_out_of_range()
    })
}

/// 歌える曲数が最大になる単一収録音高（`TR-RCL-22`）。
///
/// 単一音高プリセットの推奨値。 同数なら低いほうを採る——
/// 低く録ったほうが上へ伸ばせる幅が広い（上 7 半音・下 3 半音）。
///
/// 移調が効くので、候補は1オクターブ分を見れば足りる。 そこから外は
/// 移調で同じ位置に畳まれる。
#[must_use]
pub fn recommended_single_tone(songs: &[Song]) -> Option<i32> {
    let lo = songs.iter().filter_map(|s| s.range().map(|r| r.0)).min()?;
    (lo - MAX_SHIFT_UP..=lo + 12 - MAX_SHIFT_UP).max_by_key(|t| {
        let fits = songs
            .iter()
            .filter(|s| !range_fit(s, &[*t]).is_out_of_range())
            .count();
        // 同数なら低いほう。`max_by_key` は後ろ勝ちなので符号を反転する。
        (fits, -*t)
    })
}

/// 音高を1本足すと、音域外から外れる曲の数（`TR-RCL-22`）。
#[must_use]
pub fn songs_rescued_by(songs: &[Song], tones: &[i32], added: i32) -> usize {
    let mut with = tones.to_vec();
    with.push(added);
    songs
        .iter()
        .filter(|s| range_fit(s, tones).is_out_of_range() && !range_fit(s, &with).is_out_of_range())
        .count()
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
    /// 勧めるキー（半音、`TR-SYN-15`）。**当てていない。提示するだけ。**
    pub recommended_transpose: i32,
    /// 足すといまのキーのまま届くようになる収録音高（`TR-RCL-22`）。
    ///
    /// 届いているなら `None`。 キーを動かしたくない人のための道。
    pub rescuing_tone: Option<i32>,
    /// 収録音高との音域の整合（`TR-RCL-22`）。
    pub range_fit: RangeFit,
}

/// すべての曲の状態を出す（`TR-RCL-17`）。
///
/// 追加項目数が同じ曲は、総モーラ数の少ない順に並べる（`TR-RCL-17`）。
/// 短い曲のほうが、最初の1曲としては手が届く。
#[must_use]
pub fn status_of(
    songs: &[(String, Song)],
    rules: &Rules,
    method: Method,
    recorded: &BTreeSet<String>,
    set: UnitSet,
    full_list: &[crate::reclist::Row],
    tones: &[i32],
) -> Vec<SongStatus> {
    let mut out: Vec<SongStatus> = songs
        .iter()
        .map(|(id, song)| {
            let required = song.required_aliases(rules, method, set);
            let covered = required.intersection(recorded).count();
            let missing = required.len().saturating_sub(covered);

            // 音域外はエイリアスが揃っていても歌えない（`TR-RCL-22`）。
            // 単位の被覆より先に見る——揃っていても届かない音は鳴らない。
            let fit = range_fit(song, tones);
            let singability = if fit.is_out_of_range() {
                Singability::Unavailable
            } else if missing == 0 {
                Singability::Complete
            } else {
                let resolvable = song.moras(set).is_some_and(|m| {
                    alias::resolve_phrase(
                        rules,
                        method,
                        &m,
                        recorded,
                        set,
                        &song.phrase_breaks(set),
                    )
                    .iter()
                    .all(|e| e.unit.is_playable())
                });
                if resolvable {
                    Singability::WithFallback
                } else {
                    Singability::Unavailable
                }
            };

            // あと何行かを、フルリストの部分集合として数える（`TR-RCL-16`）。
            let still: BTreeSet<String> = required.difference(recorded).cloned().collect();
            let plan = crate::plan::rows_to_cover(rules, method, &still, full_list);

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
                recommended_transpose: recommended_transpose(song, tones),
                rescuing_tone: rescuing_tone(song, tones),
                range_fit: fit,
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

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }
    use super::*;

    fn note(lyric: &str, midi: i32) -> Note {
        Note {
            lyric: lyric.to_owned(),
            midi,
            ticks: 480,
            rest_ticks: 0,
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
            tempo_bpm: crate::guide::DEFAULT_TEMPO_BPM,
            default_portamento_ms: 0.0,
            transpose: 0,
        }
    }

    /// 1音符に2モーラ入った曲を見つける（`DEC-SYN-009`）。
    ///
    /// **全体を繋げて読めるかしか見ていなかった。** `さく` の音符が通り、
    /// そこから後ろの音高と長さが1つずつずれていた。
    #[test]
    fn 一音符に二モーラある曲を見つける() {
        assert_eq!(
            song("さくら", &["さく", "ら"]).note_not_one_mora(UnitSet::Core),
            Some(0)
        );
        assert_eq!(
            song("読めない", &["さ", "abc"]).note_not_one_mora(UnitSet::Core),
            Some(1),
            "読めない音符も並びを崩す"
        );
        // 拗音・長音・促音・撥音は、字が2つでも音符1つで1モーラ。
        assert_eq!(
            song("いろいろ", &["きゃ", "ー", "っ", "ん"]).note_not_one_mora(UnitSet::Core),
            None
        );
    }

    /// 長音を持つ曲でも休符の切れ目を数えられる（`TR-RCL-12`）。
    ///
    /// **音符ごとに切って読んでいた。** `ー` は単独では読めないので、
    /// `ー` を1つでも持つ曲では切れ目が1つも出ず、休符のあとも継続形で
    /// 解決されていた。
    #[test]
    fn 長音のある曲でも休符の切れ目を数える() {
        let mut s = song("はーか", &["は", "ー", "か"]);
        s.notes[2].rest_ticks = 480;
        assert_eq!(s.phrase_breaks(UnitSet::Core), BTreeSet::from([2]));
    }

    /// 同梱曲はどれも1音符1モーラ。 取り込みの検査で同梱曲が落ちないこと。
    #[test]
    fn 同梱曲は一音符一モーラ() {
        for s in crate::ust::bundled_songs() {
            assert_eq!(s.note_not_one_mora(UnitSet::Core), None, "{}", s.title);
        }
    }

    /// 休符で綴りの文脈が切れる（`TR-RCL-12`, `TR-SYN-12`）。
    ///
    /// **繋げて解決していた。** 休符のあとの音符が語頭形（`- か`）ではなく
    /// 継続（`a か`）に解決され、試唱は別の立ち上がりで鳴り、
    /// 被覆も別の綴りを要求していた。
    #[test]
    fn 休符のあとは語頭形を要求する() {
        let mut s = song("あか", &["あ", "か"]);
        let rules = builtin_rules();

        // 続けて歌うなら、2音目は直前の母音を見る。
        let need = s.required_aliases(&rules, Method::Sequential, UnitSet::Core);
        assert!(need.contains("a か"), "続きは継続形: {need:?}");

        // あいだに休みが入ると、2音目はフレーズの頭になる。
        s.notes[1].rest_ticks = 480;
        let need = s.required_aliases(&rules, Method::Sequential, UnitSet::Core);
        assert!(need.contains("- か"), "休符のあとは語頭形: {need:?}");
        assert!(!need.contains("a か"), "継続形は要求しない: {need:?}");
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
            s.required_aliases(&builtin_rules(), Method::Single, UnitSet::Core),
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
            &builtin_rules(),
            Method::Single,
            &have(&["さ", "く", "ら"]),
            UnitSet::Core,
            &full_list(),
            &[60],
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
            &builtin_rules(),
            Method::Single,
            &have(&["さ", "ら"]),
            UnitSet::Core,
            &full_list(),
            &[60],
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
            &builtin_rules(),
            Method::Sequential,
            &have(&["さ", "く", "ら"]),
            UnitSet::Core,
            &full_list(),
            &[60],
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
            &builtin_rules(),
            Method::Single,
            &have(&["さ", "く"]),
            UnitSet::Core,
            &full_list(),
            &[60],
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
            &builtin_rules(),
            Method::Single,
            &BTreeSet::new(),
            UnitSet::Core,
            &full_list(),
            &[60],
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
        assert!(
            s.required_aliases(&builtin_rules(), Method::Single, UnitSet::Core)
                .is_empty()
        );
    }
}

#[cfg(test)]
mod range_tests {
    use super::*;

    /// 既定の綴り（`TR-SYN-36`）。
    fn builtin_rules() -> crate::presamp::Rules {
        crate::presamp::Rules::builtin(UnitSet::Core)
    }

    /// 勧めるキーは、本人が選べる範囲に収める（`TR-SYN-15`）。
    ///
    /// **探索を広げて 24 半音を勧めたことがある。** 出しても
    /// `set_song_transpose` が `-12..=12` しか受け取らないので、押すと
    /// 断られる。届かないほど離れた曲に出すのは収録音高のほう。
    #[test]
    fn 勧めるキーは一オクターブの上下に収まる() {
        // A3（57）で録った音源に、2オクターブ上の曲。
        let far = at(&[81, 83, 81]);
        let tones = [57];
        assert!(
            range_fit(&far, &tones).is_out_of_range(),
            "そのままでは届かない"
        );
        let t = recommended_transpose(&far, &tones);
        assert!((-12..=12).contains(&t), "選べる範囲に収まる: {t}");
        // キーでは届かないので、足す音高のほうを出す（`TR-SYN-15` の (c)）。
        assert!(
            rescuing_tone(&far, &tones).is_some(),
            "足せば届く音高を出す"
        );
    }

    /// 近い曲では、大きな移調へ逃げない（`TR-SYN-15`）。
    #[test]
    fn 届いている曲にはキーを動かさない() {
        let near = at(&[57, 59, 57]);
        assert_eq!(recommended_transpose(&near, &[57]), 0);
    }

    fn at(midis: &[i32]) -> Song {
        Song {
            title: "t".to_owned(),
            notes: midis
                .iter()
                .map(|m| Note {
                    lyric: "あ".to_owned(),
                    midi: *m,
                    ticks: 480,
                    rest_ticks: 0,
                })
                .collect(),
            provenance: Provenance {
                source: String::new(),
                license: String::new(),
            },
            tempo_bpm: crate::guide::DEFAULT_TEMPO_BPM,
            default_portamento_ms: 0.0,
            transpose: 0,
        }
    }

    /// 上 7 半音・下 3 半音まで（`TR-RCL-22`）。移調 0 で見る。
    #[test]
    fn 許容シフト量の境界() {
        let tones = [60];
        assert_eq!(range_fit_at(&at(&[67]), &tones, 0).strained, 0, "+7 は許す");
        assert_eq!(
            range_fit_at(&at(&[68]), &tones, 0).strained,
            1,
            "+8 は外れる"
        );
        assert_eq!(range_fit_at(&at(&[57]), &tones, 0).strained, 0, "-3 は許す");
        assert_eq!(
            range_fit_at(&at(&[56]), &tones, 0).strained,
            1,
            "-4 は外れる"
        );
    }

    /// **勝手に移調しない**（`DEC-SYN-012`）。
    ///
    /// A3 で録った音源に A4 の曲を当てると届かない。 1オクターブ下げれば
    /// 歌えるが、**それを黙ってやらない。** 本人が入れた調で鳴らし、
    /// 下げるかどうかは本人が決める。
    #[test]
    fn 指定が無ければ書かれた調のまま見る() {
        let song = at(&[69, 71, 72, 74]);
        let fit = range_fit(&song, &[57]);
        assert_eq!(fit.transpose, 0, "入れた調のまま");
        assert!(fit.is_out_of_range(), "届いていないことは言う");

        // 勧めはする。当てはしない。
        assert_eq!(recommended_transpose(&song, &[57]), -12);
        assert_eq!(range_fit(&song, &[57]).transpose, 0, "勧めても動かない");
    }

    /// 本人が指定したキーで見る（`TR-SYN-15`）。
    #[test]
    fn 指定したキーで判定する() {
        let mut song = at(&[69, 71, 72, 74]);
        song.transpose = -12;
        let fit = range_fit(&song, &[57]);
        assert_eq!(fit.transpose, -12);
        assert_eq!(fit.strained, 0);
        assert!(!fit.is_out_of_range());
    }

    /// 勧める先が転調では困る（`TR-SYN-15`）。
    ///
    /// -11 半音でも条件は満たせるが、それは長7度下で別の調になる。
    #[test]
    fn 勧めるのはオクターブを先に() {
        let t = recommended_transpose(&at(&[69, 71, 72, 74]), &[57]);
        assert_eq!(t % 12, 0, "転調を勧めない: {t}");
    }

    /// キーを動かしたくない人のために、足せば届く音高を出す（`TR-RCL-22`）。
    #[test]
    fn 足せば届く音高を出す() {
        let song = at(&[69, 71, 72, 74]);
        assert!(range_fit(&song, &[57]).is_out_of_range());
        let add = rescuing_tone(&song, &[57]).expect("足せば届く");
        assert!(!range_fit(&song, &[57, add]).is_out_of_range());
        // 届いているなら勧めない。
        assert_eq!(rescuing_tone(&at(&[60, 62]), &[60]), None);
    }

    /// 条件を満たしているなら、動かすことを勧めない（`TR-SYN-15`）。
    #[test]
    fn 届いているなら移調を勧めない() {
        let fit = range_fit(&at(&[60, 62, 64]), &[60]);
        assert_eq!(recommended_transpose(&at(&[60, 62, 64]), &[60]), 0);
        assert_eq!(fit.transpose, 0, "理由もなく調を動かさない");
        assert!(fit.is_previewable());
    }

    /// 1音だけ届かない曲を「歌えない」にしない（`TR-RCL-22`）。
    #[test]
    fn 一割を超えたときだけ音域外() {
        let mut midis = vec![60; 19];
        midis.push(90);
        let fit = range_fit(&at(&midis), &[60]);
        assert_eq!(fit.strained, 1);
        assert!(!fit.is_out_of_range(), "1/20 = 5% なら音域内");

        let mut midis = vec![60; 8];
        midis.extend([100, 101]);
        assert!(
            range_fit(&at(&midis), &[60]).is_out_of_range(),
            "2/10 = 20%"
        );
    }

    /// 音高を足すと floor 割り当てが変わり、届く音が増える（`TR-RCL-22`）。
    #[test]
    fn 音高を足すと音域が広がる() {
        // 移調では埋まらない広さの曲。 低域と高域が 2 オクターブ離れている。
        let wide = at(&[60, 61, 84, 85, 86, 87, 88, 89, 90, 91]);
        assert!(range_fit(&wide, &[60]).is_out_of_range());
        assert_eq!(songs_rescued_by(std::slice::from_ref(&wide), &[60], 84), 1);
        assert_eq!(
            songs_rescued_by(&[at(&[60])], &[60], 78),
            0,
            "既に届く曲は数えない"
        );
    }

    /// 収録音高が無ければ、シフトのしようが無い。
    #[test]
    fn 収録音高が無ければ全部外れる() {
        let fit = range_fit(&at(&[60, 61]), &[]);
        assert_eq!(fit.strained, 2);
        assert!(fit.is_out_of_range());
        assert!(!fit.is_previewable());
    }

    /// 試唱の条件は最大 ±7 半音かつ二乗平均 4 半音以内（`TR-SYN-15`）。
    ///
    /// **この数字に実測の裏付けは無い**（`reclist.toml` の領域リスク）。
    /// だから判定に使うのは「勧めるかどうか」までで、鳴らすのは止めない
    /// （`DEC-SYN-012`）。
    #[test]
    fn 試唱の条件は指定したキーで見る() {
        let mut song = at(&[69, 71, 72]);
        song.transpose = -12;
        assert!(range_fit(&song, &[57]).is_previewable());
        // 2オクターブ以上に広がる曲は、どう移調しても収まらない。
        let wide = at(&[48, 60, 72, 84, 96]);
        assert!(!range_fit(&wide, &[60]).is_previewable());
    }

    /// 歌える曲数が最大になる音高を薦める（`TR-RCL-22`）。
    #[test]
    fn 単一音高の推奨値は歌える曲数で決まる() {
        let songs = [at(&[60, 62]), at(&[61, 63]), at(&[80, 82])];
        let t = recommended_single_tone(&songs).expect("曲がある");
        let fits = songs
            .iter()
            .filter(|s| !range_fit(s, &[t]).is_out_of_range())
            .count();
        for cand in 45..=95 {
            let other = songs
                .iter()
                .filter(|s| !range_fit(s, &[cand]).is_out_of_range())
                .count();
            assert!(other <= fits, "{cand} のほうが多く歌える: {other} > {fits}");
        }
        assert!(recommended_single_tone(&[]).is_none());
    }

    /// 音域外はエイリアスが揃っていても歌えない（`TR-RCL-22` → `TR-RCL-19` (3)）。
    #[test]
    fn 音域外は揃っていても不可() {
        let s = Song {
            title: "広すぎる曲".to_owned(),
            // 2オクターブ以上に広がる。 どう移調しても収まらない。
            notes: [48, 60, 72, 84, 96]
                .into_iter()
                .map(|midi| Note {
                    lyric: "あ".to_owned(),
                    midi,
                    ticks: 480,
                    rest_ticks: 0,
                })
                .collect(),
            provenance: Provenance {
                source: String::new(),
                license: String::new(),
            },
            tempo_bpm: crate::guide::DEFAULT_TEMPO_BPM,
            default_portamento_ms: 0.0,
            transpose: 0,
        };
        let got = status_of(
            std::slice::from_ref(&("s1".to_owned(), s)),
            &builtin_rules(),
            Method::Single,
            &["あ".to_owned()].into_iter().collect(),
            UnitSet::Core,
            &crate::reclist::generate_single(UnitSet::Core, 5).expect("生成できる"),
            &[60],
        );
        assert_eq!(got[0].missing_units, 0, "エイリアスは揃っている");
        assert_eq!(got[0].singability, Singability::Unavailable);
        assert_eq!(singable_count(&got), 0);
    }
}

#[cfg(test)]
mod format_tests {

    /// 内部形式はテンポと既定ピッチベンドを持つ（`TR-SYN-30`）。
    ///
    /// UST / USTX は読み込みの入口であって内部形式ではない。 テンポを
    /// 落とすと、読み込んだ曲がどれも同じ速さで鳴る。
    #[test]
    fn 内部形式はテンポを持つ() {
        let s = crate::ust::bundled_songs();
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
        for s in crate::ust::bundled_songs() {
            assert!(!s.provenance.source.trim().is_empty(), "{}", s.title);
            assert!(!s.provenance.license.trim().is_empty(), "{}", s.title);
        }
    }
}
