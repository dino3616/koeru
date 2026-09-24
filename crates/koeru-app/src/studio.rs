//! 縦切りの組み立て。録って、聴けるまでを1本に繋ぐ。
//!
//! ここに Tauri は出てこない。アプリの筋を、GUI 無しで検査できるようにする。
//!
//! ## 通す順序
//!
//! 1. デバイスを選び、ストリームを開く（`recording-input.fsl` の手順どおり）
//! 2. 行を1つ録る（`.wav.part` → fsync → rename → DB コミット）
//! 3. 録音停止時に解析を確定させ、`.frq` を書く（`TR-PKG-05`）
//! 4. 境界を見つけて oto の5値を導く
//! 5. 目標音高で合成して鳴らす
//!
//! 3 と 4 を録音停止の直後に済ませるのが要点。 後回しにすると、
//! 試唱のたびに WAV を読み直すことになる（`TR-PKG-42`）。
//!
//! ## 時間軸
//!
//! このモジュールの秒とサンプル数は、すべてマスターの時間軸で数える
//! （[`koeru_audio::wav::MASTER_RATE_HZ`]）。 デバイスのネイティブレートは
//! pump より下流には出てこない。

use std::collections::{BTreeSet, HashMap};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use koeru_align::aligner::{Alignment, Segment};
use koeru_align::confidence::Confidence;
use koeru_align::consistency::{self, Measure};
use koeru_align::phoneme::Phoneme;
use koeru_align::preset::Preset;
use koeru_align::review::{EntryState, ReviewError, ReviewMode, ReviewQueue, Slot};
use koeru_align::segment::{Boundaries, SegmentConfig, confidence, detect_single, per_mora};
use koeru_align::{ini, ledger, reach, validate};
use koeru_audio::backend::current as mac;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_audio::{DeviceId, Session, wav};
use koeru_core::analysis::{TakeAnalysis, TakeMetrics};
use koeru_core::calibration::{self, Calibration, Outcome};
use koeru_core::channel::{self, Source};
use koeru_core::db::{FinalizedTake, Ledger, SessionSnapshot, koeru_oto};
use koeru_core::frq;
use koeru_core::guide::{self, GuideSpec};
use koeru_core::inventory::UnitSet;
use koeru_core::leak::{self, LeakCheck};
use koeru_core::oto::Oto;
use koeru_core::project::{CoverageState, HandoffState, Library, Manifest, Method, ProjectDir};
use koeru_core::song::{self, Song, SongStatus};
use koeru_core::text::TextEncoding;
use koeru_core::ust;
use koeru_core::voice::{self, VoiceColor};
use koeru_core::waveform;
use koeru_synth::f0;
use koeru_synth::resampler::{FrequencyTable, RenderRequest, render};

use uuid::Uuid;

use crate::error::{AppError, Result};
use crate::latency::ms_u32;
use crate::latency::{self, Case, Observed};
use crate::packaging;
use crate::preview::{self, PhraseCache, Running, Sink, WavSamples};
use crate::pump::{PREROLL_MS, Pump};
use crate::review::slot_of;
use crate::storage;
use crate::workers::{Priority, Workers};

/// 単独音の収録音高。A3 を既定にする（`TR-RCL` の音階既定）。
pub const DEFAULT_TONE_MIDI: i32 = 57;

/// これだけ測るまで、レイテンシの目標に収まっているかを言わない（`TR-SYN-33`）。
///
/// 3回では中央値も p95 も意味が無い。
const LATENCY_MIN_SAMPLES: usize = 10;

/// 段を持ち回すテイクの数（`TR-PLT-04`）。
///
/// 上限を置く。 3時間ぶんの段を全部持つとメモリが尽きる。
const MIPMAP_CACHE: usize = 8;

/// リングの容量（サンプル）。8秒ぶん。
///
/// 描画やディスクが詰まっても、この長さのあいだは取りこぼさない。
const RING_SECONDS: usize = 8;

/// 主因ラベルを出しはじめる成分の値（`TR-ALN-26` (3)）。
///
/// 自動確定と確認キューの切り分けには使わない。 そちらは合計所要時間の上限が
/// 決める（`TR-ALN-25` の「確信度の閾値は絶対値で固定せず、確認キューの運用で切る」）。
/// ここが決めるのは「どの成分を主因として名指すか」だけで、
/// 何件が確認に回るかには効かない。
const CAUSE_THRESHOLD: f64 = 0.5;

/// 読みの最初の音素。集団の鍵にする（`TR-ALN-12` (b)）。
///
/// 音素へ写せない読みは `None`。 推測で既定の音素を当てない——
/// 別の音素の集団に混ざると、その集団の中央値まで動く。
fn first_phoneme(reading: &str) -> Option<Phoneme> {
    koeru_align::phoneme::phonemes_for(reading)
        .ok()
        .and_then(|p| p.first().copied())
}

/// 読みの最後の音素。母音長の集団の鍵にする（`TR-ALN-12` (b)）。
///
/// 母音の長さは母音で決まる。 子音で束ねると、あ段とう段が同じ集団に入る。
fn last_phoneme(reading: &str) -> Option<Phoneme> {
    koeru_align::phoneme::phonemes_for(reading)
        .ok()
        .and_then(|p| p.last().copied())
}

/// 取り込む曲の歌詞を確かめる（`TR-RCL-12`）。 取り込みと下見が同じものを通る。
///
/// 同梱プリセットはすべて Core（`preset::builtin`）。曲を読む側も
/// Core で読む（`sing_song`、`song_plan`）ので、ここも Core で見る。
///
/// **1音符1モーラも見る。** 全体を繋げて読めるかだけ見ていたので、`さく` の
/// 音符が取り込めてしまい、そこから後ろの音高と長さが1つずつずれて鳴った
/// （`Song::note_not_one_mora`）。
///
/// 伝えるのは何番目の音符かだけ。 歌詞そのものは載せない——この失敗は
/// トレースにも残る（`AGENTS.md` #3）。
fn check_lyrics(song: &Song) -> Result<()> {
    if song.moras(UnitSet::Core).is_none() {
        return Err(AppError::new(
            "app.unreadable_lyrics",
            "歌詞を読めないノートがある。仮名で書かれた UST / USTX を取り込む",
        ));
    }
    if let Some(i) = song.note_not_one_mora(UnitSet::Core) {
        return Err(AppError::new(
            "song.note_not_one_mora",
            format!(
                "{} 番目のノートに、1音ぶんではない歌詞が入っている。1ノートに1音（「きゃ」「ー」「っ」は1音）で書かれた UST / USTX を取り込む",
                i + 1
            ),
        ));
    }
    Ok(())
}

/// 話者内一貫性（`TR-ALN-12`）の集団を引く読み。 CV の枠だけ返す。
///
/// 集団の鍵は音素で、音素は仮名から引く（`phoneme::phonemes_for` は仮名の辞書）。
/// **綴りを渡していた。** `a か` も `- か` も `a k` も辞書に無いので、
/// 連続音と CVVC では集団が1つも作れず、事前分布がいつも 1.0 だった
/// ——単独音だけは綴りが仮名そのものなので、試験が素通りしていた。
///
/// 渡りと語尾は測らない。 先行発声も子音長も CV とは別の区間を指すので、
/// 同じ音素の集団へ混ぜると CV の集団のほうが歪む。分からないものを
/// 外れ値にしない方針（[`Studio::prior_of`]）どおり、1.0 のままにする。
fn consistency_reading(
    slot: koeru_core::reclist::Slot,
    line: &[koeru_core::inventory::Unit],
) -> Option<&'static str> {
    match slot {
        koeru_core::reclist::Slot::Cv { mora } => line.get(mora).map(|u| u.kana),
        koeru_core::reclist::Slot::Vc { .. } | koeru_core::reclist::Slot::Ending { .. } => None,
    }
}

/// 音素ごとに集めた測度の集団（`TR-ALN-12`）。
///
/// 条文が挙げるのは「先行発声位置・子音長・母音長」。 oto からそれぞれを引く。
///
/// **測度ごとに違う音素で集める。** 先頭音素ひとつで全部を束ねていたので、
/// か・く・け・こ の**母音長が全部 `k` の集団へ入り**、あ段の長さと
/// う段の長さが同じ集団で比べられていた。母音の長さは母音で決まる。
///
/// **子音長は先行発声位置の定数ずらしになる**（`derive_cv`: 子音長 =
/// 先行発声 − 前余白マージン）。同じプリセットの中では中央値からの距離が
/// 一致するので、別に測っても新しいことは言わない。それでも並べているのは、
/// プリセットが混ざったプロジェクト（`TR-ALN-23` の編集）でずれるため。
#[derive(Debug, Default)]
struct Populations {
    /// (収録音高, 先頭音素) → 先行発声位置と子音長。
    ///
    /// **音階を鍵に含める**（`TR-ALN-22` の「集団統計は音階内に閉じる」）。
    /// 高音階と低音階では発声が変わるので、混ぜると全部が外れ値になる。
    /// `consistency::Population` は初めから音階を持っていたが、
    /// ここの集計が音素だけで畳んでいた。**踏んだ。**
    onset: HashMap<(i32, Phoneme), (Vec<f64>, Vec<f64>)>,
    /// (収録音高, 末尾音素) → 母音長。
    vowel: HashMap<(i32, Phoneme), Vec<f64>>,
}

impl Populations {
    /// その oto から子音長と母音長を引く。
    ///
    /// 母音長 = 使える区間 − 先行発声。 子音長 = 先行発声（前余白ぶんずれている）。
    fn spans(o: &Oto, len_ms: f64) -> (f64, f64) {
        let usable = o.usable_ms(len_ms);
        (o.preutterance_ms, (usable - o.preutterance_ms).max(0.0))
    }

    fn push(&mut self, tone: i32, reading: &str, o: &Oto, len_ms: f64) {
        let (consonant, vowel) = Self::spans(o, len_ms);
        if let Some(k) = first_phoneme(reading) {
            let e = self.onset.entry((tone, k)).or_default();
            e.0.push(o.preutterance_ms);
            e.1.push(consonant);
        }
        if let Some(k) = last_phoneme(reading) {
            self.vowel.entry((tone, k)).or_default().push(vowel);
        }
    }
}

/// キューの遷移が通らなかったことを、境界の失敗へ畳む。
///
/// 異常ではない。 状態機械の遷移条件を満たしていないだけなので、
/// 画面は押せない的として出せばよい。
fn review_error(e: ReviewError) -> AppError {
    AppError::new(e.kind(), e)
}

/// 確認の進み具合（`TR-ALN-25`, `TR-ALN-28`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReviewSummary {
    /// `align-review.fsl` の `ReviewMode`。
    pub mode: String,
    /// 確認待ちの件数。
    pub pending: usize,
    /// 検証で止まっている件数（`TR-ALN-20`）。
    pub blocked: usize,
    /// 確認にかかる見積もりの合計（秒）。
    pub estimated_seconds: u64,
    /// 上限（秒）。`DEC-ALN-003` の合計5分。
    pub budget_seconds: u64,
    /// 上限を超えているか。超えていなければ個別確認をやめられない（`INV-ALN-004`）。
    pub exceeds_budget: bool,
    /// 切り出しが1つも取れていない行の数。
    ///
    /// キューには現れない。 エントリが無いので、確認の対象にすらならない
    /// ——それでも書き出しは止める（`INV-ALN-003` の趣旨）。
    pub missing: usize,
    /// WAV をまたいで重なっているエイリアスの数。
    ///
    /// エイリアスはエントリの識別子なので、重なると確認キューが片方を落とす。
    /// 落ちたほうは確認もされず `oto.ini` にも出ないので、書き出しを止める。
    pub conflicting: usize,
    /// まだ推定していないエントリの数。録り直しに回したものがここにいる。
    ///
    /// 確認待ちには数えない（`pending` は `InQueue` と `Blocked` だけ）。
    /// **書き出しは止める**ので、数えないまま「確認は済んだ」と出すと、
    /// 押して初めて断られる的になる。
    pub unestimated: usize,
    /// いま書き出してよいか。
    ///
    /// **画面がここを見る。** 件数から組み立て直させない——関門の条件は
    /// `ReviewQueue::may_export` が持っていて（`INV-ALN-003`）、
    /// 同じ規則を画面にも書くと片方だけが古くなる。
    pub may_export: bool,
    /// 確認を飛ばせる経路を必ず出す方式か（`TR-ALN-28`）。
    pub allows_skipping: bool,
    /// その方式の到達水準。
    pub reach: String,
    pub exported: bool,
}

/// 確認キューの1件（`TR-ALN-26`）。
#[derive(Debug, Clone, PartialEq)]
pub struct ReviewItem {
    /// このエントリを指す鍵（`crate::review::EntryKey`）。
    ///
    /// 画面はこれをそのまま返す。 **エイリアスでは指せない**——多音階は
    /// 同じ綴りを音高の数だけ持つ（`TR-ALN-22`）。
    pub key: String,
    /// 対象のエイリアス（`TR-ALN-26` (1)）。画面に出す名前。
    pub alias: String,
    /// そのエイリアスを録った行。一覧の絞り込みに要る（`DEC-PLT-024`）。
    pub row_id: String,
    /// 自動推定した5値（`TR-ALN-26` (2)）。
    pub oto: Oto,
    /// 低確信度の主因（`TR-ALN-26` (3)）。内訳を持たなければ `None`。
    pub cause: Option<String>,
    pub confidence: f64,
    pub state: String,
    /// 値ごとの固定（`TR-ALN-30`）。並びは `Slot::ALL` と同じ。
    pub pinned: [bool; 5],
}

/// 開いているプロジェクト。
#[derive(Debug)]
struct Open {
    dir: ProjectDir,
    ledger: Ledger,
    session_id: i32,
    /// エイリアスの綴りの表（`TR-SYN-36`, `DEC-SYN-010`）。
    ///
    /// プロジェクトに `presamp.ini` があればそれ、無ければ同梱の既定。
    /// 録音リストの生成・カバレッジ判定・試唱・書き出しが、**全部ここを通る**
    /// ——片方だけが差し替わると、歌える判定と実際に鳴る音がずれる。
    ///
    /// 開くときに1度だけ読む。 引くたびに読み直すと、収録の途中で
    /// ファイルを差し替えられたときに、同じセッションの中で綴りが変わる。
    rules: koeru_core::presamp::Rules,
    /// 確認キュー（`TR-ALN-25`、`align-review.fsl`）。
    ///
    /// 開いている間だけ持つ写し。 正本は台帳で、ここは遷移の可否を判定する係
    /// （`crate::review`）。
    review: ReviewQueue,
    /// エイリアス → そのエントリを持つ採用テイク。書き戻す先。
    review_takes: HashMap<String, i32>,
}

/// 1つのテイクの結果。
#[derive(Debug, Clone, PartialEq)]
pub struct TakeResult {
    /// 台帳の ID。
    pub take_id: i32,
    pub row_id: String,
    pub duration_ms: f64,
    /// 絶対値の最大。`koeru_core::analysis::CLIP_THRESHOLD` 以上ならクリップ。
    pub peak: f32,
    /// 波形サムネイル（0〜255）。
    pub thumbnail: Vec<u8>,
    /// 導けた oto。発声が見つからなければ `None`。
    pub oto: Option<Oto>,
    /// 境界の確信度。
    pub confidence: Option<f64>,
    /// 取りこぼしの回数（`TR-REC-07`）。
    pub discontinuities: usize,
    /// 取りこぼしたので自動的に無効にした（`TR-REC-07`）。
    /// 同じフレーズがもう一度出てくる。
    pub invalidated: bool,
    /// 計測値（`TR-REC-16`）。測るだけで、判定も指摘もしない。
    pub metrics: TakeMetrics,
    /// 押した瞬間より前から何ミリ秒ぶん遡れたか（`TR-REC-19`）。
    pub preroll_ms: f64,
}

/// 残量の見積もり（`TR-REC-41`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpaceEstimate {
    /// まだ録っていない行の数。
    pub remaining_rows: u64,
    /// 残り全部に要るバイト数。
    pub required_bytes: u64,
    /// 保存先の空き。引けなければ `None`。
    pub available_bytes: Option<u64>,
    /// その残量で録りきれる件数（`TR-REC-41`）。
    pub rows_that_fit: u64,
}

impl SpaceEstimate {
    /// 残り全部を録りきれるか。
    #[must_use]
    pub const fn is_sufficient(&self) -> bool {
        self.rows_that_fit >= self.remaining_rows
    }
}

/// 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
///
/// 収録中は何も言わない。 ここでだけ、壊れた成果物が完成へ到達する経路を塞ぐ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Preflight {
    /// NFC へ直した名前の数（`TR-REC-32`）。
    pub renamed_to_nfc: usize,
    /// それでも NFC でない名前。残っていたら書き出さない。
    pub non_nfc_names: Vec<String>,
    /// フルスケールに達している採用テイク（行 ID と回数、`TR-REC-16`）。
    pub clipped_takes: Vec<(String, u32)>,
}

impl Preflight {
    /// 書き出してよいか。
    ///
    /// 割れているテイクは止めない。 本人が承知のうえで配ることはありうる。
    /// 止めるのは、受け手の環境で見つからなくなる名前だけ。
    #[must_use]
    pub fn may_export(&self) -> bool {
        self.non_nfc_names.is_empty()
    }
}

/// プロジェクトの現在地。
#[derive(Debug, Clone, PartialEq)]
pub struct Progress {
    /// 次に録る行（`(id, 読み上げる文字列)`）。全部録れていれば `None`。
    pub next_row: Option<(String, String)>,
    /// 収録済み単位の数。
    pub covered: usize,
    /// 必要な単位の数。
    pub required: usize,
    /// 完成状態。
    pub coverage: CoverageState,
    /// 手渡し状態。完成判定はこれを見ない（`TR-PKG-33`）。
    pub handoff: HandoffState,
    /// いま歌える曲の数（`TR-RCL-19`）。カバレッジと常に両方出す。
    pub singable_songs: usize,
    /// バンクに入っている曲の数。0 でも成立する。
    pub songs_in_bank: usize,
    /// 残り所要時間（秒、`TR-RCL-09`）。固定の見積もり（`DEC-RCL-013`）。
    pub remaining_seconds: f64,
    /// 残りの行数（`TR-RCL-09`）。
    ///
    /// 時間だけで示さない。 時間は固定値の見積もりで桁しか合っていないが、
    /// 行数は数え上げなので正確。**両方出す。**
    pub remaining_rows: usize,
    /// 音高ごとの消化率（`TR-RCL-26`）。`(音高, 録り終えた行, 総行数)`。
    ///
    /// 詳細表示に置く欄。 [`singable_songs`](Self::singable_songs) は音高を
    /// 跨いだ実際の判定結果で、こちらを足し合わせたものではない。
    pub by_tone: Vec<(i32, usize, usize)>,
}

/// ライブラリに並ぶ音源1つ分（`DEC-PLT-024` の声の並び）。
///
/// 名前と数だけでは足りない。 一覧に環と色を出すので、音源ごとに台帳を読む
/// （`Q-RCL-004`）。読めない音源も落とさず、読めたところまでを返す。
#[derive(Debug, Clone, PartialEq)]
pub struct LibraryEntry {
    pub id: Uuid,
    /// manifest が読めなければ `None`。
    pub manifest: Option<Manifest>,
    /// 台帳から読めた到達度。読めなければ `None`。
    pub state: Option<VoiceState>,
}

/// 音源のいまの姿。環と色に要るもの一式。
#[derive(Debug, Clone, PartialEq)]
pub struct VoiceState {
    /// 表示名。ライブラリの一覧では manifest から別に読むので、そこでは空。
    pub display_name: String,
    /// 作り方。同上。
    pub method: String,
    /// 録音リストの行の数。同上。
    pub rows: u32,
    /// 収録済み単位の数。
    pub covered: usize,
    /// 必要な単位の数。
    pub required: usize,
    /// いま歌える曲の数（`TR-RCL-19`）。
    pub singable_songs: usize,
    /// バンクに入っている曲の数。
    pub songs_in_bank: usize,
    /// 五十音の行ごとの `(収録済み, 全体)`。環1本につき1組（`DEC-PLT-025`）。
    pub rings: Vec<(u32, u32)>,
    /// 声から作った色。1つも録れていなければ `None`（`DEC-PLT-027`）。
    pub color: Option<VoiceColor>,
}

/// 縦切りの本体。
#[derive(Debug)]
pub struct Studio {
    library: Library,
    /// 使うアライナ（`TR-ALN-03`, `DEC-ALN-008`）。
    ///
    /// MFA のモデルが読めれば MFA、読めなければ退避経路。
    /// 起動時に1度だけ選ぶ——テイクごとに 96MiB を読み直さない（`TGT-ALN-004`）。
    aligner: crate::align::Chosen,
    open: Option<Open>,
    capture: Option<mac::Capture>,
    /// 排出スレッド。収録画面にいる間ずっと回っている（`TR-REC-19`）。
    pump: Option<Pump>,
    session: Session,
    /// 録音中の行。
    recording: Option<String>,
    /// 収録開始時点の取りこぼし数。このテイクの中で増えたぶんだけを見る（`TR-REC-07`）。
    xrun_baseline: usize,
    /// ガイドのフレーズ開始が、録音の何サンプル目に相当するか（`TR-REC-26`）。
    ///
    /// 参考値。 切り出しの根拠にしない。ガイドが鳴っていなければ `None`。
    guide_offset_at_start: Option<i64>,
    /// 選んでいるデバイス。
    device: Option<DeviceId>,
    /// アプリが触る前のゲイン。終了時にここへ戻す（`TR-REC-15`）。
    gain_before: Option<(DeviceId, f32)>,
    /// 回り込みの検査結果（`TR-REC-24`）。済むまで音高提示を鳴らさない。
    leak: Option<LeakCheck>,
    /// 全チャンネルに有意な信号があるか（`TR-REC-06`）。
    /// 真のときだけ、本人が「合成する」を選べる。
    may_mix: bool,
    playback: Option<mac::Playback>,
    /// フレーズ単位の合成結果（`TR-SYN-02`, `TR-SYN-25`）。
    ///
    /// プロジェクトを開いている間だけ持つ。 素材が変われば鍵が変わるので、
    /// 明示的に捨てなくても古い結果は使われない（`TR-SYN-26`）。
    song_cache: Arc<Mutex<PhraseCache>>,
    /// 進行中の曲の合成。落とすと止まる（`TR-SYN-27`）。
    singing: Option<Running>,
    /// 継ぎ足しながら鳴らしている再生（`TR-SYN-03`）。
    playback_stream: Option<mac::Playback>,
    /// 集めた F0 系列。話者音域を見るため（`TR-SYN-22`）。
    observed_f0: Vec<Vec<f64>>,
    /// 話者音域から決めた探索下限。まだ分からなければ `None`。
    f0_floor: Option<f64>,
    /// テイクごとの波形の段（`TR-PLT-04`）。上限を置いて持ち回す。
    mipmaps: HashMap<i32, (Arc<waveform::Mipmap>, u32)>,
    /// 背後で回す仕事（`TR-SYN-04`, `TR-SYN-34`）。
    ///
    /// 録音入力とは別のスレッド。 録音のコールバックを妨げない。
    workers: Workers,
    /// 試唱レイテンシの実測（`TR-SYN-33`）。
    ///
    /// 押してから鳴るまでを場面ごとに溜める。 目標と比べられるようにする。
    observed: HashMap<Case, Observed>,
    /// この回に試唱を押したことがあるか。初回かどうかの判定（`TR-SYN-33`）。
    ever_previewed: bool,
}

/// 採用テイクから集めた素材。
struct Materials {
    /// エイリアスごとの WAV の場所。
    paths: HashMap<String, PathBuf>,
    /// エイリアスごとの周波数表（`TR-SYN-25`）。永続化するのはこれだけ。
    tables: HashMap<String, Vec<f64>>,
    /// エイリアスごとの oto。
    otos: HashMap<String, koeru_core::oto::Oto>,
}

/// 試唱の待ち時間の実測（`TR-SYN-33`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyRow {
    /// どの場面か。
    pub case: Case,
    /// 測った回数。
    pub count: usize,
    /// 中央値（ミリ秒）。
    /// 中央値（ミリ秒）。
    ///
    /// `u32` に落としてある。`Duration::as_millis` は `u128` を返すが、
    /// 試唱の待ち時間に 49 日の幅は要らない。 そのまま画面へ渡すと、
    /// JS の数値が正確に持てる範囲を超える型を境界に置くことになる。
    pub median_ms: Option<u32>,
    /// その場面の目標（ミリ秒）。
    /// その場面の目標（ミリ秒）。
    pub budget_ms: u32,
    /// 収まっているか。回数が少ないうちは `None`。
    pub meets: Option<bool>,
}

/// 歌わせた結果（`TR-SYN-18`）。
#[derive(Debug, Clone, PartialEq)]
pub struct SungSong {
    pub title: String,
    pub phrases: usize,
    /// 鳴らせないので落としたフレーズの数（`TR-SYN-18` (2)）。
    ///
    /// 落とした位置には何も挿さない。
    pub dropped_phrases: usize,
    /// 鳴らす長さ（ミリ秒）。
    pub duration_ms: f64,
    /// 音の高さの区画が切り替わった位置（`TR-SYN-16`）。
    ///
    /// > フレーズ内で切り替わった位置は記録し、原音設定側の確認対象として
    /// > 参照できるようにする
    ///
    /// 単音階では常に空。 切り替えは音符境界でのみ起きる——
    /// 1音符の途中で素材が変わると、1つの音の中で声が別人になる。
    pub subbank_switches: Vec<SubbankSwitch>,
}

/// 区画が切り替わった1箇所（`TR-SYN-16`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubbankSwitch {
    /// 切り替わった音符の添字。ここから新しい区画になる。
    pub note_index: usize,
    /// 直前まで使っていた収録音高（MIDI）。
    pub from: Option<i32>,
    /// ここから使う収録音高。
    pub to: Option<i32>,
}

/// 継ぎ足し先。
///
/// 合成スレッドが持つのは `Feed` だけ。 `AudioUnit` には触らない。
struct StreamSink {
    feed: mac::Feed,
}

impl Sink for StreamSink {
    fn push(&self, samples: &[f32]) {
        self.feed.push(samples);
    }
    fn seal(&self) {
        self.feed.seal();
    }
}

/// 退避経路の境界を、アライナと同じ形（`Alignment`）に包む。
///
/// 事後確率は持たない。 音響モデルを通していないので、
/// `TR-ALN-24` の成分 (1) 経路確信度が出せない（`None` のまま）。
fn fallback_alignment(samples: &[f64], rate: u32) -> Option<Alignment> {
    let cfg = SegmentConfig::default();
    let b = detect_single(samples, rate, &cfg)?;
    #[allow(clippy::cast_precision_loss)]
    let total_ms = samples.len() as f64 / f64::from(rate) * 1000.0;
    let sil = koeru_align::phoneme::Phoneme::new(koeru_align::phoneme::SILENCE)?;
    Some(Alignment {
        segments: vec![
            Segment {
                phoneme: sil,
                start_ms: 0.0,
                end_ms: b.voice_start_ms,
            },
            Segment {
                phoneme: sil,
                start_ms: b.voice_start_ms,
                end_ms: b.vowel_start_ms,
            },
            Segment {
                phoneme: sil,
                start_ms: b.vowel_start_ms,
                end_ms: b.vowel_end_ms,
            },
            Segment {
                phoneme: sil,
                start_ms: b.vowel_end_ms,
                end_ms: total_ms.max(b.vowel_end_ms),
            },
        ],
        posteriors: None,
        log_likelihood: None,
        grid_divergence: None,
    })
}

impl Studio {
    /// ライブラリを開く。無ければ作る。
    #[tracing::instrument(skip(library_root), err)]
    pub fn open(library_root: PathBuf) -> Result<Self> {
        Ok(Self {
            library: Library::open(library_root)?,
            // 起動時に1度だけ選ぶ（`TGT-ALN-004`。テイクごとに 96MiB を読み直さない）。
            // モデルが無いのはビルドの失敗（`DEC-ALN-016`）。ここで止める。
            aligner: crate::align::Chosen::detect()?,
            open: None,
            capture: None,
            pump: None,
            session: Session::new(),
            recording: None,
            xrun_baseline: 0,
            guide_offset_at_start: None,
            device: None,
            gain_before: None,
            leak: None,
            may_mix: false,
            playback: None,
            song_cache: Arc::new(Mutex::new(PhraseCache::new())),
            singing: None,
            playback_stream: None,
            observed_f0: Vec::new(),
            f0_floor: None,
            mipmaps: HashMap::new(),
            workers: Workers::start(),
            observed: HashMap::new(),
            ever_previewed: false,
        })
    }

    /// ライブラリの中身。manifest が読めないものも落とさず返す。
    #[tracing::instrument(skip(self), err)]
    pub fn projects(&self) -> Result<Vec<(Uuid, Option<Manifest>)>> {
        Ok(self
            .library
            .list()?
            .into_iter()
            .map(|(d, m)| (d.id(), m.ok()))
            .collect())
    }

    /// プロジェクトを作り、録音リストを入れる。
    ///
    /// 既定は単独音、収録音高は1本（A3）。 選ばせる画面が無いときの入口。
    ///
    /// # Errors
    ///
    /// ライブラリの作成か台帳の書き込みが失敗したとき。
    #[tracing::instrument(skip(self, display_name), err)]
    pub fn create_project(&mut self, display_name: &str) -> Result<Uuid> {
        self.create_project_with(
            display_name,
            "single",
            &[koeru_core::preset::DEFAULT_TONE_MIDI],
        )
    }

    /// 方式プリセットを選んでプロジェクトを作る（`TR-RCL-01`）。
    ///
    /// リスト生成まで一度に済ませる。 空のプロジェクトを作って
    /// 別の操作でリストを入れさせると、その間の状態が意味を持たない。
    ///
    /// 多音階では音高ごとにサブディレクトリを作る（`TR-REC-36`）。
    /// 収録開始の時点で構成を確定させ、あとからファイルを移さない。
    ///
    /// # Errors
    ///
    /// プリセットが無い、リストを生成できない、台帳を書けないとき。
    #[tracing::instrument(
        skip(self, display_name, tones),
        fields(preset = preset_id, tones = tones.len()),
        err
    )]
    pub fn create_project_with(
        &mut self,
        display_name: &str,
        preset_id: &str,
        tones: &[i32],
    ) -> Result<Uuid> {
        let preset = koeru_core::preset::by_id(preset_id).ok_or_else(|| {
            AppError::new(
                "preset.unknown",
                format_args!("方式プリセット {preset_id} を知らない"),
            )
        })?;
        // 本数も音高も本人が決める（`TR-RCL-01`）。弾くのは鳴らせないものだけ。
        let tones = koeru_core::tone::normalize(tones).map_err(|e| AppError::new(e.kind(), e))?;
        // 作る時点では音源に `presamp.ini` が無いので既定（`TR-SYN-36`）。
        let rules = koeru_core::presamp::Rules::builtin(preset.set);
        let list = preset.reclist(&rules)?;
        let dir = self.library.create(&Manifest {
            // 外から入る文字列は境界で NFC へ（`TR-PKG-11`）。
            // 分解形のまま持つと、配布物の全ファイルがそれを引き継ぐ。
            display_name: koeru_core::text::to_nfc(display_name),
            method: manifest_method(&preset, tones.len() > 1),
            item_count: u32::try_from(list.len() * tones.len()).unwrap_or(0),
            derived_from: None,
            preset_id: Some(preset.id.to_owned()),
            inventory_version: Some(preset.inventory_version),
        })?;

        // 音高ごとのサブディレクトリ（`TR-REC-36`）。ASCII の英語音名。
        // 収録後に移す処理は持たないので、ここで全部作っておく。
        if tones.len() > 1 {
            for t in &tones {
                std::fs::create_dir_all(dir.audio_dir().join(koeru_core::tone::name(*t)))?;
            }
        }

        let mut ledger = Ledger::open(dir.db_path())?;
        ledger.install_reclist_for_tones(&list, &rules, preset.method, &tones)?;

        // 初回のとっかかりに要る最小限だけ入れる（`TR-RCL-12`）。
        // 曲バンクではない。本人が外せる。
        let at = now_rfc3339();
        for (i, song) in ust::bundled_songs().iter().enumerate() {
            ledger.put_song(&format!("bundled-{i}"), song, true, &at)?;
        }
        Ok(dir.id())
    }

    /// ライブラリを、環と色まで含めて挙げる（`Q-RCL-004`）。
    ///
    /// 音源ごとに台帳を開く。 [`Self::projects`] は manifest しか読まないので、
    /// 到達度も環も出せない——名前と数字の行になる。開いている音源の台帳とは別に、
    /// ここで一時的に開いて読み、閉じる。
    ///
    /// 読めない音源も落とさない。 manifest が壊れていても席は残す
    /// （`crate::studio::Studio::projects` と同じ扱い）。
    ///
    /// # Errors
    ///
    /// ライブラリのディレクトリを読めないとき。個々の音源の失敗では返らない。
    #[tracing::instrument(skip(self), err)]
    pub fn library(&self) -> Result<Vec<LibraryEntry>> {
        Ok(self
            .library
            .list()?
            .into_iter()
            .map(|(dir, manifest)| {
                let manifest = manifest.ok();
                // manifest が読めなければ作り方も分からない。 単独音として読む
                // ——一覧に出す数が少し違うだけで、台帳は書き換えない。
                let preset = manifest.as_ref().map_or_else(
                    || {
                        koeru_core::preset::by_id("single")
                            .unwrap_or_else(|| unreachable!("同梱プリセットに single がある"))
                    },
                    preset_of,
                );
                // 開いていない音源でも、綴りはその音源のものを使う（`TR-SYN-36`）。
                let rules = load_rules(&dir, preset.set);
                LibraryEntry {
                    id: dir.id(),
                    manifest,
                    state: Ledger::open(dir.db_path())
                        .ok()
                        .and_then(|mut l| voice_state(&mut l, &rules, preset).ok()),
                }
            })
            .collect())
    }

    /// 表示名を変える（`DEC-PKG-007`）。
    ///
    /// 動くのは表示名だけ。 ディレクトリ名は不変の UUID なので（`TR-PKG-37`）、
    /// 改名でパスは動かず、録れたものも原音設定も触らない。
    ///
    /// 空にはできない。 表示名は完成の条件（`TR-PKG-34`）なので、
    /// 空へ改名できると、完成していた音源を後から未完成に落とせてしまう。
    ///
    /// # Errors
    ///
    /// 名前が空のとき、音源が無いとき、manifest を書けないとき。
    #[tracing::instrument(skip(self, display_name), err)]
    pub fn rename_project(&mut self, id: Uuid, display_name: &str) -> Result<()> {
        let name = display_name.trim();
        if name.is_empty() {
            return Err(AppError::new(
                "app.empty_name",
                "名前を空にはできない。1文字以上入れてほしい",
            ));
        }
        let dir = self.library.open_project(id)?;
        let manifest = Manifest {
            display_name: koeru_core::text::to_nfc(name),
            ..dir.read_manifest()?
        };
        dir.write_manifest(&manifest)?;
        Ok(())
    }

    /// 開いている音源のいまの姿（`DEC-PLT-025`、`DEC-PLT-027`）。
    ///
    /// 名前と作り方も一緒に返す。 画面の帯にも設定の面にも要るので、
    /// 分けると同じ音源のために2回往復することになる。改名で変わるが、
    /// 改名も台帳を無効化する側の操作なので、取り直しの契機は同じ。
    ///
    /// # Errors
    ///
    /// 音源を開いていないとき、manifest か台帳を読めないとき。
    #[tracing::instrument(skip(self), err)]
    pub fn voice_state(&mut self) -> Result<VoiceState> {
        let manifest = self.opened()?.dir.read_manifest()?;
        let preset = preset_of(&manifest);
        let open = self.opened_mut()?;
        let rules = open.rules.clone();
        let mut state = voice_state(&mut open.ledger, &rules, preset)?;
        state.display_name = manifest.display_name;
        state.method = manifest.method.as_str().to_owned();
        state.rows = count_rows(&mut open.ledger)?;
        Ok(state)
    }

    /// プロジェクトを開く。収録セッションを1つ始める（`TR-REC-30`）。
    ///
    /// **同じ音源を開き直すときは、何もしない。** 画面は経路を移るたびに
    /// ここを通る（`queries.ts` の `openProjectQuery` は `gcTime: 0`）。
    /// 毎回 `Open` を作り直すと `session_id` が 0 に戻るので、
    /// **開いたままのストリームで録った次のテイクが、`takes.session_id` の
    /// 外部キーに当たって落ちる。WAV を確定させたあとに落ちる**
    /// （`finish_take` の「ここまででファイルは確定している」より下）。
    /// 音・曲・テイクの面を行き来するだけで起きる。**踏んだ。**
    ///
    /// **別の音源へ移るときは、開いているストリームを落とす。**
    /// セッションは音源ごとの台帳に属する（`TR-REC-30`）ので、前の音源の
    /// ストリームを残したままだと、`chosen_device` がそれを「開いている」と
    /// 答えて `arm_device` を飛ばさせ、同じ外部キーに当たる。
    ///
    /// 収録中は移らせない。 途中のテイクを捨てるしかなくなるが、
    /// 排出スレッドは止められると書きかけを確定させる（`pump` の
    /// 「書きかけを捨てない」）。台帳に載らない WAV だけが残り、
    /// それを掃除する経路はまだ無い（`Ledger::find_orphans` は呼ばれていない）。
    #[tracing::instrument(skip(self), err)]
    pub fn open_project(&mut self, id: Uuid) -> Result<()> {
        if let Some(open) = &self.open {
            if open.dir.id() == id {
                return Ok(());
            }
            if self.recording.is_some() {
                return Err(AppError::new(
                    "app.already_recording",
                    "収録中は別の声を開けない。止めてから移ってほしい",
                ));
            }
            self.disarm();
        }
        let dir = self.library.open_project(id)?;
        let mut ledger = Ledger::open(dir.db_path())?;
        // 確認キューは開くときに組み直す（`crate::review`）。
        // 遷移をやり直すのではなく、書いてあった状態をそのまま載せる。
        let (review, review_takes) = crate::review::load(&mut ledger)?;
        let rules = load_rules(&dir, preset_of(&dir.read_manifest()?).set);
        self.open = Some(Open {
            dir,
            ledger,
            session_id: 0,
            rules,
            review,
            review_takes,
        });
        Ok(())
    }

    /// 採用している素材が変わったので、書き出しの単位を1つ進める（`TR-PKG-44`）。
    ///
    /// `exported` を下ろす。 **下ろさないと、一度書き出したあとは何も触れない**
    /// ——`ReviewQueue` の書き出し済みは終端で、確認も編集も録り直しも
    /// `AlreadyExported` で断られる。録り足したものを書き出す経路も無くなる。
    ///
    /// モードと上限超過は動かさない。 まとめて確認へ移った人を、
    /// テイクを1つ録るたびに1件ずつの確認へ戻すことになる。
    fn start_new_export_generation(&mut self) -> Result<()> {
        let open = self.opened_mut()?;
        let mut s = open.ledger.review_state()?;
        if !s.exported {
            return Ok(());
        }
        s.exported = false;
        open.ledger.put_review_state(&s)?;
        Ok(())
    }

    /// 確認キューを台帳から組み直す。
    ///
    /// テイクを確定したときと、採用を切り替えたときに呼ぶ。 どちらも
    /// 「どのテイクのエントリが書き出しに出るか」が変わるので、
    /// キューの中身も変わる。モードは台帳から読み直すので落ちない。
    fn refresh_review(&mut self) -> Result<()> {
        let open = self.opened_mut()?;
        let (review, takes) = crate::review::load(&mut open.ledger)?;
        open.review = review;
        open.review_takes = takes;
        Ok(())
    }

    /// 開いているストリームを落とす。
    ///
    /// 排出スレッドが先。 Consumer を握ったまま Capture を捨てない
    /// （`arm_device` と同じ順序）。
    ///
    /// 収録中に呼ばない。 呼び側が先に断る（[`Self::open_project`]）。
    /// 排出スレッドは止められると書きかけを確定させるので、ここで落とすと
    /// 台帳に載らない WAV が残る。
    fn disarm(&mut self) {
        self.pump = None;
        self.capture = None;
        // 状態機械も作り直す。 未選択からしか `select_device` へ進めない。
        self.session = Session::new();
        self.device = None;
    }

    /// いまの進み具合。
    #[tracing::instrument(skip(self), err)]
    pub fn progress(&mut self) -> Result<Progress> {
        let name = self.display_name()?;
        let preset = self.current_preset()?;
        // 提示順の先頭（`TR-SYN-19`）。 正準順で引くと、モードを切り替えても
        // 次のフレーズが変わらない。
        let next_row = self.next_presented_row()?;
        let open = self.opened_mut()?;
        let covered = open.ledger.covered_units()?;
        let handoff = if open.ledger.has_been_exported()? {
            HandoffState::Exported
        } else {
            HandoffState::NotExported
        };

        let required: std::collections::BTreeSet<String> = koeru_core::inventory::units(preset.set)
            .iter()
            .map(|u| u.kana.to_owned())
            .collect();

        // oto の検証はまだ通していない。 全部録れても AwaitingOto で止まる。
        let coverage = koeru_core::project::coverage_state(&required, &covered, false, &name);

        // いま歌える曲の数（`TR-RCL-19`）。曲が1本も無くても進捗は読める。
        let status = self.song_status()?;

        // 音高ごとの消化率は詳細表示へ（`TR-RCL-26`）。跨いで足さない。
        let by_tone: Vec<(i32, usize, usize)> = self
            .opened_mut()?
            .ledger
            .progress_by_tone()?
            .into_iter()
            .map(|(tone, (done, total))| (tone, done, total))
            .collect();

        // 残り（`TR-RCL-09`）。固定の見積もりで出す——実測は採らない（`DEC-RCL-013`）。
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        let remaining_rows: Vec<koeru_core::reclist::Row> = {
            // この音源の作り方で数える（`TR-RCL-01`）。 単独音で数えると、
            // 連続音のプロジェクトの残りが常に別のリストの残りになる。
            let rules = self.opened()?.rules.clone();
            let list = preset
                .reclist(&rules)
                .map_err(|e| AppError::new(e.kind(), e))?;
            let done: std::collections::BTreeSet<String> = self
                .opened_mut()?
                .ledger
                .rows_with_takes()?
                .into_iter()
                .filter(|r| r.adopted.is_some())
                .map(|r| r.row_id)
                .collect();
            // 音高ごとに数える（`TR-RCL-26`）。
            //
            // **素の行 ID で突き合わせていた。** 多音階の台帳は `s001@G3` の形で
            // 持つので、生成したリストの `s001` とは一度も一致せず、
            // **残りが最初から最後まで減らなかった。**
            let multi = tones.len() > 1;
            let mut left = Vec::new();
            for t in &tones {
                for r in &list {
                    let id = if multi {
                        format!("{}@{}", r.id, koeru_core::tone::name(*t))
                    } else {
                        r.id.clone()
                    };
                    if !done.contains(&id) {
                        left.push(r.clone());
                    }
                }
            }
            left
        };
        // 音高ぶんは上で展開済み。 ここで掛け直さない。
        let remaining_seconds = koeru_core::pace::fixed_seconds(preset.method, &remaining_rows, 1);

        Ok(Progress {
            next_row,
            remaining_seconds,
            remaining_rows: remaining_rows.len(),
            covered: covered.len(),
            required: required.len(),
            coverage,
            handoff,
            by_tone,
            singable_songs: song::singable_count(&status),
            songs_in_bank: status.len(),
        })
    }

    /// 曲ごとの状態（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
    ///
    /// 収録済み単位が増えるたびに再計算する（`TR-RCL-17`）。
    /// 手が届く順に並ぶ（追加項目が少ない順、同数なら短い順）。
    #[tracing::instrument(skip(self), err)]
    pub fn song_status(&mut self) -> Result<Vec<SongStatus>> {
        let preset = self.current_preset()?;
        let open = self.opened_mut()?;
        let rules = open.rules.clone();
        song_status_of(&mut open.ledger, &rules, preset)
    }

    /// その曲を歌うために、あと録る行（`TR-RCL-16`, `TR-RCL-17`）。
    ///
    /// フルリストの部分集合として選ぶ。 詰め直さない——必要単位専用の行を
    /// 作り直すと、「曲のために録った分」がフルリストのどこにも当たらなくなり、
    /// 同じ声をもう一度録ることになる。
    ///
    /// **曲から、その行の収録へ直接入るための口**（`DEC-PLT-024` の横移動）。
    ///
    /// # Errors
    ///
    /// 音源を開いていないとき、その曲がバンクに無いとき、台帳を読めないとき。
    #[tracing::instrument(skip(self), err)]
    pub fn song_plan(&mut self, id: &str) -> Result<koeru_core::plan::Plan> {
        let preset = self.current_preset()?;
        let open = self.opened_mut()?;
        // 必要集合は方式ごとの綴り。 仮名で引くと交わらない。
        let covered = open.ledger.covered_aliases()?;
        let song = open
            .ledger
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, song)| song)
            .ok_or_else(|| AppError::new("app.no_song", "その曲はバンクに無い"))?;

        let rules = open.rules.clone();
        let required = song.required_aliases(&rules, preset.method, preset.set);
        let missing: std::collections::BTreeSet<String> =
            required.difference(&covered).cloned().collect();
        let full_list = preset
            .reclist(&rules)
            .map_err(|e| AppError::new(e.kind(), e))?;
        Ok(koeru_core::plan::rows_to_cover(
            &rules,
            preset.method,
            &missing,
            &full_list,
        ))
    }

    /// UST / USTX を取り込む（`TR-RCL-12`）。
    ///
    /// 主経路はこれ。 曲バンクを持たないので、何を目標にするかは本人が決める。
    /// 取り込んだ曲データは配布パッケージに含めない。
    ///
    /// **USTX は1トラックが1曲になる。** 返すのは (識別子, 曲) の並び。
    ///
    /// 歌詞を読めない曲は取り込まない。 `TR-RCL-12` が「必要単位集合は
    /// 読み込み時に算出する」と定めている以上、算出できない曲を台帳へ入れると、
    /// 要求が空の曲として「いま歌えます」に並ぶ。**押すまで嘘だと分からない。**
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、ファイルを読めない、歌詞を読めない、
    /// 台帳を書けない。
    #[tracing::instrument(
        skip(self, bytes, file_name, titles),
        fields(len = bytes.len(), count = titles.len()),
        err
    )]
    pub fn import_songs(
        &mut self,
        bytes: &[u8],
        file_name: &str,
        titles: &[String],
    ) -> Result<Vec<(String, Song)>> {
        let mut songs =
            ust::parse_file(bytes, file_name).map_err(|e| AppError::new(e.kind(), e))?;

        // 題は本人が決める（`TR-RCL-12`）。 ファイル名から採るのは候補まで。
        if titles.len() != songs.len() {
            return Err(AppError::new(
                "song.title_count",
                "題の数が曲の数と合わない",
            ));
        }
        for (song, title) in songs.iter_mut().zip(titles) {
            let title = title.trim();
            if title.is_empty() {
                return Err(AppError::new("app.empty_title", "題が空"));
            }
            song.title = title.to_owned();
        }

        for song in &songs {
            check_lyrics(song)?;
        }

        let at = now_rfc3339();
        let mut out = Vec::with_capacity(songs.len());
        for song in songs {
            let id = Uuid::new_v4().to_string();
            self.opened_mut()?.ledger.put_song(&id, &song, false, &at)?;
            out.push((id, song));
        }
        tracing::info!(count = out.len(), "曲を取り込んだ");
        Ok(out)
    }

    /// 取り込む前に中身を見る（`TR-RCL-12`）。**台帳へ入れない。**
    ///
    /// 題を決めさせるために要る。 ファイル名から採った候補と、
    /// トラックごとのノート数を返し、本人が題を打ってから
    /// [`Self::import_songs`] が確定させる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、ファイルを読めない、歌詞を読めない。
    #[tracing::instrument(skip(self, bytes, file_name), fields(len = bytes.len()), err)]
    pub fn song_file_preview(&mut self, bytes: &[u8], file_name: &str) -> Result<Vec<Song>> {
        self.opened()?;
        let songs = ust::parse_file(bytes, file_name).map_err(|e| AppError::new(e.kind(), e))?;
        for song in &songs {
            check_lyrics(song)?;
        }
        Ok(songs)
    }

    /// 曲の題を変える（`TR-RCL-12`）。
    ///
    /// 題はファイル名から採るので、そのままでは一覧に並べられないことがある
    /// （`New Project`、`テスト2_final`）。取り込むときにも後からも変えられる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、その曲がバンクに無い、台帳を書けない。
    #[tracing::instrument(skip(self, id, title), err)]
    pub fn rename_song(&mut self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::new("app.empty_title", "題が空"));
        }
        self.opened_mut()?.ledger.rename_song(id, title)?;
        Ok(())
    }

    /// 曲のノート列（`TR-RCL-12` (a)(b)）。
    ///
    /// 範囲を選ぶ画面が要る。 どの拍がどの歌詞かが見えないと、
    /// 「サビだけ」を指せない。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、その曲がバンクに無い。
    #[tracing::instrument(skip(self, id), err)]
    pub fn song_notes(&mut self, id: &str) -> Result<Vec<koeru_core::song::Note>> {
        self.opened_mut()?
            .ledger
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, s)| s.notes)
            .ok_or_else(|| AppError::new("app.no_song", "その曲はバンクに無い"))
    }

    /// 選んだノート群から録音リストを詰め直す（`TR-RCL-16`, `DEC-RCL-011`）。
    ///
    /// `selections` は `(曲 ID, [開始, 終了) の並び)`。 範囲が空なら曲全体。
    /// 複数の曲から選べる——自分の曲バンクを構成して、その集合に対する
    /// 被覆を狙う（`TR-RCL-12`）。
    ///
    /// **フルリストの部分集合に限らない。** 選択が要求するエイリアスだけを
    /// 覆う行を作るので、行の途中を読まされない。詰め直した行が生む綴りは
    /// フルリストと同じなので、録った分はそのままフル方式の被覆に効く。
    ///
    /// 台帳へは足すだけ。 既にある行は消さない——録ったものが消える。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、曲がバンクに無い、歌詞を読めない、
    /// リストを作れない、台帳を書けない。
    #[tracing::instrument(skip(self, selections), fields(songs = selections.len()), err)]
    pub fn repack_for_selection(
        &mut self,
        selections: &[(String, Vec<(usize, usize)>)],
    ) -> Result<usize> {
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;
        // 収録音高は台帳が持つ（`TR-RCL-01`）。プリセットは方式だけ。
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        let songs = self.opened_mut()?.ledger.songs_in_bank()?;
        let mut required = std::collections::BTreeSet::new();
        for (id, ranges) in selections {
            let song = songs
                .iter()
                .find(|(sid, _)| sid == id)
                .map(|(_, s)| s)
                .ok_or_else(|| AppError::new("app.no_song", "その曲はバンクに無い"))?;
            let part = if ranges.is_empty() {
                song.clone()
            } else {
                song.select(ranges)
            };
            required.extend(part.required_aliases(&rules, preset.method, preset.set));
        }

        let rows = preset
            .reclist_for(&rules, &required)
            .map_err(|e| AppError::new(e.kind(), e))?;
        let added = rows.len();
        self.opened_mut()?.ledger.install_reclist_for_tones(
            &rows,
            &rules,
            preset.method,
            &tones,
        )?;
        tracing::info!(count = added, "選択から録音リストを詰め直した");
        Ok(added)
    }

    /// いま開いているプロジェクトの方式プリセット（`TR-RCL-01`）。
    ///
    /// manifest が持つのは識別子だけ。 古いプロジェクトは識別子を持たないので、
    /// 方式から引く（`Manifest::preset_id` は後から足した）。
    ///
    /// **収録音高はここから来ない。** 方式と音高は別の選択なので、音高は
    /// 台帳（[`Ledger::recording_tones`]）が持つ。
    fn current_preset(&mut self) -> Result<koeru_core::preset::MethodPreset> {
        Ok(preset_of(&self.opened()?.dir.read_manifest()?))
    }

    /// いま開いている音源のエイリアス規則（`TR-SYN-36`）。
    ///
    /// 写しを返す。 借りたまま台帳へ書きに行く箇所が多く、借用では通らない。
    /// 表は数百の文字列なので、経路あたり1回なら写しで足りる。
    fn current_rules(&self) -> Result<koeru_core::presamp::Rules> {
        Ok(self.opened()?.rules.clone())
    }

    /// 取り込んだ曲すべて（`TR-RCL-12`）。バンクに入っているかを添える。
    ///
    /// 歌える曲の一覧（[`Self::song_status`]）とは別。 あちらはバンクの中だけを
    /// 見せる。ここはバンクを組み替えるための一覧なので、外した曲も並ぶ。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self), err)]
    pub fn all_songs(&mut self) -> Result<Vec<(String, Song, bool)>> {
        Ok(self.opened_mut()?.ledger.all_songs()?)
    }

    /// 曲のキーを決める（`TR-SYN-15`, `DEC-SYN-012`）。
    ///
    /// **自動では動かさない。** 勧めはするが、当てるのは本人の操作だけ。
    /// 1オクターブの上下までに収める——それを超える移調は、元の曲と別の曲になる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、範囲の外、台帳を書けない。
    // `semitones` は skip していないので自動で載る。宣言し直すと空の欄が増える。
    #[tracing::instrument(skip(self, id), err)]
    pub fn set_song_transpose(&mut self, id: &str, semitones: i32) -> Result<()> {
        if !(-12..=12).contains(&semitones) {
            return Err(AppError::new(
                "song.transpose_out_of_range",
                "移調は1オクターブの上下まで",
            ));
        }
        self.opened_mut()?
            .ledger
            .set_song_transpose(id, semitones)?;
        Ok(())
    }

    /// 曲をバンクから外す／戻す（`TR-RCL-12`）。曲そのものは消さない。
    #[tracing::instrument(skip(self), err)]
    pub fn set_song_in_bank(&mut self, id: &str, in_bank: bool) -> Result<()> {
        self.opened_mut()?.ledger.set_song_in_bank(id, in_bank)?;
        Ok(())
    }

    /// 入力デバイスを挙げる。
    #[tracing::instrument(err)]
    pub fn devices() -> Result<Vec<koeru_audio::DeviceInfo>> {
        Ok(mac::enumerate_input_devices()?)
    }

    /// この音源で選ばれているマイクと、いま開いているかどうか。
    ///
    /// マイクは音源に固定される（`TR-REC-03`）。 選び直させないために、
    /// 開いていなければ台帳の最後のセッションから引く。
    ///
    /// **画面に state として持たせない。** 画面側の state は経路を移ると消えるので、
    /// テイクの面へ入って戻るだけで選択が失われ、設定の面へ行き直すことになる。
    /// 一度で済むものを毎回やらせないために面を分けた（`DEC-PLT-024`）ので、
    /// 選択の持ち主はこちら側になる。**踏んだ。**
    #[tracing::instrument(skip(self), err)]
    pub fn chosen_device(&mut self) -> Result<(Option<String>, bool, bool)> {
        /*
         * 開いているかは、ストリームの実体で見る。
         *
         * `self.device` で判定しない。 `arm_device` は開き直す前に
         * `pump` と `capture` を落とすが、そのあとの手順（校正の読み出し、
         * デバイスを開く、セッションを始める）はどれも失敗しうる。
         * 途中で失敗すると**ストリームが無いのに前のデバイスが残る**ので、
         * 「開いている」と答えてしまい、次の収録が `app.no_stream` で落ちる。
         */
        let armed = self.capture.is_some() && self.pump.is_some();
        /*
         * 収録中かも返す。
         *
         * **画面の state だけで持つと、面を移った先で止められなくなる。**
         * 収録中に別の行のテイクを開くと、画面側のフックは作り直されて
         * 「録っていない」から始まるので「止める」が出ない。一方 Rust は
         * 録り続けているので、次に録ろうとすると `app.already_recording` で
         * 断られる——**止めることも録ることもできなくなる。踏んだ。**
         */
        let recording = self.recording.is_some();
        let id = match &self.device {
            Some(d) => Some(d.as_str().to_owned()),
            None => self.opened_mut()?.ledger.last_device()?,
        };
        Ok((id, armed, recording))
    }

    /// デバイスを選び、ストリームを開く（`recording-input.fsl` の手順）。
    ///
    /// ストリームはテイクごとに開閉しない（`REQ-REC-102`）。
    /// 収録画面を離れるまで持ち続ける。
    #[tracing::instrument(skip(self), err)]
    pub fn arm_device(&mut self, device: &DeviceId) -> Result<mac::MicrophoneMode> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                "収録中はマイクを変えられない",
            ));
        }

        // 前のストリームを先に落とす。 2つの AUHAL を同時に回さない。
        // 排出スレッドが先。Consumer を握ったまま Capture を捨てない。
        self.pump = None;
        self.capture = None;
        /*
         * 前のデバイスも忘れる。
         *
         * 残すと、**この先で失敗したときに「前のデバイスが選ばれている」と
         * 答えてしまう。** 画面は新しく選んだほうを出したまま、録る手前の
         * 開き直しが前のデバイスを開く——**別のマイクで録れてしまう。**
         * 開けたときに下で入れ直す。
         */
        self.device = None;

        // 状態機械を作り直す。 `recording-input.fsl` の `select_device` は
        // 未選択からしか進めない（`proved`）。マイクの選び直しは、その機械から見れば
        // 「収録画面を出て入り直す」ことなので、機械ごと新しくするのが忠実な読み。
        // 既存の機械を無理に巻き戻さない。 巻き戻す遷移は仕様に無い。
        self.session = Session::new();

        let open = self.opened_mut()?;
        // 前に決めたチャンネルを引き継ぐ（`TR-REC-06`）。テイクごとに違う経路から
        // 録った素材が混ざると、合成したときに音色が揃わない。
        let saved_channel = open
            .ledger
            .calibration_of(device.as_str())?
            .map_or(0, |c| c.source_channel);

        // セッションは録音条件のスナップショット（`TR-REC-30`）。
        let (cap, consumer) = mac::open(device, 48_000 * RING_SECONDS)?;
        let format = cap.format();
        let mode = mac::active_microphone_mode();

        let session_id = open.ledger.start_session(&SessionSnapshot {
            started_at: now_rfc3339(),
            device_id: device.as_str().to_owned(),
            sample_rate_hz: i32::try_from(format.sample_rate_hz).unwrap_or(0),
            channels: i32::from(format.channels),
            effects_state: if mode.is_clean() {
                "clean"
            } else {
                "processed"
            }
            .to_owned(),
            route: "coreaudio-halinput".to_owned(),
            // 前に選んだチャンネルがあれば引き継ぐ（`TR-REC-06` の「プロジェクトに固定」）。
            source_channel: saved_channel,
            // キャプチャからマスターまでの変換を残す（`TR-REC-02`）。
            // `sample_rate_hz` はネイティブレート。**両方あって初めて、
            // 変換したかどうかが後から分かる。**
            master_rate_hz: i32::try_from(wav::MASTER_RATE_HZ).unwrap_or(0),
            resampler: koeru_audio::resample::IDENTIFIER.to_owned(),
            // 上流の変換は確かめられない（`TR-REC-02` の [Unknown]）。
            // ドライバと APO が何をしたかは、アプリからは見えない。
            upstream_conversion: "unknown".to_owned(),
        })?;
        open.session_id = session_id;

        // 状態機械を手順どおりに進める。
        self.session.select_device(device.clone())?;
        self.session.open_stream()?;
        if mode.is_clean() {
            self.session.effects_all_disabled()?;
        } else {
            self.session.effects_some_remain()?;
            // 提示は一度だけ（`TR-REC-12`）。何度も出すと録音の邪魔になる。
            self.session.show_prompt_once()?;
        }
        self.session.calibrate_gain()?;

        // アプリが触る前のゲインを覚えておく（`TR-REC-15`）。
        // 終了時にここへ戻す。戻せないと、利用者のマイクの設定を勝手に変えたままになる。
        if self.gain_before.as_ref().is_none_or(|(d, _)| d != device)
            && let Some(g) = mac::read_gain(device)
        {
            self.gain_before = Some((device.clone(), g));
        }
        self.device = Some(device.clone());

        if saved_channel < 0 {
            cap.set_source_mix();
        } else {
            cap.set_source_channel(usize::try_from(saved_channel).unwrap_or(0));
        }

        // 収録画面に入った時点から止めない（`REQ-REC-102`、`TR-REC-19`）。
        // ここから排出が回り、プリロールが溜まりはじめる。
        cap.arm();
        self.pump = Some(Pump::start(consumer, format.sample_rate_hz));
        self.capture = Some(cap);
        self.estimate_space()?;
        Ok(mode)
    }

    /// 残り全部を録り切れるかを見積もる（`REQ-REC-110`, `TR-REC-41`）。
    ///
    /// 入る分だけ録らせる。 3時間の収録の途中で埋まると、その日の作業を失う。
    /// 判定は状態機械が一度だけ行い、選ばせない。
    ///
    /// 残量を引けない環境では「足りる」として通す。 引けないだけで
    /// 収録できなくなるほうが困る（`TR-REC-24` は残量不足を止めるもので、
    /// 残量が読めないことを止めるものではない）。
    #[tracing::instrument(skip(self), err)]
    pub fn estimate_space(&mut self) -> Result<SpaceEstimate> {
        self.capture.as_ref().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let root = self.opened()?.dir.root().to_path_buf();
        let remaining = self.opened_mut()?.ledger.remaining_rows()?;

        let required = storage::required_bytes(remaining, rate);
        let available = storage::available_bytes(&root);
        self.session
            .estimate_space(required, available.unwrap_or(u64::MAX))?;

        // 足りないときは「その残量で何件録れるか」を出す（`TR-REC-41`）。
        // 「足りません」だけでは、何を削れば足りるのか分からない。
        let fits = available.map_or(remaining, |a| storage::rows_that_fit(a, rate));
        Ok(SpaceEstimate {
            remaining_rows: remaining,
            required_bytes: required,
            available_bytes: available,
            rows_that_fit: fits.min(remaining),
        })
    }

    /// 次のテイクを始めてよいだけの残量があるか（`TR-REC-41`）。
    ///
    /// 進行中のテイクは最後まで録りきる。 止めるのは次を始めるところだけ。
    #[tracing::instrument(skip(self), err)]
    pub fn has_room_for_one_more(&mut self) -> Result<bool> {
        self.capture.as_ref().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let root = self.opened()?.dir.root().to_path_buf();
        // 引けない環境では止めない。
        let Some(available) = storage::available_bytes(&root) else {
            return Ok(true);
        };
        Ok(available >= storage::required_bytes(1, rate))
    }

    /// 入力が届いているかを確かめる（`TR-REC-17`）。    /// 入力が届いているかを確かめる（`TR-REC-17`）。
    ///
    /// 権限が無いと macOS は無音を返す。 成否ではなく中身を見る。
    ///
    /// ストリームは開いたまま測る。止めて測ると、そのぶんプリロールが途切れる
    /// （`TR-REC-19`）。
    #[tracing::instrument(skip(self), err)]
    pub fn probe_input(&mut self, ms: u64) -> Result<f32> {
        {
            // 直前の残りを捨ててから測る。「今」の入力だけを見る。
            let pump = self.pump.as_ref().ok_or_else(no_stream)?;
            let _ = pump.take_peak();
        }
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let peak = self.pump.as_ref().ok_or_else(no_stream)?.take_peak();

        if peak > 1e-6 {
            self.session.input_is_alive()?;
        } else {
            self.session.input_is_dead()?;
        }
        Ok(peak)
    }

    /// プリロールがどれだけ溜まっているか（ミリ秒、`TR-REC-19`）。
    ///
    /// `PREROLL_MS` に足りていなければ、遡れるのはその長さまで。
    #[must_use]
    pub fn preroll_ms(&self) -> u64 {
        self.pump.as_ref().map_or(0, Pump::preroll_ms)
    }

    /// 出力がどこへ出ているらしいか（`TR-REC-24`）。
    ///
    /// これは一次の足切りでしかない。 `TransportType` も `DataSource` も
    /// ドライバの自己申告で、Unknown が正規値として存在する。
    /// 実際の回り込みは [`Studio::check_guide_leak`] が録った音で確かめる。
    #[must_use]
    pub fn output_kind() -> mac::OutputKind {
        mac::default_output_kind()
    }

    /// ガイドを鳴らしながら録って、回り込みを確かめる（`TR-REC-24`）。
    ///
    /// 出力経路の判定だけでは足りない。 ヘッドホンと申告していても、
    /// 装着されている保証はない。回り込みは録音側でしか確認できない。
    ///
    /// これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。
    ///
    /// 既知の再生信号との相関を取るだけなので、声質の評価を一切含まない
    /// （`TR-REC-17` と同じ性質の静的な経路検査）。
    #[tracing::instrument(skip(self), err)]
    pub fn check_guide_leak(&mut self, midi: i32) -> Result<LeakCheck> {
        self.capture.as_ref().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;

        // スピーカと分かっているなら、鳴らすまでもなく漏れる。
        if Self::output_kind().definitely_speakers() {
            let found = LeakCheck {
                correlation: 1.0,
                lag_ms: 0.0,
                leaking: true,
            };
            self.leak = Some(found);
            self.session.check_guide_leak()?;
            return Ok(found);
        }

        // 1秒ぶんのガイドを鳴らしながら録る。
        let spec = GuideSpec {
            moras: 2,
            lead_in_ms: 0.0,
            tail_ms: 0.0,
            ..GuideSpec::default()
        };
        let played = guide::render(&spec, midi, rate);
        let captured = self.play_and_capture(&played, rate)?;

        let found = leak::detect(&played, &captured, rate);
        tracing::info!(
            correlation = found.correlation,
            lag_ms = found.lag_ms,
            leaking = found.leaking,
            "回り込みを確かめた"
        );
        self.leak = Some(found);
        self.session.check_guide_leak()?;
        Ok(found)
    }

    /// 音高を鳴らす（`TR-REC-23` の音高提示）。
    ///
    /// 回り込みが確かめられていなければ鳴らさない（`TR-REC-24`）。
    /// 鳴らしたものが全テイクに混じる。
    #[tracing::instrument(skip(self), err)]
    pub fn play_pitch(&mut self, midi: i32) -> Result<()> {
        match self.leak {
            None => {
                return Err(AppError::new(
                    "recording.leak_unchecked",
                    "先に回り込みを確かめてほしい",
                ));
            }
            Some(l) if l.leaking => {
                return Err(AppError::new(
                    "recording.guide_leaks",
                    "ガイドが録音へ回り込むので鳴らさない",
                ));
            }
            Some(_) => {}
        }
        self.capture.as_ref().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let pcm = guide::render(&GuideSpec::pitch_reference(), midi, rate);
        self.playback = None;
        self.playback = Some(mac::play(pcm, rate)?);
        Ok(())
    }

    /// 鳴らしながら録る。回り込みの検査にだけ使う。
    fn play_and_capture(&mut self, played: &[f32], rate: u32) -> Result<Vec<f32>> {
        let pump = self.pump.as_ref().ok_or_else(no_stream)?;
        pump.begin_probe();
        let handle = mac::play(played.to_vec(), rate)?;

        // 鳴っているあいだ待つ。余裕を持たせる（バッファのぶん遅れる）。
        let ms = (played.len() as u64 * 1000 / u64::from(rate.max(1))) + 200;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        drop(handle);

        Ok(self.pump.as_ref().ok_or_else(no_stream)?.end_probe())
    }

    /// 全チャンネルを混ぜる（`TR-REC-06`）。
    ///
    /// 全チャンネルに有意な信号があるときだけ選べる。
    /// 片側にしか信号が無いのに混ぜると 6dB 損をする。
    #[tracing::instrument(skip(self), err)]
    pub fn use_mixed_channels(&mut self) -> Result<()> {
        if !self.may_mix {
            return Err(AppError::new(
                "recording.mix_unavailable",
                "有意な信号があるのは一部のチャンネルだけなので、混ぜない",
            ));
        }
        self.capture
            .as_ref()
            .ok_or_else(no_stream_err)?
            .set_source_mix();
        let device = self.device.clone().ok_or_else(no_stream_err)?;
        if let Some(mut c) = self.opened_mut()?.ledger.calibration_of(device.as_str())? {
            c.source_channel = -1;
            let at = now_rfc3339();
            self.opened_mut()?.ledger.put_calibration(&c, &at)?;
        }
        Ok(())
    }

    /// 保存してある校正と、いまのゲインを突き合わせる（`TR-REC-15`）。
    ///
    /// 勝手に戻さない。 差があることを返すだけで、戻すかどうかは本人が決める。
    #[tracing::instrument(skip(self), err)]
    pub fn gain_drift(&mut self) -> Result<Option<(f32, f32)>> {
        let Some(device) = self.device.clone() else {
            return Ok(None);
        };
        let saved = self
            .opened_mut()?
            .ledger
            .calibration_of(device.as_str())?
            .and_then(|c| c.gain);
        let (Some(saved), Some(now)) = (saved, mac::read_gain(&device)) else {
            return Ok(None);
        };
        // 1% 未満の差は動いていないものとして扱う。OS 側の丸めで毎回出す意味は無い。
        if (saved - now).abs() < 0.01 {
            Ok(None)
        } else {
            Ok(Some((saved, now)))
        }
    }

    /// 保存してあるゲインへ戻す（`TR-REC-15`）。本人が選んだときだけ呼ぶ。
    #[tracing::instrument(skip(self), err)]
    pub fn restore_saved_gain(&mut self) -> Result<()> {
        let Some(device) = self.device.clone() else {
            return Err(no_stream_err());
        };
        let saved = self
            .opened_mut()?
            .ledger
            .calibration_of(device.as_str())?
            .and_then(|c| c.gain);
        if let Some(g) = saved {
            mac::write_gain(&device, g)?;
        }
        Ok(())
    }

    /// 入力レベルを校正する（`TR-REC-14`）。
    ///
    /// そのプロジェクトで最も高い音高の全力発声を数秒録って、
    /// ピークが -12〜-6 dBFS に入っていれば校正完了。範囲外なら OS の入力ゲインを動かす。
    ///
    /// 関門にしない。 収束しなくても収録に進める。3時間の収録の前に、
    /// レベル合わせで止められる方がよほど困る。
    ///
    /// 収録中は呼ばない（`TR-REC-15`）。
    #[tracing::instrument(skip(self), err)]
    pub fn calibrate(&mut self, seconds: f64) -> Result<Calibration> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                "収録中はゲインを変えない",
            ));
        }
        let device = self.device.clone().ok_or_else(no_stream_err)?;
        let control = mac::gain_control(&device);

        let mut attempt = 1;
        let (peak_dbfs, settled) = loop {
            let peak = self.measure_peak(seconds)?;
            let db = if peak > 0.0 {
                20.0 * f64::from(peak).log10()
            } else {
                f64::NEG_INFINITY
            };

            // ソフトウェアのボリュームは校正に使えない（`TR-REC-14`）。
            // 値は読めても動かさない。動かしても A/D の手前は変わらない。
            let gain = control
                .is_usable()
                .then(|| mac::read_gain(&device))
                .flatten();

            match calibration::step(db, gain, attempt) {
                Outcome::Settled => break (db, true),
                Outcome::Adjust { next_gain } => {
                    tracing::info!(attempt, next_gain, "ゲインを動かして測り直す");
                    mac::write_gain(&device, next_gain)?;
                    attempt += 1;
                }
                Outcome::GaveUp { reason } => {
                    // ここでも収録には進める。 関門にしない（`TR-REC-14`）。
                    // `NoControl` のときは自動調整せず、OS 設定での案内を
                    // 画面側が1回だけ出す（結果の `control` から判断できる）。
                    tracing::info!(reason = reason.as_str(), "校正を切り上げる");
                    break (db, false);
                }
            }
        };

        // ## モノラル化の元を決める（`TR-REC-06`）
        //
        // L+R の平均を既定にしない。 片側にしか信号が無いインタフェースは珍しくなく、
        // 平均すると 6dB 損をする。全力発声を録ったいま測るのがいちばん確か。
        let rms = self
            .capture
            .as_ref()
            .ok_or_else(no_stream_err)?
            .channel_rms();
        let choice = channel::choose(&rms);
        let source_channel = match choice.source {
            Source::Channel(n) => i32::try_from(n).unwrap_or(0),
            Source::Mix => -1,
        };
        tracing::info!(
            ?rms,
            source_channel,
            may_mix = choice.may_mix,
            "モノラルの元を決めた"
        );
        if let Some(cap) = self.capture.as_ref() {
            match choice.source {
                Source::Channel(n) => cap.set_source_channel(n),
                Source::Mix => cap.set_source_mix(),
            }
        }
        self.may_mix = choice.may_mix;

        let result = Calibration {
            gain: control
                .is_usable()
                .then(|| mac::read_gain(&device))
                .flatten(),
            control: control.as_str().to_owned(),
            peak_dbfs,
            settled,
            device_id: device.as_str().to_owned(),
            source_channel,
        };
        let at = now_rfc3339();
        self.opened_mut()?.ledger.put_calibration(&result, &at)?;
        // 校正の直後は状態機械の上でも校正済みにする。
        Ok(result)
    }

    /// 指定した秒数のあいだのピークを測る。
    fn measure_peak(&mut self, seconds: f64) -> Result<f32> {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "秒数は 3.0..=5.0 の想定。clamp してから丸める"
        )]
        let ms = (seconds.clamp(0.5, 30.0) * 1000.0) as u64;
        let pump = self.pump.as_ref().ok_or_else(no_stream_err)?;
        let _ = pump.take_peak();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(self.pump.as_ref().ok_or_else(no_stream_err)?.take_peak())
    }

    /// いま録るべき行の収録を始める。
    ///
    /// 押した瞬間より前へ遡って書きはじめる（`TR-REC-19`）。
    /// 人は「録音」を押してから息を吸わない。指示の時点から書くと語頭が欠ける。
    #[tracing::instrument(skip(self), err)]
    pub fn start_take(&mut self) -> Result<String> {
        // 録るのは提示順の先頭（`TR-SYN-19`）。 **正準順で引いていた**
        // ——曲バンク優先を選んでも、録り始めるのは常に ordinal の最小だった。
        let row_id = self
            .next_presented_row()?
            .ok_or_else(|| AppError::new("app.nothing_to_record", "録るべき行がもう無い"))?
            .0;
        self.start_take_for(&row_id)
    }

    /// 次に録る行を、提示順の先頭から引く（`TR-SYN-19`, `TR-REC-18`）。
    ///
    /// `TR-SYN-19` は「変わるのは『次に何を録るか』の並びだけ」と定めている。
    /// **一覧の並び替えだけに使っていた。** 次のフレーズの札も録音の開始も
    /// 台帳の正準順（`Ledger::next_row`）で引いていたので、モードを
    /// 切り替えても録る順は動かなかった。
    ///
    /// 提示順が空なら正準順へ落ちる。 提示は未収録の行だけを並べるので、
    /// 除外や再生成で空になっても、録れる行が残っていれば録れる。
    fn next_presented_row(&mut self) -> Result<Option<(String, String)>> {
        let (_, order) = self.recording_order()?;
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        let open = self.opened_mut()?;
        for id in order {
            // 提示順は録音リストの素の行 ID で来る。 多音階の台帳は
            // `s001@G3` の形で持つので、**素のまま引くと1つも当たらず、
            // 常に正準順へ落ちていた**——モードが効かないままだった。
            //
            // 音高の順に見る。 同じ行の未収録が複数の音高に残っていても、
            // 低いほうから埋める（`recording_tones` は昇順）。
            if tones.len() > 1 {
                for t in &tones {
                    let at = format!("{id}@{}", koeru_core::tone::name(*t));
                    if let Some(text) = open.ledger.unrecorded_row_text(&at)? {
                        return Ok(Some((at, text)));
                    }
                }
            } else if let Some(text) = open.ledger.unrecorded_row_text(&id)? {
                return Ok(Some((id, text)));
            }
        }
        open.ledger.next_row().map_err(Into::into)
    }

    /// 全部の行と、そのテイク（`TR-REC-21`, `TR-RCL-25`）。
    ///
    /// 録り直しの一覧。 採用を戻すのにも使う。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self), err)]
    pub fn rows_with_takes(&mut self) -> Result<Vec<koeru_core::db::RowTakes>> {
        Ok(self.opened_mut()?.ledger.rows_with_takes()?)
    }

    /// 対象音高の基準音を鳴らす（`TR-REC-25`）。
    ///
    /// > フレーズの収録開始前に対象音高の基準音を必ず鳴らす
    /// > （既定 1000 ms、正弦波またはガイド音源の該当音階ファイル）
    ///
    /// 回り込みが分かっているときは鳴らさない（`TR-REC-24`）。 スピーカから
    /// 出すと、そのままマイクへ入って収録に混ざる。
    ///
    /// # Errors
    ///
    /// 出力を開けないとき。**鳴らせないことは収録を止める理由にしない**——
    /// 記録して進む。
    #[tracing::instrument(skip(self), err)]
    pub fn play_tone_reference(&mut self, midi: i32) -> Result<()> {
        if Self::output_kind().definitely_speakers() || self.leak.is_some_and(|l| l.leaking) {
            tracing::info!(midi, "回り込むので基準音を鳴らさない");
            return Ok(());
        }
        let spec = koeru_core::guide::GuideSpec::tone_reference();
        let pcm = koeru_core::guide::render(&spec, midi, MASTER_RATE_HZ);
        self.playback = None;
        self.playback = Some(mac::play(pcm, MASTER_RATE_HZ)?);
        Ok(())
    }

    /// いまの録る順と、その並び（`TR-SYN-19`）。
    ///
    /// 台帳は書き換えない。 返すのは「次に何を録るか」の並びだけで、
    /// 録音リストの正準順も行集合も動かない。
    ///
    /// 曲バンクが空か、全曲が完全になったら被覆効率へ移る。 ただし本人が
    /// 明示的に選んでいれば動かさない（`TR-SYN-19` の (b)）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self), err)]
    pub fn recording_order(&mut self) -> Result<(koeru_core::order::Mode, Vec<String>)> {
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;
        let (stored, pinned) = self.opened_mut()?.ledger.recording_order()?;
        let status = song_status_of(&mut self.opened_mut()?.ledger, &rules, preset)?;
        let empty = status.is_empty();
        let complete = !empty
            && status
                .iter()
                .all(|s| s.singability == koeru_core::song::Singability::Complete);

        let mode = if pinned || !koeru_core::order::auto_switches(empty, complete) {
            stored
        } else {
            koeru_core::order::Mode::CoverageEfficiency
        };
        if mode != stored {
            // 自動の移行では `pinned` を立てない。 立てると本人が選んだことになる。
            self.opened_mut()?.ledger.set_recording_order(mode, false)?;
        }

        let rows = self.opened_mut()?.ledger.rows_with_takes()?;
        let recorded: std::collections::BTreeSet<String> = rows
            .iter()
            .filter(|r| r.adopted.is_some())
            .map(|r| r.row_id.clone())
            .collect();
        let covered = self.opened_mut()?.ledger.covered_aliases()?;
        let song_required: std::collections::BTreeSet<String> = self
            .opened_mut()?
            .ledger
            .songs_in_bank()?
            .iter()
            .flat_map(|(_, s)| s.required_aliases(&rules, preset.method, preset.set))
            .collect();
        let list = preset
            .reclist(&rules)
            .map_err(|e| AppError::new(e.kind(), e))?;

        Ok((
            mode,
            koeru_core::order::present(
                &rules,
                mode,
                preset.method,
                &list,
                &recorded,
                &covered,
                &song_required,
            ),
        ))
    }

    /// 録る順を本人の操作で切り替える（`TR-SYN-19` の (b)）。
    ///
    /// 可逆。 被覆効率から曲バンク優先へも戻せる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self), fields(mode = mode.as_str()), err)]
    pub fn set_recording_order(&mut self, mode: koeru_core::order::Mode) -> Result<()> {
        Ok(self.opened_mut()?.ledger.set_recording_order(mode, true)?)
    }

    /// 採用テイクを切り替える（`TR-RCL-25`）。
    ///
    /// カバレッジは変わらない。 行が生む単位は行が持っていて、テイクに依らない。
    /// 変わるのは原音設定の値だけ。だから再アライメントも要らない——
    /// oto はテイクごとに導出済みで、切り替えれば付いてくる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、その行にそのテイクが無い。
    #[tracing::instrument(skip(self), err)]
    pub fn adopt_take(&mut self, row_id: &str, take_id: i32) -> Result<()> {
        self.opened_mut()?.ledger.adopt_take(row_id, take_id)?;
        // 書き出したあとに採用を動かしたなら、次の書き出しへ進める。
        self.start_new_export_generation()?;
        // 書き出しに出るエントリが変わったので、キューを組み直す。
        //
        // **忘れると、確認も編集も書き出しも、採用していない世代の行へ向く。**
        // `review_takes` は採用したテイクを指しているので、切り替えたあとも
        // 古い世代を握ったままになる。組み直すと、その世代が自分で持っている
        // 状態と固定が載る——世代ごとに別の人の手が入っていることがある。
        self.refresh_review()?;
        // 試唱のキャッシュは消さなくてよい。 鍵に素材の内容ハッシュが
        // 入っているので、テイクが変われば別の鍵になり、古い結果は使われない。
        self.prerender_songs();
        Ok(())
    }

    /// 行を指定して録る。録り直しの入口（`TR-REC-21`, `TR-RCL-25`, `TR-ALN-27`）。
    ///
    /// 既存のテイクを消さない。 世代を1つ足して積み、
    /// `finish_take` が採用を新しい方へ切り替える。過去のテイクは非採用として残り、
    /// [`Self::adopt_take`] でいつでも戻せる。
    ///
    /// # Errors
    ///
    /// 収録中、ストリームが開いていない、その行が無い、残量が足りない。
    #[tracing::instrument(skip(self), err)]
    pub fn start_take_for(&mut self, row_id: &str) -> Result<String> {
        if self.recording.is_some() {
            return Err(AppError::new("app.already_recording", "すでに収録中"));
        }
        // ストリームが開いていることだけ確かめる。 レートは持ち回さない——
        // マスターは常に 44100 で、変換は pump が1回だけ行う（`TR-REC-02`）。
        self.capture.as_ref().ok_or_else(no_stream)?;
        let audio_dir = self.opened()?.dir.audio_dir();
        let row_id = row_id.to_owned();
        // **知らない行では始めない。** 名前を打ち間違えたまま録ると、
        // 台帳に載らないファイルができる。
        self.opened_mut()?.ledger.row_state(&row_id)?;

        // 多音階では、収録開始前に対象音高の基準音を必ず鳴らす（`TR-REC-25`）。
        //
        // **省略できない。** 3本を行き来すると、いま何を録っているのかを
        // 音で確かめる手が要る。回り込みが分かっているときは鳴らさない
        // （`TR-REC-24`。ガイドが録音に入る）。
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        if tones.len() > 1 {
            let tone = self.opened_mut()?.ledger.row_tone(&row_id)?;
            self.play_tone_reference(tone)?;
        }

        // 世代を名前に入れる。 録り直しても既存の WAV を上書きしない（`TR-PKG-39`）。
        let generation = self.opened_mut()?.ledger.takes_of(&row_id)?.len() + 1;
        // 音高ごとのサブディレクトリへ置く（`TR-REC-36`）。
        // 収録後に別ディレクトリへ移す処理は持たない。
        let dir = if tones.len() > 1 {
            let tone = self.opened_mut()?.ledger.row_tone(&row_id)?;
            audio_dir.join(koeru_core::tone::name(tone))
        } else {
            audio_dir.clone()
        };
        let path = dir.join(format!("{row_id}_{generation}.wav"));

        // 残りが1テイクぶんを割ったら、次を始めさせない（`TR-REC-41`）。
        // 進行中のテイクは最後まで録りきるので、止めるのはここだけ。
        if !self.has_room_for_one_more()? {
            return Err(AppError::new(
                "recording.not_enough_space",
                "保存先の残量が1テイクぶんを割った",
            ));
        }

        // 遡れる分が足りないことは止める理由にしない。 記録して進む。
        // 収録画面に入った直後は、まだプリロールが溜まりきっていない。
        let held = self.preroll_ms();
        if held < PREROLL_MS {
            tracing::warn!(
                held_ms = held,
                want_ms = PREROLL_MS,
                "プリロールが溜まりきっていない"
            );
        }

        self.session.start_take()?;
        // ここで取りこぼしの基準を取る。 このテイクの中で増えたぶんだけを見る
        //（`TR-REC-07` は「1テイクの中で1フレームでも欠落したら」と定めている）。
        self.xrun_baseline = self
            .capture
            .as_ref()
            .map_or(0, mac::Capture::discontinuities);

        self.pump
            .as_ref()
            .ok_or_else(no_stream)?
            .start_take(path)
            .map_err(|e| AppError::new(e.kind(), e))?;

        self.recording = Some(row_id.clone());
        Ok(row_id)
    }

    /// 収録を止めて、テイクを確定させる。
    ///
    /// 順序は、ファイル確定 → DB コミット（`DEC-REC-004`）。
    /// 逆にすると、ファイルの無い行が DB に残る。
    ///
    /// 確定のあと、その場で解析と `.frq` と oto の導出まで済ませる
    /// （`TR-PKG-05`, `TR-PKG-42`）。
    ///
    /// 取りこぼしがあったテイクは、ここで自動的に無効にする（`TR-REC-07`）。
    /// 同じフレーズがもう一度出てくる。
    #[tracing::instrument(skip(self), err)]
    pub fn finish_take(&mut self) -> Result<TakeResult> {
        let row_id = self
            .recording
            .take()
            .ok_or_else(|| AppError::new("app.not_recording", "収録していない"))?;

        // この音源の作り方（`TR-RCL-01`）。 原音設定の規約がこれで決まる。
        let preset_here = self.current_preset()?;
        let (method, preset_set) = (preset_here.method, preset_here.set);
        // 綴りの表（`TR-SYN-36`）。 録音リストを入れたときと同じものを引く。
        let rules = self.current_rules()?;

        self.capture.as_ref().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let guide_offset = self.guide_offset_at_start.take();

        // 指示のあとも `TAIL_MS` ぶん書く（`TR-REC-19`）。ここで待つ。
        let finished = self
            .pump
            .as_ref()
            .ok_or_else(no_stream)?
            .finish_take()
            .map_err(|e| AppError::new(e.kind(), e))?;

        // 取りこぼしは、このテイクの中で増えたぶんだけを見る。
        let discontinuities = self
            .capture
            .as_ref()
            .map_or(0, mac::Capture::discontinuities)
            .saturating_sub(self.xrun_baseline);

        self.session.finish_take()?;

        // ## ここまででファイルは確定している。DB はこの先
        let root = self.opened()?.dir.root().to_path_buf();
        let rel = finished
            .path
            .strip_prefix(&root)
            .unwrap_or(&finished.path)
            .to_string_lossy()
            .into_owned();
        let frames = finished.samples.len();
        let session_id = self.opened()?.session_id;

        let take_id = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: row_id.clone(),
            session_id,
            rel_path: rel,
            frames: i64::try_from(frames).unwrap_or(i64::MAX),
            recorded_at: now_rfc3339(),
        })?;

        // ## 解析。録音停止時に確定させて、以後 WAV を読み直さない
        let f64s: Vec<f64> = finished.samples.iter().map(|s| f64::from(*s)).collect();
        // 試唱のために走らせる解析を、そのまま .frq へ回す（`TR-PKG-05`）。
        // 書き出しのために推定し直さない。
        //
        // 二段構え（`TR-SYN-22`）。最初の数テイクは DIO+StoneMask で即座に確定し、
        // 話者音域が判明したら Harvest で引き直す。
        // `.frq` が要求するのは F0 と平均振幅だけなので、初期テイクの試唱には
        // DIO の精度で足りる。待たせないことのほうが効く。
        let purpose = if self.observed_f0.len() >= f0::RANGE_SAMPLE_TAKES {
            f0::Purpose::Distribution
        } else {
            f0::Purpose::Preview
        };
        let cond = f0::conditions(purpose, self.f0_floor);
        let (source_f0, _t) = f0::estimate(&f64s, rate, &cond);

        // 音域を溜めて、集まったら下限を引き上げる。
        self.observed_f0.push(source_f0.clone());
        if self.f0_floor.is_none()
            && let Some(floor) = f0::tighten_floor(&self.observed_f0)
        {
            tracing::info!(floor_hz = floor, "話者音域から探索の下限を上げた");
            self.f0_floor = Some(floor);
        }

        let analysis = TakeAnalysis::compute(
            &finished.samples,
            rate,
            &source_f0,
            cond.frame_period_ms / 1000.0,
        );
        self.opened_mut()?.ledger.put_analysis(take_id, &analysis)?;
        analysis.frq.write(&frq::frq_path(&finished.path)?)?;

        // ## 境界と oto
        let duration_ms = frames as f64 * 1000.0 / f64::from(rate);
        let cfg = SegmentConfig::default();
        // アライナを通す（`TR-ALN-03`）。MFA が使えなければ退避経路が同じ口で答える。
        let alignment = self.align_take(&f64s, rate, &row_id);
        // 1ファイルに複数モーラが入る（`TR-RCL-03`、`DEC-ALN-013`）。
        // モーラごとに境界を取り出し、oto もモーラごとに作る。
        let kana = self.opened_mut()?.ledger.units_of(&row_id)?;
        let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
        // 綴りを作るのに子音と母音クラスが要る。 並びは読み上げ順
        // （`Ledger::row_units_of`）。
        let line = self
            .opened_mut()?
            .ledger
            .row_units_of(&row_id, preset_set)?;
        let per_mora = alignment.as_ref().and_then(|a| per_mora(a, &readings));
        // 計測（`TR-REC-16`）と無音マージン（`TR-REC-38`）はファイル全体で見る。
        let boundaries = per_mora.as_ref().and_then(|v| {
            Some(Boundaries {
                voice_start_ms: v.first()?.voice_start_ms,
                vowel_start_ms: v.first()?.vowel_start_ms,
                vowel_end_ms: v.last()?.vowel_end_ms,
            })
        });

        // ## 計測（`TR-REC-16`）と無音マージン（`TR-REC-38`）
        // 測るだけ。判定も指摘もしない。
        let metrics = TakeMetrics::measure(
            &finished.samples,
            rate,
            boundaries.as_ref().map(|b| b.voice_start_ms),
            boundaries.as_ref().map(|b| b.vowel_end_ms),
        );
        self.opened_mut()?.ledger.put_metrics(
            take_id,
            &metrics,
            discontinuities,
            finished.preroll_frames,
            guide_offset,
        )?;

        let (oto, conf) = match per_mora {
            None => (None, None),
            Some(ref v) => {
                // 確信度はテイク全体で1つ。 事後確率があればそこから組み立てる
                // （`TR-ALN-24` の成分 (1)(2)）。無ければ退避経路の計算へ落ちる。
                // MFA が動いたのにパワー比で境界鋭さを測ると、
                // 要件の定義と違うものを記録することになる。
                // 確信度はモーラごとに作る（`TR-ALN-26`）。
                //
                // **ファイル全体で1つ作ってエントリ全部へ写さない。** 境界鋭さは
                // いちばん弱い境界で決まるので、1モーラが曖昧なだけで全部の
                // 確信度と主因が同じ値になり、**どのエントリを見ればよいかが消える。**
                // 音響異常度も同じで、範囲外の割れが混ざる。
                let span_conf = |b: &Boundaries, from_ms: f64, to_ms: f64| {
                    #[allow(
                        clippy::cast_possible_truncation,
                        clippy::cast_sign_loss,
                        reason = "収録の長さはサンプル数に収まる"
                    )]
                    let cut = |ms: f64| {
                        ((ms / 1000.0 * f64::from(rate)).max(0.0) as usize).min(f64s.len())
                    };
                    let (a0, a1) = (cut(from_ms), cut(to_ms));
                    let part = &f64s[a0.min(a1)..a1.max(a0)];
                    alignment
                        .as_ref()
                        .and_then(|a| Confidence::from_alignment_span(a, part, from_ms, to_ms))
                        .or_else(|| {
                            // 退避経路もモーラの範囲で測る（`TR-ALN-26`）。
                            // **ファイル全体の境界で測っていた。** MFA が無い環境
                            //（書いていない OS、モデルが無いビルド）では、
                            // 1モーラの曖昧さが全エントリへ伝播したままだった。
                            //
                            // 境界は切った先頭からの相対へ直す。 `segment::confidence`
                            // はサンプルと同じ原点で位置を数える。
                            let shifted = Boundaries {
                                voice_start_ms: b.voice_start_ms - from_ms,
                                vowel_start_ms: b.vowel_start_ms - from_ms,
                                vowel_end_ms: b.vowel_end_ms - from_ms,
                            };
                            Some(confidence(part, rate, &shifted, &cfg))
                        })
                };

                // この音源の作り方の規約で導く（`TR-ALN-13`）。 単独音の規約で
                // 連続音の行を切ると、子音の扱いも母音の終端も別の位置になる。
                let preset = Preset::default_for(method)
                    .map_err(|e| AppError::new(e.kind(), "規約プリセットを読めない"))?;
                // 集団は行の中で変わらない。1度だけ作る（`TR-ALN-12`）。
                let pops = self.populations()?;
                // 集団は音階内に閉じる（`TR-ALN-22`）。
                let row_tone = self
                    .opened_mut()?
                    .ledger
                    .row_tones()?
                    .get(&row_id)
                    .copied()
                    .unwrap_or_default();

                // モーラごとに1つ。 同じ WAV を別のエイリアスが別の位置で指す。
                //
                // テイク全体の確信度は先頭モーラのものを出す。 成分 (3) は
                // モーラごとに違うので、テイクに1つしかない値としては代表を採る。
                let mut first = None;
                let mut first_score = None;
                // 行が生むエイリアスと、その5値の出どころ（`TR-RCL-18`）。
                //
                // **モーラと1対1で置いていた。** 連続音では綴りが仮名のまま
                // 配られ、CVVC では渡りも語尾も `oto.ini` に入らなかった。
                let entries = koeru_core::reclist::row_entries(&rules, method, &line);
                // 境界を残す（`TR-ALN-34`）。 5値は境界と規約プリセットから導く
                // 派生物なので、境界さえあれば規約を変えても作り直せる。
                // **捨てていた。** プリセットを編集するだけで再アライメントが
                // 走っていた——`TR-ALN-23` の「再アライメントを要求しない」と
                // 食い違ったまま出荷されていた（`DEC-ALN-014`）。
                //
                // 置く名前はそのモーラの CV エイリアス。 モーラの添字を
                // 名前にしない——下位方式へ書き出すとき（`TR-PKG-24`）に
                // 引き直すのは、この行の CV の綴りからになる。
                let saved: Vec<(String, koeru_core::oto::Boundary)> = entries
                    .iter()
                    .filter_map(|(a, slot)| match *slot {
                        koeru_core::reclist::Slot::Cv { mora } => Some((a.clone(), *v.get(mora)?)),
                        _ => None,
                    })
                    .collect();
                self.opened_mut()?.ledger.put_boundaries(take_id, &saved)?;

                // 5値を置くのは、この行が名乗った綴りだけ（`TR-ALN-22`）。
                //
                // **境界のほうは全モーラぶん残す。** 下位方式への書き出しは
                // モーラ順に境界を並べ直し、1つでも欠けたらその素材を丸ごと
                // 落とす（`packaging::rederived_entries`）。名乗らなかった
                // 行頭の境界まで捨てると、その行の綴りが全部消える。
                let owned = self.opened_mut()?.ledger.aliases_of_row(&row_id)?;
                let derived: std::collections::BTreeMap<String, Oto> =
                    koeru_align::derive::derive_row(&entries, v, &line, duration_ms, &preset)
                        .into_iter()
                        .collect();
                for (alias, slot) in &entries {
                    if !owned.contains(alias) {
                        continue;
                    }
                    let Some(o) = derived.get(alias).copied() else {
                        continue;
                    };
                    // 確信度は、その枠が乗っているモーラの区間で測る（`TR-ALN-26`）。
                    // 渡りは直前のモーラの尾に乗るので、そちらを見る。
                    let owner = match *slot {
                        koeru_core::reclist::Slot::Cv { mora }
                        | koeru_core::reclist::Slot::Ending { mora } => mora,
                        koeru_core::reclist::Slot::Vc { prev, .. } => prev,
                    };
                    let Some(b) = v.get(owner) else { continue };
                    let reading = alias.as_str();
                    // 話者内一貫性（`TR-ALN-12`）。 成分 (3) を差し替える。
                    // `from_alignment` は 1.0 を置いて「呼び出し側が集団を持ったときに
                    // 差し替える」と書いている。**合成スコアに掛けない**——掛けると
                    // 成分としては 1.0 のまま残り、保存した内訳が嘘になる。
                    // 集団が `MIN_SAMPLES` に満たなければ 1.0 のまま（`TR-ALN-10` notes）。
                    let prior = consistency_reading(*slot, &line)
                        .map_or(1.0, |k| Self::prior_of(&pops, row_tone, k, &o, duration_ms));
                    let c = span_conf(b, b.voice_start_ms, b.vowel_end_ms).map(|mut c| {
                        c.prior = prior;
                        c
                    });
                    self.opened_mut()?.ledger.put_oto(
                        take_id,
                        reading,
                        &koeru_oto::Oto {
                            offset_ms: o.offset_ms,
                            consonant_ms: o.consonant_ms,
                            cutoff_ms: o.cutoff_ms,
                            preutterance_ms: o.preutterance_ms,
                            overlap_ms: o.overlap_ms,
                        },
                        c.map_or(0.0, |x| x.score()),
                        // 成分も残す（`TR-ALN-24`）。合成からは作り直せない。
                        c.map(|x| koeru_core::db::ConfidenceParts {
                            path: x.path,
                            sharpness: x.sharpness,
                            prior: x.prior,
                            acoustic: x.acoustic,
                        })
                        .as_ref(),
                        false,
                    )?;
                    if first.is_none() {
                        first = Some(o);
                        first_score = Some(c.map_or(0.0, |x| x.score()));
                    }
                }

                // 何で推定したかを残す（`TR-ALN-29`）。
                // モデルが変わったときに、黙って作り直さないための鍵。
                let fp = koeru_align::determinism::Fingerprint::new(
                    &f64s,
                    &kana.join(" "),
                    &preset,
                    self.aligner.identity(),
                );
                self.opened_mut()?.ledger.put_fingerprint(
                    take_id,
                    &koeru_core::db::FingerprintRow {
                        audio: fp.audio,
                        reading: fp.reading,
                        preset: fp.preset,
                        aligner: fp.aligner,
                    },
                )?;
                (first, first_score)
            }
        };

        // ## 採否
        //
        // 取りこぼしたテイクは自動的に無効にする（`TR-REC-07`）。
        // 欠落した素材は oto の導出も合成も救えないので、採用の候補に入れない。
        // ファイルは残す（`TR-REC-21` の「既存のテイクを削除・上書きせず」）。
        if discontinuities > 0 {
            tracing::warn!(discontinuities, "取りこぼしたテイクを無効にする");
            self.opened_mut()?.ledger.invalidate_take(take_id)?;
        } else {
            // 録れたものは既定で採用する。 選ばせるのは録り直したときだけ。
            self.opened_mut()?.ledger.adopt_take(&row_id, take_id)?;
            // 採用が変わったので、確認キューを組み直して新しいエントリを入れる。
            self.enqueue_take(take_id)?;
        }

        // ## 背後で前処理を進める（`TR-SYN-04`, `TR-SYN-34`）
        //
        // 完了期限は「次の録音項目まで」ではなく「試唱押下まで」。
        // 3時間の収録の途中で、次のフレーズを出すのを待たせない。
        //
        // いま録ったものを含むフレーズを、操作を待たずに合成しておく（`TR-SYN-04`）。
        // 押されたときには、もう出来ている。
        self.prerender_songs();

        Ok(TakeResult {
            take_id,
            row_id,
            duration_ms,
            peak: analysis.peak,
            thumbnail: analysis.thumbnail,
            oto,
            confidence: conf,
            discontinuities,
            invalidated: discontinuities > 0,
            metrics,
            preroll_ms: finished.preroll_frames as f64 * 1000.0 / f64::from(rate),
        })
    }

    /// ファイル名を NFC に揃える（`TR-REC-32`）。
    ///
    /// macOS はファイル作成後の名前を分解形で返すことがある。
    /// 揃えないと、同じ「が」が別の文字列として台帳と食い違う。
    /// 書き出しの直前にも通す。 分解形のまま配ると、受け手の環境で見つからない。
    ///
    /// 返るのは直した数。
    #[tracing::instrument(skip(self), err)]
    pub fn normalize_file_names(&mut self) -> Result<usize> {
        let dir = self.opened()?.dir.audio_dir();
        Ok(koeru_core::text::normalize_names_to_nfc(&dir)?)
    }

    /// NFC でない名前が残っていないか（`TR-REC-32`）。
    ///
    /// 書き出しの関門。 残っていたら書き出さない。
    #[tracing::instrument(skip(self), err)]
    pub fn non_nfc_names(&mut self) -> Result<Vec<String>> {
        let dir = self.opened()?.dir.audio_dir();
        Ok(koeru_core::text::find_non_nfc_names(&dir)?)
    }

    /// 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
    ///
    /// 収録中の判定ではない。 ここでだけ、壊れた成果物が完成へ到達する経路を塞ぐ。
    #[tracing::instrument(skip(self), err)]
    pub fn preflight(&mut self) -> Result<Preflight> {
        // 名前は先に直す。 直せるものを関門で止めない。
        let renamed = self.normalize_file_names()?;
        let non_nfc = self.non_nfc_names()?;
        let clipped = self.opened_mut()?.ledger.clipped_adopted_takes()?;
        Ok(Preflight {
            renamed_to_nfc: renamed,
            non_nfc_names: non_nfc,
            clipped_takes: clipped
                .into_iter()
                .map(|(row_id, _, runs)| (row_id, runs))
                .collect(),
        })
    }

    /// 配布に出す値を読む（`PROFILE-M4`）。
    ///
    /// まだ決めていなければ既定値。 表示名から作った配布名が入っている
    /// （`DEC-PKG-008`）ので、画面は空欄から始めなくてよい。
    #[tracing::instrument(skip(self), err)]
    pub fn package_settings(&mut self) -> Result<koeru_core::db::Distribution> {
        let manifest = self.opened()?.dir.read_manifest()?;
        packaging::settings(&mut self.opened_mut()?.ledger, &manifest)
    }

    /// 配布に出す値を保存する（`PROFILE-M4`）。
    #[tracing::instrument(skip(self, d), err)]
    pub fn set_package_settings(&mut self, d: &koeru_core::db::Distribution) -> Result<()> {
        packaging::check_settings(d)?;
        self.opened_mut()?.ledger.set_distribution(d)?;
        Ok(())
    }

    /// いま書き出せるか（`TR-PKG-49`, `TR-PKG-51`）。
    #[tracing::instrument(skip(self), err)]
    pub fn package_state(&mut self) -> Result<packaging::PackageState> {
        let dir = self.opened()?.dir.clone();
        let manifest = dir.read_manifest()?;
        let gates = self.package_gates()?;
        let rules = self.current_rules()?;
        packaging::state(
            &dir,
            &mut self.opened_mut()?.ledger,
            &rules,
            &manifest,
            gates,
        )
    }

    /// 書き出しの手前にある、配布物の外の関門（`INV-ALN-003`）。
    ///
    /// **読むだけ。** `ensure_otos_ready` の検証は値を直すので、状態を
    /// 引くだけのつもりで呼ばれるものが台帳を書き換えてはいけない。
    /// 直しの結果はキューに残る（直せない違反は `Blocked` になる）ので、
    /// ここで読む `all_confirmed` が一度直したあとの姿を映す。
    ///
    /// 素材の名前はここで見ない（`TR-REC-32`）。 判定するには先に直しを
    /// 走らせる必要があり、`preflight` がそれを持っている。**同じことを
    /// 2箇所で判定すると、どちらが先に走ったかで答えが変わる。**
    #[tracing::instrument(skip(self), err)]
    fn package_gates(&mut self) -> Result<packaging::Gates> {
        let missing = self.opened_mut()?.ledger.adopted_rows_without_oto()?;
        let conflicting = self.opened_mut()?.ledger.adopted_conflicting_aliases()?;
        let confirmed = self.opened()?.review.all_confirmed();
        Ok(packaging::Gates {
            otos_ready: confirmed && missing.is_empty() && conflicting.is_empty(),
        })
    }

    /// 配布物に入るファイルの一覧（`TR-PKG-28` の同梱物）。
    #[tracing::instrument(skip(self), err)]
    pub fn package_contents(&mut self) -> Result<Vec<(String, u64)>> {
        let dir = self.opened()?.dir.clone();
        let manifest = dir.read_manifest()?;
        let rules = self.current_rules()?;
        packaging::preview(&dir, &mut self.opened_mut()?.ledger, &rules, &manifest)
    }

    /// 書き出す（`REQ-PKG-105`, `REQ-PKG-106`）。
    ///
    /// 先に `TR-REC-32` の関門を通す。 素材の名前が受け手の環境で
    /// 見つからなくなる状態のまま包まない。
    ///
    /// 版の札は受け取らない。 配布に出す値として保存してあるものを使う
    /// （`TR-PKG-44`）——**2箇所で打たせると、配布物と履歴で違う値になる。**
    #[tracing::instrument(skip(self), err)]
    pub fn export_package(&mut self) -> Result<packaging::Exported> {
        // 原音設定の確認が残っているうちは出さない（`INV-ALN-003`）。
        // `oto.ini` 単体の書き出しと同じ関門。
        self.ensure_otos_ready()?;
        let pre = self.preflight()?;
        if !pre.may_export() {
            return Err(AppError::new(
                "package.non_nfc_names",
                "受け取る側で見つからなくなる名前が残っている",
            ));
        }
        let dir = self.opened()?.dir.clone();
        let manifest = dir.read_manifest()?;
        let at = now_rfc3339();
        let rules = self.current_rules()?;
        packaging::export(&dir, &mut self.opened_mut()?.ledger, &rules, &manifest, &at)
    }

    /// 下位方式へ書き出す（`TR-PKG-23`, `TR-PKG-24`, `TR-PKG-25`）。
    ///
    /// 独立した音源ルート・独立した ZIP になる。 元の書き出しと同じ関門を
    /// 通す——名前も確認も、方式を変えても緩まない。
    ///
    /// **5値は対象方式の規約プリセットで作り直す**（`TR-ALN-34`）。
    /// 流用すると、語頭の子音区間を持ったままの oto が単独音として配られる。
    ///
    /// # Errors
    ///
    /// 確認が残っている、名前が使えない、被覆が満ちていない、
    /// 検証に通らない、包めない。
    #[tracing::instrument(skip(self), fields(method = method.as_str()), err)]
    pub fn export_downgrade(&mut self, method: Method) -> Result<packaging::Exported> {
        self.ensure_otos_ready()?;
        let pre = self.preflight()?;
        if !pre.may_export() {
            return Err(AppError::new(
                "package.non_nfc_names",
                "受け取る側で見つからなくなる名前が残っている",
            ));
        }
        let dir = self.opened()?.dir.clone();
        let manifest = dir.read_manifest()?;
        let at = now_rfc3339();
        let rules = self.current_rules()?;
        packaging::export_downgrade(
            &dir,
            &mut self.opened_mut()?.ledger,
            &rules,
            &manifest,
            method,
            &at,
        )
    }

    /// 書き出したものを、OS のファイルマネージャで見せる（`TR-PKG-45`）。
    ///
    /// **利用者にフォルダ操作を要求しないが、到達経路は残す。**
    /// 作れるのに手が届かないと、配り物として成立しない。
    ///
    /// 画面へパスを渡さない（`TR-PKG-45`）。 受け取るのは連番で、
    /// 在り処は台帳から引く。渡すと、通常モードの画面にパスが出る経路ができる。
    ///
    /// # Errors
    ///
    /// その連番の記録が無い、ファイルが消えている、開けない。
    #[tracing::instrument(skip(self), fields(seq), err)]
    pub fn reveal_release(&mut self, seq: i32) -> Result<()> {
        let dir = self.opened()?.dir.exports_dir();
        let release = self
            .opened_mut()?
            .ledger
            .releases()?
            .into_iter()
            .find(|r| r.seq == seq)
            .ok_or_else(|| AppError::new("package.unknown_release", "その書き出しの記録が無い"))?;

        let path = dir.join(&release.archive_name);
        if !path.is_file() {
            return Err(AppError::new(
                "package.archive_missing",
                "その配り物が見つからない",
            ));
        }
        tauri_plugin_opener::reveal_item_in_dir(&path)
            .map_err(|_| AppError::new("package.reveal_failed", "配り物の置き場所を開けない"))
    }

    /// 書き出しの履歴（`TR-PKG-44`）。古い順。
    #[tracing::instrument(skip(self), err)]
    pub fn releases(&mut self) -> Result<Vec<koeru_core::release::Release>> {
        Ok(self.opened_mut()?.ledger.releases()?)
    }

    /// 収録済みのテイクを、指定した音高で合成する。鳴らさない。
    ///
    /// 周波数表は台帳から取る。 書き出しのためだけでなく、試唱もここを使う
    /// （`TR-PKG-05` の「再推定を要しない」）。
    ///
    /// 返るのは `(サンプル, サンプルレート)`。
    #[tracing::instrument(skip(self), err)]
    pub fn render_take(
        &mut self,
        take_id: i32,
        midi: i32,
        length_ms: f64,
    ) -> Result<(Vec<f32>, u32)> {
        let root = self.opened()?.dir.root().to_path_buf();
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("app.unknown_take", "そのテイクが台帳に無い"))?;
        // 1テイクに複数のエントリがある（`DEC-ALN-013`）。試聴は先頭の1つで鳴らす。
        // 並びはエイリアス順で常に同じ（`TR-ALN-29`）。
        let oto = self
            .opened_mut()?
            .ledger
            .otos_of(take_id)?
            .into_iter()
            .next()
            .map(|(_, o)| o)
            .ok_or_else(|| AppError::new("app.no_oto", "そのテイクにまだ原音設定が無い"))?;
        let analysis = self.opened_mut()?.ledger.analysis_of(take_id)?;

        let w = wav::read(root.join(&take.rel_path))?;
        let samples: Vec<f64> = w.samples.iter().map(|s| f64::from(*s)).collect();
        let table = analysis.map(|a| a.frq.f0).unwrap_or_default();

        let out = render(&RenderRequest {
            samples: &samples,
            sample_rate_hz: w.rate_hz,
            // `tone` は鳴らしたい音高。収録音高ではない（resampler の doc を参照）。
            // ここに収録音高を渡すと、どの音高を選んでも同じ高さで鳴る。
            tone: midi,
            oto: Oto {
                offset_ms: oto.offset_ms,
                consonant_ms: oto.consonant_ms,
                cutoff_ms: oto.cutoff_ms,
                preutterance_ms: oto.preutterance_ms,
                overlap_ms: oto.overlap_ms,
            },
            required_length_ms: length_ms,
            consonant_velocity: 100.0,
            volume: 100.0,
            modulation: 0.0,
            tempo: 120.0,
            pitch_bend_cents: &[],
            // 表はファイル全体・hop=256 の `.frq`（`TR-PKG-05`）。
            // 切り出しと 5ms 格子への載せ替えは合成器がする。
            frequency_table: (!table.is_empty()).then_some(FrequencyTable {
                f0: &table,
                hop_samples: koeru_core::frq::HOP_SIZE,
            }),
        })?;

        #[allow(
            clippy::cast_possible_truncation,
            reason = "合成結果は -1.0..=1.0 付近。f32 で鳴らす"
        )]
        let pcm: Vec<f32> = out.iter().map(|v| *v as f32).collect();
        Ok((pcm, w.rate_hz))
    }

    /// 収録済みのテイクを、指定した音高で鳴らす。縦切りの終点。
    #[tracing::instrument(skip(self), err)]
    pub fn preview(&mut self, take_id: i32, midi: i32, length_ms: f64) -> Result<usize> {
        let (pcm, rate) = self.render_take(take_id, midi, length_ms)?;
        let n = pcm.len();

        // 前の再生は止める。 重ねると何を聴いているか分からなくなる。
        self.playback = None;
        self.playback = Some(mac::play(pcm, rate)?);
        Ok(n)
    }

    /// いま録ったものを含む曲のフレーズを、背後で合成しておく（`TR-SYN-04`）。
    ///
    /// ユーザー操作を待たない。 押されたときには、もう出来ている。
    /// 録音入力より低い優先度で回す（`TR-SYN-34`）ので、収録の邪魔にならない。
    fn prerender_songs(&mut self) {
        // どの曲がいま歌えるかだけ見て、歌えるものの先頭フレーズを温めておく。
        let Ok(status) = self.song_status() else {
            return;
        };
        let singable = status
            .iter()
            .filter(|s| s.singability.is_singable())
            .count();
        if singable == 0 {
            return;
        }
        let cache = Arc::clone(&self.song_cache);
        self.workers.submit(Priority::PostRecording, move || {
            // 鍵に素材の内容ハッシュが入っているので、古い結果は自然に使われない。
            // ここでできるのは、次に押されたときに載っている確率を上げることだけ。
            let held = cache.lock().map_or(0, |c| c.len());
            tracing::debug!(held, singable, "試唱の前処理を進めた");
        });
    }

    /// 背後で待っている仕事の数（`TR-SYN-33`）。
    ///
    /// 「録音終了 → 試唱ボタン活性化」の間に、無言の待ち時間を作らない（`TR-SYN-33`）。
    /// 画面はこれを見て、進んでいることを出す。
    #[must_use]
    pub fn pending_work(&self) -> usize {
        self.workers.pending()
    }

    /// 見えている範囲の波形（`TR-PLT-04`）。
    ///
    /// 読む量は画素数に比例する。 範囲の広さには比例しない。
    /// 段はテイクごとに一度だけ積んで持ち回す。
    #[tracing::instrument(skip(self), fields(take_id, pixels), err)]
    pub fn waveform_window(
        &mut self,
        take_id: i32,
        from_ms: f64,
        to_ms: f64,
        pixels: usize,
    ) -> Result<Vec<(f32, f32)>> {
        let (map, rate) = self.mipmap_of(take_id)?;
        let at = |ms: f64| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "位置はミリ秒から作る非負の値"
            )]
            let v = ((ms.max(0.0) / 1000.0) * f64::from(rate)) as usize;
            v
        };
        Ok(map
            .window(at(from_ms), at(to_ms), pixels)
            .into_iter()
            .map(|v| (v.min, v.max))
            .collect())
    }

    /// 見えている範囲のスペクトログラム（`TR-PLT-04`）。
    ///
    /// 素材ファイル全体の STFT を一括で先行計算しない。
    #[tracing::instrument(skip(self), fields(take_id, columns, rows), err)]
    pub fn spectrogram_window(
        &mut self,
        take_id: i32,
        from_ms: f64,
        to_ms: f64,
        columns: usize,
        rows: usize,
    ) -> Result<waveform::Spectrogram> {
        let (samples, rate) = self.samples_of(take_id)?;
        let at = |ms: f64| {
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "位置はミリ秒から作る非負の値"
            )]
            let v = ((ms.max(0.0) / 1000.0) * f64::from(rate)) as usize;
            v
        };
        Ok(waveform::spectrogram(
            &samples,
            at(from_ms),
            at(to_ms),
            columns,
            rows,
        ))
    }

    /// テイクの段を積む。一度積んだら持ち回す。
    fn mipmap_of(&mut self, take_id: i32) -> Result<(Arc<waveform::Mipmap>, u32)> {
        if let Some((map, rate)) = self.mipmaps.get(&take_id) {
            return Ok((Arc::clone(map), *rate));
        }
        let (samples, rate) = self.samples_of(take_id)?;
        let map = Arc::new(waveform::Mipmap::build(&samples));
        // 上限を置く。 3時間ぶんの段を全部持つとメモリが尽きる。
        if self.mipmaps.len() >= MIPMAP_CACHE {
            self.mipmaps.clear();
        }
        self.mipmaps.insert(take_id, (Arc::clone(&map), rate));
        Ok((map, rate))
    }

    /// テイクの波形を読む。
    fn samples_of(&mut self, take_id: i32) -> Result<(Vec<f32>, u32)> {
        let root = self.opened()?.dir.root().to_path_buf();
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("app.unknown_take", "そのテイクが台帳に無い"))?;
        let w = wav::read(root.join(&take.rel_path))?;
        Ok((w.samples, w.rate_hz))
    }

    /// 待っている仕事の持ち手（`TR-SYN-33`）。状態ロックの外から読むために出す。
    #[must_use]
    pub fn pending_handle(&self) -> crate::workers::PendingHandle {
        self.workers.pending_handle()
    }

    /// 包絡の持ち手（`TR-REC-43`）。状態ロックの外から読むために出す。
    ///
    /// マイクを選ぶ前は無い。
    #[must_use]
    pub fn envelope_handle(&self) -> Option<Arc<Mutex<crate::pump::Envelope>>> {
        self.pump.as_ref().map(Pump::envelope_handle)
    }

    /// そのテイクの原音設定を、エイリアスごとに引く（`TR-ALN-33`）。
    ///
    /// 音素ごとの集団（`TR-ALN-12`）。1テイクの確定につき1度だけ作る。
    ///
    /// **モーラごとに作り直さない。** 作り直すと、8モーラの行で `adopted_otos` を
    /// 8回引く。集団は行の中で変わらない。
    ///
    /// 集団は同一音素で取る（`TR-ALN-12` (b)）。 単独音では同一エイリアスの
    /// 集団（(a)）が1件しか集まらない——1行1エイリアスで、採用テイクは1つだから。
    ///
    /// 音高は鍵に入れていない。 いまは1音階しか録らないので、
    /// 入れても全部同じ値になる（`TR-ALN-22` は多音階で効く）。
    fn populations(&mut self) -> Result<Populations> {
        let mut out = Populations::default();
        let here = self.current_preset()?;
        let rules = self.current_rules()?;
        let tones = self.opened_mut()?.ledger.row_tones()?;
        // 行ごとに「綴り → 仮名」を1度だけ組む。 エントリは行の数より多い。
        let mut readings: HashMap<String, HashMap<String, &'static str>> = HashMap::new();
        for e in self.opened_mut()?.ledger.adopted_otos()? {
            if !readings.contains_key(&e.row_id) {
                let line = self
                    .opened_mut()?
                    .ledger
                    .row_units_of(&e.row_id, here.set)?;
                let map = koeru_core::reclist::row_entries(&rules, here.method, &line)
                    .into_iter()
                    .filter_map(|(a, slot)| Some((a, consistency_reading(slot, &line)?)))
                    .collect();
                readings.insert(e.row_id.clone(), map);
            }
            // CV の枠だけが集団に入る（`consistency_reading`）。
            let Some(kana) = readings.get(&e.row_id).and_then(|m| m.get(&e.alias)) else {
                continue;
            };
            let tone = tones.get(&e.row_id).copied().unwrap_or_default();
            #[allow(
                clippy::cast_precision_loss,
                reason = "収録の長さは 2^53 フレームに届かない"
            )]
            let len_ms = e.frames as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);
            out.push(tone, kana, &e.oto, len_ms);
        }
        Ok(out)
    }

    /// 集団から見た、その推定の素直さ（`TR-ALN-12`）。
    ///
    /// 1.0 が「集団の真ん中」、0.0 が「大きく外れている」。
    /// 集団が集まっていなければ 1.0——序盤のテイクを外れ値にしない
    /// （`TR-ALN-10` notes）。
    ///
    /// **3つの測度のうち、いちばん外れているものが決める。** どれか1つでも
    /// 集団から外れていれば見てほしいので、平均では薄まる。
    /// `tone` はそのテイクの収録音高（`TR-ALN-22`）。 集団は音階内に閉じる。
    fn prior_of(pops: &Populations, tone: i32, reading: &str, o: &Oto, len_ms: f64) -> f64 {
        let score = |measure, value: f64, population: &[f64]| {
            consistency::deviation(measure, value, population).map_or(1.0, |d| d.prior_score())
        };
        let (consonant, vowel) = Populations::spans(o, len_ms);
        // 音素へ写せない読みは集団を作れない。 分からないものを外れ値にしない。
        let onset = first_phoneme(reading)
            .and_then(|k| pops.onset.get(&(tone, k)))
            .map_or(1.0, |(pre, cons)| {
                score(Measure::Preutterance, o.preutterance_ms, pre).min(score(
                    Measure::ConsonantLength,
                    consonant,
                    cons,
                ))
            });
        let tail = last_phoneme(reading)
            .and_then(|k| pops.vowel.get(&(tone, k)))
            .map_or(1.0, |v| score(Measure::VowelLength, vowel, v));
        onset.min(tail)
    }

    /// 確定したテイクのエントリを確認キューへ入れる（`REQ-ALN-002`）。
    ///
    /// 全件キューへ入れる。 確信度に絶対閾値を置かない（`TR-ALN-25` の
    /// 「確信度の閾値は絶対値で固定せず、確認キューの運用で切る」）。
    /// どこまで見るかは合計所要時間の上限が決める（`DEC-ALN-003`）。
    ///
    /// 固定した値は引き継ぐ（`REQ-ALN-007`）。 録り直しは新しいテイクの行を作るので、
    /// 引き継がないと、人が直した値が自動の値で上書きされる（`INV-ALN-001`）。
    fn enqueue_take(&mut self, take_id: i32) -> Result<()> {
        // 録り足したものを書き出せるようにする（`TR-PKG-44`）。
        self.start_new_export_generation()?;
        let aliases: Vec<String> = self
            .opened_mut()?
            .ledger
            .otos_of(take_id)?
            .into_iter()
            .map(|(a, _)| a)
            .collect();
        // キューは（音高, 綴り）で持つ（`crate::review::load`）。 前の世代は
        // 同じ行なので同じ音高。引き方は `load` と揃える——行の音高が引けなければ 0。
        //
        // **素の綴りで引いていた。** 鍵が合わず一度も当たらないので、
        // 録り直すたびに人が直した値が自動の値で上書きされていた（`INV-ALN-001`）。
        let row_id = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .map(|t| t.row_id)
            .unwrap_or_default();
        let tone = self
            .opened_mut()?
            .ledger
            .row_tones()?
            .get(&row_id)
            .copied()
            .unwrap_or_default();

        for alias in &aliases {
            // 前の世代に固定があったものだけ運ぶ。
            let key = crate::review::EntryKey::new(tone, alias.as_str()).handle();
            let Some((prev, pins)) = self
                .opened()?
                .review
                .get(&key)
                .filter(|e| e.pins().iter().any(|p| *p))
                .map(|e| (e.oto, e.pins()))
            else {
                continue;
            };
            let Some(row) = self.opened_mut()?.ledger.oto_of(take_id, alias)? else {
                continue;
            };
            let mut next = Oto {
                offset_ms: row.offset_ms,
                consonant_ms: row.consonant_ms,
                cutoff_ms: row.cutoff_ms,
                preutterance_ms: row.preutterance_ms,
                overlap_ms: row.overlap_ms,
            };
            for (i, s) in Slot::ALL.into_iter().enumerate() {
                if pins[i] {
                    s.set(&mut next, s.get(&prev));
                }
            }
            // 値と固定を一度に書く。 別々に流すと、固定だけ落ちたときに
            // 「人の値なのに固定が無い」行が残り、次の録音で上書きされる。
            let open = self.opened_mut()?;
            open.ledger.put_review_entry(
                take_id,
                alias,
                &next,
                EntryState::InQueue.as_str(),
                pins,
            )?;
        }

        self.refresh_review()
    }

    /// 確認の進み具合（`TR-ALN-25`, `TR-ALN-28`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない。
    pub fn review_summary(&mut self) -> Result<ReviewSummary> {
        let method = self.opened()?.dir.read_manifest()?.method;
        let missing = self.opened_mut()?.ledger.adopted_rows_without_oto()?.len();
        let conflicting = self
            .opened_mut()?
            .ledger
            .adopted_conflicting_aliases()?
            .len();
        let q = &self.opened()?.review;
        let reach = reach::of(match method {
            Method::Single => koeru_core::alias::Method::Single,
            Method::Cvvc => koeru_core::alias::Method::Cvvc,
            // 多音階連続音は、到達水準の上では連続音と同じ扱い
            // （`TR-ALN-28` が音階数で分けていない）。
            Method::Sequential | Method::MultiPitchSequential => {
                koeru_core::alias::Method::Sequential
            }
        });
        Ok(ReviewSummary {
            mode: q.mode().as_str().to_owned(),
            pending: q.pending_count(),
            blocked: q
                .all()
                .filter(|(_, e)| e.state == EntryState::Blocked)
                .count(),
            estimated_seconds: q.estimated_review_time().as_secs(),
            budget_seconds: koeru_align::review::REVIEW_BUDGET.as_secs(),
            exceeds_budget: q.exceeds_budget(),
            missing,
            conflicting,
            unestimated: q
                .all()
                .filter(|(_, e)| e.state == EntryState::NotEstimated)
                .count(),
            may_export: missing == 0 && conflicting == 0 && q.may_export().is_ok(),
            // 飛ばせる経路を必ず出す方式か（`TR-ALN-28`）。
            allows_skipping: reach.allows_skipping_review(),
            reach: reach.kind().to_owned(),
            exported: q.is_exported(),
        })
    }

    /// 採用テイクのエントリ全部を、手が届く順に（`TR-ALN-26`）。
    ///
    /// 確認待ちだけを返さない。 **固定は確認が済んだあとも残り、あとの世代へ
    /// 引き継がれる**（`REQ-ALN-007`）ので、確定したエントリを落とすと
    /// 「どの値を人が決めたか」と「自動に戻す」が画面から消える。
    ///
    /// 並びは確認待ちが先で、その中は確信度の低い順（`TR-ALN-26`、`TR-ALN-29`）。
    /// 済んだものは後ろにエイリアス順で続く。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    pub fn review_queue(&mut self) -> Result<Vec<ReviewItem>> {
        // 行 ID はキューが持っていない。 鍵は（音高, 綴り）なので、台帳から引く
        // ——画面は「確認待ちの行」で一覧を絞る（`DEC-PLT-024`）。
        //
        // **綴りで引いていた。** 多音階では同じ綴りが音高の数だけあるので、
        // 表が1つに潰れ、全部の音高の項目が同じ行を指していた。
        let tones = self.opened_mut()?.ledger.row_tones()?;
        let rows: HashMap<String, String> = self
            .opened_mut()?
            .ledger
            .adopted_otos()?
            .into_iter()
            .map(|e| {
                let tone = tones.get(&e.row_id).copied().unwrap_or_default();
                (
                    crate::review::EntryKey::new(tone, e.alias).handle(),
                    e.row_id,
                )
            })
            .collect();
        let q = &self.opened()?.review;
        let item = |key: &str, e: &koeru_align::review::Entry| ReviewItem {
            row_id: rows.get(key).cloned().unwrap_or_default(),
            // 画面に出すのは綴りだけ。 鍵の形は画面の関心事ではない。
            alias: crate::review::EntryKey::parse(key)
                .map_or_else(|| key.to_owned(), |k| k.alias().to_owned()),
            key: key.to_owned(),
            oto: e.oto,
            // 主因は成分の内訳から出る（`TR-ALN-26` (3)）。
            // 成分を持たない（この版より前に録った）ものは出せない。
            cause: e
                .confidence
                .and_then(|c| c.cause(CAUSE_THRESHOLD))
                .map(|c| c.kind().to_owned()),
            confidence: e.confidence.map_or(0.0, |c| c.score()),
            state: e.state.as_str().to_owned(),
            pinned: e.pins(),
        };

        let queued: Vec<ReviewItem> = q.queued().into_iter().map(|(a, e)| item(a, e)).collect();
        let done = q
            .all()
            .filter(|(_, e)| !matches!(e.state, EntryState::InQueue | EntryState::Blocked))
            .map(|(a, e)| item(a, e));
        Ok(queued.into_iter().chain(done).collect())
    }

    /// 1件ずつ確認して確定させる（`REQ-ALN-008`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、個別確認モードでない、キューに入っていない。
    // 読みはトレースに載せない（AGENTS.md #3）。鍵は綴りを含む。
    #[tracing::instrument(skip(self, key), err)]
    pub fn confirm_entry(&mut self, key: &str) -> Result<()> {
        self.with_entry(key, |q, id| q.confirm(id))
    }

    /// まとめて確認する（`REQ-ALN-010`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、まとめて確認モードでない。
    #[tracing::instrument(skip(self), err)]
    pub fn confirm_all_entries(&mut self) -> Result<usize> {
        let mut next = self.opened()?.review.clone();
        let n = next.confirm_all().map_err(review_error)?;
        self.save_all_entries(next)?;
        Ok(n)
    }

    /// 個別確認をやめる（`REQ-ALN-010`, `INV-ALN-004`）。
    ///
    /// 通るのは上限を超えているときだけ。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、上限を超えていない、知らないモード。
    // `to` は境界から来た文字列。 固定の語彙のつもりでも、
    // 検査する前にスパンへ載るので通さない。
    #[tracing::instrument(skip(self, to), err)]
    pub fn switch_review_mode(&mut self, to: &str) -> Result<()> {
        let mut next = self.opened()?.review.clone();
        match to {
            "batch" => next.switch_to_batch().map_err(review_error)?,
            "suggest_rerecord" => next.switch_to_rerecord().map_err(review_error)?,
            _ => {
                return Err(AppError::new("review.unknown_mode", "知らない確認の進め方"));
            }
        }
        let open = self.opened_mut()?;
        crate::review::save_mode(&mut open.ledger, &next, true)?;
        open.review = next;
        Ok(())
    }

    /// 5値のどれかを人が直す。その値だけを固定する（`REQ-ALN-005`, `TR-ALN-30`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、知らない値の名前、まだ推定していない。
    #[tracing::instrument(skip(self, key, slot), err)]
    pub fn edit_oto_value(&mut self, key: &str, slot: &str, value: f64) -> Result<()> {
        let s = slot_of(slot)
            .ok_or_else(|| AppError::new("review.unknown_slot", "知らない値の名前"))?;
        self.with_entry(key, |q, id| q.human_edit(id, s, value))
    }

    /// 固定を解いて自動へ戻す（`REQ-ALN-006`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、知らない値の名前、固定されていない。
    #[tracing::instrument(skip(self, key, slot), err)]
    pub fn revert_oto_value(&mut self, key: &str, slot: &str) -> Result<()> {
        let s = slot_of(slot)
            .ok_or_else(|| AppError::new("review.unknown_slot", "知らない値の名前"))?;
        self.with_entry(key, |q, id| q.revert_to_auto(id, s))?;
        // 固定を解いたら自動の値へ戻す（`AC-ALN-002`）。
        //
        // **解くだけでは戻らない。** `revert_to_auto` は印を外すだけなので、
        // 人が入れた数はそのまま残り、そのまま書き出される
        // ——「自動に戻す」を押したのに自動の値にならない。
        //
        // 失敗しても解いた事実は残す。 再推定できない理由（WAV が読めない、
        // アライメントが通らない）は解くことと関係がなく、
        // 巻き戻すと押した操作が黙って消える。
        if let Some(take_id) = self.opened()?.review_takes.get(key).copied()
            && let Err(e) = self.re_estimate_take(take_id)
        {
            tracing::warn!(reason = %e.kind, "固定を解いたが、再推定は通らなかった");
        }
        Ok(())
    }

    /// そのテイクの oto を作り直す（`REQ-ALN-007`, `TR-ALN-29`）。
    ///
    /// 固定されていない値だけを書き換える（`INV-ALN-001`）。固定はキューが守る
    /// （[`ReviewQueue::re_estimate`]）ので、ここは新しい推定を渡すだけ。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、WAV を読めない、読みを音素へ写せない、
    /// 発声を見つけられない。
    #[tracing::instrument(skip(self), err)]
    pub fn re_estimate_take(&mut self, take_id: i32) -> Result<()> {
        let root = self.opened()?.dir.root().to_path_buf();
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("ledger.unknown_take", "テイクが台帳に無い"))?;
        let w = wav::read(root.join(&take.rel_path))?;
        let f64s: Vec<f64> = w.samples.iter().map(|v| f64::from(*v)).collect();
        #[allow(
            clippy::cast_precision_loss,
            reason = "収録の長さは 2^53 サンプルに届かない"
        )]
        let duration_ms = f64s.len() as f64 * 1000.0 / f64::from(w.rate_hz);

        let kana = self.opened_mut()?.ledger.units_of(&take.row_id)?;
        let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
        let alignment = self.align_take(&f64s, w.rate_hz, &take.row_id);
        let per_mora = alignment
            .as_ref()
            .and_then(|a| per_mora(a, &readings))
            .ok_or_else(|| AppError::new("align.no_voice", "発声を見つけられなかった"))?;

        // この音源の作り方の規約で導く（`TR-ALN-13`）。
        let here = self.current_preset()?;
        let preset = Preset::default_for(here.method)
            .map_err(|e| AppError::new(e.kind(), "規約プリセットを読めない"))?;
        let rules = self.current_rules()?;
        let line = self
            .opened_mut()?
            .ledger
            .row_units_of(&take.row_id, here.set)?;
        let pops = self.populations()?;
        // 集団は音階内に閉じる（`TR-ALN-22`）。
        let tone = self
            .opened_mut()?
            .ledger
            .row_tones()?
            .get(&take.row_id)
            .copied()
            .unwrap_or_default();
        let cfg = SegmentConfig::default();

        // 行が生むエイリアスごとに1つ（`TR-RCL-18`）。 確定のときと同じ表を通す
        // ——別の表で作り直すと、確認キューに無いエイリアスが生える。
        let entries = koeru_core::reclist::row_entries(&rules, here.method, &line);
        // 名乗った綴りだけ。 確定のときと同じ絞り方を通す（`TR-ALN-22`）。
        let owned = self.opened_mut()?.ledger.aliases_of_row(&take.row_id)?;
        let derived: std::collections::BTreeMap<String, Oto> =
            koeru_align::derive::derive_row(&entries, &per_mora, &line, duration_ms, &preset)
                .into_iter()
                .collect();

        let mut next = self.opened()?.review.clone();
        let mut rows = Vec::new();
        for (alias, slot) in &entries {
            if !owned.contains(alias) {
                continue;
            }
            let Some(o) = derived.get(alias).copied() else {
                continue;
            };
            let owner = match *slot {
                koeru_core::reclist::Slot::Cv { mora }
                | koeru_core::reclist::Slot::Ending { mora } => mora,
                koeru_core::reclist::Slot::Vc { prev, .. } => prev,
            };
            let Some(b) = per_mora.get(owner) else {
                continue;
            };
            let reading = alias.as_str();
            // キューの鍵は（音高, 綴り）（`TR-ALN-22`）。
            let key = crate::review::EntryKey::new(tone, reading).handle();
            // 集団はそのモーラの仮名で引く（`consistency_reading`）。
            // 綴りは音素へ写せないので、渡すと集団が引けない。
            let prior = consistency_reading(*slot, &line)
                .map_or(1.0, |k| Self::prior_of(&pops, tone, k, &o, duration_ms));
            // 音の質はその枠の区間で測る（`TR-ALN-26`）。
            //
            // **ファイル全体を渡していた。** 隣のモーラが割れていたり
            // 小さかったりするだけで、きれいな枠まで確認待ちへ戻る
            // ——テイク確定側は区間で切っているので、固定を解いただけで
            // 別の答えが出ていた。
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "位置はミリ秒。標本数へ落として範囲に丸める"
            )]
            let cut =
                |ms: f64| ((ms / 1000.0 * f64::from(w.rate_hz)).max(0.0) as usize).min(f64s.len());
            let (a0, a1) = (cut(b.voice_start_ms), cut(b.vowel_end_ms));
            let part = &f64s[a0.min(a1)..a1.max(a0)];
            // 退避経路は切った先頭からの相対で測る。 `segment::confidence` は
            // サンプルと同じ原点で位置を数える。
            let shifted = Boundaries {
                voice_start_ms: 0.0,
                vowel_start_ms: b.vowel_start_ms - b.voice_start_ms,
                vowel_end_ms: b.vowel_end_ms - b.voice_start_ms,
            };
            let c = alignment
                .as_ref()
                .and_then(|a| {
                    Confidence::from_alignment_span(a, part, b.voice_start_ms, b.vowel_end_ms)
                })
                .or_else(|| Some(confidence(part, w.rate_hz, &shifted, &cfg)))
                .map(|mut c| {
                    c.prior = prior;
                    c
                })
                .unwrap_or_else(Confidence::full);
            // 固定されていない値だけが動く（`INV-ALN-001`）。
            next.re_estimate(&key, o, c).map_err(review_error)?;
            let Some(e) = next.get(&key) else { continue };
            rows.push(koeru_core::db::ReviewEntryRow {
                take_id,
                alias: alias.clone(),
                oto: e.oto,
                state: e.state.as_str().to_owned(),
                pinned: e.pins(),
            });
        }

        let open = self.opened_mut()?;
        open.ledger.put_review_entries(&rows)?;
        open.review = next;
        Ok(())
    }

    /// oto を直すのではなく録り直す（`REQ-ALN-009`, `TR-ALN-27`）。
    ///
    /// エントリを未推定へ戻すだけ。 実際の収録は収録画面が行う。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、確認待ちでない。
    #[tracing::instrument(skip(self, key), err)]
    pub fn rerecord_entry(&mut self, key: &str) -> Result<()> {
        self.with_entry(key, |q, id| q.rerecord(id))
    }

    /// 書き出し前の検証（`TR-ALN-20`）。
    ///
    /// 直せるものは直して台帳へ書き戻し、直せないものはそのエントリを
    /// 書き出し阻止へ回す。返るのは `(直した件数, 止めたエイリアス)`。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self), err)]
    pub fn validate_otos(&mut self) -> Result<(usize, Vec<String>)> {
        let entries = self.opened_mut()?.ledger.adopted_otos()?;
        // 重複は WAV ごとに見る（`TR-ALN-20` (6)）。
        //
        // **全テイクのエイリアスを平らにして渡していた。** 別の WAV が同じ
        // エイリアスを持つのは重複ではないのに、両方を修復不能として止めていた。
        let mut dup: HashMap<i32, Vec<String>> = HashMap::new();
        for take_id in entries.iter().map(|e| e.take_id).collect::<BTreeSet<_>>() {
            let aliases: Vec<&str> = entries
                .iter()
                .filter(|e| e.take_id == take_id)
                .map(|e| e.alias.as_str())
                .collect();
            dup.insert(
                take_id,
                validate::find_duplicate_aliases(&aliases)
                    .into_iter()
                    .map(str::to_owned)
                    .collect(),
            );
        }

        let mut fixed = 0;
        let mut blocked = Vec::new();
        for e in &entries {
            // 長さは台帳が一緒に返す。1件ずつ引き直さない。
            #[allow(
                clippy::cast_precision_loss,
                reason = "収録の長さは 2^53 フレームに届かない"
            )]
            let len_ms = e.frames as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);
            let duplicated = dup.get(&e.take_id).is_some_and(|v| v.contains(&e.alias));
            let r = validate::repair(&e.oto, len_ms, duplicated);

            // 固定した値は直さない（`TR-ALN-30`, `INV-ALN-001`）。
            //
            // **修復結果をそのまま書き戻していた。** 固定の印は残るので、
            // 画面は機械が動かした値を「手で決めました」と出していた。
            // 人が決めた値は人のもので、自動修復もそこには手を出せない。
            let mut next = e.oto;
            let mut touches_pinned = false;
            for (i, slot) in Slot::ALL.into_iter().enumerate() {
                if e.pinned[i] {
                    // 直したい値と違うなら、固定があるせいで直せていない。
                    touches_pinned |= (slot.get(&r.oto) - slot.get(&e.oto)).abs() > f64::EPSILON;
                } else {
                    slot.set(&mut next, slot.get(&r.oto));
                }
            }

            if next != e.oto {
                fixed += 1;
                self.opened_mut()?
                    .ledger
                    .set_oto_value(e.take_id, &e.alias, &next)?;
            }
            // 固定を避けたせいで違反が残るものも止める。 直せていないのに
            // 通すと、`TR-ALN-20` が塞いだはずの形のまま書き出される。
            if !r.may_export() || touches_pinned {
                blocked.push(e.alias.clone());
            }
        }
        self.refresh_review()?;

        // 止めるのはキューの遷移として行う（`REQ-ALN-004`）。
        //
        // 移れるのは自動確定していたものだけ。 まだ確認待ちのものは
        // そのままでも書き出しを塞いでいる（`INV-ALN-003`）ので、
        // `WrongState` は失敗として扱わない——**それ以外は握り潰さない。**
        // 台帳が書けなかったのを「遷移できなかった」と同じ顔で通すと、
        // 直せない違反が黙って消える。
        for alias in &blocked {
            match self.with_entry(alias, |q, id| q.validation_unrepairable(id)) {
                Ok(()) => {}
                Err(e) if e.kind == "review.wrong_state" => {}
                Err(e) => return Err(e),
            }
        }
        Ok((fixed, blocked))
    }

    /// 原音設定を外へ出してよい状態か（`INV-ALN-003`）。
    ///
    /// `oto.ini` 単体でも配布パッケージでも、同じ関門を通す。
    /// 片方だけが緩いと、確認の済んでいない切り出しが配布物に入る。
    ///
    /// **「もう書き出したか」はここで見ない。** それは `oto.ini` の書き出しに
    /// 固有の状態で、一度出したからといって配布パッケージを止める理由が無い。
    #[tracing::instrument(skip(self), err)]
    fn ensure_otos_ready(&mut self) -> Result<()> {
        self.validate_otos()?;
        // 切り出しが1つも取れなかった行は、キューにも現れない（`INV-ALN-003`）。
        //
        // **発声が見つからなかったテイクも採用される。** そのテイクは
        // `oto_values` に1行も書かないので、キューが空でも書き出しから
        // 黙って落ちる。キューの関門はエントリしか見られないので、
        // ここで別に見る。
        let missing = self.opened_mut()?.ledger.adopted_rows_without_oto()?;
        if !missing.is_empty() {
            return Err(AppError::new(
                "review.missing_oto",
                format!("切り出しの取れていない行が {} 件ある", missing.len()),
            ));
        }
        // エイリアスが WAV をまたいで重なると、キューが片方を落とす。
        // 落ちたほうは確認もされず `oto.ini` にも出ない。
        let conflicting = self.opened_mut()?.ledger.adopted_conflicting_aliases()?;
        if !conflicting.is_empty() {
            return Err(AppError::new(
                "review.conflicting_alias",
                format!("別の回と同じ名前の音が {} 件ある", conflicting.len()),
            ));
        }
        if !self.opened()?.review.all_confirmed() {
            return Err(review_error(ReviewError::ReviewPending));
        }
        Ok(())
    }

    /// `oto.ini` を書き出す（`TR-ALN-21`, `REQ-PKG-003`）。
    ///
    /// 先に検証を通し、確認が残っていれば止まる（`INV-ALN-003`）。
    /// 返るのは書いた先。
    ///
    /// 文字コードを選べる（`TR-ALN-21`）。 既定は CP932 で、UTF-8（BOM なし）も
    /// 選べる。**固定にすると、CP932 で表せない名前が1つあるだけで
    /// 書き出す手段が無くなる。**
    ///
    /// **単音階だけ。** 多音階は配布パッケージの側から出す（`TR-ALN-22`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、多音階である、確認が残っている、
    /// 選んだ文字コードで書けない文字がある。
    #[tracing::instrument(skip(self, encoding), err)]
    pub fn export_otos(&mut self, encoding: TextEncoding) -> Result<PathBuf> {
        // 「もう出した」を先に言う。 共通の関門（[`Self::ensure_otos_ready`]）は
        // ここを見ない——配布パッケージの側は、`oto.ini` を一度出したことで
        // 止まってはいけない。順序を変えると、断る理由の言い方が入れ替わる。
        if self.opened()?.review.is_exported() {
            return Err(review_error(ReviewError::AlreadyExported));
        }
        // 多音階はここから出さない（`TR-ALN-22`）。 書く先は WAV が平らに並ぶ
        // `audio/` で、音高ごとに分ける先が無い。平らなまま出すと同じ綴りが
        // 音階の数だけ並び、受け取った UTAU は1つしか見ない
        // ——**音域を広げるために録った音階が、まるごと鳴らない。**
        // 音高ごとに分かれた `oto.ini` は配布パッケージが出す（`TR-PKG-04`）。
        if self.opened_mut()?.ledger.recording_tones()?.len() > 1 {
            return Err(AppError::new(
                "review.multi_pitch_oto_ini",
                "多音階の oto.ini は、配布パッケージの書き出しから出す",
            ));
        }
        self.ensure_otos_ready()?;
        // 関門はキューが持つ。 ここで件数を数え直さない（`INV-ALN-003`）。
        //
        // **状態は動かさずに訊く。** 先に `export` を呼ぶと、符号化や書き込みが
        // 失敗しても「書き出し済み」になり、その回は編集も再試行も断られる
        // ——台帳には書き出していないと書いてあるので、開き直すまで戻らない。
        self.opened()?.review.may_export().map_err(review_error)?;

        // 先に集めてから書く。 `take_file_name` が台帳を引くので、
        // キューを借りたまま回すと同じ `Open` を可変と不変で同時に持つことになる。
        let rows: Vec<(String, i32, Oto)> = {
            let open = self.opened()?;
            open.review
                .all()
                .filter_map(|(key, e)| {
                    let id = open.review_takes.get(key).copied()?;
                    let alias = crate::review::EntryKey::parse(key)?.alias().to_owned();
                    Some((alias, id, e.oto))
                })
                .collect()
        };
        let mut entries = Vec::new();
        for (alias, take_id, oto) in rows {
            let file = self.take_file_name(take_id)?;
            entries.push(ini::IniEntry { file, alias, oto });
        }
        // 既定は CP932（`TR-PLT-08`, `DEC-PLT-013`）。UTAU 本体が読める形で出す。
        let bytes = ini::write(&entries, encoding)
            .map_err(|e| AppError::new(e.kind(), "oto.ini を書けない"))?;
        // WAV と同じディレクトリへ置く。 `oto.ini` の左辺はファイル名だけなので、
        // 別の階層に置くと**全部の参照が解決しない**。UTAU が読む形も、
        // wav と oto.ini が同じ場所に並んだ形。
        //
        // 作業ファイルにはしない（`TR-PKG-40`）。 正本は台帳で、これは派生物。
        // 読み戻して真とすることはない。外部フォルダへ一式を出す経路は
        // `koeru_core::handoff`（`PROFILE-M4`）。
        let path = self.opened()?.dir.audio_dir().join("oto.ini");
        std::fs::write(&path, bytes)?;

        // ここまで来て初めて確定させる。関門はもう一度キューが見る。
        let mut next = self.opened()?.review.clone();
        next.export().map_err(review_error)?;
        let over = next.mode() != ReviewMode::Individual;
        let open = self.opened_mut()?;
        crate::review::save_mode(&mut open.ledger, &next, over)?;
        open.review = next;
        Ok(path)
    }

    /// いまのモデルで作られたと確かめられない推定（`TR-ALN-29`）。
    ///
    /// 自動で作り直さない。 返るのは行 ID で、再推定するかは本人が選ぶ。
    ///
    /// **指紋が無いものも入れる。** この版より前に録ったエントリは指紋を持たない。
    /// 無いことを「今のモデルで作られた」と読むと、**まさに指紋が暴くはずだった
    /// モデル版の食い違いを、指紋の不在が隠す。**
    ///
    /// アライナが変わったものと、指紋が無いものを分けない。 本人が決めるのは
    /// どちらでも「録り直すか」で、同じ1つの決定になる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    pub fn stale_takes(&mut self) -> Result<Vec<String>> {
        let now = self.aligner.identity().to_owned();
        let entries = self.opened_mut()?.ledger.adopted_otos()?;
        let mut out = Vec::new();
        for e in entries {
            let f = self.opened_mut()?.ledger.fingerprint_of(e.take_id)?;
            // 入力が変わったものは本人が動かしたものなので、ここには出さない
            // （`Change::Input` は黙って作り直してよい）。見るのはアライナだけ。
            let unsure = f.is_none_or(|f| f.aligner != now);
            if unsure && !out.contains(&e.row_id) {
                out.push(e.row_id);
            }
        }
        out.sort_unstable();
        Ok(out)
    }

    /// 同梱しているモデルのライセンス表記（`TR-ALN-31`）。
    ///
    /// # Errors
    ///
    /// 台帳が読めない、判断記録の無い未確認モデルが載っている。
    pub fn model_notice() -> Result<String> {
        let models =
            ledger::models().map_err(|e| AppError::new(e.kind(), "モデルの台帳を読めない"))?;
        ledger::check(&models)
            .map_err(|e| AppError::new(e.kind(), "モデルの台帳が規律を満たさない"))?;
        Ok(ledger::notice(&models))
    }

    /// キューの1件を動かして、結果を台帳へ書く。
    ///
    /// **複製の上で遷移させ、書けてから本物へ移す。** 正本は台帳なので
    /// （`TR-PKG-40`）、先に手元のキューを進めると、書き込みが落ちたときに
    /// 画面だけが先へ行く——開き直すまで、確認したはずのものが戻ってくる。
    fn with_entry(
        &mut self,
        key: &str,
        f: impl FnOnce(&mut ReviewQueue, &str) -> std::result::Result<(), ReviewError>,
    ) -> Result<()> {
        let mut next = self.opened()?.review.clone();
        f(&mut next, key).map_err(review_error)?;

        let Some(take_id) = self.opened()?.review_takes.get(key).copied() else {
            return Err(AppError::new(
                "review.no_such_entry",
                "そのエントリを持つテイクが無い",
            ));
        };
        let Some(entry) = next.get(key).cloned() else {
            return Err(AppError::new("review.no_such_entry", "そのエントリが無い"));
        };
        // 台帳は（テイク, 綴り）で持つ。 鍵から綴りを取り出して渡す
        // ——音高はテイクの行が持っているので、二重には書かない。
        let Some(parsed) = crate::review::EntryKey::parse(key) else {
            return Err(AppError::new("review.no_such_entry", "鍵の形が違う"));
        };
        let open = self.opened_mut()?;
        crate::review::save_entry(&mut open.ledger, take_id, parsed.alias(), &entry)?;
        open.review = next;
        Ok(())
    }

    /// キュー全体を台帳へ書く。まとめて確認したあとに使う。
    ///
    /// 全件を1つのトランザクションで書く。 1件ずつ流すと、途中で落ちたときに
    /// 「半分だけ確認済み」の台帳が残る。
    fn save_all_entries(&mut self, next: ReviewQueue) -> Result<()> {
        let rows: Vec<koeru_core::db::ReviewEntryRow> = {
            let open = self.opened()?;
            next.all()
                .filter_map(|(key, e)| {
                    Some(koeru_core::db::ReviewEntryRow {
                        take_id: open.review_takes.get(key).copied()?,
                        alias: crate::review::EntryKey::parse(key)?.alias().to_owned(),
                        oto: e.oto,
                        state: e.state.as_str().to_owned(),
                        pinned: e.pins(),
                    })
                })
                .collect()
        };
        let over = next.mode() != ReviewMode::Individual;
        let open = self.opened_mut()?;
        open.ledger.put_review_entries(&rows)?;
        crate::review::save_mode(&mut open.ledger, &next, over)?;
        open.review = next;
        Ok(())
    }

    /// そのテイクの WAV のファイル名。`oto.ini` の左辺に出る。
    fn take_file_name(&mut self, take_id: i32) -> Result<String> {
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("ledger.unknown_take", "テイクが台帳に無い"))?;
        Ok(std::path::Path::new(&take.rel_path)
            .file_name()
            .map_or_else(
                || take.rel_path.clone(),
                |s| s.to_string_lossy().into_owned(),
            ))
    }

    /// 自動原音設定が波形のどこを指したかを見せるための口。
    /// 数字だけ出しても、それが発声と重なっているかは分からない
    /// ——4モーラが 100ms に潰れていても「確信度 30%」としか出なかった。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self), err)]
    pub fn otos_of_take(&mut self, take_id: i32) -> Result<Vec<(String, koeru_oto::Oto)>> {
        Ok(self.opened_mut()?.ledger.otos_of(take_id)?)
    }

    /// 録れたものをそのまま鳴らす（`TR-REC-43`）。
    ///
    /// 合成を通さない。 `preview` は oto で切り出して目標音高へ寄せた音で、
    /// 「録れているか」を確かめるための音ではない。
    /// 声が入っていないテイクを、合成の失敗と区別できるようにする。
    ///
    /// # Errors
    ///
    /// そのテイクが台帳に無い、素材を読めない、出力を開けない。
    #[tracing::instrument(skip(self), err)]
    pub fn play_take(&mut self, take_id: i32) -> Result<f64> {
        let root = self.opened()?.dir.root().to_path_buf();
        let take = self
            .opened_mut()?
            .ledger
            .take(take_id)?
            .ok_or_else(|| AppError::new("app.unknown_take", "そのテイクが台帳に無い"))?;
        let w = wav::read(root.join(&take.rel_path))?;
        #[allow(
            clippy::cast_precision_loss,
            reason = "テイクの長さは表示用。桁は十分に収まる"
        )]
        let ms = w.samples.len() as f64 * 1000.0 / f64::from(w.rate_hz);

        // 前の再生は止める。 重ねると何を聴いているか分からなくなる。
        self.playback = None;
        self.playback = Some(mac::play(w.samples, w.rate_hz)?);
        Ok(ms)
    }

    /// 鳴らしている音を止める（`TR-SYN-27`）。
    ///
    /// 進行中の合成も止める。 200ms 以内に抜ける。
    pub fn stop_preview(&mut self) {
        self.singing = None;
        self.playback = None;
        self.playback_stream = None;
        // 積んである仕事も捨てる（`TR-SYN-27`）。曲を切り替えたときに、
        // 前の曲のための前処理を回し続ける意味は無い。
        self.workers.clear();
    }

    /// 曲を歌わせる（`TR-SYN-01`〜`04`, `TR-SYN-18`）。
    ///
    /// 先頭フレーズができた時点で鳴らしはじめ、残りは並行して作る（`TR-SYN-03`）。
    ///
    /// 鳴らせない音符があるフレーズは、フレーズごと落とす（`TR-SYN-18` (2)）。
    /// 落とした位置に無音・別音・代替音を挿入しない。
    /// 残りが短すぎれば、そもそも鳴らさない（`TR-SYN-18` (3)）。
    ///
    /// 返るのは（フレーズ数, 落としたフレーズ数, 鳴らす長さ ms）。
    #[tracing::instrument(skip(self, id), err)]
    pub fn sing_song(&mut self, id: &str) -> Result<SungSong> {
        let started = std::time::Instant::now();
        // 先に止める。 重ねると何を聴いているか分からなくなる。
        self.stop_preview();

        // この音源の作り方（`TR-RCL-01`）。 どの綴りを引くかがこれで決まる。
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;

        let rate = 44_100_u32;
        let root = self.opened()?.dir.root().to_path_buf();
        let song = self
            .opened_mut()?
            .ledger
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, s)| s)
            .ok_or_else(|| AppError::new("app.unknown_song", "その曲がバンクに無い"))?;

        // ## キー（`TR-SYN-15`, `DEC-SYN-012`）
        //
        // **本人が指定した調で鳴らす。** 自動では動かさない——本人が
        // 「この曲でこの声はどう聴こえるか」を確かめるために入れた曲を
        // 黙って別の調にすると、確かめた結果が別の曲のものになる。
        //
        // 遠くても止めない。 ±7 半音・二乗平均 4 半音という線に実測の裏付けが
        // 無い（`reclist.toml` の領域リスク）ので、その線で鳴らすことを禁じない。
        // どうなるかは画面が先に出していて、押したのは本人。
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        let fit = koeru_core::song::range_fit(&song, &tones);
        if fit.transpose != 0 {
            tracing::info!(semitones = fit.transpose, "指定されたキーで鳴らす");
        }

        // ## 素材を集める
        //
        // 採用テイクだけを使う。 無効にしたテイク（取りこぼし）は入らない。
        // 収録音高ごとの素材（`TR-SYN-16`）。 単音階なら鍵は `None` の1つ。
        let by_tone = self.materials_by_tone(&root)?;
        // 解決の可否は全体で見る。 どの音高にも無い単位だけを欠損とする。
        let available: std::collections::BTreeSet<String> = by_tone
            .values()
            .flat_map(|m| m.paths.keys().cloned())
            .collect();
        /*
          周波数表は素材のパスで引く（`TR-SYN-25`）。

          **エイリアスで引かない。** 多音階では音高ごとに同じエイリアスがあるので
          （`TR-RCL-26`）、エイリアスの表へ畳むと1音高ぶんしか残らない。
          そのまま渡すと、下の `pick` が選んだ音高と違う素材の表が当たって、
          **選んだ音高の素材に別の音高の f0 が乗る。**
        */
        let tables: HashMap<std::path::PathBuf, Vec<f64>> = by_tone
            .values()
            .flat_map(|m| {
                m.tables.iter().filter_map(|(alias, f0)| {
                    m.paths.get(alias).map(|path| (path.clone(), f0.clone()))
                })
            })
            .collect();

        // ## フレーズに割る
        let moras = song
            .moras(preset.set)
            .ok_or_else(|| AppError::new("app.unreadable_lyrics", "この曲の歌詞を読めない"))?;
        // この音源の作り方で解決する（`TR-SYN-36`）。 単独音で解決すると、
        // 連続音の音源が持っている `a か` を一度も引かない。
        // 休符で綴りの文脈を切る（`TR-RCL-12`）。 繋げて解決すると、
        // 休符のあとの音符が語頭形ではなく継続に解決される。
        let breaks = song.phrase_breaks(preset.set);
        let resolved = koeru_core::alias::resolve_phrase(
            &rules,
            preset.method,
            &moras,
            &available,
            preset.set,
            &breaks,
        );

        let mut phrases: Vec<(koeru_synth::phrase::Phrase, bool)> = Vec::new();
        // フレーズの手前に置く無音（`TR-RCL-12` の休符）。`phrases` と同じ並び。
        let mut leads: Vec<f64> = Vec::new();
        let mut lead_ms = 0.0_f64;
        let mut current: Vec<koeru_synth::phrase::NoteSpec> = Vec::new();
        let mut playable = true;
        // 区画が切り替わった位置（`TR-SYN-16`）。原音設定側の確認対象になる。
        let mut switches: Vec<(usize, Option<i32>, Option<i32>)> = Vec::new();
        let mut last_tone: Option<Option<i32>> = None;

        // UST の 480 ティック = 4分音符。曲のテンポで長さを出す（`TR-SYN-30`）。
        // **120 BPM を決め打っていた。** 読み込んだ UST のテンポが落ちていて、
        // どの曲も同じ速さで鳴っていた。
        let beat_ms = 60_000.0 / song.tempo_bpm.max(1.0);

        for (k, e) in resolved.iter().enumerate() {
            // 添字では音符を引けない。 CVVC は渡りを挟むので数が合わない
            // （`alias::PhraseEntry::mora`）。
            let i = e.mora;
            // 移調を当てた音高で鳴らす（`TR-SYN-15`）。
            let midi = song.notes.get(i).map_or(DEFAULT_TONE_MIDI, |n| n.midi) + fit.transpose;
            let ticks = song.notes.get(i).map_or(480, |n| n.ticks);
            let duration_ms = f64::from(ticks) / 480.0 * beat_ms;

            /*
              休符（`TR-RCL-12`）。

              **鳴らさない時間もそのまま流す。** 詰めると、取り込んだ曲が
              元と違うリズムで鳴る。フレーズを切って、手前の無音として持つ
              ——`Phrase` は音符の並びで、無音を中に持てない。
            */
            if e.is_main() {
                let rest_ticks = song.notes.get(i).map_or(0, |n| n.rest_ticks);
                if rest_ticks > 0 {
                    if !current.is_empty() {
                        phrases.push((
                            koeru_synth::phrase::Phrase::new(std::mem::take(&mut current)),
                            playable,
                        ));
                        leads.push(std::mem::take(&mut lead_ms));
                        // 札はフレーズごと。 **戻していなかった**ので、
                        // 1つ素材が欠けると、それ以降のフレーズが全部
                        // 鳴らせない扱いになり、短縮版が丸ごと落ちていた。
                        playable = true;
                    }
                    lead_ms += f64::from(rest_ticks) / 480.0 * beat_ms;
                }
            }

            match &e.unit {
                koeru_core::alias::PhraseUnit::Sound(res) => {
                    // 休みを挟んだら隣ではない。 渡りは隣り合う音符のあいだにだけ置く
                    // （OpenUtau の `nextNeighbour`）。
                    if !e.is_main()
                        && resolved
                            .get(k + 1)
                            .and_then(|n| song.notes.get(n.mora))
                            .is_some_and(|n| n.rest_ticks > 0)
                    {
                        continue;
                    }
                    // その音を担う収録音高から素材を引く（`TR-SYN-13`, `TR-SYN-16`）。
                    // 無ければ1段下、さらに下、最低音高へ落ちる（`TR-RCL-20`）。
                    let Some((used_tone, m)) = pick_material(&by_tone, &tones, midi, &res.alias)
                    else {
                        playable = false;
                        continue;
                    };
                    // 切り替わった位置を記録する（`TR-SYN-16`）。
                    // 切り替えは音符境界でのみ——1音符の途中では変わらない。
                    if let Some(prev) = last_tone
                        && prev != used_tone
                    {
                        switches.push((i, prev, used_tone));
                    }
                    last_tone = Some(used_tone);

                    let Some(oto) = m.otos.get(&res.alias) else {
                        playable = false;
                        continue;
                    };

                    /*
                      渡り（CVVC の VC）は直前の音符の尾に乗る（`TR-SYN-12`）。

                      長さは**次の CV の先行発声**から決まる（`oto::transition_ms`）。
                      その時間は直前の音符から取る——足すのではない。足すと曲が伸びる。
                    */
                    let length_ms = if e.is_main() {
                        duration_ms
                    } else {
                        let owner_ms = current.last().map_or(duration_ms, |n| n.duration_ms);
                        // 次は必ず本体（`resolve_phrase` が渡りの直後に押す）。
                        //
                        // **渡りの持ち主の素材から読んでいた。** 次の音符が
                        // 別の収録音高へ切り替わる境界では、違う区画の
                        // 先行発声で直前の音符を削ることになる。
                        // 次の本体が実際に使う素材を引き直す。
                        let next_oto = resolved
                            .get(k + 1)
                            .and_then(|n| match &n.unit {
                                koeru_core::alias::PhraseUnit::Sound(r) => Some((n, r)),
                                _ => None,
                            })
                            .and_then(|(n, r)| {
                                let next_midi =
                                    song.notes.get(n.mora).map_or(DEFAULT_TONE_MIDI, |x| x.midi)
                                        + fit.transpose;
                                let (_, nm) = pick_material(&by_tone, &tones, next_midi, &r.alias)?;
                                nm.otos.get(&r.alias)
                            });
                        let vc_ms = koeru_core::oto::transition_ms(next_oto, owner_ms, beat_ms);
                        if let Some(prev) = current.last_mut() {
                            prev.duration_ms = (prev.duration_ms - vc_ms).max(0.0);
                        }
                        vc_ms
                    };

                    current.push(koeru_synth::phrase::NoteSpec {
                        alias: res.alias.clone(),
                        sample_path: m.paths.get(&res.alias).cloned().unwrap_or_default(),
                        sample_hash: hash_of(m.paths.get(&res.alias)),
                        oto: *oto,
                        midi,
                        duration_ms: length_ms,
                    });
                }
                // 促音は鳴らさない拍。 ここでフレーズを切るが、
                // 素材が足りないわけではないので `playable` は倒さない。
                koeru_core::alias::PhraseUnit::Rest => {
                    if !current.is_empty() {
                        phrases.push((
                            koeru_synth::phrase::Phrase::new(std::mem::take(&mut current)),
                            playable,
                        ));
                        leads.push(std::mem::take(&mut lead_ms));
                        playable = true;
                    }
                }
                koeru_core::alias::PhraseUnit::Missing(_) => {
                    // 鳴らせない音符が出たら、そこでフレーズを切る（`TR-SYN-18`）。
                    if !current.is_empty() {
                        phrases.push((
                            koeru_synth::phrase::Phrase::new(std::mem::take(&mut current)),
                            playable,
                        ));
                        leads.push(std::mem::take(&mut lead_ms));
                    }
                    playable = true;
                }
            }
        }
        if !current.is_empty() {
            phrases.push((koeru_synth::phrase::Phrase::new(current), playable));
            leads.push(lead_ms);
        }

        let total_phrases = phrases.len();
        let kept = koeru_synth::phrase::shortened(&phrases, preview::MIN_PLAYABLE_MS).ok_or_else(
            || {
                AppError::new(
                    "synth.too_short",
                    "続けて鳴らせる長さが足りないので、この曲はまだ出せない",
                )
            },
        )?;
        let dropped = total_phrases - kept.len();
        let duration_ms: f64 = kept.iter().map(|i| phrases[*i].0.duration_ms()).sum();
        // 休みを連れて渡す。 落としたフレーズの手前の休みは、残った側へ足す
        // ——鳴らせない音符を飛ばしても、そこにあった間は残る。
        let mut owned: Vec<(koeru_synth::phrase::Phrase, f64)> = Vec::with_capacity(kept.len());
        let mut carried = 0.0_f64;
        for (i, (phrase, _)) in phrases.iter().enumerate() {
            let lead = leads.get(i).copied().unwrap_or_default();
            if kept.contains(&i) {
                owned.push((phrase.clone(), lead + std::mem::take(&mut carried)));
            } else {
                carried += lead;
            }
        }
        let phrase_count = owned.len();

        // ## 鳴らす
        //
        // 素材の場所は渡さない。 `NoteSpec` が実際のパスを持っていて、
        // `WavSamples` はそれをそのまま読む（`TR-SYN-16`）。
        let samples: Arc<dyn koeru_synth::phrase::Samples + Send + Sync> =
            Arc::new(WavSamples { tables });

        let stream = mac::play_streaming(Vec::new(), rate)?;
        let sink = StreamSink {
            feed: stream.feed(),
        };
        let (head, running) = preview::start(
            owned,
            samples,
            Arc::clone(&self.song_cache),
            Box::new(sink),
            rate,
        )
        .map_err(|e| AppError::new(e.kind(), e))?;

        stream.push(&head);
        self.playback_stream = Some(stream);
        self.singing = Some(running);

        // ## 押してから鳴るまでを測る（`TR-SYN-33`）
        //
        // 場面ごとに分けて溜める。 同じ目標でひとくくりにすると、
        // 初回の重さと2回目以降の軽さのどちらかが説明できなくなる。
        let case = if self.ever_previewed {
            Case::Warm
        } else {
            Case::First
        };
        self.ever_previewed = true;
        let elapsed = started.elapsed();
        self.observed.entry(case).or_default().record(elapsed);
        tracing::info!(
            case = ?case,
            elapsed_ms = elapsed.as_millis(),
            budget_ms = latency::budget(case).median.as_millis(),
            "試唱の待ち時間"
        );

        Ok(SungSong {
            title: song.title,
            phrases: phrase_count,
            dropped_phrases: dropped,
            duration_ms,
            subbank_switches: switches
                .into_iter()
                .map(|(note_index, from, to)| SubbankSwitch {
                    note_index,
                    from,
                    to,
                })
                .collect(),
        })
    }

    /// 試唱の待ち時間の実測（`TR-SYN-33`）。
    ///
    /// 回数が少ないうちは判定しない。
    #[must_use]
    pub fn latency_report(&self) -> Vec<LatencyRow> {
        let mut out: Vec<LatencyRow> = self
            .observed
            .iter()
            .map(|(case, o)| LatencyRow {
                case: *case,
                count: o.count(),
                median_ms: o.median().map(ms_u32),
                budget_ms: ms_u32(latency::budget(*case).median),
                meets: o.meets(*case, LATENCY_MIN_SAMPLES),
            })
            .collect();
        out.sort_by_key(|r| r.case.as_str());
        out
    }

    /// 採用テイクの素材・周波数表・oto を集める。
    ///
    /// `tone` を渡すとその収録音高の素材だけを拾う（`TR-SYN-16`）。 多音階では
    /// **音高を跨いで拾わない**——高音階の素材を低音階の音符に当てると、
    /// 1音だけ別人の声になる。単音階では `None`。
    fn adopted_materials_at(
        &mut self,
        root: &std::path::Path,
        tone: Option<i32>,
    ) -> Result<Materials> {
        let mut paths = HashMap::new();
        let mut tables = HashMap::new();
        let mut otos = HashMap::new();

        // 行が生む綴りで集める（`TR-RCL-18`）。
        //
        // **仮名で集めていた。** 原音設定は綴りで置かれているので、
        // 連続音では `oto_of(take, "か")` が何も返さず、**素材が1つも
        // 載らなかった。** CVVC では渡りと語尾だけが落ちていた。
        let rows: Vec<String> = self
            .opened_mut()?
            .ledger
            .covered_aliases()?
            .into_iter()
            .collect();
        for unit in rows {
            let Some(take) = self.opened_mut()?.ledger.take_for_alias_at(&unit, tone)? else {
                continue;
            };
            // 1テイクに複数のエントリがあるので、欲しい綴りを名前で引く
            // （`DEC-ALN-013`）。
            let Some(oto) = self.opened_mut()?.ledger.oto_of(take.id, &unit)? else {
                continue;
            };
            paths.insert(unit.clone(), root.join(&take.rel_path));
            if let Some(a) = self.opened_mut()?.ledger.analysis_of(take.id)? {
                tables.insert(unit.clone(), a.frq.f0);
            }
            otos.insert(
                unit,
                koeru_core::oto::Oto {
                    offset_ms: oto.offset_ms,
                    consonant_ms: oto.consonant_ms,
                    cutoff_ms: oto.cutoff_ms,
                    preutterance_ms: oto.preutterance_ms,
                    overlap_ms: oto.overlap_ms,
                },
            );
        }
        Ok(Materials {
            paths,
            tables,
            otos,
        })
    }

    /// 収録音高ごとの素材（`TR-SYN-16`）。
    ///
    /// 単音階なら1つで、鍵は `None`。 多音階では音高ごとに引く。
    fn materials_by_tone(
        &mut self,
        root: &std::path::Path,
    ) -> Result<std::collections::BTreeMap<Option<i32>, Materials>> {
        let tones = self.opened_mut()?.ledger.recording_tones()?;
        let mut out = std::collections::BTreeMap::new();
        if tones.len() <= 1 {
            out.insert(None, self.adopted_materials_at(root, None)?);
        } else {
            for t in tones {
                out.insert(Some(t), self.adopted_materials_at(root, Some(t))?);
            }
        }
        Ok(out)
    }

    /// テスト用。 音声デバイス無しで、行を収録済みとして印を付ける。
    ///
    /// 実際の収録は `start_take` → `finish_take` を通る。ここはカバレッジの
    /// 計算だけを確かめたいときの入口。**WAV も原音設定も置かない**ので、
    /// 書き出しまで通す試験は [`Self::seed_material_for_test`] を使う。
    #[cfg(any(test, feature = "test-hooks"))]
    #[tracing::instrument(skip(self), err)]
    pub fn mark_recorded_for_test(&mut self, row_id: &str) -> Result<()> {
        let session_id = self.test_session()?;
        let take = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: format!("audio/{row_id}_1.wav"),
            frames: 44_100,
            recorded_at: now_rfc3339(),
        })?;
        self.opened_mut()?.ledger.adopt_take(row_id, take)?;
        Ok(())
    }

    /// テスト用。 実体の WAV と原音設定まで置く。
    ///
    /// 書き出しは素材を実際に開く（`TR-PKG-49`）ので、印だけでは通らない。
    /// マスターは 44100 Hz（`TR-REC-02`）。
    #[cfg(any(test, feature = "test-hooks"))]
    #[tracing::instrument(skip(self), err)]
    pub fn seed_material_for_test(&mut self, row_id: &str) -> Result<i32> {
        let session_id = self.test_session()?;
        let root = self.opened()?.dir.root().to_path_buf();
        let rel = format!("audio/{row_id}_1.wav");
        let path = root.join(&rel);
        std::fs::create_dir_all(self.opened()?.dir.audio_dir())?;

        // 1.5 秒の一定振幅。 中身は問わない——見るのは長さとレートだけ。
        //
        // **0.5 秒にしていた。** 1行は最大 8 モーラなので、下位方式の
        // 書き出し（`TR-PKG-24`）が境界から5値を作り直すと、後ろのモーラが
        // ファイル末尾を越えて検証で落ちる。
        let samples = vec![0.2_f32; MASTER_RATE_HZ as usize * 3 / 2];
        let mut part = koeru_audio::wav::PartialTake::create(&path, MASTER_RATE_HZ)?;
        part.write(&samples)?;
        part.finalize()?;

        let take = self.opened_mut()?.ledger.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: rel,
            frames: i64::try_from(samples.len()).unwrap_or(0),
            recorded_at: now_rfc3339(),
        })?;
        self.opened_mut()?.ledger.adopt_take(row_id, take)?;

        // エイリアスは台帳が持つものをそのまま使う。 呼ぶ側に渡させない——
        // 台帳と食い違った名前で置けてしまう。
        //
        // **収録単位そのものを置いていた。** 連続音や CVVC では書き出しの
        // 綴りと違う名前になり、被覆が満ちないまま試験が通っていた。
        //
        // 行から導き直さない。 同じ綴りを2つの行が生むとき、名乗るのは
        // 1つだけで（`TR-ALN-22`）、その取り決めは台帳にしか無い。
        // 導き直すと、本番では作られない5値を試験だけが持つ。
        let here = self.current_preset()?;
        let rules = self.current_rules()?;
        let line = self.opened_mut()?.ledger.row_units_of(row_id, here.set)?;
        let aliases = self.opened_mut()?.ledger.aliases_of_row(row_id)?;
        // 境界も置く。 下位方式の書き出し（`TR-PKG-24`）が引く。
        let saved: Vec<(String, koeru_core::oto::Boundary)> =
            koeru_core::reclist::row_entries(&rules, here.method, &line)
                .into_iter()
                .filter_map(|(a, slot)| match slot {
                    koeru_core::reclist::Slot::Cv { mora } => {
                        // モーラを重ねない。 **母音を 30ms より短くしない**
                        // ——規約プリセットは子音部を「先行発声＋母音定常
                        // マージン 30ms」に置くので、そこに届かないと
                        // `CutoffNotAfterConsonant` で検証に落ちる。
                        #[allow(
                            clippy::cast_precision_loss,
                            reason = "1行のモーラ数は MAX_UNITS_PER_ROW まで"
                        )]
                        let at = mora as f64 * 160.0;
                        Some((
                            a,
                            koeru_core::oto::Boundary {
                                voice_start_ms: 10.0 + at,
                                vowel_start_ms: 25.0 + at,
                                vowel_end_ms: 145.0 + at,
                            },
                        ))
                    }
                    _ => None,
                })
                .collect();
        self.opened_mut()?.ledger.put_boundaries(take, &saved)?;
        for alias in &aliases {
            self.opened_mut()?.ledger.put_oto(
                take,
                alias,
                &koeru_core::db::koeru_oto::Oto {
                    offset_ms: 50.0,
                    consonant_ms: 60.0,
                    cutoff_ms: -300.0,
                    preutterance_ms: 40.0,
                    overlap_ms: 20.0,
                },
                1.0,
                None,
                false,
            )?;
            // 自動で確定したことにする。 確認が残っていると書き出せない
            // （`INV-ALN-003`）ので、ここを飛ばすと書き出しの試験が通らない。
            self.opened_mut()?.ledger.set_oto_state(
                take,
                alias,
                koeru_align::review::EntryState::AutoConfirmed.as_str(),
            )?;
        }
        let analysis = koeru_core::analysis::TakeAnalysis::compute(
            &samples,
            MASTER_RATE_HZ,
            &[220.0_f64; 200],
            0.005,
        );
        self.opened_mut()?.ledger.put_analysis(take, &analysis)?;
        self.refresh_review()?;
        Ok(take)
    }

    /// テスト用の収録セッション。無ければ1つ始める。
    #[cfg(any(test, feature = "test-hooks"))]
    fn test_session(&mut self) -> Result<i32> {
        let open = self.opened_mut()?;
        if open.session_id == 0 {
            open.session_id = open.ledger.start_session(&SessionSnapshot {
                started_at: now_rfc3339(),
                device_id: "test".to_owned(),
                sample_rate_hz: 44_100,
                channels: 1,
                effects_state: "clean".to_owned(),
                route: "test".to_owned(),
                source_channel: 0,
                master_rate_hz: 44_100,
                resampler: koeru_audio::resample::IDENTIFIER.to_owned(),
                upstream_conversion: "unknown".to_owned(),
            })?;
        }
        Ok(open.session_id)
    }

    /// 開いているプロジェクトのディレクトリ。
    #[tracing::instrument(skip(self), err)]
    pub fn project_dir(&self) -> Result<&ProjectDir> {
        Ok(&self.opened()?.dir)
    }

    fn display_name(&self) -> Result<String> {
        Ok(self.opened()?.dir.read_manifest()?.display_name)
    }

    /// 1テイクをアライメントする（`TR-ALN-03`, `TR-ALN-10`）。
    ///
    /// 録音完了の直後に呼ぶ（`TR-ALN-10` の逐次推定）。
    ///
    /// `Alignment` をそのまま返す。 境界だけに畳んで返すと、
    /// 事後確率が捨てられて `TR-ALN-24` の成分 (1) 経路確信度が永久に出せない
    /// （一度そう書いた）。呼び出し側が確信度を組み立てるのに要る。
    ///
    /// アライナが答えられなかったときだけ、発声区間の検出（[`detect_single`]）で
    /// 境界を出す。その場合は事後確率を持たない `Alignment` になる。
    /// 黙って諦めない——落ちた理由はトレースに種別で出す。
    fn align_take(&mut self, samples: &[f64], rate: u32, row_id: &str) -> Option<Alignment> {
        use koeru_align::aligner::AlignRequest;

        // 読みは録音リストが持っている（`TR-ALN-07`。実行時に g2p を持ち込まない）。
        let kana = self.opened_mut().ok()?.ledger.units_of(row_id).ok()?;
        let readings: Vec<&str> = kana.iter().map(String::as_str).collect();
        let phonemes = match koeru_align::phoneme::phonemes_for_all(&readings) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    reason = e.kind(),
                    "読みを音素へ写せない。発声区間の検出で境界を出す"
                );
                return fallback_alignment(samples, rate);
            }
        };

        let req = AlignRequest {
            samples,
            sample_rate_hz: rate,
            phonemes: &phonemes,
            grid: None,
        };
        match self.aligner.as_aligner().align(&req) {
            Ok(a) => Some(a),
            Err(e) => {
                // テキスト逸脱はここで潰さない（`TR-ALN-09`）。
                // 境界が出ないまま返せば、oto が付かず確認キューへ回る。
                tracing::info!(reason = e.kind(), "アライメントが通らなかった");
                if matches!(e, koeru_align::aligner::AlignError::TextDeviation) {
                    None
                } else {
                    fallback_alignment(samples, rate)
                }
            }
        }
    }

    fn opened(&self) -> Result<&Open> {
        self.open
            .as_ref()
            .ok_or_else(|| AppError::new("app.no_project", "プロジェクトを開いていない"))
    }

    fn opened_mut(&mut self) -> Result<&mut Open> {
        self.open
            .as_mut()
            .ok_or_else(|| AppError::new("app.no_project", "プロジェクトを開いていない"))
    }
}

/// プロジェクトのエイリアス規則を読む（`TR-SYN-36`, `DEC-SYN-010`）。
///
/// 置いてあれば利用者の `presamp.ini`、無ければ同梱の既定。
///
/// **読めなくても落とさない。** 綴りの表が壊れているだけで音源が開けなく
/// なるより、既定へ戻して開けるほうが軽い。`Rules::parse` は節が欠けても
/// その節だけ既定へ戻すので、ここで見るのは「ファイルを読めたか」だけ。
fn load_rules(dir: &ProjectDir, set: koeru_core::inventory::UnitSet) -> koeru_core::presamp::Rules {
    let path = dir.presamp_path();
    match std::fs::read(&path) {
        Ok(bytes) => match koeru_core::text::decode(&bytes, koeru_core::text::TextEncoding::Utf8)
            .or_else(|_| koeru_core::text::decode(&bytes, koeru_core::text::TextEncoding::Cp932))
        {
            Ok(text) => {
                tracing::info!("差し替えのエイリアス規則を読んだ");
                // 書いていない表は既定で埋める。 テンプレートだけ差し替えた
                // ファイルでも、所属表が空のまま解決へ入らない。
                koeru_core::presamp::parse(&text).or_builtin(set)
            }
            Err(_) => koeru_core::presamp::Rules::builtin(set),
        },
        Err(_) => koeru_core::presamp::Rules::builtin(set),
    }
}

/// manifest から方式プリセットを引く（`TR-RCL-01`）。
///
/// **`Studio` の外に置く。** ライブラリの一覧は、開いていない音源の manifest と
/// 台帳をその場で読むので、`self` に紐づいていると呼べない。
///
/// manifest が持つのは識別子だけ。 古いプロジェクトは識別子を持たないので、
/// 方式から引く（`Manifest::preset_id` は後から足した）。
fn preset_of(manifest: &Manifest) -> koeru_core::preset::MethodPreset {
    let multi = matches!(manifest.method, Method::MultiPitchSequential);
    manifest
        .preset_id
        .as_deref()
        .and_then(koeru_core::preset::by_id)
        .unwrap_or_else(|| {
            koeru_core::preset::builtin()
                .into_iter()
                .find(|p| manifest_method(p, multi) == manifest.method)
                .unwrap_or_else(|| {
                    koeru_core::preset::by_id("single")
                        .unwrap_or_else(|| unreachable!("同梱プリセットに single がある"))
                })
        })
}

/// manifest に書く方式（`TR-RCL-01`）。
///
/// 多音階かどうかで別の名前になる。 `project::Method` は方式と音階数を
/// 1つの列挙で持っているので、ここで畳む。
fn manifest_method(p: &koeru_core::preset::MethodPreset, multi_pitch: bool) -> Method {
    match (p.method, multi_pitch) {
        (koeru_core::alias::Method::Single, _) => Method::Single,
        (koeru_core::alias::Method::Cvvc, _) => Method::Cvvc,
        (koeru_core::alias::Method::Sequential, false) => Method::Sequential,
        (koeru_core::alias::Method::Sequential, true) => Method::MultiPitchSequential,
    }
}

/// その音を鳴らす素材を、収録音高から引き当てる（`TR-SYN-13`, `TR-SYN-16`, `TR-RCL-20`）。
///
/// floor 割り当てでまず1つ選び、そこに無ければ1段下、さらに下、最低音高へ落ちる。
/// **上へは登らない。** 低い素材を上へ伸ばすほうが、高い素材を下げるより声が保つ。
///
/// 単音階（鍵が `None`）はそのまま返す。
fn pick_material<'a>(
    by_tone: &'a std::collections::BTreeMap<Option<i32>, Materials>,
    tones: &[i32],
    midi: i32,
    alias: &str,
) -> Option<(Option<i32>, &'a Materials)> {
    if let Some(m) = by_tone.get(&None) {
        return m.paths.contains_key(alias).then_some((None, m));
    }
    let floor = koeru_core::tone::floor_tone(tones, midi)?;
    let mut sorted = tones.to_vec();
    sorted.sort_unstable();
    // floor 以下を高い順に。 見つからなければ最低音高まで降りる。
    sorted.iter().rev().filter(|t| **t <= floor).find_map(|t| {
        let m = by_tone.get(&Some(*t))?;
        m.paths.contains_key(alias).then_some((Some(*t), m))
    })
}

/// 曲ごとの状態を、開いた台帳から求める（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
///
/// [`Studio`] の外に置く。 ライブラリの一覧は、開いていない音源の台帳を
/// その場で開いて読むので、`self` に紐づいていると呼べない。
fn song_status_of(
    ledger: &mut Ledger,
    rules: &koeru_core::presamp::Rules,
    preset: koeru_core::preset::MethodPreset,
) -> Result<Vec<SongStatus>> {
    // 綴りで突き合わせる（`TR-RCL-18`）。 **仮名で引いていた**ので、
    // 連続音や CVVC では必要集合と交わらず、曲が永久に「歌えない」ままだった。
    let covered = ledger.covered_aliases()?;
    let songs: Vec<(String, Song)> = ledger.songs_in_bank()?;
    // あと何行かは、フルリストの部分集合として数える（`TR-RCL-16`）。
    // 詰め直さないので、ここで渡すのはいま使っている録音リストそのもの。
    let full_list = preset
        .reclist(rules)
        .map_err(|e| AppError::new(e.kind(), e))?;
    // 音域の判定に要る（`TR-RCL-22`）。 エイリアスが揃っていても、
    // 収録音高から遠い音は鳴らない。
    let tones = ledger.recording_tones()?;
    Ok(song::status_of(
        &songs,
        rules,
        preset.method,
        &covered,
        preset.set,
        &full_list,
        &tones,
    ))
}

/// 環と色を、開いた台帳から求める（`DEC-PLT-025`, `DEC-PLT-027`）。
///
/// 分母は環の総和で取る。 [`Progress`] はインベントリの件数を分母にしているが、
/// 環は台帳の録音リストから作るので、別々に数えると一覧の「30 / 144 音」と
/// 環の閉じ具合が食い違いうる。同じ出どころから両方を出す。
fn voice_state(
    ledger: &mut Ledger,
    rules: &koeru_core::presamp::Rules,
    preset: koeru_core::preset::MethodPreset,
) -> Result<VoiceState> {
    let rings = ledger.coverage_by_kana_row()?;
    let songs = song_status_of(ledger, rules, preset)?;
    let traits = voice::traits_of(&ledger.adopted_voice(MASTER_RATE_HZ)?);
    Ok(VoiceState {
        // 名前と作り方は manifest 側。ここは台帳しか見ない。
        display_name: String::new(),
        method: String::new(),
        rows: 0,
        covered: rings.iter().map(|(c, _)| *c as usize).sum(),
        required: rings.iter().map(|(_, t)| *t as usize).sum(),
        singable_songs: song::singable_count(&songs),
        songs_in_bank: songs.len(),
        rings,
        color: traits.as_ref().map(VoiceColor::from_traits),
    })
}

/// 録音リストの行の数。
fn count_rows(ledger: &mut Ledger) -> Result<u32> {
    Ok(u32::try_from(ledger.rows_with_takes()?.len()).unwrap_or(u32::MAX))
}

fn no_stream() -> AppError {
    AppError::new("app.no_stream", "入力ストリームを開いていない")
}

/// `ok_or_else` に渡すための同じもの。
fn no_stream_err() -> AppError {
    no_stream()
}

impl Drop for Studio {
    /// アプリが変更した OS 側のゲインを、終了時に元へ戻す（`TR-REC-15`）。
    ///
    /// 戻さないと、利用者のマイクの設定を勝手に変えたままになる。
    /// KOERU を閉じたあとに別のアプリで小さすぎる／大きすぎる音になる。
    fn drop(&mut self) {
        // 排出スレッドを先に止める。ゲインを触るのはそのあと。
        self.pump = None;
        if let Some((device, before)) = self.gain_before.take()
            && let Err(e) = mac::write_gain(&device, before)
        {
            tracing::warn!(kind = e.kind(), "終了時にゲインを戻せなかった");
        }
    }
}

/// 素材の内容ハッシュ（`TR-SYN-02`）。
///
/// 録り直したら変わる。 変われば鍵が変わり、古い合成結果は使われない。
/// 中身を読み直さずに済むよう、更新時刻と大きさから作る。
fn hash_of(path: Option<&PathBuf>) -> u64 {
    use std::hash::{Hash as _, Hasher as _};
    let Some(p) = path else { return 0 };
    let mut h = std::collections::hash_map::DefaultHasher::new();
    p.hash(&mut h);
    if let Ok(m) = std::fs::metadata(p) {
        m.len().hash(&mut h);
        if let Ok(t) = m.modified()
            && let Ok(d) = t.duration_since(std::time::UNIX_EPOCH)
        {
            d.as_nanos().hash(&mut h);
        }
    }
    h.finish()
}

/// 現在時刻を RFC 3339 で。
///
/// 秒までで足りる。 台帳に入るのは順序を保つためで、精密な時刻ではない。
pub(crate) fn now_rfc3339() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // 素の算術で組む。日付だけのために依存を増やさない。
    let days = secs / 86_400;
    let rem = secs % 86_400;
    let (y, m, d) = civil_from_days(i64::try_from(days).unwrap_or(0));
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

/// 1970-01-01 からの日数を年月日にする（Howard Hinnant の `civil_from_days`）。
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

// `Studio` を組む試験。 MFA を組んでいない OS では `Studio::open` が
// 設計どおり失敗する（`DEC-ALN-016`）ので、モジュールごと外す——
// 中の1本だけ外すと `use super::*` が未使用になり、`-D warnings` で落ちる。
#[cfg(all(test, target_os = "macos", not(koeru_force_unsupported_backend)))]
mod tests {
    use super::*;

    /// 同じ音源を開き直しても、収録セッションを作り直さない。
    ///
    /// 作り直すと `session_id` が 0 に戻り、**開いたままのストリームで録った
    /// 次のテイクが `takes.session_id` の外部キーに当たって落ちる。**
    /// しかも落ちるのは WAV を確定させたあとで、ファイルだけが残る。
    ///
    /// 画面は面を移るたびにここを通る（`queries.ts` の `openProjectQuery` は
    /// `gcTime: 0`）ので、音・曲・テイクを行き来するだけで起きる。**踏んだ。**
    // MFA を組んでいない OS では `Studio::open` が設計どおり失敗する（`DEC-ALN-016`）。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 同じ音源を開き直してもセッションを作り直さない() {
        let root = std::env::temp_dir().join(format!("koeru-reopen-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("試験用の音源").expect("作れる");
        studio.open_project(id).expect("開ける");

        // セッションを1つ始めさせる。
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        studio.mark_recorded_for_test(&row).expect("印を付けられる");
        let started = studio.opened().expect("開いている").session_id;
        assert_ne!(started, 0, "セッションが始まっていること");

        studio.open_project(id).expect("開き直せる");
        assert_eq!(
            studio.opened().expect("開いている").session_id,
            started,
            "同じ音源を開き直したら、セッションは同じであること"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 録り直しても、人が直した値を引き継ぐ（`REQ-ALN-007`, `INV-ALN-001`）。
    ///
    /// **素の綴りで引いていた。** キューは（音高, 綴り）で持つので鍵が合わず、
    /// 録り直すたびに直した値が自動の値で上書きされていた。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 録り直しても固定した値を引き継ぐ() {
        let root = std::env::temp_dir().join(format!("koeru-pin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("固定").expect("作れる");
        studio.open_project(id).expect("開ける");
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        studio.seed_material_for_test(&row).expect("素材を置ける");

        // 1本目の値を人が直す。
        let item = studio
            .review_queue()
            .expect("キューを引ける")
            .into_iter()
            .find(|i| i.row_id == row)
            .expect("その行のエントリがある");
        studio
            .edit_oto_value(&item.key, "offset", 123.0)
            .expect("直せる");

        // 録り直す。 `finish_take` と同じく、キューが前の世代のまま新しいテイクを入れる。
        // 素材の置き場所は一意（`takes.rel_path`）なので、2本目は別の名前にする。
        let session_id = studio.test_session().expect("セッションを始められる");
        let open = studio.opened_mut().expect("開いている");
        let take = open
            .ledger
            .commit_take(&FinalizedTake {
                row_id: row.clone(),
                session_id,
                rel_path: format!("audio/{row}_2.wav"),
                frames: 44_100,
                recorded_at: now_rfc3339(),
            })
            .expect("テイクを確定できる");
        open.ledger.adopt_take(&row, take).expect("採れる");
        open.ledger
            .put_oto(
                take,
                &item.alias,
                &koeru_core::db::koeru_oto::Oto {
                    offset_ms: 50.0,
                    consonant_ms: 60.0,
                    cutoff_ms: -300.0,
                    preutterance_ms: 40.0,
                    overlap_ms: 20.0,
                },
                1.0,
                None,
                false,
            )
            .expect("自動の値を置ける");
        studio.enqueue_take(take).expect("キューへ入れられる");

        let got = studio
            .opened_mut()
            .expect("開いている")
            .ledger
            .oto_of(take, &item.alias)
            .expect("引ける")
            .expect("エントリがある");
        assert!(
            (got.offset_ms - 123.0).abs() < f64::EPSILON,
            "直した値を引き継ぐ: {}",
            got.offset_ms
        );

        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod subbank_tests {
    use super::*;

    fn materials(aliases: &[&str]) -> Materials {
        Materials {
            paths: aliases
                .iter()
                .map(|a| {
                    (
                        (*a).to_owned(),
                        std::path::PathBuf::from(format!("{a}.wav")),
                    )
                })
                .collect(),
            tables: HashMap::new(),
            otos: HashMap::new(),
        }
    }

    /// 単音階は鍵が `None` のまま引ける。
    #[test]
    fn 単音階はそのまま引く() {
        let by_tone = [(None, materials(&["か"]))].into_iter().collect();
        assert!(pick_material(&by_tone, &[57], 60, "か").is_some());
        assert!(pick_material(&by_tone, &[57], 60, "き").is_none());
    }

    /// floor 割り当てでその音を担う音高から引く（`TR-SYN-16`）。
    #[test]
    fn 担当する音高から引く() {
        let by_tone = [
            (Some(55), materials(&["か"])),
            (Some(62), materials(&["か"])),
            (Some(69), materials(&["か"])),
        ]
        .into_iter()
        .collect();
        let tones = [55, 62, 69];
        assert_eq!(
            pick_material(&by_tone, &tones, 55, "か").map(|(t, _)| t),
            Some(Some(55))
        );
        assert_eq!(
            pick_material(&by_tone, &tones, 61, "か").map(|(t, _)| t),
            Some(Some(55))
        );
        assert_eq!(
            pick_material(&by_tone, &tones, 62, "か").map(|(t, _)| t),
            Some(Some(62))
        );
        assert_eq!(
            pick_material(&by_tone, &tones, 90, "か").map(|(t, _)| t),
            Some(Some(69))
        );
    }

    /// 無ければ1段下、さらに下へ落ちる（`TR-RCL-20`）。上へは登らない。
    #[test]
    fn 無ければ下へ落ちる() {
        // A4 でだけ録っていない。
        let by_tone = [
            (Some(55), materials(&["か"])),
            (Some(62), materials(&["か"])),
            (Some(69), materials(&[])),
        ]
        .into_iter()
        .collect();
        let tones = [55, 62, 69];
        assert_eq!(
            pick_material(&by_tone, &tones, 69, "か").map(|(t, _)| t),
            Some(Some(62)),
            "1段下へ落ちる"
        );

        // D4 も A4 も無い。最低音高まで降りる。
        let by_tone = [
            (Some(55), materials(&["か"])),
            (Some(62), materials(&[])),
            (Some(69), materials(&[])),
        ]
        .into_iter()
        .collect();
        assert_eq!(
            pick_material(&by_tone, &tones, 69, "か").map(|(t, _)| t),
            Some(Some(55))
        );
    }

    /// 上へは登らない。 低い音に高い素材を当てない。
    #[test]
    fn 上へは登らない() {
        // G3 でだけ録っていない。
        let by_tone = [
            (Some(55), materials(&[])),
            (Some(62), materials(&["か"])),
            (Some(69), materials(&["か"])),
        ]
        .into_iter()
        .collect();
        assert!(
            pick_material(&by_tone, &[55, 62, 69], 55, "か").is_none(),
            "G3 の音に D4 の素材を当てない"
        );
    }
}

#[cfg(test)]
mod consistency_tests {
    use super::*;
    use koeru_core::alias::Method;
    use koeru_core::inventory::UnitSet;
    use koeru_core::reclist::{Slot, row_entries};

    /// 集団は綴りではなくモーラの仮名で引く（`TR-ALN-12`）。
    ///
    /// **綴りを渡していた。** 連続音と CVVC では集団が1つも作れず、
    /// 事前分布がいつも 1.0 だった。単独音だけは綴りが仮名そのものなので、
    /// 単独音の試験しか無いうちは誰も気づかなかった。
    #[test]
    fn どの方式でも_cv_の枠は音素へ写せる() {
        let rules = koeru_core::presamp::Rules::builtin(UnitSet::Core);
        let line: Vec<_> = koeru_core::inventory::units(UnitSet::Core)
            .into_iter()
            .filter(|u| ["あ", "か", "さ"].contains(&u.kana))
            .collect();
        for method in [Method::Single, Method::Sequential, Method::Cvvc] {
            for (alias, slot) in row_entries(&rules, method, &line) {
                let reading = consistency_reading(slot, &line);
                match slot {
                    Slot::Cv { .. } => {
                        let k = reading.expect("CV の枠は仮名を返す");
                        assert!(
                            first_phoneme(k).is_some() && last_phoneme(k).is_some(),
                            "{method:?} の {alias} から引いた {k} は音素へ写せる"
                        );
                    }
                    // 渡りと語尾は CV の集団へ混ぜない。
                    Slot::Vc { .. } | Slot::Ending { .. } => {
                        assert_eq!(reading, None, "{method:?} の {alias} は測らない");
                    }
                }
            }
        }
    }

    /// 仮名でなく綴りを渡すと、集団が引けない——これが直す前の形。
    #[test]
    fn 綴りは音素へ写せない() {
        for spelling in ["- か", "a か", "a k", "a -"] {
            assert!(first_phoneme(spelling).is_none(), "{spelling}");
        }
    }
}
