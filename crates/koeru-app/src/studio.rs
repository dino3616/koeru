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

use koeru_align::review::{EntryState, ReviewError, ReviewMode, ReviewQueue, Slot};
use koeru_align::{ini, ledger, reach, validate};
use koeru_audio::backend::current as mac;
use koeru_audio::wav::MASTER_RATE_HZ;
use koeru_audio::{Device, DeviceId, Session, wav};
use koeru_core::analysis::{TakeAnalysis, TakeMetrics};
use koeru_core::calibration::{self, Calibration, Outcome};
use koeru_core::channel::{self, Source};
#[cfg(any(test, feature = "test-hooks"))]
use koeru_core::db::FinalizedTake;
use koeru_core::db::{
    Answer, CaptureId, Commit, CommitRequest, Ledger, NewIntent, OperationId, Receipt,
    SessionSnapshot, koeru_oto,
};
use koeru_core::fingerprint::Digest;
use koeru_core::frq;
use koeru_core::guide::{self, GuideSpec, GuideTempo};
use koeru_core::leak::{self, LeakCheck};
use koeru_core::oto::Oto;
use koeru_core::project::{CoverageState, HandoffState, Library, Manifest, Method, ProjectDir};
use koeru_core::song::{self, Song, SongStatus};
use koeru_core::text::TextEncoding;
use koeru_core::ust;
use koeru_core::voice::{self, VoiceTint};
use koeru_core::waveform;
// `Outcome` は収録の校正（`calibration::Outcome`）が名乗っているので、確定の状態は
// `koeru_failure::Outcome` と書く。
use koeru_failure::{Class, Failure};
use koeru_model::id::SubbankId;
use koeru_model::materials::{Role, UseContext};
use koeru_model::retake::{self, Anchor, Onsets, Pins};
// 背後の仕事（`DEC-PLT-047`）。 `Receipt` は台帳の受領証（`koeru_core::db::Receipt`）が
// 名乗っているので、仕事の受領証は `jobs::Receipt` と書く。
use koeru_runtime::importing;
use koeru_runtime::jobs::{
    self, CoalesceKey, ComputeError, JobConfig, JobError, JobFailure, JobId, JobKind, JobRuntime,
    JobSpec, OverflowPolicy, Priority, PublishError,
};
use koeru_runtime::time::now_rfc3339;
use koeru_runtime::{ProjectLease, ProjectRuntime, WriterGuard};
use koeru_synth::f0;
use koeru_synth::resampler::{FrequencyTable, RenderRequest, render};

use uuid::Uuid;

use crate::capture_lease::{CaptureLease, TakeGaps, TakeGuard};
use crate::error::{AppError, Result};
use crate::latency::ms_u32;
use crate::latency::{self, Case, Observed};
use crate::packaging;
use crate::playback_lease::PlaybackLease;
use crate::preview::{self, PhraseCache, Sink, WavSamples};
use crate::pump::{PREROLL_MS, Pump};
use crate::review::slot_of;
use crate::storage;
use crate::take_analysis::{
    self, ANALYSIS_DEADLINE, Populations, ReEstimateInput, RowReads, Setup, Step, Stop,
    TakeAnalysisInput, TakeAnalysisOutput, TakeFacts, consistency_reading,
};

mod distribution;
pub use distribution::{
    DistributionAnswer, DistributionDenied, DistributionSummary, PackagedSubbankChange,
    PackagedSubbankSummary, ReviseDistribution,
};
mod subbank;
pub use subbank::{
    AddSubbank, ReviseSubbank, SubbankAnswer, SubbankDenied, SubbankRemoval, SubbankSummary,
    Violation,
};

/// 単独音の収録音高。A3 を既定にする（`TR-RCL` の音階既定）。
pub const DEFAULT_TONE_MIDI: i32 = 57;

/// 試唱の前処理をまとめる鍵（`OverflowPolicy::Coalesce`）。 待っている前処理は1つだけ。
const PRERENDER_KEY: &str = "prerender_songs";

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

/// 見失っていたデバイスが戻ったあとの、入力生死の再確認にかける時間（ミリ秒）。
///
/// 画面が使っている既定値と揃える（`ui/src/lib/levels.ts` の `PROBE_MS`）。
/// 自動では録音を始めない（`REQ-REC-109`）ので、ここは「つながっているか」を
/// もう一度確かめるだけ。
const DEVICE_RETURN_PROBE_MS: u64 = 400;

/// 主因ラベルを出しはじめる成分の値（`TR-ALN-26` (3)）。
///
/// 自動確定と確認キューの切り分けには使わない。 そちらは合計所要時間の上限が
/// 決める（`TR-ALN-25` の「確信度の閾値は絶対値で固定せず、確認キューの運用で切る」）。
/// ここが決めるのは「どの成分を主因として名指すか」だけで、
/// 何件が確認に回るかには効かない。
const CAUSE_THRESHOLD: f64 = 0.5;

/// 分岐不一致の主因（`TR-ALN-16`, `DEC-ALN-018`）。 送信してよい固定文字列。
///
/// 確信度の成分より先に名指す。 キューの先頭に並べた理由がこれなので
/// （`ReviewQueue::queued`）、成分の主因を出すと並べた理由と食い違う。
const BRANCH_MISMATCH_CAUSE: &str = "branch.mismatch";

/// 取り込めない曲の理由（`importing::Diagnostic`）を、画面へ出す失敗に写す（`TR-RCL-12`）。
/// 取り込みと下見が同じものを通る。 何を取り込んでよいかは `koeru_runtime::importing` が決める。
///
/// 伝えるのは何番目の音符かだけ。 歌詞そのものは載せない——この失敗は
/// トレースにも残る（`AGENTS.md` #3）。
fn diagnostic_error(d: importing::Diagnostic) -> AppError {
    match d.problem {
        importing::Problem::NoteOutOfRange { note } => AppError::new(
            "song.note_out_of_range",
            Class::InvalidInput,
            format!(
                "{} 番目のノートの音高が、MIDI の範囲（0〜127）の外にある",
                note + 1
            ),
        ),
        importing::Problem::UnreadableLyrics => AppError::new(
            "app.unreadable_lyrics",
            Class::InvalidInput,
            "歌詞を読めないノートがある。仮名で書かれた UST / USTX を取り込む",
        ),
        importing::Problem::NoteNotOneMora { note } => AppError::new(
            "song.note_not_one_mora",
            Class::InvalidInput,
            format!(
                "{} 番目のノートに、1音ぶんではない歌詞が入っている。1ノートに1音（「きゃ」「ー」「っ」は1音）で書かれた UST / USTX を取り込む",
                note + 1
            ),
        ),
    }
}

/// 取り込みを断った理由を、画面へ出す失敗に写す。 台帳は変わっていない。
fn rejection_error(r: importing::Rejection) -> AppError {
    match r {
        importing::Rejection::TitleCount => AppError::new(
            "song.title_count",
            Class::InvalidInput,
            "題の数が曲の数と合わない",
        ),
        importing::Rejection::EmptyTitle => empty_title(),
        importing::Rejection::Unimportable(d) => diagnostic_error(d),
    }
}

/// キューの遷移が通らなかったことを、境界の失敗へ畳む。
///
/// 異常ではない。 状態機械の遷移条件を満たしていないだけなので、
/// 画面は押せない的として出せばよい。
fn review_error(e: ReviewError) -> AppError {
    AppError::from_failure(e)
}

/// 指した曲がバンクに無い。 読んだ一覧が古い。
///
/// `app.no_song` と `app.unknown_song` の2つを名乗っていたのを1つにした。
fn no_song() -> AppError {
    AppError::new("app.unknown_song", Class::Conflict, "その曲はバンクに無い")
}

fn empty_title() -> AppError {
    AppError::new("app.empty_title", Class::InvalidInput, "題が空")
}

/// 開いている音源が無い。 画面が開く前に読みに来た、または閉じたあとに届いた。
fn already_recording() -> AppError {
    AppError::new(
        "app.already_recording",
        Class::Rejected,
        "収録中は別の声を開けない。止めてから移ってほしい",
    )
}

fn no_project() -> AppError {
    AppError::new(
        "app.no_project",
        Class::Rejected,
        "プロジェクトを開いていない",
    )
}

fn non_nfc_names() -> AppError {
    AppError::new(
        "package.non_nfc_names",
        Class::Rejected,
        "受け取る側で見つからなくなる名前が残っている",
    )
}

/// 先頭の配布物へ書こうとした設定が、台帳に断られた。 画面が配布物を意識しない旧い口のための失敗。
fn refused_package_settings(r: koeru_core::db::shelf::DistributionRefusal) -> AppError {
    use koeru_core::db::shelf::DistributionRefusal as R;
    match r {
        R::Conflict { .. } => AppError::new(
            "package.archive_name_taken",
            Class::Conflict,
            "その配布名は別の配布物が使っている",
        ),
        R::VersionTaken => AppError::new(
            "package.version_taken",
            Class::Rejected,
            "その版は、この配布物でもう書き出している",
        ),
        R::UnknownDistribution | R::UnknownSubbank | R::LastOne => AppError::new(
            "package.unexpected_refusal",
            Class::Internal,
            "配布の設定が想定していない理由で断られた",
        ),
    }
}

fn unknown_slot() -> AppError {
    AppError::new(
        "review.unknown_slot",
        Class::InvalidInput,
        "知らない値の名前",
    )
}

/// 指したテイクが台帳に無い。 台帳の失敗と同じ code を名乗る。
fn unknown_take() -> AppError {
    koeru_core::db::LedgerError::UnknownTake.into()
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
///
/// 台帳の書き手は [`ProjectRuntime`] が持つ（`DEC-PLT-034`、`DEC-PLT-044`）。
/// ここが持つのは書き手の側の状態——確認キュー・綴りの表・収録セッション——と、
/// **プロジェクトの寿命を持つ写し**（[`ProjectCaches`]）。 `Open` が捨てられれば
/// 写しも一緒に捨てられるので、別のプロジェクトへ移っても前のものが残らない。
#[derive(Debug)]
struct Open {
    /// 書き手・読み手プール・貸与を持つ実行時。 `Arc` で配り、`AppState` が
    /// 状態ロックの外から読めるようにする（[`Studio::runtime`]）。
    runtime: Arc<ProjectRuntime>,
    /// この `Open` が開いたときの貸与。
    ///
    /// 実行時から毎回読み直さず、ここに持つ。 `Studio` が書き手へ添える貸与は
    /// 「自分が開いたもの」で、実行時が別の貸与へ入れ替わっていれば
    /// `runtime.lease_expired` で拾える。
    lease: ProjectLease,
    session_id: i32,
    /// エイリアスの綴りの表（`TR-SYN-36`, `DEC-SYN-010`）。
    ///
    /// 台帳の写しから組む（`DEC-SYN-013`）。 作るときに選んだ表で、あとから変わらない。
    /// 録音リストの生成・カバレッジ判定・試唱・書き出しが、**全部ここを通る**
    /// ——片方だけが差し替わると、歌える判定と実際に鳴る音がずれる。
    rules: koeru_core::presamp::Rules,
    /// 確認キュー（`TR-ALN-25`、`align-review.fsl`）。
    ///
    /// 開いている間だけ持つ写し。 正本は台帳で、ここは遷移の可否を判定する係
    /// （`crate::review`）。
    review: ReviewQueue,
    /// エイリアス → そのエントリを持つ採用テイク。書き戻す先。
    review_takes: HashMap<String, i32>,
    /// 開いたときに書き換えられていた `presamp.ini` を残したファイル名（`DEC-SYN-013`）。
    ///
    /// 本人が閉じるまで持つ。 画面は経路を移るたびに開き直すが、同じ音源なら
    /// ここを通らないので、1度きりの応答で返すと移った先で消える。
    presamp_restored: Option<String>,
    caches: ProjectCaches,
}

impl Open {
    /// 台帳の書き手。 **文の終わりで手放す。**
    ///
    /// 戻り値は `std::sync::Mutex` の guard そのもので、再入できない
    /// （[`ProjectRuntime::writer`]）。 別の `Studio` のメソッドを呼ぶあいだ、
    /// アライメント・合成・DSP・ファイルのコピー・待ちのあいだは持たない。
    /// `if let` / `match` / `for` の式に直接書くと本体のあいだ生き続けるので、
    /// 先に `let` で受けて値だけ持ち出す。
    fn writer(&self) -> Result<WriterGuard<'_>> {
        Ok(self.writer_raw()?)
    }

    /// [`Self::writer`] の、失敗を `AppError` へ畳まない形。 失敗をその場で記録する
    /// だけの経路（`Result` を返せない後始末）が使う。
    fn writer_raw(&self) -> std::result::Result<WriterGuard<'_>, koeru_runtime::RuntimeError> {
        self.runtime.writer(&self.lease)
    }

    fn dir(&self) -> &ProjectDir {
        self.runtime.dir()
    }
}

/// プロジェクトの寿命を持つ写し。
///
/// **`Studio` 直下に置かない。** 以前は `Studio` が持っていて、別のプロジェクトへ
/// 移っても捨てなかった。 テイクの ID は台帳ごとに 1 から振るので、A のテイク N の
/// 波形の段が、B のテイク N の波形として返った。
#[derive(Debug)]
struct ProjectCaches {
    /// フレーズ単位の合成結果（`TR-SYN-02`, `TR-SYN-25`）。
    ///
    /// 素材が変われば鍵が変わるので、同じプロジェクトの中では明示的に捨てなくても
    /// 古い結果は使われない（`TR-SYN-26`）。 プロジェクトが変われば、ここごと捨てる。
    song_cache: Arc<Mutex<PhraseCache>>,
    /// 集めた F0 系列。話者音域を見るため（`TR-SYN-22`）。
    observed_f0: Vec<Vec<f64>>,
    /// 話者音域から決めた探索下限。まだ分からなければ `None`。
    f0_floor: Option<f64>,
    /// テイクごとの波形の段（`TR-PLT-04`）。上限を置いて持ち回す。
    mipmaps: HashMap<i32, (Arc<waveform::Mipmap>, u32)>,
}

impl ProjectCaches {
    fn new() -> Self {
        Self {
            song_cache: Arc::new(Mutex::new(PhraseCache::new())),
            observed_f0: Vec::new(),
            f0_floor: None,
            mipmaps: HashMap::new(),
        }
    }
}

/// 録音している1回。 台帳の予定と同じ識別子を持つ（`DEC-REC-010`）。
#[derive(Debug, Clone)]
struct ActiveCapture {
    row_id: String,
    capture: CaptureId,
    /// このテイクの基準（`TR-REC-07`）。`Studio::xrun_baseline` を置き換える（T06c）。
    guard: TakeGuard,
}

/// 録音を確定する要求の結果（`DEC-PLT-035`、`DEC-PLT-038`）。
#[derive(Debug)]
pub enum Finish {
    /// この要求でテイクを確定した。
    Committed {
        result: Box<TakeResult>,
        receipt: Receipt,
    },
    /// 確定せずに答えた。 送り直し・識別子の使い回し・もう閉じた録音で、テイクは増えていない。
    Answered(Answer),
    /// 落ちたあとに残った録音（孤児・書きかけ）。 本人が採るか捨てるまで確定しない
    /// （`TR-REC-28`）。
    RecoveryRequired,
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
    /// タイムスタンプが飛んだ回数（`TR-REC-07`）。
    pub discontinuities: usize,
    /// リングが満杯で捨てたサンプル数（`TR-REC-07`）。
    pub dropped: usize,
    /// レンダの失敗回数（`TR-REC-07`）。
    pub render_errors: usize,
    /// 取りこぼし・不連続・レンダの失敗のどれかが増えたので自動的に無効にした
    /// （`TR-REC-07`）。 同じフレーズがもう一度出てくる。
    pub invalidated: bool,
    /// 計測値（`TR-REC-16`）。測るだけで、判定も指摘もしない。
    pub metrics: TakeMetrics,
    /// 押した瞬間より前から何ミリ秒ぶん遡れたか（`TR-REC-19`）。
    pub preroll_ms: f64,
    /// 確定のあとで落ちた工程（解析・`.frq`・原音設定・確認キュー）。
    ///
    /// **テイクは確定している**（`outcome` は `committed`）。 録り直しを求めない。
    /// 作れなかった派生物は、推定し直しで作り直せる。
    pub followup: Option<AppError>,
}

/// [`Studio::admit_take_analysis`] が受理した結果。
#[derive(Debug)]
struct DerivedTake {
    peak: f32,
    thumbnail: Vec<u8>,
    metrics: TakeMetrics,
    oto: Option<Oto>,
    confidence: Option<f64>,
    /// 最初に落ちた工程の失敗。 そこから先の派生物は無い。
    failure: Option<AppError>,
}

impl DerivedTake {
    /// 何も置いていない状態。 画面へ返す値だけは計算の出力から持つ。
    fn unplaced_from(analysis: &TakeAnalysis, measured: TakeMetrics) -> Self {
        Self {
            peak: analysis.peak,
            thumbnail: analysis.thumbnail.clone(),
            // 境界が分かる前の測り方。 揃えられれば置くときに測り直した値へ替わる。
            metrics: measured,
            oto: None,
            confidence: None,
            failure: None,
        }
    }

    /// 計算の出力はあるが、受理を始める前に断った（貸与が生きていない）。
    fn unplaced(output: &TakeAnalysisOutput) -> Self {
        Self::unplaced_from(&output.analysis, output.measured)
    }

    /// 計算の出力が無い（取り消し・期限切れ・公開の拒否・上限・計算の panic）。
    ///
    /// 画面へ返す値は手元の標本から作る。 波形の要約と最大値は F0 を要らない。 計測は境界が
    /// 分からないので、境界なしの測り方（アライメントが揃わなかったときと同じ）。
    fn from_samples(samples: &[f32], rate: u32, failure: AppError) -> Self {
        Self {
            peak: koeru_core::analysis::peak(samples),
            thumbnail: koeru_core::analysis::thumbnail(
                samples,
                koeru_core::analysis::THUMBNAIL_BUCKETS,
            ),
            metrics: TakeMetrics::measure(samples, rate, None, None),
            oto: None,
            confidence: None,
            failure: Some(failure),
        }
    }
}

/// 受理が途中で落ちたときの失敗（`JobRuntime::publish` の `admit` の `Err`）。
///
/// **置けたところまでの結果を一緒に持ち出す。** 公開の `Err` は失敗しか返せないが、画面へ
/// 返す値（波形の要約・計測）と置けた oto は、失敗しても返す（以前と同じ）。 仕事の状態は
/// `Failed`——失敗の code と分類は落ちた工程のもの。
struct Unfinished {
    /// `failure` は空。 取り出して [`Self::failure`] に入れてある。
    derived: Box<DerivedTake>,
    failure: AppError,
}

// 画面へ返す値（波形・計測）は利用者のデータなので、`Debug` には code だけを出す。
impl std::fmt::Debug for Unfinished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Unfinished")
            .field("code", &self.failure.code)
            .finish_non_exhaustive()
    }
}

impl std::fmt::Display for Unfinished {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.failure, f)
    }
}

impl std::error::Error for Unfinished {}

impl Failure for Unfinished {
    fn code(&self) -> &'static str {
        self.failure.code
    }

    fn class(&self) -> Class {
        self.failure.class.into()
    }
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
    /// 台帳から読めた到達度。読めなければ `None`。 `migration` が `Current`
    /// でないときは読まない——一覧は読み取り専用で開き、台帳を移行しない。
    pub state: Option<VoiceState>,
    /// 台帳の migration の状態（`DEC-PLT-041`）。
    pub migration: LibraryMigrationState,
}

/// 一覧に出す、台帳の migration の状態。 読み取り専用で開いて分かる範囲だけ。
///
/// 一覧はここで移行しない（`open_project` が移行する）。 `Current` のときだけ
/// [`LibraryEntry::state`] を読む。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LibraryMigrationState {
    /// 今の版（または台帳がまだ無い新規）。
    Current,
    /// バイナリの知っている版のうち、まだ当たっていないものがある。 開けば移行する。
    Pending,
    /// バイナリの知らない版で作られている。 このビルドでは開けない。
    Newer,
    /// 前回の移行が失敗した記録が残っている。 `project.db` は移行前のまま。
    Failed { code: String },
    /// 台帳の状態を読めなかった（開けない、壊れている）。
    Unknown,
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
    pub color: Option<VoiceTint>,
}

/// 縦切りの本体。
#[derive(Debug)]
pub struct Studio {
    library: Library,
    /// 起動時にライブラリの置き場所について分かったこと（`DEC-PKG-016`）。
    ///
    /// `open` は `library_root` だけを受け取り、この欄の既定は「まだ調べていない」。
    /// 呼び出し側（`crate::run`）が `library_root` を決める過程で既に調べているので、
    /// `open` が返した直後に `pub(crate)` の可視性で直接書き込む。 `open` の引数も
    /// メソッドも増やさない——studio.rs は他の変更と並行して触られるので、
    /// ここに触れる面を1つに絞る。
    pub(crate) boot: crate::LibraryBoot,
    /// 使うアライナ（`TR-ALN-03`, `DEC-ALN-008`）。
    ///
    /// MFA のモデルが読めれば MFA、読めなければ退避経路。
    /// 起動時に1度だけ選ぶ——テイクごとに 96MiB を読み直さない（`TGT-ALN-004`）。
    aligner: crate::align::Chosen,
    open: Option<Open>,
    /// キャプチャストリームと排出スレッド（`TR-REC-19`）。収録画面にいる間ずっと開いている。
    ///
    /// 落とす順序（Pump → Capture）は `CaptureLease` の `Drop` が型で守る
    /// （`crate::capture_lease`。T06c）。
    lease: Option<CaptureLease>,
    session: Session,
    /// 録音中の1回。
    recording: Option<ActiveCapture>,
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
    /// 単一の出力枠（`DEC-REC-011`）。 音高提示・基準音・試唱・テイクの再生・
    /// 曲の試唱が、これを取り合う。 新しい出力を始める前に必ず前を止める。
    output: PlaybackLease,
    /// 背後で回す仕事（`TR-SYN-04`, `TR-SYN-34`、`DEC-PLT-047`）。
    ///
    /// 録音入力とは別のスレッド。 録音のコールバックを妨げない。
    /// **アプリの寿命で1つ。 `ProjectRuntime` の中には置かない**——project を移っても
    /// 作り直さず、貸与ごとに仕事を取り消す（`Self::release_project`）。
    /// `AppState` が写しを持ち、状態ロックの外から待ち数を読む（[`Self::jobs`]）。
    jobs: Arc<JobRuntime>,
    /// いちばん新しく積んだ試唱の前処理（[`Self::prerender_songs`]）。
    /// [`Self::stop_preview`] が取り消す（`TR-SYN-27`）。 解析は取り消さない。
    prerender: Option<JobId>,
    /// テスト用。 解析の期限を短くして、期限切れを実機の時間なしで作る。
    #[cfg(any(test, feature = "test-hooks"))]
    test_analysis_deadline: Option<std::time::Duration>,
    /// 試唱レイテンシの実測（`TR-SYN-33`）。
    ///
    /// 押してから鳴るまでを場面ごとに溜める。 目標と比べられるようにする。
    observed: HashMap<Case, Observed>,
    /// この回に試唱を押したことがあるか。初回かどうかの判定（`TR-SYN-33`）。
    ever_previewed: bool,
    /// テスト用。 実機無しで `check_device` の生死判定を差し込む
    /// （`crate::studio::tests`、`tests/device_loss.rs`）。
    ///
    /// 本番の経路は `mac::is_alive` を呼ぶ1つだけ——ここは答えを差し替える
    /// だけで、分岐そのものは増やさない。
    #[cfg(any(test, feature = "test-hooks"))]
    test_device_alive: Option<bool>,
}

/// 採用テイクから集めた素材。
#[derive(Default)]
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

/// 確定したテイクの音高の記録（[`Studio::take_pitch`]）。
#[derive(Debug, Clone, PartialEq)]
pub struct TakePitch {
    /// 格子の間隔（マスターの標本数）。
    pub hop: u32,
    /// 格子ごとの F0（Hz）。 テイクの全長を覆う。 無声・無音は [`koeru_core::frq::UNVOICED`]。
    pub f0: Vec<f64>,
}

/// ガイドのテンポを変えた結果（`DEC-REC-012`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuideTempoChange {
    /// 変えた。 値が前と同じなら何も書いていない。
    Set(GuideTempo),
    /// 録っている途中なので変えていない。 そのテイクは録り始めたセッションに属する。
    Recording,
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

impl Studio {
    /// ライブラリを開く。無ければ作る。
    #[tracing::instrument(skip(library_root))]
    pub fn open(library_root: PathBuf) -> Result<Self> {
        Ok(Self {
            library: Library::open(library_root)?,
            boot: crate::LibraryBoot::default(),
            // 起動時に1度だけ選ぶ（`TGT-ALN-004`。テイクごとに 96MiB を読み直さない）。
            // モデルが無いのはビルドの失敗（`DEC-ALN-016`）。ここで止める。
            aligner: crate::align::Chosen::detect()?,
            open: None,
            lease: None,
            session: Session::new(),
            recording: None,
            guide_offset_at_start: None,
            device: None,
            gain_before: None,
            leak: None,
            may_mix: false,
            output: PlaybackLease::new(),
            // 作業スレッドを起こせなければ回復できない（`JobRuntime::start`）。
            jobs: JobRuntime::start(JobConfig::default()),
            prerender: None,
            #[cfg(any(test, feature = "test-hooks"))]
            test_analysis_deadline: None,
            observed: HashMap::new(),
            ever_previewed: false,
            #[cfg(any(test, feature = "test-hooks"))]
            test_device_alive: None,
        })
    }

    /// 起動時にライブラリの置き場所について分かったこと（`DEC-PKG-016`）。
    ///
    /// 画面への表示はまだ持たない（表示は T09 の notices）。 ここは検出と結果の
    /// 保持まで。
    #[must_use]
    pub fn boot(&self) -> crate::LibraryBoot {
        self.boot
    }

    /// ライブラリの中身。manifest が読めないものも落とさず返す。
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self, display_name))]
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
    // 画面から来た識別子はスパンに載せない。 固定の語彙のつもりでも、検査する前に載る。
    #[tracing::instrument(skip(self, display_name, preset_id, tones), fields(tones = tones.len()))]
    pub fn create_project_with(
        &mut self,
        display_name: &str,
        preset_id: &str,
        tones: &[i32],
    ) -> Result<Uuid> {
        self.create_project_with_presamp(display_name, preset_id, tones, None)
    }

    /// 綴りの表を選んでプロジェクトを作る（`TR-SYN-36`, `DEC-SYN-013`）。
    ///
    /// `presamp` は本人が選んだ `presamp.ini` の中身。 `None` なら同梱の既定。
    /// 選んだ表はここで固定し、台帳に写しを持つ——あとから変える道は無い。
    ///
    /// **作ったあとでフォルダへ置かせていた。** 台帳は作った瞬間に既定の表で
    /// 綴りを書くので、あとから置いた表は台帳と噛み合わず、差し替えた綴りは
    /// 一度も効かなかった。
    ///
    /// # Errors
    ///
    /// 表を読めない、プリセットが無い、リストを生成できない、台帳を書けないとき。
    #[tracing::instrument(
        skip(self, display_name, preset_id, tones, presamp),
        fields(tones = tones.len(), presamp = presamp.is_some())
    )]
    pub fn create_project_with_presamp(
        &mut self,
        display_name: &str,
        preset_id: &str,
        tones: &[i32],
        presamp: Option<&[u8]>,
    ) -> Result<Uuid> {
        // 画面から来た識別子を文言へ差し込まない（`DEC-PLT-038`）。
        let preset = koeru_core::preset::by_id(preset_id).ok_or_else(|| {
            AppError::new(
                "preset.unknown",
                Class::InvalidInput,
                "知らない方式プリセット",
            )
        })?;
        // 本数も音高も本人が決める（`TR-RCL-01`）。弾くのは鳴らせないものだけ。
        let tones = koeru_core::tone::normalize(tones)?;
        // 綴りの表（`TR-SYN-36`）。 書いていない節は既定で埋める——テンプレート
        // だけ差し替えた表でも、所属表が空のまま録音リストを作らない。
        let rules = match presamp {
            None => koeru_core::presamp::Rules::builtin(preset.set),
            Some(bytes) => {
                let text = koeru_core::text::decode(bytes, koeru_core::text::TextEncoding::Utf8)
                    .or_else(|_| {
                        koeru_core::text::decode(bytes, koeru_core::text::TextEncoding::Cp932)
                    })
                    .map_err(|_| {
                        AppError::new(
                            "presamp.unreadable",
                            Class::InvalidInput,
                            "presamp.ini を読めない。UTF-8 か Shift_JIS で保存してほしい",
                        )
                    })?;
                koeru_core::presamp::parse(&text).or_builtin(preset.set)
            }
        };
        // 写しは表そのものではなく、書き直した形で持つ。 開いたときに突き合わせる
        // 控え（`ProjectDir::restore_presamp`）と同じ文字列になる。改行は配布する
        // `presamp.ini` と揃える——控えを開いた人が見比べられる。
        let snapshot = koeru_core::presamp::write(&rules, koeru_package::profile::NEWLINE);
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
        ledger.put_presamp_snapshot(&snapshot)?;
        dir.write_presamp(&snapshot)?;
        ledger.install_reclist_for_tones(&list, &rules, preset.method, Some(preset.id), &tones)?;

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
    /// 音源ごとに台帳を**読み取り専用で**開く（`DEC-PLT-041`）。 [`Self::projects`] は
    /// manifest しか読まないので、到達度も環も出せない——名前と数字の行になる。
    /// 開いている音源の台帳とは別に、ここで一時的に開いて読み、閉じる。
    ///
    /// **一覧では移行しない。** 台帳の migration の状態を [`LibraryEntry::migration`]
    /// に出すだけで、`Current` のときだけ今までどおり [`LibraryEntry::state`] を読む。
    /// 移行が要る台帳を一覧を出すだけで書き換えると、一覧を開いただけで
    /// 全プロジェクトが黙って移行されることになる。移行するのは `open_project`。
    ///
    /// 読めない音源も落とさない。 manifest が壊れていても席は残す
    /// （`crate::studio::Studio::projects` と同じ扱い）。
    ///
    /// # Errors
    ///
    /// ライブラリのディレクトリを読めないとき。個々の音源の失敗では返らない。
    #[tracing::instrument(skip(self))]
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
                let migration = library_migration_state(&dir);
                // 今の版のときだけ、開いていない音源でも綴りはその音源のものを使う
                // （`TR-SYN-36`）。 台帳の写しから読む（`DEC-SYN-013`）。 フォルダの控えは
                // 読まない——書き換えられたままの表で数えると、開いたときと数が変わる。
                let state = matches!(migration, LibraryMigrationState::Current)
                    .then(|| Ledger::open(dir.db_path()).ok())
                    .flatten()
                    .and_then(|mut l| {
                        let rules = rules_of(&mut l, preset.set).ok()?;
                        voice_state(&mut l, &rules, preset).ok()
                    });
                LibraryEntry {
                    id: dir.id(),
                    manifest,
                    state,
                    migration,
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
    #[tracing::instrument(skip(self, display_name))]
    pub fn rename_project(&mut self, id: Uuid, display_name: &str) -> Result<()> {
        let name = display_name.trim();
        if name.is_empty() {
            return Err(AppError::new(
                "app.empty_name",
                Class::InvalidInput,
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
    #[tracing::instrument(skip(self))]
    pub fn voice_state(&mut self) -> Result<VoiceState> {
        let manifest = self.opened()?.dir().read_manifest()?;
        let preset = preset_of(&manifest);
        let open = self.opened_mut()?;
        let rules = open.rules.clone();
        let mut state = voice_state(&mut *open.writer()?, &rules, preset)?;
        state.display_name = manifest.display_name;
        state.method = manifest.method.as_str().to_owned();
        state.rows = count_rows(&mut *open.writer()?)?;
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
    /// 「書きかけを捨てない」）。台帳に載らない WAV が孤児として残る。
    ///
    /// 開くときに、落ちたあとに残ったものを見て回る（`koeru_core::capture::verify`）。
    /// 孤児は予定と突き合わせて [`Self::capture_report`] に並べるだけで、採りも消しもしない。
    ///
    /// **移行はここで走らせる。** 一覧（[`Self::library`]）は読み取り専用で状態を
    /// 出すだけで、台帳を書き換えない（`DEC-PLT-041`）。 移行できない・新しい版の
    /// 台帳は、型のついた失敗で断って開かない。
    ///
    /// **台帳を開く・移行する・検証する部分は [`ProjectRuntime::open`] が持つ**
    /// （`DEC-PLT-034`）。 ここは、その書き手を使う綴りの表の確認と確認キューの
    /// 組み直しだけを足す。
    ///
    /// **別の音源へ移るときは、次が開けると分かってから前を閉じる。** 失敗しうる
    /// こと（一覧の照会、移行、検証、綴りの表、確認キュー）を全部先に済ませ、
    /// 開けたときだけ前の音源を手放す。 開けなかったら前の音源は何も変わらず、
    /// ストリームも開いたまま残る。 別々の台帳なので、2つの実行時は一瞬だけ並ぶ。
    /// 前を手放すときは、前の実行時を `close` して貸与を失効させ（それを握っていた
    /// 呼び出し側、`AppState` の枠や将来の GraphQL の resolver は
    /// `runtime.lease_expired` を受け取る）、背後の仕事と再生も止める
    /// （[`Self::stop_preview`]）。 前の音源のための仕事が、移った先の資源を
    /// 触らないようにする。
    #[tracing::instrument(skip(self))]
    pub fn open_project(&mut self, id: Uuid) -> Result<()> {
        if let Some(open) = &self.open {
            if open.dir().id() == id {
                return Ok(());
            }
            if self.recording.is_some() {
                return Err(already_recording());
            }
        }
        let dir = self.library.open_project(id)?;
        let switching = self.open.is_some();
        let runtime = ProjectRuntime::open(dir)?;
        match Self::build_open(&runtime) {
            Ok(open) => {
                // ここまで失敗しうるものは全部済んでいる。 前の音源を手放す。
                // 閉じられなくても（もう閉じていた）、次は開けているので進む。
                if let Err(e) = self.release_project() {
                    e.record("project.switch_close");
                }
                self.open = Some(open);
                tracing::info!(switching, "プロジェクトを開いた");
                Ok(())
            }
            Err(e) => {
                // 開けなかった。 貸与を振ったまま放り出さない。
                if let Err(close) = runtime.close(runtime.lease()) {
                    koeru_failure::record_failure(
                        &close,
                        koeru_failure::Outcome::NotCommitted,
                        "project.open_rollback",
                    );
                }
                Err(e)
            }
        }
    }

    /// 開いたばかりの実行時から、書き手の側の状態を組む。
    ///
    /// 書き手は文ごとに取って手放す（`Open::writer` の doc）。 ファイルを触る
    /// `restore_presamp` は書き手を握らずに行う。
    fn build_open(runtime: &Arc<ProjectRuntime>) -> Result<Open> {
        let lease = *runtime.lease();
        let dir = runtime.dir();
        // 確認キューは開くときに組み直す（`crate::review`）。
        // 遷移をやり直すのではなく、書いてあった状態をそのまま載せる。
        let (review, review_takes) = crate::review::load(&mut *runtime.writer(&lease)?)?;
        let set = preset_of(&dir.read_manifest()?).set;
        // 綴りの表は台帳の写し（`DEC-SYN-013`）。 写しを持たない古いプロジェクトは、
        // 作ったときの台帳が既定の表で書かれているので、既定を写しにする。
        let stored = runtime.writer(&lease)?.presamp_snapshot()?;
        let snapshot = if let Some(t) = stored {
            t
        } else {
            let t = koeru_core::presamp::write(
                &koeru_core::presamp::Rules::builtin(set),
                koeru_package::profile::NEWLINE,
            );
            runtime.writer(&lease)?.put_presamp_snapshot(&t)?;
            t
        };
        // 配布物のフォルダ名の既定は表示名から作る（`DEC-PKG-008`）。 SQL の移行では作れないので、
        // まだ決まっていない配布物へここで入れる。 決まっているものには触れない。
        let display_name = dir.read_manifest()?.display_name;
        runtime.writer(&lease)?.fill_default_archive_name(
            &koeru_core::names::default_distribution_name(&display_name),
        )?;
        // フォルダの控えが書き換えられていれば戻し、中身を別名で残す。
        let presamp_restored = dir.restore_presamp(&snapshot)?;
        if presamp_restored.is_some() {
            tracing::info!("書き換えられた綴りの表を戻した");
        }
        let rules = koeru_core::presamp::parse(&snapshot).or_builtin(set);
        Ok(Open {
            runtime: Arc::clone(runtime),
            lease,
            session_id: 0,
            rules,
            review,
            review_takes,
            presamp_restored,
            caches: ProjectCaches::new(),
        })
    }

    /// 開いているプロジェクトを閉じる。
    ///
    /// 収録中は閉じない（[`Self::open_project`] が移るのを断るのと同じ理由。
    /// `app.already_recording`）。 閉じられるなら、再生と背後の仕事を止め、
    /// ストリームを落とし、実行時の貸与を閉じて、何も開いていない状態へ戻る。
    ///
    /// 閉じたあとはプロジェクトが要る操作が `app.no_project` で断られ、
    /// 開き直せば新しい貸与が振られる（同じ値は二度と出ない）。
    ///
    /// **tauri-specta の操作にしない**（`DEC-PLT-035`）。 Rust の呼び出し側と
    /// 試験が使い、画面からは T09 の GraphQL `closeProject` が呼ぶ。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていないとき（`app.no_project`）、収録中のとき
    /// （`app.already_recording`）。
    #[tracing::instrument(skip(self))]
    pub fn close_project(&mut self) -> Result<()> {
        self.opened()?;
        if self.recording.is_some() {
            return Err(already_recording());
        }
        self.release_project()?;
        tracing::info!("プロジェクトを閉じた");
        Ok(())
    }

    /// 開いているプロジェクトを手放す。 無ければ何もしない。
    ///
    /// 順序が要る。 まず再生と背後の仕事を止め、ストリームを落とす（排出スレッドが
    /// 先。`disarm`）。 そのあとで実行時の貸与を閉じる——閉じたあとに残った仕事が
    /// 書き手を取ろうとすれば、`runtime.lease_expired` で断られる。
    /// `Open` を捨てることで、プロジェクトの寿命を持つ写し（[`ProjectCaches`]）も
    /// 一緒に捨てる。
    ///
    /// **`JobRuntime` の貸与も、実行時を閉じる前に取り消す**（[`JobRuntime::cancel_lease`]）。
    /// 前でも後でもよい（台帳への書き込みは貸与が閉じていれば writer が断る）が、前にする——
    /// 実行時が閉じているのに「公開してよい」と言える間を作らない。 取り消すと、走っている解析には
    /// 合図が立ち、待っている仕事は外れ、遅れて戻った完了は公開されない。 これで、閉じた
    /// project へ（開き直した同じ project へも）背後の仕事が書き戻さない（A → B → A）。
    fn release_project(&mut self) -> Result<()> {
        if self.open.is_none() {
            return Ok(());
        }
        self.stop_preview();
        self.disarm();
        if let Some(open) = self.open.take() {
            self.jobs.cancel_lease(&open.lease);
            open.runtime.close(&open.lease)?;
        }
        Ok(())
    }

    /// 背後の仕事の実行域。 アプリの寿命で1つ。
    ///
    /// **`Studio` の状態ロックの外から使うための持ち手。** `AppState` が組み立てのときに
    /// 写しを取り、待ち数（`TR-SYN-33`）をロックの外から読む。 テイクの確定は解析を待つので、
    /// 待ち数がいちばん動くときに状態ロックが握られている。
    #[must_use]
    pub fn jobs(&self) -> Arc<JobRuntime> {
        Arc::clone(&self.jobs)
    }

    /// 開いているプロジェクトの実行時。
    ///
    /// **`Studio` の状態ロックの外から使うための持ち手。** `AppState` が
    /// `commands` の経路で開く・移る・閉じるたびにここから写しを取り、ロックの外の
    /// 枠へ置く（`AppState::project_runtime`）。 T09 の resolver はその枠を読む
    /// ——`finish_take` が数秒握るロックを待たない。
    ///
    /// 貸与が切れたあとに握り続けた持ち手は、`read` / `writer` が
    /// `runtime.lease_expired` を返す。
    #[must_use]
    pub fn runtime(&self) -> Option<Arc<ProjectRuntime>> {
        self.open.as_ref().map(|o| Arc::clone(&o.runtime))
    }

    /// 開いたときの検証が見つけた、落ちたあとに残った録音（`TR-REC-28`）。
    ///
    /// 孤児はどの行・どの収録セッションの録音かを予定から示す。 画面へはまだ出していない
    /// ——新しい操作を tauri-specta に足さず（`DEC-PLT-035`）、consumer の契約の経路で出す。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない。
    pub fn capture_report(&self) -> Result<koeru_core::capture::Report> {
        Ok((*self.opened()?.runtime.open_report()).clone())
    }

    /// 開いたときに戻した `presamp.ini` の中身を残したファイル名（`DEC-SYN-013`）。
    ///
    /// 戻していなければ `None`。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない。
    pub fn presamp_notice(&self) -> Result<Option<String>> {
        Ok(self.opened()?.presamp_restored.clone())
    }

    /// 戻したことを本人が読んだので、知らせを下ろす（`DEC-SYN-013`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない。
    pub fn dismiss_presamp_notice(&mut self) -> Result<()> {
        self.opened_mut()?.presamp_restored = None;
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
        let mut s = open.writer()?.review_state()?;
        if !s.exported {
            return Ok(());
        }
        s.exported = false;
        open.writer()?.put_review_state(&s)?;
        Ok(())
    }

    /// 確認キューを台帳から組み直す。
    ///
    /// テイクを確定したときと、採用を切り替えたときに呼ぶ。 どちらも
    /// 「どのテイクのエントリが書き出しに出るか」が変わるので、
    /// キューの中身も変わる。モードは台帳から読み直すので落ちない。
    fn refresh_review(&mut self) -> Result<()> {
        let open = self.opened_mut()?;
        let (review, takes) = crate::review::load(&mut *open.writer()?)?;
        open.review = review;
        open.review_takes = takes;
        Ok(())
    }

    /// 開いているキャプチャ（`lease` の一部）。
    ///
    /// `lease` を1つのフィールドへ集約する前（T06c より前）は `capture` という
    /// 欄を直接持っていた。 既存の呼び出し側をそのまま読める形にするための口。
    fn capture(&self) -> Option<&mac::Capture> {
        self.lease.as_ref().map(CaptureLease::capture)
    }

    /// 開いている排出スレッド（`lease` の一部）。
    fn pump(&self) -> Option<&Pump> {
        self.lease.as_ref().map(CaptureLease::pump)
    }

    /// デバイス一覧が変わった回数だけを、ロックを取らずに読むための持ち手
    /// （`lease` の一部、`TR-REC-04`）。
    ///
    /// `commands::arm_device` が呼んで `AppState` へ渡す。 ストリームを
    /// 開いていなければ見張りも無いので `None`。
    #[must_use]
    pub fn device_watch_handle(&self) -> Option<mac::DeviceListChangedHandle> {
        self.lease
            .as_ref()
            .map(CaptureLease::device_list_changed_handle)
    }

    /// 開いているストリームを落とす。
    ///
    /// 落とす順序（排出スレッドが先。Consumer を握ったまま Capture を捨てない）は
    /// `CaptureLease` の `Drop` が型で守る（`arm_device` も同じ）。
    ///
    /// 収録中に呼ばない。 呼び側が先に断る（[`Self::open_project`]）。
    /// 排出スレッドは止められると書きかけを確定させるので、ここで落とすと
    /// 台帳に載らない WAV が残る。
    fn disarm(&mut self) {
        self.lease = None;
        // 状態機械も作り直す。 未選択からしか `select_device` へ進めない。
        self.session = Session::new();
        self.device = None;
    }

    /// いまの進み具合。
    #[tracing::instrument(skip(self))]
    pub fn progress(&mut self) -> Result<Progress> {
        let name = self.display_name()?;
        let preset = self.current_preset()?;
        // 提示順の先頭（`TR-SYN-19`）。 正準順で引くと、モードを切り替えても
        // 次のフレーズが変わらない。
        let next_row = self.next_presented_row()?;
        let open = self.opened_mut()?;
        let covered = open.writer()?.covered_units()?;
        let handoff = if open.writer()?.has_been_exported()? {
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
            .writer()?
            .progress_by_tone()?
            .into_iter()
            .map(|(tone, (done, total))| (tone, done, total))
            .collect();

        // 残り（`TR-RCL-09`）。固定の見積もりで出す——実測は採らない（`DEC-RCL-013`）。
        let tones = self.writer()?.recording_tones()?;
        let remaining_rows: Vec<koeru_core::reclist::Row> = {
            // この音源の作り方で数える（`TR-RCL-01`）。 単独音で数えると、
            // 連続音のプロジェクトの残りが常に別のリストの残りになる。
            let rules = self.opened()?.rules.clone();
            let list = preset.reclist(&rules).map_err(AppError::from_failure)?;
            let done: std::collections::BTreeSet<String> = self
                .writer()?
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
    #[tracing::instrument(skip(self))]
    pub fn song_status(&mut self) -> Result<Vec<SongStatus>> {
        let preset = self.current_preset()?;
        let open = self.opened_mut()?;
        let rules = open.rules.clone();
        song_status_of(&mut *open.writer()?, &rules, preset)
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
    #[tracing::instrument(skip(self))]
    pub fn song_plan(&mut self, id: &str) -> Result<koeru_core::plan::Plan> {
        let preset = self.current_preset()?;
        let open = self.opened_mut()?;
        // 必要集合は方式ごとの綴り。 仮名で引くと交わらない。
        let covered = open.writer()?.covered_aliases()?;
        let song = open
            .writer()?
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, song)| song)
            .ok_or_else(no_song)?;

        let rules = open.rules.clone();
        let required = song.required_aliases(&rules, preset.method, preset.set);
        let missing: std::collections::BTreeSet<String> =
            required.difference(&covered).cloned().collect();
        let full_list = preset.reclist(&rules).map_err(AppError::from_failure)?;
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
        fields(len = bytes.len(), count = titles.len())
    )]
    pub fn import_songs(
        &mut self,
        bytes: &[u8],
        file_name: &str,
        titles: &[String],
    ) -> Result<Vec<(String, Song)>> {
        let plan = importing::prepare(bytes, file_name)?;
        let open = self.opened()?;
        // 識別子はここで振る。 **画面が振って送る形になるまでの仮の置き場**（`DEC-PLT-035` の
        // `operationId`、T09 / T11）。 いまは押すたびに新しい操作なので、応答が失われた
        // 送り直しは、まだ二重に取り込む。 取り込みの側（`importing::commit`）は冪等になっている。
        let operation = OperationId::generate();
        // 題は本人が決める（`TR-RCL-12`）。 ファイル名から採るのは候補まで。
        match importing::commit(&open.runtime, &open.lease, &operation, &plan, titles)? {
            importing::Outcome::Imported(songs) | importing::Outcome::Replayed(songs) => Ok(songs),
            importing::Outcome::OperationIdReused(_) => Err(AppError::new(
                "song.operation_id_reused",
                Class::Conflict,
                "同じ操作の識別子で、別の取り込みが来た",
            )),
            importing::Outcome::Rejected(r) => Err(rejection_error(r)),
        }
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
    #[tracing::instrument(skip(self, bytes, file_name), fields(len = bytes.len()))]
    pub fn song_file_preview(&mut self, bytes: &[u8], file_name: &str) -> Result<Vec<Song>> {
        self.opened()?;
        let plan = importing::prepare(bytes, file_name)?;
        if let Some(d) = plan.diagnostics().first() {
            return Err(diagnostic_error(*d));
        }
        Ok(plan.songs().to_vec())
    }

    /// 曲の題を変える（`TR-RCL-12`）。
    ///
    /// 題はファイル名から採るので、そのままでは一覧に並べられないことがある
    /// （`New Project`、`テスト2_final`）。取り込むときにも後からも変えられる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、その曲がバンクに無い、台帳を書けない。
    #[tracing::instrument(skip(self, id, title))]
    pub fn rename_song(&mut self, id: &str, title: &str) -> Result<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(empty_title());
        }
        self.writer()?.rename_song(id, title)?;
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
    #[tracing::instrument(skip(self, id))]
    pub fn song_notes(&mut self, id: &str) -> Result<Vec<koeru_core::song::Note>> {
        self.writer()?
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, s)| s.notes)
            .ok_or_else(no_song)
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
    #[tracing::instrument(skip(self, selections), fields(songs = selections.len()))]
    pub fn repack_for_selection(
        &mut self,
        selections: &[(String, Vec<(usize, usize)>)],
    ) -> Result<usize> {
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;
        // 収録音高は台帳が持つ（`TR-RCL-01`）。プリセットは方式だけ。
        let tones = self.writer()?.recording_tones()?;
        let songs = self.writer()?.songs_in_bank()?;
        // 要るものは（収録音高, 綴り）の組（`TR-RCL-15`）。 ノートの floor の音高でだけ要る。
        //
        // **全部の音高に同じ行を入れていた。** 低い曲を選んでも、使わない高い
        // 区画の行まで読ませることになる。
        let mut required: std::collections::BTreeSet<(i32, String)> =
            std::collections::BTreeSet::new();
        for (id, ranges) in selections {
            let song = songs
                .iter()
                .find(|(sid, _)| sid == id)
                .map(|(_, s)| s)
                .ok_or_else(no_song)?;
            let part = if ranges.is_empty() {
                song.clone()
            } else {
                song.select(ranges)
            };
            required.extend(part.required_by_tone(&rules, preset.method, preset.set, &tones));
        }

        // 一度録った綴りは読ませない（`DEC-RCL-016`）。 まだ録っていない詰め直しの
        // 行が既に作る綴りも除く——選び直すたびに同じ綴りを別の行でもう一度読ませない。
        let covered = self.writer()?.covered_aliases_by_tone()?;
        let suffixed = tones.len() > 1;
        // 返すのは台帳に実際に入った行の数。 **作った数を返していた**ので、
        // 同じ範囲を2度詰め直しても「N 行を足しました」と出ていた
        // ——既にある行は飛ばすので、台帳は増えていない。
        let mut added = 0;
        for t in &tones {
            // 被覆は音階で数える。 同じ音階の枠が2つあれば、並びの先頭の枠へ入れる。
            let Some(subbank) = self.writer()?.subbank_for_tone(*t)? else {
                continue;
            };
            let waiting: std::collections::BTreeSet<String> = self
                .writer()?
                .untaken_rows(subbank, koeru_core::db::RowOrigin::Repack)?
                .into_iter()
                .flat_map(|(_, a)| a)
                .collect();
            let want: std::collections::BTreeSet<String> = required
                .iter()
                .filter(|(rt, a)| {
                    rt == t
                        && !waiting.contains(a)
                        && !covered.get(t).is_some_and(|c| c.contains(a))
                })
                .map(|(_, a)| a.clone())
                .collect();
            if want.is_empty() {
                continue;
            }
            let rows = preset
                .reclist_for(&rules, &want)
                .map_err(AppError::from_failure)?;
            added += self.writer()?.install_rows(
                &rows,
                &rules,
                preset.method,
                subbank,
                suffixed,
                koeru_core::db::RowOrigin::Repack,
            )?;
        }
        tracing::info!(count = added, "選択から録音リストを詰め直した");
        Ok(added)
    }

    /// 録った行と別の出どころの、まだ録っていない行を組み直す（`DEC-RCL-016`）。
    ///
    /// フルリストの行を録ったら詰め直しの行を、詰め直しの行を録ったらフルリストの
    /// 行を、まだ録っていない綴りだけを覆う行へ作り直す。**一度録った綴りを
    /// 二度読ませない。** 曲のために録った `さ` を、フルリストの「さしすせそ」で
    /// もう一度読ませていた。
    ///
    /// 録っている側は組み直さない。 本人が読み進めている並びが、読むたびに
    /// 足元で変わる。組み直すのはテイクを1本も持たない行だけ（`Ledger::untaken_rows`）。
    fn reconcile_other_origin(&mut self, row_id: &str) -> Result<()> {
        let project_preset = self.current_preset()?;
        let rules = self.current_rules()?;
        let ledger = &mut *self.writer()?;
        let origin = ledger.row_origin(row_id)?.other();
        let tone = ledger.row_tone(row_id)?;
        // 組み直すのは録った行の枠だけ。 同じ音階でも、声色の違う枠の行は動かさない。
        let subbank = ledger.row_subbank(row_id)?;
        let preset = subbank_preset(ledger, subbank)?.unwrap_or(project_preset);
        let waiting = ledger.untaken_rows(subbank, origin)?;
        let covered = ledger
            .covered_aliases_by_tone()?
            .remove(&tone)
            .unwrap_or_default();
        // 録った綴りを1つも含まないなら、組み直しても同じ行になる。
        if !waiting
            .iter()
            .any(|(_, a)| a.iter().any(|x| covered.contains(x)))
        {
            return Ok(());
        }
        let want: std::collections::BTreeSet<String> = waiting
            .iter()
            .flat_map(|(_, a)| a.iter())
            .filter(|a| !covered.contains(*a))
            .cloned()
            .collect();
        let rows = if want.is_empty() {
            Vec::new()
        } else {
            preset
                .reclist_for(&rules, &want)
                .map_err(AppError::from_failure)?
        };
        let suffixed = ledger.recording_tones()?.len() > 1;
        ledger.replace_untaken_rows(&rows, &rules, preset.method, subbank, suffixed, origin)?;
        tracing::info!(
            rows = rows.len(),
            count = waiting.len(),
            "まだ録っていない行を組み直した"
        );
        Ok(())
    }

    /// いま開いているプロジェクトの方式プリセット（`TR-RCL-01`）。
    ///
    /// manifest が持つのは識別子だけ。 古いプロジェクトは識別子を持たないので、
    /// 方式から引く（`Manifest::preset_id` は後から足した）。
    ///
    /// **収録音高はここから来ない。** 方式と音高は別の選択なので、音高は
    /// 台帳（[`Ledger::recording_tones`]）が持つ。
    fn current_preset(&mut self) -> Result<koeru_core::preset::MethodPreset> {
        Ok(preset_of(&self.opened()?.dir().read_manifest()?))
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
    #[tracing::instrument(skip(self))]
    pub fn all_songs(&mut self) -> Result<Vec<(String, Song, bool)>> {
        Ok(self.writer()?.all_songs()?)
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
    #[tracing::instrument(skip(self, id))]
    pub fn set_song_transpose(&mut self, id: &str, semitones: i32) -> Result<()> {
        if !(-12..=12).contains(&semitones) {
            return Err(AppError::new(
                "song.transpose_out_of_range",
                Class::InvalidInput,
                "移調は1オクターブの上下まで",
            ));
        }
        self.writer()?.set_song_transpose(id, semitones)?;
        Ok(())
    }

    /// 曲をバンクから外す／戻す（`TR-RCL-12`）。曲そのものは消さない。
    #[tracing::instrument(skip(self))]
    pub fn set_song_in_bank(&mut self, id: &str, in_bank: bool) -> Result<()> {
        self.writer()?.set_song_in_bank(id, in_bank)?;
        Ok(())
    }

    /// 入力デバイスを挙げる。
    #[tracing::instrument]
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
    #[tracing::instrument(skip(self))]
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
        let armed = self.lease.is_some();
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
            None => self.writer()?.last_device()?,
        };
        Ok((id, armed, recording))
    }

    /// デバイスを選び、ストリームを開く（`recording-input.fsl` の手順）。
    ///
    /// ストリームはテイクごとに開閉しない（`REQ-REC-102`）。
    /// 収録画面を離れるまで持ち続ける。
    ///
    /// 本人が明示的に選ぶ経路。 選択を必ず作り直す
    /// ——見失っていたデバイスの自動復帰は [`Self::on_device_returned`] が別に持つ。
    #[tracing::instrument(skip(self))]
    pub fn arm_device(&mut self, device: &DeviceId) -> Result<mac::MicrophoneMode> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                Class::Rejected,
                "収録中はマイクを変えられない",
            ));
        }

        // 前のストリームを先に落とす。 2つの AUHAL を同時に回さない。
        // 落とす順序（排出スレッドが先。Consumer を握ったまま Capture を捨てない）は
        // `CaptureLease` の `Drop` が型で守る。
        self.lease = None;
        /*
         * 前のデバイスも忘れる。
         *
         * 残すと、**この先で失敗したときに「前のデバイスが選ばれている」と
         * 答えてしまう。** 画面は新しく選んだほうを出したまま、録る手前の
         * 開き直しが前のデバイスを開く——**別のマイクで録れてしまう。**
         * 開けたときに下で入れ直す。
         *
         * 本人が選び直す経路だからこそ、ここで手放してよい。 見失っていた
         * デバイスの自動復帰（[`Self::on_device_returned`]）はここを通らない
         * ——再オープンに失敗しても選択を手放さない（`REQ-REC-109`）。
         */
        self.device = None;

        // 状態機械を作り直す。 `recording-input.fsl` の `select_device` は
        // 未選択からしか進めない（`proved`）。マイクの選び直しは、その機械から見れば
        // 「収録画面を出て入り直す」ことなので、機械ごと新しくするのが忠実な読み。
        // 既存の機械を無理に巻き戻さない。 巻き戻す遷移は仕様に無い。
        self.session = Session::new();

        self.open_armed_stream(device, SelectionOrigin::Fresh)
    }

    /// 実際にストリームを開き、`CaptureLease` を組み立てる（`arm_device` の本体）。
    ///
    /// 呼び出し側が、ここへ入る前にデバイスの選択（`session` を `Selected` へ
    /// 進める段取り）を済ませていることは要らない——その FSL の遷移
    /// （`select_device` か `same_device_returned` か）は [`SelectionOrigin`] で
    /// 指定し、ここが `mac::open` の成否を確かめたあとに行う。
    ///
    /// **この順序が要る。** `mac::open` は失敗しうる（実機が無い、抜けたままなど）。
    /// 先に FSL の遷移を済ませてしまうと、失敗したときに「選択したのに開けて
    /// いない」半端な状態が残る。 自動復帰（[`SelectionOrigin::Returned`]）は
    /// これに頼っている——失敗しても `Lost` のまま、`self.device` も保つので、
    /// 次にデバイス一覧が動いたときにまた試せる（`REQ-REC-109`）。
    fn open_armed_stream(
        &mut self,
        device: &DeviceId,
        origin: SelectionOrigin,
    ) -> Result<mac::MicrophoneMode> {
        let open = self.opened_mut()?;
        // 前に決めたチャンネルを引き継ぐ（`TR-REC-06`）。テイクごとに違う経路から
        // 録った素材が混ざると、合成したときに音色が揃わない。
        let saved_channel = open
            .writer()?
            .calibration_of(device.as_str())?
            .map_or(0, |c| c.source_channel);

        // セッションは録音条件のスナップショット（`TR-REC-30`）。
        // ここで失敗しても、呼び出し側の状態はまだ何も変えていない。
        let (cap, consumer) = mac::open(device, 48_000 * RING_SECONDS)?;
        let format = cap.format();
        let mode = mac::active_microphone_mode();

        let session_id = open.writer()?.start_session(&SessionSnapshot {
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

        // 状態機械を手順どおりに進める。 ここまで来れば `mac::open` は成功している
        // ので、FSL の遷移を踏んでよい。
        match origin {
            SelectionOrigin::Fresh => self.session.select_device(device.clone())?,
            SelectionOrigin::Returned => self.session.same_device_returned(device)?,
        }
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
        let pump = Pump::start(consumer, format.sample_rate_hz);
        // 着脱の見張りも同じ寿命で始める（`TR-REC-04`）。 見張りスレッド
        // （`lib.rs`）がロックを取らずに読める持ち手は、コマンド側
        // （`commands::arm_device`）がここから引いて `AppState` へ渡す。
        let watch = mac::watch(device);
        self.lease = Some(CaptureLease::new(cap, pump, watch));
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
    #[tracing::instrument(skip(self))]
    pub fn estimate_space(&mut self) -> Result<SpaceEstimate> {
        self.capture().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let root = self.opened()?.dir().root().to_path_buf();
        let remaining = self.writer()?.remaining_rows()?;

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
    #[tracing::instrument(skip(self))]
    pub fn has_room_for_one_more(&mut self) -> Result<bool> {
        self.capture().ok_or_else(no_stream)?;
        let rate = MASTER_RATE_HZ;
        let root = self.opened()?.dir().root().to_path_buf();
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
    #[tracing::instrument(skip(self))]
    pub fn probe_input(&mut self, ms: u64) -> Result<f32> {
        self.require_stream()?;
        {
            // 直前の残りを捨ててから測る。「今」の入力だけを見る。
            let pump = self.pump().ok_or_else(no_stream)?;
            let _ = pump.take_peak();
        }
        std::thread::sleep(std::time::Duration::from_millis(ms));
        let peak = self.pump().ok_or_else(no_stream)?.take_peak();

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
        self.pump().map_or(0, Pump::preroll_ms)
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
    #[tracing::instrument(skip(self))]
    pub fn check_guide_leak(&mut self, midi: i32) -> Result<LeakCheck> {
        self.capture().ok_or_else(no_stream)?;
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
    #[tracing::instrument(skip(self))]
    pub fn play_pitch(&mut self, midi: i32) -> Result<()> {
        match self.leak {
            None => {
                // 収録セッションの前提と同じ（`INV-REC-105`）。 code を2つ名乗らない。
                return Err(AppError::from(koeru_audio::SessionError::LeakNotChecked)
                    .saying("先に回り込みを確かめてほしい"));
            }
            Some(l) if l.leaking => {
                return Err(AppError::new(
                    "recording.guide_leaks",
                    Class::Rejected,
                    "ガイドが録音へ回り込むので鳴らさない",
                ));
            }
            Some(_) => {}
        }
        self.require_stream()?;
        let rate = MASTER_RATE_HZ;
        let pcm = guide::render(&GuideSpec::pitch_reference(), midi, rate);
        // 先に止める。 単一の出力枠を取り合う（`DEC-REC-011`）ので、
        // 曲の試唱が鳴っていればここで止まる。
        self.output.stop();
        self.output.set_clip(mac::play(pcm, rate)?);
        Ok(())
    }

    /// 鳴らしながら録る。回り込みの検査にだけ使う。
    fn play_and_capture(&mut self, played: &[f32], rate: u32) -> Result<Vec<f32>> {
        // 曲が鳴っている最中に検査すると、測るものに混じって歪む
        // （`DEC-REC-011`）。 単一の出力枠を先に空ける。
        self.output.stop();
        let pump = self.pump().ok_or_else(no_stream)?;
        pump.begin_probe();
        let handle = mac::play(played.to_vec(), rate)?;

        // 鳴っているあいだ待つ。余裕を持たせる（バッファのぶん遅れる）。
        let ms = (played.len() as u64 * 1000 / u64::from(rate.max(1))) + 200;
        std::thread::sleep(std::time::Duration::from_millis(ms));
        drop(handle);

        Ok(self.pump().ok_or_else(no_stream)?.end_probe())
    }

    /// 全チャンネルを混ぜる（`TR-REC-06`）。
    ///
    /// 全チャンネルに有意な信号があるときだけ選べる。
    /// 片側にしか信号が無いのに混ぜると 6dB 損をする。
    #[tracing::instrument(skip(self))]
    pub fn use_mixed_channels(&mut self) -> Result<()> {
        if !self.may_mix {
            return Err(AppError::new(
                "recording.mix_unavailable",
                Class::Rejected,
                "有意な信号があるのは一部のチャンネルだけなので、混ぜない",
            ));
        }
        self.capture().ok_or_else(no_stream_err)?.set_source_mix();
        let device = self.device.clone().ok_or_else(no_stream_err)?;
        // 先に値だけ取り出す。 `if let` の式に書くと、書き手の guard が本体のあいだ
        // 生き、下の `writer()` が自分自身を待つ（`Open::writer`）。
        let saved = self.writer()?.calibration_of(device.as_str())?;
        if let Some(mut c) = saved {
            c.source_channel = -1;
            let at = now_rfc3339();
            self.writer()?.put_calibration(&c, &at)?;
        }
        Ok(())
    }

    /// 保存してある校正と、いまのゲインを突き合わせる（`TR-REC-15`）。
    ///
    /// 勝手に戻さない。 差があることを返すだけで、戻すかどうかは本人が決める。
    #[tracing::instrument(skip(self))]
    pub fn gain_drift(&mut self) -> Result<Option<(f32, f32)>> {
        let Some(device) = self.device.clone() else {
            return Ok(None);
        };
        let saved = self
            .writer()?
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
    #[tracing::instrument(skip(self))]
    pub fn restore_saved_gain(&mut self) -> Result<()> {
        let Some(device) = self.device.clone() else {
            return Err(no_stream_err());
        };
        let saved = self
            .writer()?
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
    #[tracing::instrument(skip(self))]
    pub fn calibrate(&mut self, seconds: f64) -> Result<Calibration> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                Class::Rejected,
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
        let rms = self.capture().ok_or_else(no_stream_err)?.channel_rms();
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
        if let Some(cap) = self.capture() {
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
        self.writer()?.put_calibration(&result, &at)?;
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
        let pump = self.pump().ok_or_else(no_stream_err)?;
        let _ = pump.take_peak();
        std::thread::sleep(std::time::Duration::from_millis(ms));
        Ok(self.pump().ok_or_else(no_stream_err)?.take_peak())
    }

    /// いま録るべき行の収録を始める。
    ///
    /// 押した瞬間より前へ遡って書きはじめる（`TR-REC-19`）。
    /// 人は「録音」を押してから息を吸わない。指示の時点から書くと語頭が欠ける。
    #[tracing::instrument(skip(self))]
    pub fn start_take(&mut self) -> Result<String> {
        self.start_take_in(None)
    }

    /// 枠を指して、その枠で次に録る行の収録を始める（`DEC-RCL-018`）。 `None` なら
    /// 全部の枠を通した録る順の先頭から（[`Studio::start_take`]）。
    ///
    /// # Errors
    ///
    /// その枠に録る行がもう無い、収録を始められない。
    #[tracing::instrument(skip(self))]
    pub fn start_take_in(&mut self, subbank: Option<SubbankId>) -> Result<String> {
        // 録るのは提示順の先頭（`TR-SYN-19`）。 **正準順で引いていた**
        // ——曲バンク優先を選んでも、録り始めるのは常に ordinal の最小だった。
        let row_id = self
            .next_row_in(subbank)?
            .ok_or_else(|| {
                AppError::new(
                    "app.nothing_to_record",
                    Class::Rejected,
                    "録るべき行がもう無い",
                )
            })?
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
        self.next_row_in(None)
    }

    /// 枠の中で次に録る行（`TR-SYN-19`, `DEC-RCL-018`）。 `None` なら全部の枠から。
    ///
    /// 提示順の先頭で、空なら枠の正準順へ落ちる。 画面が「次の行」に出す
    /// [`Studio::recording_order_in`] の先頭と同じ行を返す。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    pub fn next_row_in(&mut self, subbank: Option<SubbankId>) -> Result<Option<(String, String)>> {
        let (_, order) = self.recording_order_in(subbank)?;
        let open = self.opened_mut()?;
        for id in order {
            // 提示順は台帳の行 ID で来る（多音階なら `s001@G3`）。 音高を
            // 跨いだ並びは `recording_order` が決めるので、ここは先頭から引くだけ。
            if let Some(text) = open.writer()?.unrecorded_row_text(&id)? {
                return Ok(Some((id, text)));
            }
        }
        match subbank {
            Some(id) => open.writer()?.next_row_of(id).map_err(Into::into),
            None => open.writer()?.next_row().map_err(Into::into),
        }
    }

    /// 全部の行と、そのテイク（`TR-REC-21`, `TR-RCL-25`）。
    ///
    /// 録り直しの一覧。 採用を戻すのにも使う。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn rows_with_takes(&mut self) -> Result<Vec<koeru_core::db::RowTakes>> {
        Ok(self.writer()?.rows_with_takes()?)
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
    #[tracing::instrument(skip(self))]
    pub fn play_tone_reference(&mut self, midi: i32) -> Result<()> {
        if Self::output_kind().definitely_speakers() || self.leak.is_some_and(|l| l.leaking) {
            tracing::info!(midi, "回り込むので基準音を鳴らさない");
            return Ok(());
        }
        let spec = koeru_core::guide::GuideSpec::tone_reference();
        let pcm = koeru_core::guide::render(&spec, midi, MASTER_RATE_HZ);
        self.output.stop();
        self.output.set_clip(mac::play(pcm, MASTER_RATE_HZ)?);
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
    #[tracing::instrument(skip(self))]
    pub fn recording_order(&mut self) -> Result<(koeru_core::order::Mode, Vec<String>)> {
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;
        let (stored, pinned) = self.writer()?.recording_order()?;
        let status = song_status_of(&mut *self.writer()?, &rules, preset)?;
        let empty = status.is_empty();
        // 「完全」は必要単位がすべて収録済みのこと（`TR-SYN-19` の (a)）。
        //
        // **歌えるかで見ていた。** 歌えるかは音域も含むので、収録音高から遠い曲が
        // 1つでもバンクにあると決して満たされず、自動の切り替えが起きなかった
        // ——その曲の音域は、どの行を録っても変わらない。
        let complete = !empty && status.iter().all(|s| s.missing_units == 0);

        let mode = if pinned || !koeru_core::order::auto_switches(empty, complete) {
            stored
        } else {
            koeru_core::order::Mode::CoverageEfficiency
        };
        if mode != stored {
            // 自動の移行では `pinned` を立てない。 立てると本人が選んだことになる。
            self.writer()?.set_recording_order(mode, false)?;
        }

        let rows = self.writer()?.rows_with_takes()?;
        let recorded: std::collections::BTreeSet<String> = rows
            .iter()
            .filter(|r| r.adopted.is_some())
            .map(|r| r.row_id.clone())
            .collect();
        let tones = self.writer()?.recording_tones()?;
        // 曲が要る綴りは、そのノートの floor の音高ごとに（`TR-RCL-15`, `DEC-SYN-014`）。
        let mut song_required: std::collections::BTreeMap<i32, std::collections::BTreeSet<String>> =
            std::collections::BTreeMap::new();
        let bank = self.writer()?.songs_in_bank()?;
        for (_, song) in bank {
            for (t, a) in song.required_by_tone(&rules, preset.method, preset.set, &tones) {
                song_required.entry(t).or_default().insert(a);
            }
        }
        // 並べるのは台帳の行（`TR-SYN-19`, `DEC-RCL-016`）。
        //
        // **プリセットから生成し直したリストを並べていた。** 詰め直した行も、
        // 録った綴りに合わせて組み直した行も一度も並ばず、録り終えた綴りを
        // もう一度読ませるフルリストの行が出続けた。
        let listed = self.writer()?.listed_rows(preset.set)?;

        // 音高ごとに並べ、低い音高から順に繋ぐ（`TR-RCL-26`）。
        //
        // **音高を跨いで1本で並べていた。** 被覆を和集合で渡したので、1音高を
        // 録り終えると全部の綴りが揃ったことになり、残りの音高ではどの行も
        // 点が 0——モードに関係なく正準順へ戻っていた。
        //
        // 音高の向きは台帳の正準順（`ordinal`）と同じ。 並べ直すのは、各音高の中だけ。
        let by_tone = self.writer()?.covered_aliases_by_tone()?;
        let empty = std::collections::BTreeSet::new();
        let mut order = Vec::new();
        for t in &tones {
            let here: Vec<koeru_core::reclist::Row> = listed
                .iter()
                .filter(|(rt, _)| rt == t)
                .map(|(_, r)| r.clone())
                .collect();
            order.extend(koeru_core::order::present(
                &rules,
                mode,
                preset.method,
                &here,
                &recorded,
                by_tone.get(t).unwrap_or(&empty),
                song_required.get(t).unwrap_or(&empty),
            ));
        }
        Ok((mode, order))
    }

    /// 枠の中の録る順（`TR-SYN-19`, `DEC-RCL-018`）。 `None` なら全部の枠を通した順。
    ///
    /// 並べ方は [`Studio::recording_order`] のまま。 枠で絞るのは、並べたあとに1回だけ。
    /// 枠ごとに並べ直すと、曲バンク優先の点が枠をまたいで比べられなくなる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn recording_order_in(
        &mut self,
        subbank: Option<SubbankId>,
    ) -> Result<(koeru_core::order::Mode, Vec<String>)> {
        let (mode, mut order) = self.recording_order()?;
        if let Some(id) = subbank {
            let inside: std::collections::BTreeSet<String> = self
                .writer()?
                .rows_with_takes()?
                .into_iter()
                .filter(|r| r.subbank == id)
                .map(|r| r.row_id)
                .collect();
            order.retain(|row| inside.contains(row));
        }
        Ok((mode, order))
    }

    /// 録る順を本人の操作で切り替える（`TR-SYN-19` の (b)）。
    ///
    /// 可逆。 被覆効率から曲バンク優先へも戻せる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self), fields(mode = mode.as_str()))]
    pub fn set_recording_order(&mut self, mode: koeru_core::order::Mode) -> Result<()> {
        Ok(self.writer()?.set_recording_order(mode, true)?)
    }

    /// ガイドのテンポ（`TR-REC-23`、`DEC-REC-012`）。 本人が変えていなければ既定。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn guide_tempo(&self) -> Result<GuideTempo> {
        Ok(self.writer()?.guide_tempo()?)
    }

    /// ガイドのテンポを変える（`DEC-REC-012`）。 録ったテイクは変えない。
    ///
    /// 収録セッションを始めていれば切って始め直す（`TR-REC-30`）。 テイクは属する
    /// セッションのテンポで録られたことになるので、1つのセッションの中でテンポを変えない。
    /// 録っている途中は変えない——そのテイクは録り始めたセッションに属する。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, tempo))]
    pub fn set_guide_tempo(&mut self, tempo: GuideTempo) -> Result<GuideTempoChange> {
        if self.recording.is_some() {
            return Ok(GuideTempoChange::Recording);
        }
        let open = self.opened_mut()?;
        let session = (open.session_id != 0).then_some(open.session_id);
        let started = open
            .writer()?
            .set_guide_tempo(tempo, session, &now_rfc3339())?;
        if let Some(started) = started {
            open.session_id = started;
        }
        Ok(GuideTempoChange::Set(tempo))
    }

    /// 1フレーズぶんのガイド。 テンポは台帳にある本人の値（`TR-REC-23`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を読めない。
    pub fn phrase_guide(&self, moras: u32) -> Result<GuideSpec> {
        Ok(GuideSpec::phrase(self.guide_tempo()?, moras))
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
    #[tracing::instrument(skip(self))]
    pub fn adopt_take(&mut self, row_id: &str, take_id: i32) -> Result<()> {
        self.writer()?.adopt_take(row_id, take_id)?;
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
    /// 書きかけを開く前に、どの行をどの収録セッションで録るかを台帳へ予定として書く
    /// （`DEC-REC-010`）。 途中で落ちても、残った WAV がどの録音かを予定から示せる。
    ///
    /// # Errors
    ///
    /// 収録中、ストリームが開いていない、その行が無い、残量が足りない、
    /// 落ちたときの予定に印を付けられずに残っている。
    #[tracing::instrument(skip(self))]
    pub fn start_take_for(&mut self, row_id: &str) -> Result<String> {
        if self.recording.is_some() {
            return Err(AppError::new(
                "app.already_recording",
                Class::Rejected,
                "すでに収録中",
            ));
        }
        // ストリームが開いていることだけ確かめる。 レートは持ち回さない——
        // マスターは常に 44100 で、変換は pump が1回だけ行う（`TR-REC-02`）。
        self.require_stream()?;
        // 遡る起点は指示の時点で取る（`TR-REC-19`）。 このあと台帳を読み、予定を書くので、
        // 排出スレッドが開始を受け取った時点から遡ると、そのぶん語頭が欠ける。
        let from = self.pump().ok_or_else(no_stream)?.position();
        let (root, audio_dir, session_id) = {
            let open = self.opened()?;
            (
                open.dir().root().to_path_buf(),
                open.dir().audio_dir(),
                open.session_id,
            )
        };
        let row_id = row_id.to_owned();
        // **知らない行では始めない。** 名前を打ち間違えたまま録ると、
        // 台帳に載らないファイルができる。
        self.writer()?.row_state(&row_id)?;

        // 多音階では、収録開始前に対象音高の基準音を必ず鳴らす（`TR-REC-25`）。
        //
        // **省略できない。** 3本を行き来すると、いま何を録っているのかを
        // 音で確かめる手が要る。回り込みが分かっているときは鳴らさない
        // （`TR-REC-24`。ガイドが録音に入る）。
        let tones = self.writer()?.recording_tones()?;
        if tones.len() > 1 {
            let tone = self.writer()?.row_tone(&row_id)?;
            self.play_tone_reference(tone)?;
        }

        // 世代を名前に入れる。 録り直しても既存の WAV を上書きしない（`TR-PKG-39`）。
        let generation = self.writer()?.takes_of(&row_id)?.len() + 1;
        // 音高ごとのサブディレクトリへ置く（`TR-REC-36`）。
        // 収録後に別ディレクトリへ移す処理は持たない。
        let dir = if tones.len() > 1 {
            let tone = self.writer()?.row_tone(&row_id)?;
            audio_dir.join(koeru_core::tone::name(tone))
        } else {
            audio_dir.clone()
        };
        // 孤児・書きかけ・予定が持つ名前を避ける。 同じ名前へ rename すると孤児を上書きする。
        let path = koeru_core::capture::free_take_path(
            &mut *self.writer()?,
            &root,
            &dir,
            &row_id,
            generation,
        )
        .map_err(AppError::from_failure)?;

        // 残りが1テイクぶんを割ったら、次を始めさせない（`TR-REC-41`）。
        // 進行中のテイクは最後まで録りきるので、止めるのはここだけ。
        if !self.has_room_for_one_more()? {
            // 収録セッションの前提と同じ（`TR-REC-41`）。 code を2つ名乗らない。
            return Err(AppError::from(koeru_audio::SessionError::NotEnoughSpace)
                .saying("保存先の残量が1テイクぶんを割った"));
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

        // 録る前に予定を書く（`DEC-REC-010`）。 どの行を、どの収録セッションで録るか。
        let capture = CaptureId::generate();
        self.writer()?.declare_capture(&NewIntent {
            capture: &capture,
            row_id: &row_id,
            session_id,
            rel_path: &koeru_core::capture::rel_path(&root, &path),
            declared_at: &now_rfc3339(),
        })?;
        // ここから先で始められなければ、予定に場所を見て印を付ける。 付けないと予定が
        // 録音中のまま残り、次の録音の予定を書けない。
        let guard = match self.begin_capture(path, from) {
            Ok(guard) => guard,
            Err(e) => {
                self.settle_capture(&capture);
                return Err(e);
            }
        };

        self.recording = Some(ActiveCapture {
            row_id: row_id.clone(),
            capture,
            guard,
        });
        Ok(row_id)
    }

    /// 予定を書いたあと、状態機械を進めて書きかけを開く。
    ///
    /// このテイクの基準（`TakeGuard`）を、書きかけを開く直前に取る。 `from`
    /// （プリロールの起点）はここより前、予定を書く前に読んだ値
    /// （`TR-REC-19`）で、取り直さない。
    fn begin_capture(&mut self, path: PathBuf, from: u64) -> Result<TakeGuard> {
        self.session.start_take()?;
        let lease = self.lease.as_ref().ok_or_else(no_stream)?;
        let guard = TakeGuard::begin(lease, from);
        lease
            .pump()
            .start_take(path, from)
            .map_err(AppError::from_failure)?;
        Ok(guard)
    }

    /// 確定できなかった録音の予定へ、場所に残ったものを見て印を付ける
    /// （`koeru_core::capture::settle`）。
    ///
    /// 呼び出し側の失敗をそのまま返させるため、ここの失敗は記録だけする。
    fn settle_capture(&mut self, capture: &CaptureId) {
        let Ok(open) = self.opened() else { return };
        let root = open.dir().root().to_path_buf();
        let mut writer = match open.writer_raw() {
            Ok(w) => w,
            Err(e) => {
                koeru_failure::record_failure(
                    &e,
                    koeru_failure::Outcome::NotCommitted,
                    "capture.settle",
                );
                return;
            }
        };
        if let Err(e) = koeru_core::capture::settle(&mut writer, &root, capture, &now_rfc3339()) {
            koeru_failure::record_failure(
                &e,
                koeru_failure::Outcome::NotCommitted,
                "capture.settle",
            );
        }
    }

    /// 収録を止めて、テイクを確定させる。
    ///
    /// 順序は、ファイル確定 → DB コミット（`DEC-REC-004`）。
    /// 逆にすると、ファイルの無い行が DB に残る。
    ///
    /// 取りこぼしがあったテイクは、確定と同じ一手で無効として載せる（`TR-REC-07`）。
    /// 採用は動かさないので、同じフレーズがもう一度出てくる。
    ///
    /// 確定のあと、その場で解析と `.frq` と oto の導出まで済ませる
    /// （`TR-PKG-05`, `TR-PKG-42`）。 **そこで落ちても失敗を返さない。**
    /// テイクは確定しているので、派生物の失敗は [`TakeResult::followup`] に入れて返す
    /// （`DEC-PLT-038`）。 一度は失敗として返していて、保存済みの録音が
    /// 「録れなかった」と見え、録り直すと同じ行に2本残った。
    ///
    /// 今の画面は操作の識別子を送ってこないので、ここで振る（[`Self::finish_capture`]）。
    ///
    /// # Errors
    ///
    /// 確定の前に落ちたとき。 ファイルの確定より後・台帳より前に落ちたら、
    /// 確定した WAV は孤児として残る（`REQ-REC-006`）。消さない。
    #[tracing::instrument(skip(self))]
    pub fn finish_take(&mut self) -> Result<TakeResult> {
        let capture = self
            .recording
            .as_ref()
            .map(|r| r.capture.clone())
            .ok_or_else(|| AppError::new("app.not_recording", Class::Rejected, "収録していない"))?;
        match self.finish_capture(&OperationId::generate(), &capture)? {
            Finish::Committed { result, .. } => Ok(*result),
            // 振ったばかりの識別子と録音中の予定なので、受領証も孤児も無い。
            Finish::Answered(_) | Finish::RecoveryRequired => Err(AppError::new(
                "app.capture_not_committed",
                Class::Internal,
                "録音を確定できなかった",
            )),
        }
    }

    /// 操作の識別子を付けて、録音を止めてテイクを確定させる（`DEC-PLT-035`）。
    ///
    /// 同じ識別子の送り直しには、録音がもう止まっていても受領証で答え、テイクを増やさない
    /// （`project-storage.fsl` の `retry_commit`）。 同じ識別子で別の録音を指したら、
    /// 先の受領証を返して何もしない。
    ///
    /// 録音していない予定を指したら、落ちたあとに残った録音なので [`Finish::RecoveryRequired`]。
    /// 確定の途中で失敗したら、予定に場所を見て印を付けてから失敗を返す——孤児になった
    /// WAV は次の検証を待たずに予定と結びつき、次の録音も始められる。
    ///
    /// # Errors
    ///
    /// 予定が無い、確定の前に落ちた（[`Self::finish_take`] と同じ）。
    #[tracing::instrument(skip(self, operation, capture))]
    pub fn finish_capture(
        &mut self,
        operation: &OperationId,
        capture: &CaptureId,
    ) -> Result<Finish> {
        // 送り直しは録音が止まったあとに来る。 「録音していない」と断る前に受領証で答える。
        let answered = self.writer()?.answer_retry(operation, capture)?;
        if let Some(answer) = answered {
            return Ok(Finish::Answered(answer));
        }
        if !self
            .recording
            .as_ref()
            .is_some_and(|active| active.capture == *capture)
        {
            return Ok(Finish::RecoveryRequired);
        }
        self.capture().ok_or_else(no_stream)?;
        // ここから先で返るときは、予定を録音中のまま残さない（`settle_capture`）。
        let (row_id, guard) = self
            .recording
            .take()
            .map(|active| (active.row_id, active.guard))
            .unwrap_or_default();
        let guide_offset = self.guide_offset_at_start.take();
        // 台帳に載る前の失敗は、どこで落ちても何も確定していない。
        let not_committed = |e: AppError| e.with_outcome(koeru_failure::Outcome::NotCommitted);

        // 指示のあとも `TAIL_MS` ぶん書く（`TR-REC-19`）。ここで待つ。
        let stopped = self
            .pump()
            .ok_or_else(no_stream)
            .and_then(|p| p.finish_take().map_err(AppError::from));
        let finished = match stopped {
            Ok(f) => f,
            Err(e) => {
                self.settle_capture(capture);
                return Err(not_committed(e));
            }
        };

        // このテイクの中で増えたぶんの3つの数と、範囲に入った欠落の位置（`TR-REC-07`）。
        let gaps = self
            .lease
            .as_ref()
            .map_or_else(TakeGaps::default, |lease| guard.finish(lease));
        let discontinuities = gaps.discontinuities;

        // ## ここまででファイルは確定している。台帳はこの先
        //
        // 台帳に載らなければ、確定した WAV は孤児になる（`REQ-REC-006`）。
        if let Err(e) = self.session.finish_take() {
            self.settle_capture(capture);
            return Err(not_committed(e.into()));
        }
        // 無効化は、取りこぼし・不連続・レンダの失敗のどれかが増えたら（人が決めた）。
        let invalidated = gaps.invalidates_take();
        if invalidated {
            tracing::warn!(
                dropped = gaps.dropped,
                discontinuities,
                render_errors = gaps.render_errors,
                "取りこぼしたテイクを無効として載せる"
            );
        }
        // 行・収録セッション・WAV の場所は予定が持っている。 予定を閉じ、受領証を残すのも
        // テイクの行と同じ一手（`project-storage.fsl` の ASSUME-9）。
        let committed = self.writer()?.commit_capture(&CommitRequest {
            operation,
            capture,
            frames: i64::try_from(finished.samples.len()).unwrap_or(i64::MAX),
            recorded_at: &now_rfc3339(),
            valid: !invalidated,
        });
        let receipt = match committed {
            Ok(Commit::Committed(r)) => r,
            Ok(Commit::Answered(a)) => return Ok(Finish::Answered(a)),
            Err(e) => {
                self.settle_capture(capture);
                return Err(not_committed(e.into()));
            }
        };
        let take_id = receipt.take_id;
        self.seal_master_digest(take_id);

        // ## ここから先は派生物。落ちてもテイクは確定している
        //
        // 入力を集める → 計算する → 受理して書く、の順。 計算は `JobRuntime` の作業スレッドで
        // 走り、応答は解析を待つ（`DEC-PLT-047`。待たずに応答する形は T09 / T12）。
        let frames = finished.samples.len();
        let duration_ms = frames as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);
        let preroll_ms = finished.preroll_frames as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);
        let facts = TakeFacts {
            counts: koeru_core::db::TakeGapCounts {
                discontinuities,
                dropped: gaps.dropped,
                render_errors: gaps.render_errors,
                overflowed: gaps.overflowed,
            },
            gaps: gaps.entries.clone(),
            guide_offset,
            preroll_frames: finished.preroll_frames,
            duration_ms,
        };
        let derived = self.derive_take(take_id, &row_id, finished, facts);
        // 記録は画面へ渡すときに1回だけ（`crate::error::AppError` の直列化）。
        // `JobRuntime` が仕事の状態の遷移を記録しているが、それは失敗の記録（`record`）とは別の
        // 行で、ここで重ねて `record` はしない。
        let followup = derived
            .failure
            .map(|e| e.with_outcome(koeru_failure::Outcome::Committed));

        // ## 背後で前処理を進める（`TR-SYN-04`, `TR-SYN-34`）
        //
        // 完了期限は「次の録音項目まで」ではなく「試唱押下まで」。
        // 3時間の収録の途中で、次のフレーズを出すのを待たせない。
        //
        // いま録ったものを含むフレーズを、操作を待たずに合成しておく（`TR-SYN-04`）。
        // 押されたときには、もう出来ている。
        self.prerender_songs();

        let result = TakeResult {
            take_id,
            row_id,
            duration_ms,
            peak: derived.peak,
            thumbnail: derived.thumbnail,
            oto: derived.oto,
            confidence: derived.confidence,
            discontinuities,
            dropped: gaps.dropped,
            render_errors: gaps.render_errors,
            invalidated,
            metrics: derived.metrics,
            preroll_ms,
            followup,
        };
        Ok(Finish::Committed {
            result: Box::new(result),
            receipt,
        })
    }

    /// 確定したマスターの内容の指紋を、台帳へ1度だけ記録する（`DEC-PLT-053`）。
    ///
    /// ファイルは確定のあとに読む。 台帳の行と同じトランザクションには入れられない。
    /// **落ちても録音は確定している**ので、失敗は返さず記録だけ残す——空のまま残った指紋は、
    /// 要るときに [`Self::master_digest`] が計算して置く。
    fn seal_master_digest(&mut self, take_id: i32) {
        let rel_path = self
            .writer()
            .and_then(|mut w| w.take(take_id).map_err(AppError::from))
            .and_then(|t| t.ok_or_else(unknown_take))
            .map(|t| t.rel_path);
        let sealed = rel_path.and_then(|rel| self.master_digest(take_id, &rel));
        if let Err(e) = sealed {
            e.with_outcome(koeru_failure::Outcome::Committed)
                .record("capture.master_digest");
        }
    }

    /// テイクの派生物を作る。 入力を集め、`JobRuntime` で計算し、受理して置く。
    ///
    /// 失敗を返さない（テイクは確定している）。 落ちた工程の失敗は [`DerivedTake::failure`]
    /// に入れ、画面へ返す値（波形の要約・計測）は、手元の標本から作れるぶんを必ず返す。
    fn derive_take(
        &mut self,
        take_id: i32,
        row_id: &str,
        finished: crate::pump::Finished,
        facts: TakeFacts,
    ) -> DerivedTake {
        let discontinuities = facts.counts.discontinuities;
        let input = self.collect_take_analysis(take_id, row_id, finished, facts);
        let mut derived = self.analyze_through_jobs(input);
        // 録れた綴りを、もう片方の出どころで二度読ませない（`DEC-RCL-016`）。
        // 原音設定を作れなくても、解析が取り消されても、録れてはいる。 解析の受理とは
        // 別に、確定した事実から導く。
        let reconciled = if discontinuities == 0 {
            self.reconcile_other_origin(row_id)
        } else {
            Ok(())
        };
        // 落ちた工程の失敗が先。 組み直しの失敗はその次（以前と同じ並び）。
        derived.failure = derived.failure.take().or_else(|| reconciled.err());
        derived
    }

    /// 解析を `JobRuntime` に積み、結果を待って、受理する（`DEC-PLT-047`）。
    ///
    /// テイクの確定の応答は当面、解析を待つ（T09 / T12 で待たない形を決める）。 待つ間も
    /// 状態ロックは握ったまま——ほかの操作は待たされる。 以前の同期の解析と同じ。
    ///
    /// 保存済みのテイクの解析は、上限に当たっても失敗にしない（`OverflowPolicy::Defer`）。
    /// 期限を過ぎたら失敗として返し、自動でやり直さない。
    ///
    /// **失敗は、テイクを取り消さない。** 取り消し・期限切れ・公開の拒否・上限は、
    /// `DerivedTake::failure` に `jobs.*` の code で載り、`TakeResult::followup` に
    /// 確定済みとして出る。 解析の値は無いので、画面へ返す値は手元の標本から作る。
    fn analyze_through_jobs(&mut self, input: TakeAnalysisInput) -> DerivedTake {
        let (take_id, row_id) = (input.take_id, input.row_id.clone());
        // 計算の値が無いときの、画面へ返す値の元。 持ち出すのは持ち手だけ。
        let samples = Arc::clone(&input.samples);
        let rate = input.rate;
        let degraded = move |failure: AppError| DerivedTake::from_samples(&samples, rate, failure);

        let lease = match self.opened() {
            Ok(open) => open.lease,
            Err(e) => return degraded(e),
        };
        let (based_on, bytes, deadline) = (input.based_on(), input.input_bytes(), self.deadline());
        // 計算の closure が受け取るのは取消の合図だけ。 台帳の書き手も `Studio` も持ち込まない。
        let spec = JobSpec::new(
            lease,
            JobKind::Analysis,
            Priority::PostRecording,
            move |probe| {
                take_analysis::analyze_take(&input, &|| probe.check()).map_err(ComputeError::from)
            },
        )
        .overflow(OverflowPolicy::Defer)
        .input_bytes(bytes)
        .deadline(deadline)
        .based_on(based_on);

        let runtime = Arc::clone(&self.jobs);
        let completed = match runtime.submit(spec).and_then(jobs::JobTicket::wait) {
            Ok(completed) => completed,
            Err(e) => return degraded(AppError::from_failure(e)),
        };

        // 受理の直前に、計算が読んだ入力の今の値を同じ名前で作り直す。 いまは状態ロックを
        // 握ったまま待つので変わらないが、待たない形（T09 / T12）にすると効く。
        let current = self.current_analysis_based_on(take_id, &row_id);
        let receipt = jobs::Receipt::new(u64::try_from(take_id).unwrap_or_default());
        // `admit` の中から `JobRuntime` の API を呼ばない（公開中の取消は公開の終わりを待つ）。
        match runtime.publish(completed, &current, |done| {
            self.admit_take_analysis_job(done, receipt)
        }) {
            Ok(published) => published.value,
            // 公開しなかった。 エンジンの失敗ではない（古い・取り消された・置き換えられた・貸与が閉じた）。
            Err(PublishError::Refused(reason)) => degraded(AppError::from_failure(reason)),
            // 置けたところまでは置いてある。 仕事は `Failed` で、結果は画面へ返す。
            Err(PublishError::Admit(mut unfinished)) => {
                unfinished.derived.failure = Some(unfinished.failure);
                *unfinished.derived
            }
        }
    }

    /// 解析の期限。 試験は短くできる。
    fn deadline(&self) -> std::time::Duration {
        #[cfg(any(test, feature = "test-hooks"))]
        if let Some(deadline) = self.test_analysis_deadline {
            return deadline;
        }
        ANALYSIS_DEADLINE
    }

    /// `JobRuntime::publish` の `admit`。 計算の出力を受理して、台帳とファイルへ置く。
    ///
    /// 貸与が生きているかの最終確認は、まず `ProjectRuntime::writer` が担う
    /// （`RuntimeError::LeaseExpired`）。 通ったら [`Self::admit_take_analysis`] が置く。
    ///
    /// **仕事の状態は、置けたかで決める。** 全部置けたら `Succeeded`（受領証はテイクの ID）。
    /// 途中の工程が落ちたら `Failed`——失敗の code と分類は落ちた工程のもの。 `JobRuntime` が
    /// 確定の状態を失敗の分類の既定から決めるので、置けた工程があっても `Failed` の確定の
    /// 状態は分類の既定になる。 画面へ返す `followup` は確定済みで出す。
    fn admit_take_analysis_job(
        &mut self,
        done: jobs::Completed<TakeAnalysisOutput>,
        receipt: jobs::Receipt,
    ) -> std::result::Result<(DerivedTake, jobs::Receipt), Unfinished> {
        let alive = self.opened().and_then(|open| {
            open.runtime
                .writer(done.lease())
                .map(drop)
                .map_err(AppError::from)
        });
        let output = done.into_value();
        if let Err(failure) = alive {
            return Err(Unfinished {
                derived: Box::new(DerivedTake::unplaced(&output)),
                failure,
            });
        }
        let mut derived = self.admit_take_analysis(output);
        match derived.failure.take() {
            None => Ok((derived, receipt)),
            Some(failure) => Err(Unfinished {
                derived: Box::new(derived),
                failure,
            }),
        }
    }

    /// 受理の直前の、計算が読んだ入力の今の値（[`take_analysis::analysis_based_on`]）。
    fn current_analysis_based_on(&mut self, take_id: i32, row_id: &str) -> jobs::BasedOn {
        let (purpose, floor) = self.f0_conditions();
        let row = self.read_row(row_id);
        take_analysis::analysis_based_on(take_id, purpose, floor, &row)
    }

    /// F0 の条件。 音域を集めた数から決める（`TR-SYN-22`）。
    ///
    /// 音域は開いているプロジェクトのもの（`ProjectCaches`）。 開いていなければ
    /// 溜めるものが無いので、初めのテイクと同じ DIO の条件で推定する。
    fn f0_conditions(&self) -> (f0::Purpose, Option<f64>) {
        let (observed, floor) = self.open.as_ref().map_or((0, None), |o| {
            (o.caches.observed_f0.len(), o.caches.f0_floor)
        });
        let purpose = if observed >= f0::RANGE_SAMPLE_TAKES {
            f0::Purpose::Distribution
        } else {
            f0::Purpose::Preview
        };
        (purpose, floor)
    }

    /// 入力を集める。 台帳から読むだけで、何も書かない。
    ///
    /// 読めなかったものは失敗ごと入力に持つ（[`take_analysis`] の冒頭）。 計算は
    /// その読みを要る段に着いたときに出力へ移す。
    fn collect_take_analysis(
        &mut self,
        take_id: i32,
        row_id: &str,
        finished: crate::pump::Finished,
        facts: TakeFacts,
    ) -> TakeAnalysisInput {
        let (purpose, floor) = self.f0_conditions();
        TakeAnalysisInput {
            take_id,
            row_id: row_id.to_owned(),
            wav_path: finished.path,
            // 複写しない。 持ち出すのは持ち手だけ。
            samples: Arc::new(finished.samples),
            rate: MASTER_RATE_HZ,
            f0_purpose: purpose,
            f0_floor: floor,
            facts,
            row: self.read_row(row_id),
            aligner: self.aligner.clone(),
        }
    }

    /// 行についての事実を台帳から読む。 読みの失敗は値で持つ。
    ///
    /// どれも、テイクの解析値・計測・境界の書き込みに左右されない（持ち主にも集団にも
    /// 効かない）。 だから以前のように書き込みのあいだではなく、計算の前にまとめて読める。
    fn read_row(&mut self, row_id: &str) -> RowReads {
        let setup = self.current_preset().and_then(|preset| {
            Ok(Setup {
                method: preset.method,
                set: preset.set,
                rules: self.current_rules()?,
            })
        });
        let kana = self
            .writer()
            .and_then(|mut w| w.units_of(row_id).map_err(AppError::from));
        let line = match &setup {
            Ok(s) => self
                .writer()
                .and_then(|mut w| w.row_units_of(row_id, s.set).map_err(AppError::from)),
            Err(e) => Err(e.clone()),
        };
        let populations = self.populations();
        let tone = self
            .writer()
            .and_then(|mut w| Ok(w.row_tones()?.get(row_id).copied().unwrap_or_default()));
        let owned = self
            .writer()
            .and_then(|mut w| w.owned_aliases_of_row(row_id).map_err(AppError::from));
        RowReads {
            setup,
            kana,
            line,
            populations,
            tone,
            owned,
        }
    }

    /// 計算の出力を受理して、台帳とファイルへ置く。 解析・`.frq`・計測・境界・oto・確認キュー。
    ///
    /// 失敗を返さない。 最初に落ちた工程の失敗を [`DerivedTake::failure`] に入れ、
    /// そこまでに置けたものを返す。 計測と波形の要約は台帳に置けなくても画面へ返す
    /// ——測るのは手元の標本だけで済む。
    ///
    /// もう片方の出どころの組み直し（`reconcile_other_origin`）はここに含めない。 解析が
    /// 取り消されても、確定した事実から導けるので、[`Self::derive_take`] が別に呼ぶ。
    fn admit_take_analysis(&mut self, output: TakeAnalysisOutput) -> DerivedTake {
        let TakeAnalysisOutput {
            take_id,
            wav_path,
            f0,
            analysis,
            measured,
            steps,
            failure,
        } = output;

        self.note_f0_range(f0);

        let mut out = DerivedTake::unplaced_from(&analysis, measured);
        out.failure = self
            .place_steps(take_id, &wav_path, &analysis, steps, &mut out)
            // 計算が途中で落ちていたら、置けた分の次がその失敗。
            .and_then(|()| failure.map_or(Ok(()), Err))
            .err();
        out
    }

    /// 音域を溜めて、集まったら下限を引き上げる（`TR-SYN-22`）。
    ///
    /// このテイクの推定は更新前の値で済んでいる。 変わるのは次のテイクの条件だけ。
    fn note_f0_range(&mut self, f0: Vec<f64>) {
        if let Some(open) = self.open.as_mut() {
            let caches = &mut open.caches;
            caches.observed_f0.push(f0);
            if caches.f0_floor.is_none()
                && let Some(floor) = f0::tighten_floor(&caches.observed_f0)
            {
                tracing::info!(floor_hz = floor, "話者音域から探索の下限を上げた");
                caches.f0_floor = Some(floor);
            }
        }
    }

    /// [`Self::admit_take_analysis`] のうち、台帳とファイルへ置く工程。
    /// 上から順に置き、最初に落ちた工程で止まる。 作れた順に `out` を埋める。
    fn place_steps(
        &mut self,
        take_id: i32,
        wav_path: &std::path::Path,
        analysis: &TakeAnalysis,
        steps: Vec<Step>,
        out: &mut DerivedTake,
    ) -> Result<()> {
        for step in steps {
            match step {
                Step::Analysis => {
                    self.writer()?.put_analysis(take_id, analysis)?;
                    frq::write(&analysis.frq, &frq::frq_path(wav_path)?)?;
                }
                Step::AlignmentFailed(e) => {
                    e.with_outcome(koeru_failure::Outcome::Committed)
                        .record("take.align");
                }
                Step::Measured(m) => {
                    let m = *m;
                    self.writer()?.put_metrics(
                        take_id,
                        &m.metrics,
                        m.counts,
                        m.preroll_frames,
                        m.guide_offset,
                    )?;
                    self.writer()?.put_take_gaps(take_id, &m.gaps)?;
                    out.metrics = m.metrics;
                }
                Step::Boundaries(saved) => {
                    self.writer()?.put_boundaries(take_id, &saved)?;
                }
                Step::Entries(e) => {
                    let e = *e;
                    for row in &e.rows {
                        self.writer()?.put_oto(
                            take_id,
                            &row.alias,
                            &row.oto,
                            row.score,
                            Some(&row.parts),
                            false,
                        )?;
                        self.writer()?
                            .set_branch_mismatch(take_id, &row.alias, row.mismatch)?;
                    }
                    self.writer()?.put_fingerprint(take_id, &e.fingerprint)?;
                    out.oto = e.first.map(|(oto, _)| oto);
                    out.confidence = e.first.map(|(_, score)| score);
                }
                Step::Enqueue => self.enqueue_take(take_id)?,
            }
        }
        Ok(())
    }

    /// ファイル名を NFC に揃える（`TR-REC-32`）。
    ///
    /// macOS はファイル作成後の名前を分解形で返すことがある。
    /// 揃えないと、同じ「が」が別の文字列として台帳と食い違う。
    /// 書き出しの直前にも通す。 分解形のまま配ると、受け手の環境で見つからない。
    ///
    /// 返るのは直した数。
    #[tracing::instrument(skip(self))]
    pub fn normalize_file_names(&mut self) -> Result<usize> {
        let dir = self.opened()?.dir().audio_dir();
        Ok(koeru_core::text::normalize_names_to_nfc(&dir)?)
    }

    /// NFC でない名前が残っていないか（`TR-REC-32`）。
    ///
    /// 書き出しの関門。 残っていたら書き出さない。
    #[tracing::instrument(skip(self))]
    pub fn non_nfc_names(&mut self) -> Result<Vec<String>> {
        let dir = self.opened()?.dir().audio_dir();
        Ok(koeru_core::text::find_non_nfc_names(&dir)?)
    }

    /// 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
    ///
    /// 収録中の判定ではない。 ここでだけ、壊れた成果物が完成へ到達する経路を塞ぐ。
    #[tracing::instrument(skip(self))]
    pub fn preflight(&mut self) -> Result<Preflight> {
        // 名前は先に直す。 直せるものを関門で止めない。
        let renamed = self.normalize_file_names()?;
        let non_nfc = self.non_nfc_names()?;
        let clipped = self.writer()?.clipped_adopted_takes()?;
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
    #[tracing::instrument(skip(self))]
    pub fn package_settings(&mut self) -> Result<koeru_core::db::Distribution> {
        let manifest = self.opened()?.dir().read_manifest()?;
        packaging::settings(&mut *self.writer()?, &manifest)
    }

    /// 配布に出す値を保存する（`PROFILE-M4`）。
    #[tracing::instrument(skip(self, d))]
    pub fn set_package_settings(&mut self, d: &koeru_core::db::Distribution) -> Result<()> {
        packaging::check_settings(d)?;
        let mut ledger = self.writer()?;
        // フォルダ名と下書きの版は配布物ごとの値（`DEC-PKG-018`）。 配布物が1つだった頃の画面は
        // これを音源の設定として読み書きするので、先頭の配布物へ書く。 断られたら何も書かない。
        if let Some(first) = ledger.distributions()?.into_iter().next() {
            let version = d.version.as_deref().filter(|v| !v.is_empty());
            let change = koeru_core::db::shelf::DistributionRevision {
                archive_name: Some(&d.distribution_name),
                draft_version: version,
                clear_draft_version: version.is_none(),
                ..Default::default()
            };
            if let Err(refusal) = ledger.revise_distribution(first.id, &change)? {
                return Err(refused_package_settings(refusal));
            }
        }
        ledger.set_distribution(d)?;
        Ok(())
    }

    /// いま書き出せるか（`TR-PKG-49`, `TR-PKG-51`）。
    #[tracing::instrument(skip(self))]
    pub fn package_state(&mut self) -> Result<packaging::PackageState> {
        let dir = self.opened()?.dir().clone();
        let manifest = dir.read_manifest()?;
        let gates = self.package_gates()?;
        let rules = self.current_rules()?;
        packaging::state(&dir, &mut *self.writer()?, &rules, &manifest, gates)
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
    #[tracing::instrument(skip(self))]
    fn package_gates(&mut self) -> Result<packaging::Gates> {
        let missing = self.writer()?.adopted_rows_without_oto()?;
        let conflicting = self.writer()?.adopted_conflicting_aliases()?;
        let confirmed = self.opened()?.review.all_confirmed();
        Ok(packaging::Gates {
            otos_ready: confirmed && missing.is_empty() && conflicting.is_empty(),
        })
    }

    /// 配布物に入るファイルの一覧（`TR-PKG-28` の同梱物）。
    #[tracing::instrument(skip(self))]
    pub fn package_contents(&mut self) -> Result<Vec<(String, u64)>> {
        let dir = self.opened()?.dir().clone();
        let manifest = dir.read_manifest()?;
        let rules = self.current_rules()?;
        packaging::preview(&dir, &mut *self.writer()?, &rules, &manifest)
    }

    /// 書き出す（`REQ-PKG-105`, `REQ-PKG-106`）。
    ///
    /// 先に `TR-REC-32` の関門を通す。 素材の名前が受け手の環境で
    /// 見つからなくなる状態のまま包まない。
    ///
    /// 版の札は受け取らない。 配布に出す値として保存してあるものを使う
    /// （`TR-PKG-44`）——**2箇所で打たせると、配布物と履歴で違う値になる。**
    #[tracing::instrument(skip(self))]
    pub fn export_package(&mut self) -> Result<packaging::Exported> {
        // 先頭の配布物を、その下書きの番号で（`DEC-PKG-018`）。 配布物と版を選ぶ口は
        // `export_distribution`。
        let target = self.first_target()?;
        self.export_package_to(&target)
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
    #[tracing::instrument(skip(self), fields(method = method.as_str()))]
    pub fn export_downgrade(&mut self, method: Method) -> Result<packaging::Exported> {
        let target = self.first_target()?;
        self.export_downgrade_to(method, &target)
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
    #[tracing::instrument(skip(self), fields(seq))]
    pub fn reveal_release(&mut self, seq: i32) -> Result<()> {
        let dir = self.opened()?.dir().exports_dir();
        let release = self
            .writer()?
            .releases()?
            .into_iter()
            .find(|r| r.seq == seq)
            .ok_or_else(|| {
                AppError::new(
                    "package.unknown_release",
                    Class::Conflict,
                    "その書き出しの記録が無い",
                )
            })?;

        let path = dir.join(&release.archive_name);
        if !path.is_file() {
            // 本人が配り物を動かした・消したこともありうる。記録を直さず、無いと伝える。
            return Err(AppError::new(
                "package.archive_missing",
                Class::Rejected,
                "その配り物が見つからない",
            ));
        }
        // OS の失敗の中身はパスを含みうるので捨てる。
        tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|_| {
            AppError::new(
                "package.reveal_failed",
                Class::Unsupported,
                "配り物の置き場所を開けない",
            )
        })
    }

    /// 書き出しの履歴（`TR-PKG-44`）。古い順。
    #[tracing::instrument(skip(self))]
    pub fn releases(&mut self) -> Result<Vec<koeru_core::release::Release>> {
        Ok(self.writer()?.releases()?)
    }

    /// 収録済みのテイクを、指定した音高で合成する。鳴らさない。
    ///
    /// 周波数表は台帳から取る。 書き出しのためだけでなく、試唱もここを使う
    /// （`TR-PKG-05` の「再推定を要しない」）。
    ///
    /// 返るのは `(サンプル, サンプルレート)`。
    #[tracing::instrument(skip(self))]
    pub fn render_take(
        &mut self,
        take_id: i32,
        midi: i32,
        length_ms: f64,
    ) -> Result<(Vec<f32>, u32)> {
        let root = self.opened()?.dir().root().to_path_buf();
        let take = self.writer()?.take(take_id)?.ok_or_else(unknown_take)?;
        // 1テイクに複数のエントリがある（`DEC-ALN-013`）。試聴は先頭の1つで鳴らす。
        // 並びはエイリアス順で常に同じ（`TR-ALN-29`）。
        //
        // **素材の解決（`DEC-PLT-052`）を通していない。** 引き数が綴りではなくテイクで、
        // 「このテイクを聴く」入口だから。 どの綴りを鳴らすかを決める経路ではないので、
        // 持ち主の規則は掛からない。 先頭の1つを選ぶ決め方を解決へ寄せるなら、
        // 候補に「テイクの代表」という用途を足す判断が要る（T08c の後続、未決）。
        let oto = self
            .writer()?
            .otos_of(take_id)?
            .into_iter()
            .next()
            .map(|(_, o)| o)
            .ok_or_else(|| {
                AppError::new(
                    "app.no_oto",
                    Class::Rejected,
                    "そのテイクにまだ原音設定が無い",
                )
            })?;
        let analysis = self.writer()?.analysis_of(take_id)?;

        let w = wav::read(root.join(&take.rel_path))?;
        let samples: Vec<f64> = w.samples.iter().map(|s| f64::from(*s)).collect();
        let table = analysis.map(|a| a.frq.f0).unwrap_or_default();

        let out = render(&RenderRequest {
            samples: &samples,
            sample_rate_hz: w.rate_hz,
            // `tone` は鳴らしたい音高。収録音高ではない（resampler の doc を参照）。
            // ここに収録音高を渡すと、どの音高を選んでも同じ高さで鳴る。
            tone: midi,
            oto,
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
    #[tracing::instrument(skip(self))]
    pub fn preview(&mut self, take_id: i32, midi: i32, length_ms: f64) -> Result<usize> {
        let (pcm, rate) = self.render_take(take_id, midi, length_ms)?;
        let n = pcm.len();

        // 前の出力は止める。 重ねると何を聴いているか分からなくなる（`DEC-REC-011`）。
        self.output.stop();
        self.output.set_clip(mac::play(pcm, rate)?);
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
        // 写しは、いま開いているプロジェクトのもの。 仕事が捕まえた `Arc` は、
        // プロジェクトを移れば（`release_project` が貸与の仕事を取り消す）使われない。
        let Some((cache, lease)) = self
            .open
            .as_ref()
            .map(|o| (Arc::clone(&o.caches.song_cache), o.lease))
        else {
            return;
        };
        // **中身は以前のまま。** 実際には何も温めていない（`held` を数えて記録するだけ）。
        // `Workers` から `JobRuntime` へ移しただけで、温める計算を足してはいない。
        let spec = JobSpec::new(
            lease,
            JobKind::PreviewRender,
            Priority::PostRecording,
            move |_probe| {
                // 鍵に素材の内容ハッシュが入っているので、古い結果は自然に使われない。
                // ここでできるのは、次に押されたときに載っている確率を上げることだけ。
                let held = cache.lock().map_or(0, |c| c.len());
                tracing::debug!(held, singable, "試唱の前処理を進めた");
                Ok(())
            },
        )
        // 積み重ならない。 前処理は「いまの状態」を温めるだけなので、待っている古い
        // 前処理は新しいものに置き換えてよい。
        .overflow(OverflowPolicy::Coalesce(CoalesceKey::new(PRERENDER_KEY)));
        // 完了の callback は何も書かずに公開して仕事を終わらせ、`JobRuntime` を弱く持つ
        // （試唱の仕事と同じもの。 理由は `preview::publish_nothing`）。
        let submitted = self
            .jobs
            .submit_notify(spec, preview::publish_nothing(&self.jobs));
        match submitted {
            Ok(handle) => self.prerender = Some(handle.job()),
            // 前処理が積めなくても、押されたときに合成するだけ。 失敗にしない。
            Err(e) => tracing::debug!(code = e.code(), "試唱の前処理を積めなかった"),
        }
    }

    /// 背後で待っている仕事の数（`TR-SYN-33`）。
    ///
    /// 「録音終了 → 試唱ボタン活性化」の間に、無言の待ち時間を作らない（`TR-SYN-33`）。
    /// 画面はこれを見て、進んでいることを出す。
    ///
    /// 状態ロックの外から読むときは [`Self::jobs`] の写しから [`JobRuntime::counts`] を引く。
    #[must_use]
    pub fn pending_work(&self) -> usize {
        self.jobs.counts().pending
    }

    /// 見えている範囲の波形（`TR-PLT-04`）。
    ///
    /// 読む量は画素数に比例する。 範囲の広さには比例しない。
    /// 段はテイクごとに一度だけ積んで持ち回す。
    #[tracing::instrument(skip(self), fields(take_id, pixels))]
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
    #[tracing::instrument(skip(self), fields(take_id, columns, rows))]
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
        if let Some((map, rate)) = self.opened()?.caches.mipmaps.get(&take_id) {
            return Ok((Arc::clone(map), *rate));
        }
        let (samples, rate) = self.samples_of(take_id)?;
        let map = Arc::new(waveform::Mipmap::build(&samples));
        // 上限を置く。 3時間ぶんの段を全部持つとメモリが尽きる。
        let mipmaps = &mut self.opened_mut()?.caches.mipmaps;
        if mipmaps.len() >= MIPMAP_CACHE {
            mipmaps.clear();
        }
        mipmaps.insert(take_id, (Arc::clone(&map), rate));
        Ok((map, rate))
    }

    /// テイクの波形を読む。
    fn samples_of(&mut self, take_id: i32) -> Result<(Vec<f32>, u32)> {
        let root = self.opened()?.dir().root().to_path_buf();
        let take = self.writer()?.take(take_id)?.ok_or_else(unknown_take)?;
        let w = wav::read(root.join(&take.rel_path))?;
        Ok((w.samples, w.rate_hz))
    }

    /// 包絡の持ち手（`TR-REC-43`）。状態ロックの外から読むために出す。
    ///
    /// マイクを選ぶ前は無い。
    #[must_use]
    pub fn envelope_handle(&self) -> Option<Arc<Mutex<crate::pump::Envelope>>> {
        self.pump().map(Pump::envelope_handle)
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
        let tones = self.writer()?.row_tones()?;
        // 行ごとに「綴り → 仮名」を1度だけ組む。 エントリは行の数より多い。
        let mut readings: HashMap<String, HashMap<String, &'static str>> = HashMap::new();
        // 先に取り出す。 `for` の式に書くと guard が繰り返しのあいだ生き、本体の `writer()` が
        // 自分自身を待つ。
        let adopted = self.writer()?.adopted_otos()?;
        for e in adopted {
            if !readings.contains_key(&e.row_id) {
                let line = self.writer()?.row_units_of(&e.row_id, here.set)?;
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

    /// 確定したテイクのエントリを確認キューへ入れる（`REQ-ALN-002`）。
    ///
    /// 全件キューへ入れる。 確信度に絶対閾値を置かない（`TR-ALN-25` の
    /// 「確信度の閾値は絶対値で固定せず、確認キューの運用で切る」）。
    /// どこまで見るかは合計所要時間の上限が決める（`DEC-ALN-003`）。
    ///
    /// 固定した値は引き継ぐ（`REQ-ALN-007`）。 録り直しは新しいテイクの行を作るので、
    /// 引き継がないと、人が直した値が自動の値で上書きされる（`INV-ALN-001`）。
    ///
    /// 左ブランクは発声の始まりに合わせて写す（`DEC-ALN-019`）。 揃えられなければ
    /// 絶対位置のまま写し、どちらも確認キューに入る。
    fn enqueue_take(&mut self, take_id: i32) -> Result<()> {
        // 録り足したものを書き出せるようにする（`TR-PKG-44`）。
        self.start_new_export_generation()?;
        let aliases: Vec<String> = self
            .writer()?
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
            .writer()?
            .take(take_id)?
            .map(|t| t.row_id)
            .unwrap_or_default();
        let tone = self
            .writer()?
            .row_tones()?
            .get(&row_id)
            .copied()
            .unwrap_or_default();

        // 発声の始まりはテイクごとに1度だけ引く。 前の世代は綴りごとに別のテイクでありうる。
        let mut onsets_by_take: HashMap<i32, std::collections::BTreeMap<String, f64>> =
            HashMap::new();
        let (mut shifted, mut absolute) = (0_usize, 0_usize);
        for alias in &aliases {
            // 前の世代に固定があったものだけ運ぶ。
            let key = crate::review::EntryKey::new(tone, alias.as_str()).handle();
            let Some((prev, pins, which)) = self
                .opened()?
                .review
                .get(&key)
                .filter(|e| e.pins().iter().any(|p| *p))
                .map(|e| {
                    let which = Pins {
                        offset: e.is_pinned(Slot::Offset),
                        consonant: e.is_pinned(Slot::Consonant),
                        cutoff: e.is_pinned(Slot::Cutoff),
                        preutterance: e.is_pinned(Slot::Preutterance),
                        overlap: e.is_pinned(Slot::Overlap),
                    };
                    (e.oto, e.pins(), which)
                })
            else {
                continue;
            };
            let Some(fresh) = self.writer()?.oto_of(take_id, alias)? else {
                continue;
            };
            let prev_take = self.opened()?.review_takes.get(&key).copied();
            let mut onset_in = |take: Option<i32>| -> Result<Option<f64>> {
                let Some(take) = take else { return Ok(None) };
                if let std::collections::hash_map::Entry::Vacant(slot) = onsets_by_take.entry(take)
                {
                    slot.insert(self.onsets_of_take(take)?);
                }
                Ok(onsets_by_take
                    .get(&take)
                    .and_then(|m| m.get(alias))
                    .copied())
            };
            let onsets = Onsets {
                previous: onset_in(prev_take)?,
                next: onset_in(Some(take_id))?,
            };
            let carried = retake::carry(&prev, &fresh, which, onsets);
            match carried.anchor {
                Anchor::NotPinned => {}
                Anchor::Shifted { .. } => shifted += 1,
                Anchor::Absolute(why) => {
                    absolute += 1;
                    tracing::info!(
                        reason = why.as_str(),
                        "固定した左ブランクを当て直せなかった"
                    );
                }
            }
            // 値と固定を一度に書く。 別々に流すと、固定だけ落ちたときに
            // 「人の値なのに固定が無い」行が残り、次の録音で上書きされる。
            let open = self.opened_mut()?;
            open.writer()?.put_review_entry(
                take_id,
                alias,
                &carried.oto,
                EntryState::InQueue.as_str(),
                pins,
            )?;
        }
        if shifted + absolute > 0 {
            tracing::info!(shifted, absolute, "固定した左ブランクを写した");
        }

        self.refresh_review()
    }

    /// そのテイクで、綴りごとに乗るモーラの発声の始まり（`DEC-ALN-019`）。
    ///
    /// 境界を残していないテイクなら空。
    fn onsets_of_take(&mut self, take_id: i32) -> Result<std::collections::BTreeMap<String, f64>> {
        let Some(row_id) = self.writer()?.take(take_id)?.map(|t| t.row_id) else {
            return Ok(std::collections::BTreeMap::new());
        };
        let here = self.current_preset()?;
        let rules = self.current_rules()?;
        let line = self.writer()?.row_units_of(&row_id, here.set)?;
        let entries = koeru_core::reclist::row_entries(&rules, here.method, &line);
        let saved = self.writer()?.boundaries_for_take(take_id)?;
        Ok(retake::onsets(&entries, &saved))
    }

    /// 確認の進み具合（`TR-ALN-25`, `TR-ALN-28`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない。
    pub fn review_summary(&mut self) -> Result<ReviewSummary> {
        let method = self.opened()?.dir().read_manifest()?.method;
        let missing = self.writer()?.adopted_rows_without_oto()?.len();
        let conflicting = self.writer()?.adopted_conflicting_aliases()?.len();
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
        let tones = self.writer()?.row_tones()?;
        let rows: HashMap<String, String> = self
            .writer()?
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
            cause: if e.branch_mismatch {
                Some(BRANCH_MISMATCH_CAUSE.to_owned())
            } else {
                e.confidence
                    .and_then(|c| c.cause(CAUSE_THRESHOLD))
                    .map(|c| c.kind().to_owned())
            },
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
    #[tracing::instrument(skip(self, key))]
    pub fn confirm_entry(&mut self, key: &str) -> Result<()> {
        self.with_entry(key, |q, id| q.confirm(id))
    }

    /// まとめて確認する（`REQ-ALN-010`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、まとめて確認モードでない。
    #[tracing::instrument(skip(self))]
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
    #[tracing::instrument(skip(self, to))]
    pub fn switch_review_mode(&mut self, to: &str) -> Result<()> {
        let mut next = self.opened()?.review.clone();
        match to {
            "batch" => next.switch_to_batch().map_err(review_error)?,
            "suggest_rerecord" => next.switch_to_rerecord().map_err(review_error)?,
            _ => {
                return Err(AppError::new(
                    "review.unknown_mode",
                    Class::InvalidInput,
                    "知らない確認の進め方",
                ));
            }
        }
        let open = self.opened_mut()?;
        crate::review::save_mode(&mut *open.writer()?, &next, true)?;
        open.review = next;
        Ok(())
    }

    /// 5値のどれかを人が直す。その値だけを固定する（`REQ-ALN-005`, `TR-ALN-30`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、知らない値の名前、まだ推定していない。
    #[tracing::instrument(skip(self, key, slot))]
    pub fn edit_oto_value(&mut self, key: &str, slot: &str, value: f64) -> Result<()> {
        let s = slot_of(slot).ok_or_else(unknown_slot)?;
        self.with_entry(key, |q, id| q.human_edit(id, s, value))
    }

    /// 固定を解いて自動へ戻す（`REQ-ALN-006`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、知らない値の名前、固定されていない。
    #[tracing::instrument(skip(self, key, slot))]
    pub fn revert_oto_value(&mut self, key: &str, slot: &str) -> Result<()> {
        let s = slot_of(slot).ok_or_else(unknown_slot)?;
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
            // 固定を解いたことは確定している。 記録するのは再推定の失敗だけ。
            e.with_outcome(koeru_failure::Outcome::Committed)
                .record("review.revert.re_estimate");
        }
        Ok(())
    }

    /// そのテイクの oto を作り直す（`REQ-ALN-007`, `TR-ALN-29`）。
    ///
    /// 固定されていない値だけを書き換える（`INV-ALN-001`）。固定はキューが守る
    /// （[`ReviewQueue::re_estimate`]）ので、ここは新しい推定を渡すだけ。
    ///
    /// 入力を集める → 計算する → キューと台帳へ受理する、の3段。 同じスレッドで続けて走らせる。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、WAV を読めない、読みを音素へ写せない、
    /// 発声を見つけられない。
    #[tracing::instrument(skip(self))]
    pub fn re_estimate_take(&mut self, take_id: i32) -> Result<()> {
        let input = self.collect_re_estimate(take_id)?;
        let lease = self.opened()?.lease;
        let (row_id, rel_path) = (input.row_id.clone(), input.rel_path.clone());
        let (based_on, bytes) = (input.based_on(), input.input_bytes());

        // 計算の失敗（読みを読めない・揃えられない）は、画面へ返す元の失敗をそのまま返す。
        // 仕事の状態へは code と分類だけが載るので、全体は脇の持ち手で運ぶ。
        let failed: Arc<Mutex<Option<AppError>>> = Arc::new(Mutex::new(None));
        let carried = Arc::clone(&failed);
        // 背景の lane に載る（`TR-SYN-34`）。 同じテイクの再推定は積み重ならない。
        // 期限は解析と同じ仮の値（`Q-PLT-006`）。
        let spec = JobSpec::new(
            lease,
            JobKind::Analysis,
            Priority::Reestimate,
            move |probe| {
                take_analysis::re_estimate(&input, &|| probe.check()).map_err(|stop| match stop {
                    Stop::Cancelled(cancelled) => ComputeError::from(cancelled),
                    Stop::Failed(e) => {
                        let failure = JobFailure {
                            code: e.code,
                            class: e.class.into(),
                            // 計算は台帳へ書かない。
                            outcome: koeru_failure::Outcome::NotCommitted,
                        };
                        if let Ok(mut slot) = carried.lock() {
                            *slot = Some(e);
                        }
                        ComputeError::Failed(failure)
                    }
                })
            },
        )
        .overflow(OverflowPolicy::Coalesce(CoalesceKey::new(format!(
            "re_estimate:{take_id}"
        ))))
        .input_bytes(bytes)
        .deadline(self.deadline())
        .based_on(based_on);

        // 仕事の失敗は、何も書いていない。
        let not_committed = |e: AppError| e.with_outcome(koeru_failure::Outcome::NotCommitted);
        let runtime = Arc::clone(&self.jobs);
        let completed = match runtime.submit(spec).and_then(jobs::JobTicket::wait) {
            Ok(completed) => completed,
            Err(JobError::ComputeFailed(failure)) => {
                let original = failed.lock().ok().and_then(|mut slot| slot.take());
                return Err(original.unwrap_or_else(|| {
                    not_committed(AppError::from_failure(JobError::ComputeFailed(failure)))
                }));
            }
            Err(e) => return Err(not_committed(AppError::from_failure(e))),
        };

        let current = self.current_re_estimate_based_on(take_id, &row_id, &rel_path);
        let receipt = jobs::Receipt::new(u64::try_from(take_id).unwrap_or_default());
        // `admit` の中から `JobRuntime` の API を呼ばない（公開中の取消は公開の終わりを待つ）。
        match runtime.publish(completed, &current, |done| {
            self.admit_re_estimate_job(done, receipt)
        }) {
            Ok(_) => Ok(()),
            Err(PublishError::Refused(reason)) => {
                Err(not_committed(AppError::from_failure(reason)))
            }
            Err(PublishError::Admit(e)) => Err(e),
        }
    }

    /// `JobRuntime::publish` の `admit`。 貸与が生きているかを `ProjectRuntime::writer` で
    /// 確かめてから、キューと台帳へ受理する。
    fn admit_re_estimate_job(
        &mut self,
        done: jobs::Completed<take_analysis::ReEstimation>,
        receipt: jobs::Receipt,
    ) -> Result<((), jobs::Receipt)> {
        self.opened()?.runtime.writer(done.lease()).map(drop)?;
        self.admit_re_estimate(done.into_value())?;
        Ok(((), receipt))
    }

    /// 受理の直前の、推定し直しが読んだ入力の今の値（[`take_analysis::re_estimate_based_on`]）。
    fn current_re_estimate_based_on(
        &mut self,
        take_id: i32,
        row_id: &str,
        rel_path: &str,
    ) -> jobs::BasedOn {
        let wav = self.master_stamp(take_id, rel_path);
        let row = self.read_row(row_id);
        take_analysis::re_estimate_based_on(take_id, &wav, &row)
    }

    /// テイクの WAV の指紋の印（`DEC-PLT-053`）。 開いていなければ空（読んだ標本が無いので、
    /// 突き合わせは別の入力が断る）。 読めなかったことも入力の一部で、読めた印とは別になる。
    fn master_stamp(&mut self, take_id: i32, rel_path: &str) -> String {
        if self.open.is_none() {
            return String::new();
        }
        match self.master_digest(take_id, rel_path) {
            Ok(digest) => digest.to_hex(),
            Err(e) => format!("unreadable:{}", e.code),
        }
    }

    /// マスターの内容の指紋。 台帳に記録してあればそれを返し、無ければ（この列を足す前に録った
    /// テイク、確定の直後に落ちたテイク）ファイルから計算して記録する（`DEC-PLT-053`）。
    ///
    /// 記録したあとはファイルを読み直さない。 マスターは確定したら変えない
    /// （`TR-REC-21`）ので、台帳の値を信じる。 ファイルが後から変わっていても気づかない——
    /// それを見るのは、マスターと台帳の整合を確かめる側（T04 の manifest）。
    fn master_digest(&mut self, take_id: i32, rel_path: &str) -> Result<Digest> {
        if let Some(digest) = self.writer()?.master_digest_of(take_id)? {
            return Ok(digest);
        }
        let path = self.opened()?.dir().root().join(rel_path);
        let digest = koeru_core::master::digest_file(&path)?;
        Ok(self.writer()?.record_master_digest(take_id, &digest)?)
    }

    /// 素材（採用テイクの WAV）の内容の指紋。 試唱の合成結果の鍵に混ぜる（`TR-SYN-02`）。
    ///
    /// パスと更新時刻を混ぜない。 同じ中身が別の場所にあれば同じ鍵、同じ場所で中身が変われば
    /// 別の鍵になる。 **以前は場所・大きさ・更新時刻から作っていた**ので、中身が変わっても
    /// 大きさと時刻が同じなら鍵が動かず、古い合成結果が鳴った。
    ///
    /// プロジェクトの外か、台帳に無い場所のファイルは、中身から直接計算する。
    fn material_digest(&mut self, path: Option<&PathBuf>) -> Result<Option<Digest>> {
        let Some(path) = path else { return Ok(None) };
        let root = self.opened()?.dir().root().to_path_buf();
        let rel = path
            .strip_prefix(&root)
            .ok()
            .map(|r| r.to_string_lossy().into_owned());
        let take_id = match &rel {
            Some(rel) => self.writer()?.master_digest_at(rel)?.map(|(id, _)| id),
            None => None,
        };
        match (take_id, rel) {
            (Some(take_id), Some(rel)) => self.master_digest(take_id, &rel).map(Some),
            _ => Ok(Some(koeru_core::master::digest_file(path)?)),
        }
    }

    /// 推定し直しの入力を集める。 台帳と WAV を読むだけで、何も書かない。
    fn collect_re_estimate(&mut self, take_id: i32) -> Result<ReEstimateInput> {
        let root = self.opened()?.dir().root().to_path_buf();
        let take = self.writer()?.take(take_id)?.ok_or_else(unknown_take)?;
        let w = wav::read(root.join(&take.rel_path))?;
        Ok(ReEstimateInput {
            take_id,
            wav: self.master_stamp(take_id, &take.rel_path),
            rel_path: take.rel_path.clone(),
            rate: w.rate_hz,
            samples: Arc::new(w.samples),
            row: self.read_row(&take.row_id),
            row_id: take.row_id.clone(),
            aligner: self.aligner.clone(),
        })
    }

    /// 推定し直した結果を、キューと台帳へ受理する。
    fn admit_re_estimate(&mut self, estimation: take_analysis::ReEstimation) -> Result<()> {
        let take_analysis::ReEstimation {
            take_id,
            tone,
            entries,
            mismatches,
        } = estimation;
        let mut next = self.opened()?.review.clone();
        let mut rows = Vec::new();
        for e in &entries {
            // キューの鍵は（音高, 綴り）（`TR-ALN-22`）。
            let key = crate::review::EntryKey::new(tone, e.alias.as_str()).handle();
            // 固定されていない値だけが動く（`INV-ALN-001`）。
            next.re_estimate(&key, e.oto, e.confidence)
                .map_err(review_error)?;
            let Some(entry) = next.get(&key) else {
                continue;
            };
            rows.push(koeru_core::db::ReviewEntryRow {
                take_id,
                alias: e.alias.clone(),
                oto: entry.oto,
                state: entry.state.as_str().to_owned(),
                pinned: entry.pins(),
            });
        }

        let open = self.opened_mut()?;
        open.writer()?.put_review_entries(&rows)?;
        for r in &rows {
            open.writer()?.set_branch_mismatch(
                r.take_id,
                &r.alias,
                mismatches.contains(&r.alias),
            )?;
        }
        // 印は台帳から載せ直す。 キューの写しは遷移しか運ばない。
        self.refresh_review()
    }

    /// oto を直すのではなく録り直す（`REQ-ALN-009`, `TR-ALN-27`）。
    ///
    /// エントリを未推定へ戻すだけ。 実際の収録は収録画面が行う。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、確認待ちでない。
    #[tracing::instrument(skip(self, key))]
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
    #[tracing::instrument(skip(self))]
    pub fn validate_otos(&mut self) -> Result<(usize, Vec<String>)> {
        let entries = self.writer()?.adopted_otos()?;
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
        // キューの鍵は（音高, 綴り）（`crate::review::load`）。 止めるときは同じ鍵で引く。
        let tones = self.writer()?.row_tones()?;
        let mut blocked_keys = Vec::new();
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
                self.writer()?.set_oto_value(e.take_id, &e.alias, &next)?;
            }
            // 固定を避けたせいで違反が残るものも止める。 直せていないのに
            // 通すと、`TR-ALN-20` が塞いだはずの形のまま書き出される。
            if !r.may_export() || touches_pinned {
                blocked.push(e.alias.clone());
                let tone = tones.get(&e.row_id).copied().unwrap_or_default();
                blocked_keys.push(crate::review::EntryKey::new(tone, e.alias.as_str()).handle());
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
        //
        // **素の綴りで引いていた。** 鍵が合わず `review.no_such_entry` になり、
        // それを失敗として返すので、直せない違反が1件でもあると検証そのものが
        // 落ちていた——止めるべきエントリはキューに現れない。
        for key in &blocked_keys {
            match self.with_entry(key, |q, id| q.validation_unrepairable(id)) {
                Ok(()) => {}
                Err(e) if e.code == ReviewError::WrongState.code() => {}
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
    #[tracing::instrument(skip(self))]
    fn ensure_otos_ready(&mut self) -> Result<()> {
        self.validate_otos()?;
        // 切り出しが1つも取れなかった行は、キューにも現れない（`INV-ALN-003`）。
        //
        // **発声が見つからなかったテイクも採用される。** そのテイクは
        // `oto_values` に1行も書かないので、キューが空でも書き出しから
        // 黙って落ちる。キューの関門はエントリしか見られないので、
        // ここで別に見る。
        let missing = self.writer()?.adopted_rows_without_oto()?;
        if !missing.is_empty() {
            return Err(AppError::new(
                "review.missing_oto",
                Class::Rejected,
                format!("切り出しの取れていない行が {} 件ある", missing.len()),
            ));
        }
        // エイリアスが WAV をまたいで重なると、キューが片方を落とす。
        // 落ちたほうは確認もされず `oto.ini` にも出ない。
        let conflicting = self.writer()?.adopted_conflicting_aliases()?;
        if !conflicting.is_empty() {
            return Err(AppError::new(
                "review.conflicting_alias",
                Class::Rejected,
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
    #[tracing::instrument(skip(self, encoding))]
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
        if self.writer()?.recording_tones()?.len() > 1 {
            return Err(AppError::new(
                "review.multi_pitch_oto_ini",
                Class::Rejected,
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
            .map_err(|e| AppError::from_failure(e).saying("oto.ini を書けない"))?;
        // WAV と同じディレクトリへ置く。 `oto.ini` の左辺はファイル名だけなので、
        // 別の階層に置くと**全部の参照が解決しない**。UTAU が読む形も、
        // wav と oto.ini が同じ場所に並んだ形。
        //
        // 作業ファイルにはしない（`TR-PKG-40`）。 正本は台帳で、これは派生物。
        // 読み戻して真とすることはない。外部フォルダへ一式を出す経路は
        // `koeru_core::handoff`（`PROFILE-M4`）。
        let path = self.opened()?.dir().audio_dir().join("oto.ini");
        std::fs::write(&path, bytes)?;

        // ここまで来て初めて確定させる。関門はもう一度キューが見る。
        let mut next = self.opened()?.review.clone();
        next.export().map_err(review_error)?;
        let over = next.mode() != ReviewMode::Individual;
        let open = self.opened_mut()?;
        crate::review::save_mode(&mut *open.writer()?, &next, over)?;
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
        let entries = self.writer()?.adopted_otos()?;
        let mut out = Vec::new();
        for e in entries {
            let f = self.writer()?.fingerprint_of(e.take_id)?;
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
        let models = ledger::models()
            .map_err(|e| AppError::from_failure(e).saying("モデルの台帳を読めない"))?;
        ledger::check(&models)
            .map_err(|e| AppError::from_failure(e).saying("モデルの台帳が規律を満たさない"))?;
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

        // 確認キューの失敗と同じ code を名乗る。
        let Some(take_id) = self.opened()?.review_takes.get(key).copied() else {
            return Err(AppError::from_failure(ReviewError::NoSuchEntry)
                .saying("そのエントリを持つテイクが無い"));
        };
        let Some(entry) = next.get(key).cloned() else {
            return Err(review_error(ReviewError::NoSuchEntry));
        };
        // 台帳は（テイク, 綴り）で持つ。 鍵から綴りを取り出して渡す
        // ——音高はテイクの行が持っているので、二重には書かない。
        //
        // 読めない鍵は「無いエントリ」ではない。 古い一覧ではなく、渡し方の誤り。
        let Some(parsed) = crate::review::EntryKey::parse(key) else {
            return Err(AppError::new(
                "review.malformed_key",
                Class::InvalidInput,
                "鍵の形が違う",
            ));
        };
        let open = self.opened_mut()?;
        crate::review::save_entry(&mut *open.writer()?, take_id, parsed.alias(), &entry)?;
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
        open.writer()?.put_review_entries(&rows)?;
        crate::review::save_mode(&mut *open.writer()?, &next, over)?;
        open.review = next;
        Ok(())
    }

    /// そのテイクの WAV のファイル名。`oto.ini` の左辺に出る。
    fn take_file_name(&mut self, take_id: i32) -> Result<String> {
        let take = self.writer()?.take(take_id)?.ok_or_else(unknown_take)?;
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
    #[tracing::instrument(skip(self))]
    pub fn otos_of_take(&mut self, take_id: i32) -> Result<Vec<(String, koeru_oto::Oto)>> {
        Ok(self.writer()?.otos_of(take_id)?)
    }

    /// テイクに名前を付ける・変える・消す（収録の画面のテイクの切り替え）。
    ///
    /// 前後の空白を除いて空なら消し、画面は既定の表示（何本目か）へ戻る。 長さも字も
    /// 制限しない——曲の題（[`Self::rename_song`]）と同じで、表示のためだけの文字列。
    /// 録音もエントリも触らない。
    ///
    /// 付け直したテイクを返す。 そのテイクが無ければ `None`。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、台帳を書けない。
    #[tracing::instrument(skip(self, label))]
    pub fn rename_take(
        &mut self,
        take_id: i32,
        label: Option<&str>,
    ) -> Result<Option<koeru_core::db::Take>> {
        let label = label.map(str::trim).filter(|l| !l.is_empty());
        Ok(self.writer()?.set_take_label(take_id, label)?)
    }

    /// 確定のあとの解析が出した音高の記録（`TR-PKG-05`）。
    ///
    /// 周波数表と同じ系列を、テイクの全長で返す。 切り出さない（`DEC-SYN-008`）。
    /// 解析がまだ置いていなければ `None`——保留・失敗・取消のどれでも同じで、
    /// どれなのかは解析の状態が持つ。 録っている途中には読まない（`DEC-PLT-036`）。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、そのテイクが無い、台帳を読めない。
    #[tracing::instrument(skip(self))]
    pub fn take_pitch(&mut self, take_id: i32) -> Result<Option<TakePitch>> {
        let mut w = self.writer()?;
        if w.take(take_id)?.is_none() {
            return Err(unknown_take());
        }
        Ok(w.analysis_of(take_id)?.map(|a| TakePitch {
            hop: a.hop_size(),
            f0: a.frq.f0,
        }))
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
    #[tracing::instrument(skip(self))]
    pub fn play_take(&mut self, take_id: i32) -> Result<f64> {
        let root = self.opened()?.dir().root().to_path_buf();
        let take = self.writer()?.take(take_id)?.ok_or_else(unknown_take)?;
        let w = wav::read(root.join(&take.rel_path))?;
        #[allow(
            clippy::cast_precision_loss,
            reason = "テイクの長さは表示用。桁は十分に収まる"
        )]
        let ms = w.samples.len() as f64 * 1000.0 / f64::from(w.rate_hz);

        // 前の出力は止める。 重ねると何を聴いているか分からなくなる（`DEC-REC-011`）。
        self.output.stop();
        self.output.set_clip(mac::play(w.samples, w.rate_hz)?);
        Ok(ms)
    }

    /// 鳴らしている音を止める（`TR-SYN-27`）。
    ///
    /// 進行中の合成も止める。 200ms 以内に抜ける。 合図 → 再生 → 合成の
    /// 待ち合わせの順は `PlaybackLease`（`crate::playback_lease`）が持つ。
    /// 合成は `JobRuntime` の試唱 lane の仕事で、出力を落とすときに取り消される
    /// （`preview::Running`）。 新しい試唱の前にも呼ぶので、古い試唱は新しい試唱が
    /// 積まれる前に取り消される。
    pub fn stop_preview(&mut self) {
        self.output.stop();
        // 積んである前処理も捨てる（`TR-SYN-27`）。曲を切り替えたときに、
        // 前の曲のための前処理を回し続ける意味は無い。 これは出力の資源ではなく
        // 前景の仕事の後始末なので、`PlaybackLease` の外、ここで行う。
        //
        // **試唱の仕事と前処理だけを取り消す**（`JobRuntime::cancel`）。 貸与の仕事を全部取り消す
        // `cancel_lease` は、テイクの解析まで止める。 試唱を止めても、録ったテイクの
        // 解析は進める——以前の `workers.clear()` も、積んであった前処理だけを捨てていた。
        if let Some(job) = self.prerender.take() {
            // 終わっていれば何も起きない（`AlreadyFinished` など）。 結果は要らない。
            let _ = self.jobs.cancel(job);
        }
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
    #[tracing::instrument(skip(self, id))]
    pub fn sing_song(&mut self, id: &str) -> Result<SungSong> {
        let started = std::time::Instant::now();
        // 先に止める。 重ねると何を聴いているか分からなくなる。
        self.stop_preview();

        // この音源の作り方（`TR-RCL-01`）。 どの綴りを引くかがこれで決まる。
        let preset = self.current_preset()?;
        let rules = self.current_rules()?;

        let rate = 44_100_u32;
        let root = self.opened()?.dir().root().to_path_buf();
        let song = self
            .writer()?
            .songs_in_bank()?
            .into_iter()
            .find(|(sid, _)| sid == id)
            .map(|(_, s)| s)
            .ok_or_else(no_song)?;

        // ## キー（`TR-SYN-15`, `DEC-SYN-012`）
        //
        // **本人が指定した調で鳴らす。** 自動では動かさない——本人が
        // 「この曲でこの声はどう聴こえるか」を確かめるために入れた曲を
        // 黙って別の調にすると、確かめた結果が別の曲のものになる。
        //
        // 遠くても止めない。 ±7 半音・二乗平均 4 半音という線に実測の裏付けが
        // 無い（`reclist.toml` の領域リスク）ので、その線で鳴らすことを禁じない。
        // どうなるかは画面が先に出していて、押したのは本人。
        let tones = self.writer()?.recording_tones()?;
        let fit = koeru_core::song::range_fit(&song, &tones);
        if fit.transpose != 0 {
            tracing::info!(semitones = fit.transpose, "指定されたキーで鳴らす");
        }

        // ## 素材を集める
        //
        // 採用テイクだけを使う。 無効にしたテイク（取りこぼし）は入らない。
        // 収録音高ごとの素材（`TR-SYN-16`）。 単音階なら鍵は `None` の1つ。
        let by_tone = self.materials_by_tone(&root, &rules, preset)?;
        // 解決はノートごとに、そのノートが使える区画だけで行う（`DEC-SYN-014`）。
        //
        // **全体の和集合で綴りを選んでいた。** 選んだ綴りがそのノートの区画に
        // 無いと、素材を引く段で落ちて黙って鳴らなかった——代用の綴りが
        // その区画にあっても使わなかった。
        //
        // 単音階は区画が1つで、鍵は `None`。 解決の側は音高で数えるので、
        // 収録音高の1本へ読み替える。
        let have: std::collections::BTreeMap<i32, std::collections::BTreeSet<String>> = by_tone
            .iter()
            .filter_map(|(t, m)| {
                let tone = t.or_else(|| tones.first().copied())?;
                Some((tone, m.paths.keys().cloned().collect()))
            })
            .collect();
        let key_of = |tone: Option<i32>| {
            if by_tone.contains_key(&None) {
                None
            } else {
                tone
            }
        };
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
        //
        // この音源の作り方で解決する（`TR-SYN-36`）。 単独音で解決すると、
        // 連続音の音源が持っている `a か` を一度も引かない。
        // 休符で綴りの文脈を切る（`TR-RCL-12`）。 繋げて解決すると、
        // 休符のあとの音符が語頭形ではなく継続に解決される。
        // 曲の状態と同じ入口を通る（`TR-RCL-20`）。
        let resolved = song
            .resolve_by_tone(&rules, preset.method, preset.set, &tones, &have)
            .ok_or_else(|| {
                AppError::new(
                    "app.unreadable_lyrics",
                    Class::InvalidInput,
                    "この曲の歌詞を読めない",
                )
            })?;

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
            // 合成器は長さから出力の標本数を決めて確保する。 取り込んだ曲の
            // 長さを信じると、極端な BPM や `Length` で確保が落ちる
            // （`preview::MAX_SEGMENT_MS`。休みの無音と同じ上限）。
            let duration_ms =
                (f64::from(ticks) / 480.0 * beat_ms).min(crate::preview::MAX_SEGMENT_MS);

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
                    // 解決した区画から素材を引く（`TR-SYN-13`, `TR-SYN-16`）。
                    // どの区画かは解決が決めている（`DEC-SYN-014`）。 **ここで
                    // 引き直さない**——別の順で引くと、判定と違う素材で鳴る。
                    let used_tone = key_of(res.tone);
                    let Some(m) = by_tone.get(&used_tone) else {
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
                            .and_then(|(_, r)| by_tone.get(&key_of(r.tone))?.otos.get(&r.alias));
                        let vc_ms = koeru_core::oto::transition_ms(next_oto, owner_ms, beat_ms);
                        if let Some(prev) = current.last_mut() {
                            prev.duration_ms = (prev.duration_ms - vc_ms).max(0.0);
                        }
                        vc_ms
                    };

                    current.push(koeru_synth::phrase::NoteSpec {
                        alias: res.alias.clone(),
                        sample_path: m.paths.get(&res.alias).cloned().unwrap_or_default(),
                        sample_hash: self.material_digest(m.paths.get(&res.alias))?,
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
                    Class::Rejected,
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

        // 合成の写しは、いま開いているプロジェクトのもの。 背後の仕事へ渡した
        // `Arc` は、プロジェクトを移るときに `stop_preview` が止めて待つ。
        let (song_cache, lease) = {
            let open = self.opened()?;
            (Arc::clone(&open.caches.song_cache), open.lease)
        };
        let stream = mac::play_streaming(Vec::new(), rate)?;
        let sink = StreamSink {
            feed: stream.feed(),
        };
        // 先頭フレーズも背後の仕事（試唱 lane）が流す。 ここで流すと、リングが満杯のあいだ
        // 状態のロックを握ったまま待つ（`preview::start`）。
        let running = preview::start(
            &self.jobs,
            lease,
            owned,
            samples,
            song_cache,
            Box::new(sink),
            rate,
        )
        .map_err(AppError::from_failure)?;

        self.output.set_song(stream, running);

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

    /// 収録音高ごとの、鳴らす素材・周波数表・oto（`TR-SYN-16`）。
    ///
    /// 単音階なら1つで、鍵は `None`。 多音階では音高ごとに引き、
    /// **音高を跨いで拾わない**——高音階の素材を低音階の音符に当てると、
    /// 1音だけ別人の声になる。
    ///
    /// どの素材を使うかは、書き出しと同じ解決が決める（`DEC-PLT-052`）。 綴りの持ち主の
    /// 行（`DEC-RCL-016`）の採用テイクで、5値があるものだけ。 **並びが前の行のテイクを
    /// 引いていた。** 先に録ったのが並びの後ろの行だと、5値の無いテイクを引いて、
    /// 持ち主の素材があるのに綴りが黙って鳴らなかった。
    ///
    /// 台帳の1つの版から読む。 解いている間に録音が確定しても、途中の版を混ぜない。
    fn materials_by_tone(
        &mut self,
        root: &std::path::Path,
        rules: &koeru_core::presamp::Rules,
        preset: koeru_core::preset::MethodPreset,
    ) -> Result<std::collections::BTreeMap<Option<i32>, Materials>> {
        let (runtime, lease) = {
            let open = self.opened()?;
            (Arc::clone(&open.runtime), open.lease)
        };
        let session = runtime.read(&lease)?;
        let facet = session.materials()?;
        // 試唱が鳴らさない役割（語尾）を引くための一覧。 書き出しは同じ事実から語尾も配る。
        let catalog = koeru_model::target::Catalog::new(rules, preset.method, preset.set);
        let resolution = facet.resolve(UseContext::Preview, |(_, alias)| Role::of(&catalog, alias));

        let tones = facet.tones();
        let multi = tones.len() > 1;
        let mut out: std::collections::BTreeMap<Option<i32>, Materials> =
            std::collections::BTreeMap::new();
        if multi {
            // 1テイクも録っていない音高も、空の区画として持つ（解決が区画を数える）。
            for t in tones {
                out.insert(Some(*t), Materials::default());
            }
        } else {
            out.insert(None, Materials::default());
        }
        // 周波数表はテイクごとに1度だけ引く。 綴りごとに引くと、同じ表を何度も読む。
        let mut tables: HashMap<koeru_model::id::TakeId, Option<Vec<f64>>> = HashMap::new();
        for c in resolution.used() {
            let (tone, alias) = &c.key;
            let (Some(rel), Some(oto)) = (facet.rel_path(c.take), facet.oto(c)) else {
                continue;
            };
            let table = match tables.get(&c.take) {
                Some(t) => t.clone(),
                None => {
                    let t = session.analysis_of(c.take)?.map(|a| a.frq.f0);
                    tables.insert(c.take, t.clone());
                    t
                }
            };
            let m = out.entry(multi.then_some(*tone)).or_default();
            m.paths.insert(alias.clone(), root.join(rel));
            if let Some(f0) = table {
                m.tables.insert(alias.clone(), f0);
            }
            m.otos.insert(alias.clone(), *oto);
        }
        Ok(out)
    }

    /// テスト用。 音声デバイス無しで、行を収録済みとして印を付ける。
    ///
    /// 実際の収録は `start_take` → `finish_take` を通る。ここはカバレッジの
    /// 計算だけを確かめたいときの入口。**WAV も原音設定も置かない**ので、
    /// 書き出しまで通す試験は [`Self::seed_material_for_test`] を使う。
    #[cfg(any(test, feature = "test-hooks"))]
    #[tracing::instrument(skip(self))]
    pub fn mark_recorded_for_test(&mut self, row_id: &str) -> Result<()> {
        let session_id = self.test_session()?;
        let take = self.writer()?.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: format!("audio/{row_id}_1.wav"),
            frames: 44_100,
            recorded_at: now_rfc3339(),
        })?;
        self.writer()?.adopt_take(row_id, take)?;
        // 本番の確定と同じく、もう片方の出どころを組み直す（`DEC-RCL-016`）。
        self.reconcile_other_origin(row_id)?;
        Ok(())
    }

    /// テスト用。 実体の WAV と原音設定まで置く。
    ///
    /// 書き出しは素材を実際に開く（`TR-PKG-49`）ので、印だけでは通らない。
    /// マスターは 44100 Hz（`TR-REC-02`）。
    #[cfg(any(test, feature = "test-hooks"))]
    #[tracing::instrument(skip(self))]
    pub fn seed_material_for_test(&mut self, row_id: &str) -> Result<i32> {
        let session_id = self.test_session()?;
        let root = self.opened()?.dir().root().to_path_buf();
        let rel = format!("audio/{row_id}_1.wav");
        let path = root.join(&rel);
        std::fs::create_dir_all(self.opened()?.dir().audio_dir())?;

        // 1.5 秒の一定振幅。 中身は問わない——見るのは長さとレートだけ。
        //
        // **0.5 秒にしていた。** 1行は最大 8 モーラなので、下位方式の
        // 書き出し（`TR-PKG-24`）が境界から5値を作り直すと、後ろのモーラが
        // ファイル末尾を越えて検証で落ちる。
        let samples = vec![0.2_f32; MASTER_RATE_HZ as usize * 3 / 2];
        let mut part = koeru_audio::wav::PartialTake::create(&path, MASTER_RATE_HZ)?;
        part.write(&samples)?;
        part.finalize()?;

        let take = self.writer()?.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: rel,
            frames: i64::try_from(samples.len()).unwrap_or(0),
            recorded_at: now_rfc3339(),
        })?;
        self.writer()?.adopt_take(row_id, take)?;

        // エイリアスは台帳が持つものをそのまま使う。 呼ぶ側に渡させない——
        // 台帳と食い違った名前で置けてしまう。
        //
        // **収録単位そのものを置いていた。** 連続音や CVVC では書き出しの
        // 綴りと違う名前になり、被覆が満ちないまま試験が通っていた。
        //
        // 行から導き直さない。 同じ綴りを2つの行が生むとき、持つのは
        // 先に録った1つだけで（`DEC-RCL-016`）、その取り決めは台帳にしか無い。
        // 導き直すと、本番では作られない5値を試験だけが持つ。
        let here = self.current_preset()?;
        let rules = self.current_rules()?;
        let line = self.writer()?.row_units_of(row_id, here.set)?;
        let aliases = self.writer()?.owned_aliases_of_row(row_id)?;
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
        self.writer()?.put_boundaries(take, &saved)?;
        for alias in &aliases {
            self.writer()?.put_oto(
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
            self.writer()?.set_oto_state(
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
        self.writer()?.put_analysis(take, &analysis)?;
        self.refresh_review()?;
        // 本番の確定と同じく、もう片方の出どころを組み直す（`DEC-RCL-016`）。
        self.reconcile_other_origin(row_id)?;
        Ok(take)
    }

    /// テスト用の収録セッション。無ければ1つ始める。
    #[cfg(any(test, feature = "test-hooks"))]
    fn test_session(&mut self) -> Result<i32> {
        let open = self.opened_mut()?;
        if open.session_id == 0 {
            let started = open.writer()?.start_session(&SessionSnapshot {
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
            open.session_id = started;
        }
        Ok(open.session_id)
    }

    /// 開いているプロジェクトのディレクトリ。
    #[tracing::instrument(skip(self))]
    pub fn project_dir(&self) -> Result<&ProjectDir> {
        Ok(self.opened()?.dir())
    }

    fn display_name(&self) -> Result<String> {
        Ok(self.opened()?.dir().read_manifest()?.display_name)
    }

    /// 台帳の書き手。 **文の終わりで手放す**（`Open::writer`）。
    fn writer(&self) -> Result<WriterGuard<'_>> {
        self.opened()?.writer()
    }

    fn opened(&self) -> Result<&Open> {
        self.open.as_ref().ok_or_else(no_project)
    }

    fn opened_mut(&mut self) -> Result<&mut Open> {
        self.open.as_mut().ok_or_else(no_project)
    }

    // ## デバイスの消失（`TR-REC-04`, `REQ-REC-109`, `DEC-REC-011`）

    /// ストリームが要る操作の前提を確かめる。
    ///
    /// デバイスを見失っている間は専用の code で断る（`recording.device_lost`）。
    /// 見失ってすらいなければ、今までどおり `app.no_stream`
    /// ——一度も選んでいない・まだ開いていないだけのときと区別する理由が無い。
    fn require_stream(&self) -> Result<()> {
        if self.session.device() == Device::Lost {
            return Err(device_lost());
        }
        if self.lease.is_none() {
            return Err(no_stream());
        }
        Ok(())
    }

    /// デバイス一覧が変わった通知を受けて、選択中デバイスの生死を確かめ直す。
    ///
    /// 見張りスレッド（`lib.rs`）から、カウンタが動いたときだけ呼ばれる。
    /// RT コールバックの中身ではないので、ここでロックを取ってよい。
    #[tracing::instrument(skip(self))]
    pub fn check_device(&mut self) -> Result<()> {
        let Some(id) = self.device.clone() else {
            // まだ選んでいない。 見るものが無い。
            return Ok(());
        };
        let alive = self.probe_device_alive(&id)?;
        match device_transition(self.session.device(), alive) {
            DeviceTransition::None => Ok(()),
            DeviceTransition::Lost => {
                self.on_device_lost();
                Ok(())
            }
            DeviceTransition::Returned => self.on_device_returned(&id),
        }
    }

    /// 本番の経路。 OS へ生死を問い合わせる。
    #[cfg(not(any(test, feature = "test-hooks")))]
    fn probe_device_alive(&self, id: &DeviceId) -> Result<bool> {
        Ok(mac::is_alive(id)?)
    }

    /// テスト用の経路。 差し込んだ答えがあればそれを使う（`tests/device_loss.rs`）。
    ///
    /// 別の分岐を新設しない——本来 `mac::is_alive` が答える値を、ここで
    /// 差し替えるだけで、そこから先（[`device_transition`] 以降）は本番と共通。
    #[cfg(any(test, feature = "test-hooks"))]
    fn probe_device_alive(&self, id: &DeviceId) -> Result<bool> {
        if let Some(forced) = self.test_device_alive {
            return Ok(forced);
        }
        Ok(mac::is_alive(id)?)
    }

    /// 選択済みデバイスを見失った（`REQ-REC-109`）。
    ///
    /// 進行中のテイクがあれば、まずストリームを落とす——排出スレッドは
    /// 止められると書きかけを確定させる（[`Self::disarm`] の doc）ので、
    /// このあと台帳の予定と実ファイルの場所が揃う。 そのあと台帳を
    /// `Discarded` で閉じ、確定した WAV を削除する。
    ///
    /// **これは孤児の自動削除ではない**（`TR-REC-31` が禁じているのは、
    /// 台帳に載っていない確定済み WAV を見て回るときの自動削除）。
    /// ここで消すのは、アプリ自身がこの瞬間に「この予定はもう有効ではない」と
    /// 決めた録音で、台帳の予定は同じ手続きの中で `Discarded` として閉じて
    /// いる——本人が採るか捨てるかを選ぶ孤児とは別の扱い（`DEC-REC-011`）。
    fn on_device_lost(&mut self) {
        tracing::warn!("選択済みデバイスを見失った");
        let active = self.recording.take();
        self.lease = None;
        if let Some(active) = active {
            self.discard_lost_capture(&active.capture);
        }
        if let Err(e) = self.session.device_lost() {
            koeru_failure::record_failure(&e, koeru_failure::Outcome::Unknown, "device.lost");
        }
    }

    /// 見失ったときの進行中テイクを、台帳の予定ごと破棄する（[`Self::on_device_lost`]）。
    ///
    /// 順序はここが要——台帳を先に閉じてから実ファイルを消す。 逆にすると、
    /// ファイルが無いのに予定だけ開いたまま残る窓ができる。
    fn discard_lost_capture(&mut self, capture: &CaptureId) {
        let at = now_rfc3339();
        // 消す実ファイルの場所は、閉じる前の予定から引く。
        let rel_path = self
            .writer()
            .ok()
            .and_then(|mut w| w.intent(capture).ok().flatten())
            .map(|intent| intent.rel_path);

        // 書き手を取れない（もう閉じている）ときも、予定を閉じられなかったことを
        // 記録して進む。 実ファイルの扱いは、予定が閉じたかどうかとは別。
        match self.open.as_ref().map(Open::writer_raw) {
            Some(Ok(mut w)) => {
                if let Err(e) = w.discard_capture(capture, &at) {
                    koeru_failure::record_failure(
                        &e,
                        koeru_failure::Outcome::NotCommitted,
                        "device_lost.discard",
                    );
                }
            }
            Some(Err(e)) => koeru_failure::record_failure(
                &e,
                koeru_failure::Outcome::NotCommitted,
                "device_lost.discard",
            ),
            None => {}
        }

        if let (Ok(open), Some(rel)) = (self.opened(), rel_path) {
            let root = open.dir().root().to_path_buf();
            delete_capture_file(&root, &rel);
        }
    }

    /// 見失っていた同じデバイスが戻った（`REQ-REC-109`）。
    ///
    /// 自動では録音を始めない。 別のデバイスへも自動で切り替えない
    /// ——ここに来るのは同一識別子が生きているときだけ（[`device_transition`]）。
    ///
    /// `arm_device` は呼ばない。 あれは本人が明示的に選ぶ経路で、
    /// 呼んだ時点で選択（`self.device`）を手放す——再オープンが実機の
    /// 都合で失敗しうる自動復帰にそのまま使うと、失敗するたびに選択が
    /// 失われ、二度と自動では拾い直せなくなる（**踏んだ**）。
    ///
    /// 代わりに [`Self::open_armed_stream`] を直接呼ぶ。 `SelectionOrigin::Returned`
    /// を渡すので、`mac::open` が成功するまで `Session::same_device_returned` を
    /// 呼ばない——失敗している間は `self.device` も `Session` の `Lost` も
    /// 崩れないので、次にデバイス一覧が動いたとき（`Studio::check_device`）に
    /// また同じ経路で試せる。 USB の抜き差しでは、一覧に出てから実際に
    /// 開けるようになるまで間があるので、1回目の再オープンが失敗する経路が
    /// 現実にある。
    fn on_device_returned(&mut self, id: &DeviceId) -> Result<()> {
        tracing::info!("見失っていたデバイスが戻ったので、再オープンを試みる");
        if let Err(e) = self.open_armed_stream(id, SelectionOrigin::Returned) {
            // 選択と `Lost` はここでは崩れていない（`open_armed_stream` の doc）。
            // 次にデバイス一覧が動いたとき、また同じ経路で試せる。
            tracing::warn!("再オープンに失敗した。次のデバイス一覧の変化を待つ");
            return Err(e);
        }
        self.probe_input(DEVICE_RETURN_PROBE_MS)?;
        Ok(())
    }

    // ## テスト用（`tests/device_loss.rs`）
    //
    // 実機・実デバイス無しで、デバイス消失時の破棄と復帰の検知を確かめるための入口。
    // 生死の答えだけを差し込み、判断そのもの（`check_device` 以降）は本番と共通の経路を通す。

    /// 実機無しで、次の [`Self::check_device`] が使う生死の答えを固定する。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_set_device_alive(&mut self, alive: bool) {
        self.test_device_alive = Some(alive);
    }

    /// 実機無しで、選択済みデバイスにする（`arm_device` の代わり）。
    ///
    /// `mac::open` を呼ばない。 状態機械だけを `Selected` まで進める
    /// ——`check_device` が見るのはそこだけ。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_select_device(&mut self, id: &DeviceId) -> Result<()> {
        self.session = Session::new();
        self.session.select_device(id.clone())?;
        self.device = Some(id.clone());
        Ok(())
    }

    /// 実機無しで、収録中の予定を作る（`on_device_lost` の破棄を確かめるため）。
    ///
    /// 確定済みの WAV をその場に直接置く。 本番は排出スレッドが `.wav.part`
    /// を確定させて作る（[`Self::disarm`] の doc）が、ここはハードウェアを
    /// 使わないので、同じ場所に同じ形のファイルを直接置く。
    ///
    /// 返すのは予定の識別子と、置いた WAV の実パス（破棄後に消えたかを
    /// 呼び出し側が確かめられるように）。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_begin_capture(&mut self, row_id: &str) -> Result<(CaptureId, PathBuf)> {
        let session_id = self.test_session()?;
        let root = self.opened()?.dir().root().to_path_buf();
        let audio_dir = self.opened()?.dir().audio_dir();
        std::fs::create_dir_all(&audio_dir)?;
        let path =
            koeru_core::capture::free_take_path(&mut *self.writer()?, &root, &audio_dir, row_id, 1)
                .map_err(AppError::from_failure)?;

        // 中身は問わない。 見るのは、破棄したときにファイルが消えるかどうかだけ。
        let samples = vec![0.1_f32; MASTER_RATE_HZ as usize / 10];
        let mut part = koeru_audio::wav::PartialTake::create(&path, MASTER_RATE_HZ)?;
        part.write(&samples)?;
        part.finalize()?;

        let capture = CaptureId::generate();
        self.writer()?.declare_capture(&NewIntent {
            capture: &capture,
            row_id,
            session_id,
            rel_path: &koeru_core::capture::rel_path(&root, &path),
            declared_at: &now_rfc3339(),
        })?;
        self.recording = Some(ActiveCapture {
            row_id: row_id.to_owned(),
            capture: capture.clone(),
            guard: TakeGuard::default(),
        });
        Ok((capture, path))
    }

    // ## テスト用（`tests/analysis_jobs.rs`）
    //
    // 実機のマイク無しで、確定のあとの派生物（解析・受理）が `JobRuntime` を通ることを確かめる。

    /// テスト用。 解析の期限を差し替える。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_set_analysis_deadline(&mut self, deadline: std::time::Duration) {
        self.test_analysis_deadline = Some(deadline);
    }

    /// テスト用。 `samples` を1テイクとして台帳へ確定し、確定のあとの派生物
    /// （解析・受理・組み直し）を本番と同じ経路で作って、画面へ返す形で返す。
    ///
    /// 本番は排出スレッドが WAV を確定して `finish_capture` が台帳へ載せる。 ここは WAV を
    /// 直接置き、`commit_take` で載せる。 そのあとは `finish_capture` と同じ [`Self::derive_take`]。
    ///
    /// # Errors
    ///
    /// プロジェクトを開いていない、WAV か台帳を書けない。 派生物の失敗は `followup` に載る。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_derive_take(
        &mut self,
        row_id: &str,
        // その行のモーラ数を受け取って、標本を作る。
        voice: impl FnOnce(usize) -> Vec<f32>,
    ) -> Result<TakeResult> {
        let moras = self.writer()?.units_of(row_id)?.len();
        let samples = voice(moras);
        let session_id = self.test_session()?;
        let root = self.opened()?.dir().root().to_path_buf();
        let audio_dir = self.opened()?.dir().audio_dir();
        std::fs::create_dir_all(&audio_dir)?;
        let path =
            koeru_core::capture::free_take_path(&mut *self.writer()?, &root, &audio_dir, row_id, 1)
                .map_err(AppError::from_failure)?;
        let mut part = koeru_audio::wav::PartialTake::create(&path, MASTER_RATE_HZ)?;
        part.write(&samples)?;
        part.finalize()?;
        let take_id = self.writer()?.commit_take(&FinalizedTake {
            row_id: row_id.to_owned(),
            session_id,
            rel_path: koeru_core::capture::rel_path(&root, &path),
            frames: i64::try_from(samples.len()).unwrap_or(0),
            recorded_at: now_rfc3339(),
        })?;
        self.writer()?.adopt_take(row_id, take_id)?;

        let duration_ms = samples.len() as f64 * 1000.0 / f64::from(MASTER_RATE_HZ);
        let facts = TakeFacts {
            counts: koeru_core::db::TakeGapCounts::default(),
            gaps: Vec::new(),
            guide_offset: None,
            preroll_frames: 0,
            duration_ms,
        };
        let finished = crate::pump::Finished {
            path,
            samples,
            preroll_frames: 0,
        };
        let derived = self.derive_take(take_id, row_id, finished, facts);
        Ok(TakeResult {
            take_id,
            row_id: row_id.to_owned(),
            duration_ms,
            peak: derived.peak,
            thumbnail: derived.thumbnail,
            oto: derived.oto,
            confidence: derived.confidence,
            discontinuities: 0,
            dropped: 0,
            render_errors: 0,
            invalidated: false,
            metrics: derived.metrics,
            preroll_ms: 0.0,
            followup: derived
                .failure
                .map(|e| e.with_outcome(koeru_failure::Outcome::Committed)),
        })
    }

    /// テスト用。 予定の状態を読む（`tests/device_loss.rs`）。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_intent_state(&mut self, capture: &CaptureId) -> Result<Option<&'static str>> {
        Ok(self.writer()?.intent(capture)?.map(|i| i.state.as_str()))
    }

    // ## テスト用（`tests/project_lifecycle.rs`）
    //
    // プロジェクトを移る・閉じるとき、寿命がプロジェクトに結びつく写しが
    // 持ち越されないことを、中身を覗かずに確かめるための入口。

    /// テスト用。 開いているプロジェクトの写しの量。
    ///
    /// 返すのは（波形の段の数、集めた F0 系列の数、探索下限が決まっているか）。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_project_caches(&self) -> Result<(usize, usize, bool)> {
        let c = &self.opened()?.caches;
        Ok((c.mipmaps.len(), c.observed_f0.len(), c.f0_floor.is_some()))
    }

    /// テスト用。 話者音域の写しを埋める。 本番は `derive_take` が溜める。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_observe_f0(&mut self, series: Vec<f64>, floor: f64) -> Result<()> {
        let c = &mut self.opened_mut()?.caches;
        c.observed_f0.push(series);
        c.f0_floor = Some(floor);
        Ok(())
    }

    /// テスト用。 開いている収録セッションの識別子（0 は未開始）。
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn test_session_id(&self) -> Result<i32> {
        Ok(self.opened()?.session_id)
    }

    /// テスト用。 いまの `Device` 状態（`tests/device_loss.rs`）。
    #[cfg(any(test, feature = "test-hooks"))]
    #[must_use]
    pub fn test_device_state(&self) -> &'static str {
        match self.session.device() {
            Device::NotSelected => "not_selected",
            Device::Selected => "selected",
            Device::Lost => "lost",
        }
    }
}

/// `Studio::open_armed_stream` に、どの FSL の遷移でストリームへ辿り着いたかを渡す。
///
/// `mac::open` の成否を確かめたあとにしか使わない——先に踏むと、
/// 開けなかったときに「選択したのに開けていない」半端な状態が残る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SelectionOrigin {
    /// 本人が明示的に選んだ（`arm_device`）。 `Session::select_device` を使う。
    Fresh,
    /// 見失っていた同じデバイスが戻った（`Studio::on_device_returned`）。
    /// `Session::same_device_returned` を使う。
    Returned,
}

/// 一覧の変化のあと、選択済みデバイスの生死からどちらへ動くかを決める
/// （`TR-REC-04`、`REQ-REC-109`）。
///
/// 純粋関数で、判断だけを持つ。 I/O も台帳も知らないので、実機・実プロジェクト
/// 無しで全網羅を試験できる（`crate::studio::tests::device_transition`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceTransition {
    /// 何もしない（一覧は動いたが、選択中デバイスの生死は変わっていない）。
    None,
    /// 選択済みデバイスを見失った。
    Lost,
    /// 見失っていた同じデバイスが戻った。
    Returned,
}

fn device_transition(state: Device, selected_is_alive: bool) -> DeviceTransition {
    match (state, selected_is_alive) {
        (Device::Selected, false) => DeviceTransition::Lost,
        (Device::Lost, true) => DeviceTransition::Returned,
        // NotSelected のときは何も選んでいないので、生死は関係ない。
        // Selected で生きている／Lost で死んだままも、動かない。
        _ => DeviceTransition::None,
    }
}

/// 選択済みデバイスを見失っている間の操作の失敗（`TR-REC-04`, `REQ-REC-109`）。
///
/// `koeru_audio::SessionError::DeviceState` は変種ごとに同じ code
/// （`recording.device_state`）を返すので、見失った、という専用の事情を
/// 画面へ伝えられない。ここでは `check_device` が状態を見て確定させたあとの
/// 話として、専用の code を1つ割り当てる（`koeru-audio` 側は変えない）。
fn device_lost() -> AppError {
    AppError::new(
        "recording.device_lost",
        Class::DeviceUnavailable,
        "マイクを見失った。つながっているか確かめてほしい",
    )
}

/// 破棄した進行中テイクの実ファイルを消す（`.wav` と、残っていれば `.wav.part`）。
///
/// **孤児の自動削除ではない**（`Studio::on_device_lost` の doc）。 消せなくても
/// 続ける——台帳の予定はもう閉じているので、ファイルが残っても孤児として
/// 次回の起動時検証（`TR-REC-31`）に出るだけで、収録は止まらない。
fn delete_capture_file(root: &std::path::Path, rel: &str) {
    let path = root.join(rel);
    let part = {
        let mut p = path.clone().into_os_string();
        p.push(".part");
        PathBuf::from(p)
    };
    for candidate in [path, part] {
        match std::fs::remove_file(&candidate) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                // パスは載せない（トレースのホワイトリスト方針、`AGENTS.md` の禁止事項3）。
                koeru_failure::record(
                    "device_lost.delete_file_failed",
                    koeru_failure::io_class(&e),
                    koeru_failure::Outcome::NotCommitted,
                    "device_lost.delete_file",
                );
            }
        }
    }
}

/// 枠が作られたときの方式プリセット（`DEC-RCL-018`）。 プリセットを持たない枠（移行前の
/// 古い project から作った枠）は `None` で、呼び出し側が project のプリセットへ倒す。
fn subbank_preset(
    ledger: &mut Ledger,
    subbank: koeru_model::id::SubbankId,
) -> Result<Option<koeru_core::preset::MethodPreset>> {
    Ok(ledger
        .subbanks()?
        .into_iter()
        .find(|s| s.id == subbank)
        .and_then(|s| s.preset_id)
        .and_then(|p| koeru_core::preset::by_id(&p)))
}

/// プロジェクトのエイリアス規則（`TR-SYN-36`, `DEC-SYN-010`, `DEC-SYN-013`）。
///
/// 台帳の写しから組む。 写しを持たない古いプロジェクトは同梱の既定。
///
/// **音源フォルダの `presamp.ini` を読まない。** あれは本人が中身を見るための控えで、
/// あとから書き換えられうる。台帳の綴りは作ったときの表で書かれている。
fn rules_of(
    ledger: &mut Ledger,
    set: koeru_core::inventory::UnitSet,
) -> Result<koeru_core::presamp::Rules> {
    Ok(ledger.presamp_snapshot()?.map_or_else(
        || koeru_core::presamp::Rules::builtin(set),
        |t| koeru_core::presamp::parse(&t).or_builtin(set),
    ))
}

/// ライブラリの一覧に出す migration の状態（`DEC-PLT-041`）。
///
/// **`Studio` の外に置く。** 開いていない音源にも呼ぶので `self` に紐づかない
/// （`preset_of` と同じ理由）。 前回の移行が失敗した記録があれば、それを優先して出す
/// ——台帳は移行前のままなので、状態を読んでも `Pending` にしかならない。
fn library_migration_state(dir: &koeru_core::project::ProjectDir) -> LibraryMigrationState {
    if let Some(failure) = dir.migration_failure() {
        return LibraryMigrationState::Failed { code: failure.code };
    }
    match koeru_core::db::migration_state(&dir.db_path()) {
        Ok(koeru_core::db::MigrationState::Fresh | koeru_core::db::MigrationState::Current) => {
            LibraryMigrationState::Current
        }
        Ok(koeru_core::db::MigrationState::Pending) => LibraryMigrationState::Pending,
        Ok(koeru_core::db::MigrationState::Newer) => LibraryMigrationState::Newer,
        Err(_) => LibraryMigrationState::Unknown,
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
    //
    // 音高ごとに渡す（`TR-RCL-26`, `DEC-SYN-014`）。 試唱と同じく、ノートごとに
    // そのノートが使える区画だけで解く。
    let covered = ledger.covered_aliases_by_tone()?;
    let songs: Vec<(String, Song)> = ledger.songs_in_bank()?;
    // あと何行かは、フルリストの部分集合として数える（`TR-RCL-16`）。
    // 詰め直さないので、ここで渡すのはいま使っている録音リストそのもの。
    let full_list = preset.reclist(rules).map_err(AppError::from_failure)?;
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
        color: traits.as_ref().map(VoiceTint::from_traits),
    })
}

/// 録音リストの行の数。
fn count_rows(ledger: &mut Ledger) -> Result<u32> {
    Ok(u32::try_from(ledger.rows_with_takes()?.len()).unwrap_or(u32::MAX))
}

fn no_stream() -> AppError {
    AppError::new(
        "app.no_stream",
        Class::Rejected,
        "入力ストリームを開いていない",
    )
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
        // 試唱は止める順が決まっている（`stop_preview`）。 フィールドの宣言順に
        // 任せると、合成の待ち合わせが再生より先に来る。
        self.stop_preview();
        // キャプチャストリームと排出スレッドを先に落とす（順序は `CaptureLease` の
        // `Drop` が守る）。 ゲインを触るのはそのあと。
        self.lease = None;
        // 開いている音源の貸与も閉じる。 `AppState` の枠や T09 の resolver が
        // `Arc` を握り続けていても、`runtime.lease_expired` で断られる。
        if let Some(open) = self.open.take() {
            // 背後の仕事も、貸与ごと取り消す（`release_project`）。
            self.jobs.cancel_lease(&open.lease);
            if let Err(e) = open.runtime.close(&open.lease) {
                koeru_failure::record_failure(
                    &e,
                    koeru_failure::Outcome::NotCommitted,
                    "shutdown.close_project",
                );
            }
        }
        if let Some((device, before)) = self.gain_before.take()
            && let Err(e) = mac::write_gain(&device, before)
        {
            // 終了の途中なので、記録して諦める。 利用者のマイクの設定は変わったまま残る。
            koeru_failure::record_failure(
                &e,
                koeru_failure::Outcome::NotCommitted,
                "shutdown.restore_gain",
            );
        }
    }
}

/// `device_transition` の純粋な分岐を試験する。
///
/// `Studio::open` を要らない——`Session::device()` の3値と生死の2値の
/// 組み合わせだけで決まる（`TR-REC-04`、`REQ-REC-109`）。 実機はもちろん
/// プロジェクトも無しで、書いていない OS 向けの組み立てでも同じ数だけ通る。
#[cfg(test)]
mod device_transition_tests {
    use super::{Device, DeviceTransition, device_transition};

    #[test]
    fn 選択中に見えなくなったら見失う() {
        assert_eq!(
            device_transition(Device::Selected, false),
            DeviceTransition::Lost
        );
    }

    #[test]
    fn 見失っていて同じ識別子が見えたら戻る() {
        assert_eq!(
            device_transition(Device::Lost, true),
            DeviceTransition::Returned
        );
    }

    #[test]
    fn 選択中で生きていれば何もしない() {
        assert_eq!(
            device_transition(Device::Selected, true),
            DeviceTransition::None
        );
    }

    #[test]
    fn 見失ったままなら何もしない() {
        // 一覧の変化が別のデバイスの着脱で、選択中のものはまだ見えない。
        assert_eq!(
            device_transition(Device::Lost, false),
            DeviceTransition::None
        );
    }

    #[test]
    fn 何も選んでいなければ何もしない() {
        // まだ選択していない・別のデバイスが挿さっただけ。生死の値は関係ない。
        assert_eq!(
            device_transition(Device::NotSelected, false),
            DeviceTransition::None
        );
        assert_eq!(
            device_transition(Device::NotSelected, true),
            DeviceTransition::None
        );
    }
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

    /// テイクの名前は付け、変え、消せる。 空白だけの名前は消すのと同じで、既定の表示へ戻る。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn テイクに名前を付け_変え_消せる() {
        let root = std::env::temp_dir().join(format!("koeru-take-label-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("名前").expect("作れる");
        studio.open_project(id).expect("開ける");
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        studio.mark_recorded_for_test(&row).expect("印を付けられる");
        let label_of = |s: &mut Studio| {
            let rows = s.rows_with_takes().expect("行を引ける");
            let r = rows
                .into_iter()
                .find(|r| r.row_id == row)
                .expect("行がある");
            r.takes
                .into_iter()
                .map(|t| (t.id, t.label))
                .next()
                .expect("テイクがある")
        };
        let (take, before) = label_of(&mut studio);
        assert_eq!(before, None, "付けるまでは名前を持たない");

        let named = studio
            .rename_take(take, Some("  ちょい明め "))
            .expect("付けられる")
            .expect("テイクがある");
        assert_eq!(
            named.label.as_deref(),
            Some("ちょい明め"),
            "前後の空白は落とす"
        );
        assert_eq!(label_of(&mut studio).1.as_deref(), Some("ちょい明め"));

        studio.rename_take(take, Some("本命")).expect("変えられる");
        assert_eq!(label_of(&mut studio).1.as_deref(), Some("本命"));

        let cleared = studio
            .rename_take(take, Some("   "))
            .expect("消せる")
            .expect("テイクがある");
        assert_eq!(cleared.label, None, "空白だけは消すのと同じ");
        studio.rename_take(take, Some("本命")).expect("付け直せる");
        studio.rename_take(take, None).expect("消せる");
        assert_eq!(label_of(&mut studio).1, None);

        assert!(
            studio
                .rename_take(take + 1000, Some("無い"))
                .expect("失敗にしない")
                .is_none(),
            "無いテイクは値で返す"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 確定のあとの解析が置いた F0 を、`.frq` の格子のまま読める。 解析がまだ置いていない
    /// テイクは値で「無い」と返し、無いテイクは失敗にする。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 確定したテイクの音高の記録を読める() {
        let root = std::env::temp_dir().join(format!("koeru-take-pitch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("音高").expect("作れる");
        studio.open_project(id).expect("開ける");
        let rows = studio.rows_with_takes().expect("行を引ける");

        let analysed = studio
            .seed_material_for_test(&rows[0].row_id)
            .expect("置ける");
        let pitch = studio
            .take_pitch(analysed)
            .expect("読める")
            .expect("解析が置いてある");
        assert_eq!(pitch.hop, koeru_core::frq::HOP_SIZE);
        let frames = usize::try_from(MASTER_RATE_HZ).unwrap_or(0) * 3 / 2;
        assert_eq!(
            pitch.f0.len(),
            frames.div_ceil(256),
            "テイク全体を格子で覆う"
        );
        assert!(
            pitch.f0.iter().any(|&f| (f - 220.0).abs() < 1e-9),
            "{:?}",
            &pitch.f0[..4]
        );

        studio
            .mark_recorded_for_test(&rows[1].row_id)
            .expect("印を付けられる");
        let pending = studio
            .rows_with_takes()
            .expect("行を引ける")
            .into_iter()
            .find(|r| r.row_id == rows[1].row_id)
            .and_then(|r| r.takes.into_iter().next())
            .expect("テイクがある");
        assert!(
            studio
                .take_pitch(pending.id)
                .expect("失敗にしない")
                .is_none(),
            "解析がまだ置いていなければ無い"
        );

        let missing = studio.take_pitch(analysed + 1000).expect_err("無いテイク");
        assert_eq!(missing.code, "ledger.unknown_take");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 録り直しても、人が直した値を引き継ぐ（`REQ-ALN-007`, `INV-ALN-001`）。
    ///
    /// **素の綴りで引いていた。** キューは（音高, 綴り）で持つので鍵が合わず、
    /// 録り直すたびに直した値が自動の値で上書きされていた。
    ///
    /// 新しいテイクは境界を持たないので、当て直さずに絶対位置のまま写る（`DEC-ALN-019`）。
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
            .writer()
            .expect("書き手")
            .commit_take(&FinalizedTake {
                row_id: row.clone(),
                session_id,
                rel_path: format!("audio/{row}_2.wav"),
                frames: 44_100,
                recorded_at: now_rfc3339(),
            })
            .expect("テイクを確定できる");
        open.writer()
            .expect("書き手")
            .adopt_take(&row, take)
            .expect("採れる");
        open.writer()
            .expect("書き手")
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
            .writer()
            .expect("開いている")
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

    /// 録り直したテイクでは、固定した左ブランクを発声の始まりに合わせて写す（`DEC-ALN-019`）。
    ///
    /// **絶対位置のまま写していた。** 発声が 100ms 遅れただけで、固定した左ブランクが
    /// 新しい録音の無音の中を指した。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 録り直したら固定した左ブランクを発声に合わせる() {
        let root = std::env::temp_dir().join(format!("koeru-reanchor-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("当て直し").expect("作れる");
        studio.open_project(id).expect("開ける");
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        let first = studio.seed_material_for_test(&row).expect("素材を置ける");
        let item = studio
            .review_queue()
            .expect("キューを引ける")
            .into_iter()
            .find(|i| i.row_id == row)
            .expect("その行のエントリがある");
        studio
            .edit_oto_value(&item.key, "offset", 123.0)
            .expect("直せる");
        let was = studio
            .onsets_of_take(first)
            .expect("引ける")
            .get(&item.alias)
            .copied()
            .expect("1本目はその綴りの発声の始まりを持つ");

        // 2本目は同じ並びで、どのモーラも 100ms 遅れて始まる。
        let session_id = studio.test_session().expect("セッションを始められる");
        let open = studio.opened_mut().expect("開いている");
        let take = open
            .writer()
            .expect("書き手")
            .commit_take(&FinalizedTake {
                row_id: row.clone(),
                session_id,
                rel_path: format!("audio/{row}_2.wav"),
                frames: 44_100,
                recorded_at: now_rfc3339(),
            })
            .expect("テイクを確定できる");
        open.writer()
            .expect("書き手")
            .adopt_take(&row, take)
            .expect("採れる");
        let later: Vec<(String, koeru_core::oto::Boundary)> = open
            .writer()
            .expect("書き手")
            .boundaries_for_take(first)
            .expect("引ける")
            .into_iter()
            .map(|(a, b)| {
                (
                    a,
                    koeru_core::oto::Boundary {
                        voice_start_ms: b.voice_start_ms + 100.0,
                        vowel_start_ms: b.vowel_start_ms + 100.0,
                        vowel_end_ms: b.vowel_end_ms + 100.0,
                    },
                )
            })
            .collect();
        open.writer()
            .expect("書き手")
            .put_boundaries(take, &later)
            .expect("置ける");
        open.writer()
            .expect("書き手")
            .put_oto(
                take,
                &item.alias,
                &koeru_core::db::koeru_oto::Oto {
                    offset_ms: 150.0,
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

        let now = studio
            .onsets_of_take(take)
            .expect("引ける")
            .get(&item.alias)
            .copied()
            .expect("2本目もその綴りの発声の始まりを持つ");
        assert!((now - was - 100.0).abs() < f64::EPSILON);
        let got = studio
            .writer()
            .expect("開いている")
            .oto_of(take, &item.alias)
            .expect("引ける")
            .expect("エントリがある");
        assert!(
            (got.offset_ms - 223.0).abs() < f64::EPSILON,
            "発声の始まりの差だけ動く: {}",
            got.offset_ms
        );
        let state = studio
            .review_queue()
            .expect("キューを引ける")
            .into_iter()
            .find(|i| i.key == item.key)
            .map(|i| i.state)
            .expect("エントリが残っている");
        assert_eq!(state, "in_queue", "当て直しても確認に入る");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 直せない違反は、そのエントリを書き出し阻止へ回す（`REQ-ALN-004`, `TR-ALN-20`）。
    ///
    /// **素の綴りで引いていた。** キューの鍵は（音高, 綴り）なので当たらず、
    /// `review.no_such_entry` を失敗として返していた——直せない違反が1件でも
    /// あると検証そのものが落ち、止めるべきエントリはキューに現れなかった。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 直せない違反はエントリを止める() {
        let root = std::env::temp_dir().join(format!("koeru-block-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("阻止").expect("作れる");
        studio.open_project(id).expect("開ける");
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        studio.seed_material_for_test(&row).expect("素材を置ける");
        let item = studio
            .review_queue()
            .expect("キューを引ける")
            .into_iter()
            .find(|i| i.row_id == row)
            .expect("その行のエントリがある");

        // ファイルの外を指すオフセットを人が固定する。 修復はオフセットを
        // 動かしたいが、固定があるので動かせない——直せない違反になる。
        studio
            .edit_oto_value(&item.key, "offset", 999_999.0)
            .expect("固定できる");

        let (_, blocked) = studio.validate_otos().expect("検証は落ちない");
        assert!(
            blocked.contains(&item.alias),
            "止めた綴りを返す: {blocked:?}"
        );
        let state = studio
            .review_queue()
            .expect("キューを引ける")
            .into_iter()
            .find(|i| i.key == item.key)
            .map(|i| i.state)
            .expect("エントリが残っている");
        assert_eq!(state, "blocked", "書き出し阻止へ回る");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 解析は、計算では何も置かず、受理で初めて台帳・`.frq`・音域へ置く（`DEC-PLT-034`）。
    ///
    /// 計算した oto が、そのまま台帳に載り、画面へ返る値と一致する。
    /// 合成音なので構造しか見ていない（`EVID-ALN-001`）。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 解析は受理で初めて置かれる() {
        use crate::take_analysis::{Step, analyze_take, tests::synthetic_voice};

        let root = std::env::temp_dir().join(format!("koeru-admit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("受理").expect("作れる");
        studio.open_project(id).expect("開ける");
        let session_id = studio.test_session().expect("セッションを始められる");
        let rows = studio.rows_with_takes().expect("行を引ける");
        let row = rows[0].row_id.clone();
        let moras = studio
            .writer()
            .expect("書き手")
            .units_of(&row)
            .expect("読み")
            .len();

        let samples = synthetic_voice(moras, 180.0);
        let rel = format!("audio/{row}_1.wav");
        let path = studio.opened().expect("開いている").dir().root().join(&rel);
        std::fs::create_dir_all(studio.opened().expect("開いている").dir().audio_dir())
            .expect("音声の置き場を作れる");
        let mut part =
            koeru_audio::wav::PartialTake::create(&path, MASTER_RATE_HZ).expect("作れる");
        part.write(&samples).expect("書ける");
        part.finalize().expect("確定できる");
        let take = studio
            .writer()
            .expect("書き手")
            .commit_take(&FinalizedTake {
                row_id: row.clone(),
                session_id,
                rel_path: rel,
                frames: i64::try_from(samples.len()).unwrap_or(0),
                recorded_at: now_rfc3339(),
            })
            .expect("確定できる");
        studio
            .writer()
            .expect("書き手")
            .adopt_take(&row, take)
            .expect("採用できる");

        let facts = TakeFacts {
            counts: koeru_core::db::TakeGapCounts::default(),
            gaps: Vec::new(),
            guide_offset: None,
            preroll_frames: 0,
            duration_ms: samples.len() as f64 * 1000.0 / f64::from(MASTER_RATE_HZ),
        };
        let finished = crate::pump::Finished {
            path: path.clone(),
            samples,
            preroll_frames: 0,
        };
        let observed = |s: &Studio| s.opened().expect("開いている").caches.observed_f0.len();
        let before = observed(&studio);

        let input = studio.collect_take_analysis(take, &row, finished, facts);
        let output = analyze_take(&input, &|| Ok(())).expect("取り消されない");

        // 計算では何も置かない。 音域も動かさない。
        assert!(
            studio
                .writer()
                .expect("書き手")
                .analysis_of(take)
                .expect("引ける")
                .is_none(),
            "計算は台帳へ書かない"
        );
        assert!(!koeru_core::frq::frq_path(&path).expect("名前").exists());
        assert_eq!(observed(&studio), before, "音域の更新は受理でする");

        let computed = output.steps.iter().find_map(|s| match s {
            Step::Entries(e) => e.first,
            _ => None,
        });
        let derived = studio.admit_take_analysis(output);

        assert!(derived.failure.is_none(), "{:?}", derived.failure);
        assert!(
            studio
                .writer()
                .expect("書き手")
                .analysis_of(take)
                .expect("引ける")
                .is_some(),
            "受理で解析値を置く"
        );
        assert!(koeru_core::frq::frq_path(&path).expect("名前").exists());
        assert_eq!(observed(&studio), before + 1, "受理で音域へ足す");
        // 計算した oto が、画面へ返る値と台帳の値の両方になる。
        assert_eq!(derived.oto, computed.map(|(oto, _)| oto));
        assert_eq!(derived.confidence, computed.map(|(_, score)| score));
        if let Some(oto) = derived.oto {
            let stored: Vec<_> = studio
                .writer()
                .expect("書き手")
                .otos_of(take)
                .expect("引ける")
                .into_iter()
                .map(|(_, o)| o)
                .collect();
            assert!(stored.contains(&oto), "先頭の oto は台帳にある");
        }

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 試唱を止めると、前処理の仕事だけが取り消される。 テイクの解析は止めない
    /// （`TR-SYN-27`。`cancel_lease` ではなく `cancel` を使う理由）。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 試唱を止めると前処理だけが取り消され解析は止まらない() {
        use koeru_runtime::jobs::JobStatus;
        use std::sync::mpsc;
        use std::time::Duration;

        let root = std::env::temp_dir().join(format!("koeru-stop-preview-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("止める").expect("作れる");
        studio.open_project(id).expect("開ける");
        let jobs = studio.jobs();
        let lease = studio.opened().expect("開いている").lease;

        // 前景の作業スレッドを塞ぎ、前処理と解析を並べる。
        let (started_tx, started) = mpsc::channel();
        let (release, hold) = mpsc::channel::<()>();
        let blocker = jobs
            .submit(JobSpec::new(
                lease,
                JobKind::Analysis,
                Priority::PostRecording,
                move |_| {
                    let _ = started_tx.send(());
                    let _ = hold.recv_timeout(Duration::from_secs(60));
                    Ok(())
                },
            ))
            .expect("受け付けること");
        started
            .recv_timeout(Duration::from_secs(60))
            .expect("走り出す");
        let prerender = jobs
            .submit_notify(
                JobSpec::new(
                    lease,
                    JobKind::PreviewRender,
                    Priority::PostRecording,
                    |_| Ok(()),
                ),
                |_| {},
            )
            .expect("受け付けること");
        let analysis = jobs
            .submit(JobSpec::new(
                lease,
                JobKind::Analysis,
                Priority::PostRecording,
                |_| Ok(()),
            ))
            .expect("受け付けること");
        studio.prerender = Some(prerender.job());

        studio.stop_preview();

        assert!(matches!(
            jobs.job(prerender.job()).expect("表にある").status,
            JobStatus::Cancelled { .. }
        ));
        assert!(
            matches!(
                jobs.job(analysis.job()).expect("表にある").status,
                JobStatus::Queued { .. }
            ),
            "解析は止めない"
        );
        assert!(studio.prerender.is_none(), "覚えを捨てる");
        let _ = release.send(());
        blocker.wait().expect("終わる");
        analysis.wait().expect("解析は進む");

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 受理の直前に作り直した印は、何も変わっていなければ確定のときの印と一致する。
    /// 計算が読んだものが変われば（F0 の条件・行の読み）一致しなくなる。
    #[cfg(all(target_os = "macos", not(koeru_force_unsupported_backend)))]
    #[test]
    fn 読んだ入力の印は変わらなければ一致し変われば外れる() {
        let root = std::env::temp_dir().join(format!("koeru-stamp-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("印").expect("作れる");
        studio.open_project(id).expect("開ける");
        let rows = studio.rows_with_takes().expect("行を引ける");
        let row = rows[0].row_id.clone();
        let other = rows[1].row_id.clone();
        let take = 1;

        let (purpose, floor) = studio.f0_conditions();
        let stamped =
            take_analysis::analysis_based_on(take, purpose, floor, &studio.read_row(&row));
        assert!(
            stamped.is_current_in(&studio.current_analysis_based_on(take, &row)),
            "何も変わっていない"
        );
        assert!(!stamped.is_empty());

        // 別の行を読む印とは一致しない（読みが違う）。
        assert!(!stamped.is_current_in(&studio.current_analysis_based_on(take, &other)));
        // 別のテイクの印とも一致しない。
        assert!(!stamped.is_current_in(&studio.current_analysis_based_on(take + 1, &row)));
        // F0 の下限が決まると外れる。
        studio
            .test_observe_f0(vec![220.0; 4], 80.0)
            .expect("埋められる");
        assert!(!stamped.is_current_in(&studio.current_analysis_based_on(take, &row)));

        // 指紋は中身を出さない。
        assert!(!format!("{stamped:?}").contains(&row));

        let _ = std::fs::remove_dir_all(&root);
    }
}

#[cfg(test)]
mod consistency_tests {
    use crate::take_analysis::{consistency_reading, first_phoneme, last_phoneme};
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

/// 解析の値が無いときに画面へ返す値と、受理が途中で落ちたときの失敗の持ち方。
/// 実機も MFA も要らないので、書いていない OS 向けの組み立てでも同じ数だけ通る。
#[cfg(test)]
mod derived_take_tests {
    use super::*;

    /// 計算の出力が無くても、手元の標本から波形の要約・最大値・計測を作れる。
    /// oto は付かず、失敗が載る。
    #[test]
    fn 計算の出力が無いときは手元の標本から作る() {
        let samples = vec![0.0_f32, 0.5, -0.25, 0.1];
        let failure = AppError::from(JobError::DeadlineExceeded);
        let derived = DerivedTake::from_samples(&samples, MASTER_RATE_HZ, failure);
        assert!((derived.peak - 0.5).abs() < f32::EPSILON);
        assert_eq!(
            derived.thumbnail.len(),
            koeru_core::analysis::THUMBNAIL_BUCKETS
        );
        assert!(derived.oto.is_none() && derived.confidence.is_none());
        assert_eq!(
            derived.failure.as_ref().map(|e| e.code),
            Some("jobs.deadline_exceeded")
        );
    }

    /// 受理が途中で落ちた失敗は、code と分類を落ちた工程のまま運び、`Debug` に
    /// 波形や計測を出さない。
    #[test]
    fn 受理の途中の失敗は_code_と分類を運び_debug_で値を出さない() {
        let failure = AppError::from(koeru_core::db::LedgerError::UnknownTake);
        let unfinished = Unfinished {
            derived: Box::new(DerivedTake::from_samples(
                &[0.9_f32],
                MASTER_RATE_HZ,
                failure.clone(),
            )),
            failure,
        };
        assert_eq!(Failure::code(&unfinished), "ledger.unknown_take");
        assert_eq!(Failure::class(&unfinished), Class::Conflict);
        let shown = format!("{unfinished:?}");
        assert!(shown.contains("ledger.unknown_take"), "{shown}");
        assert!(!shown.contains("0.9"), "{shown}");
    }
}

/// 試唱が鳴らす素材と、書き出しが配る素材（`DEC-PLT-052`）。 本物の SQLite の台帳を通す。
// MFA を組んでいない OS では `Studio::open` が設計どおり失敗する（`DEC-ALN-016`）。
#[cfg(all(test, target_os = "macos", not(koeru_force_unsupported_backend)))]
mod material_tests {
    use super::*;

    fn opened(tag: &str, preset: &str, tones: &[i32]) -> Studio {
        let root = std::env::temp_dir().join(format!(
            "koeru-materials-{tag}-{}-{}",
            std::process::id(),
            tones.len()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let mut s = Studio::open(root).expect("ライブラリを開ける");
        let id = s
            .create_project_with("素材", preset, tones)
            .expect("作れる");
        s.open_project(id).expect("開ける");
        s
    }

    /// `- い` を生む行の ID を、並びの順に。
    fn producers_of_head_i(s: &mut Studio) -> Vec<String> {
        let rows: Vec<String> = s
            .rows_with_takes()
            .expect("引ける")
            .into_iter()
            .map(|r| r.row_id)
            .collect();
        let mut out = Vec::new();
        for r in rows {
            if s.writer()
                .expect("書き手")
                .aliases_of_row(&r)
                .expect("引ける")
                .contains("- い")
            {
                out.push(r);
            }
        }
        out
    }

    fn by_tone(s: &mut Studio) -> std::collections::BTreeMap<Option<i32>, Materials> {
        let root = s.opened().expect("開いている").dir().root().to_path_buf();
        let rules = s.current_rules().expect("引ける");
        let preset = s.current_preset().expect("引ける");
        s.materials_by_tone(&root, &rules, preset)
            .expect("素材を集められる")
    }

    /// 試唱は、綴りの持ち主の行のテイクで鳴らす（`DEC-RCL-016`）。
    ///
    /// **並びが前の行のテイクを引いていた。** 先に録ったのが並びの後ろの行（詰め直した行を
    /// 先に録ったときなど）だと、5値は持ち主の行にしか無いので、引いた先に何も無くて
    /// 綴りが黙って鳴らなかった。 どちらの順で録っても、同じ綴りが揃って鳴る。
    #[test]
    fn 試唱は先に録った行の素材で鳴らし_録る順に依らず綴りが揃う() {
        let mut sounded = Vec::new();
        for hi_first in [true, false] {
            let mut s = opened(
                if hi_first { "hi-first" } else { "lo-first" },
                "sequential",
                &[57],
            );
            let producers = producers_of_head_i(&mut s);
            assert!(producers.len() > 1, "第2段が語頭 CV を重ねて生む前提");
            let (lo, hi) = (producers[0].clone(), producers[producers.len() - 1].clone());
            let order = if hi_first { [&hi, &lo] } else { [&lo, &hi] };
            for row in order {
                s.seed_material_for_test(row).expect("置ける");
            }
            let owner = if hi_first { &hi } else { &lo };

            let root = s.opened().expect("開いている").dir().root().to_path_buf();
            let got = by_tone(&mut s);
            let m = got.get(&None).expect("単音階の鍵は None");
            assert_eq!(
                m.paths.get("- い"),
                Some(&root.join(format!("audio/{owner}_1.wav"))),
                "先に録った行のテイクを引く"
            );
            assert!(m.otos.contains_key("- い"));
            assert!(
                m.tables.contains_key("- い"),
                "周波数表も持ち主のテイクのもの"
            );
            let owned = s
                .writer()
                .expect("書き手")
                .owned_aliases_of_row(owner)
                .expect("引ける");
            assert!(
                owned.iter().all(|a| m.paths.contains_key(a)),
                "持ち主の綴りが全部鳴る"
            );
            sounded.push(m.paths.keys().cloned().collect::<BTreeSet<_>>());
        }
        assert_eq!(sounded[0], sounded[1], "録る順を替えても鳴る綴りは同じ");
    }

    /// 同じ綴りでも、収録音高が違えば別の素材（`TR-SYN-16`）。 区画の鍵は音高。
    #[test]
    fn 試唱は音高ごとに別の素材を引く() {
        let mut s = opened("tones", "sequential", &[55, 62]);
        let rows: Vec<String> = s
            .rows_with_takes()
            .expect("引ける")
            .into_iter()
            .map(|r| r.row_id)
            .collect();
        let low = rows
            .iter()
            .find(|r| r.ends_with("@G3"))
            .expect("G3 の行")
            .clone();
        let high = rows
            .iter()
            .find(|r| r.ends_with("@D4"))
            .expect("D4 の行")
            .clone();
        s.seed_material_for_test(&high).expect("置ける");
        s.seed_material_for_test(&low).expect("置ける");

        let root = s.opened().expect("開いている").dir().root().to_path_buf();
        let got = by_tone(&mut s);
        assert_eq!(got.len(), 2, "音高の数だけ区画を持つ");
        let a = s
            .writer()
            .expect("書き手")
            .owned_aliases_of_row(&low)
            .expect("引ける")
            .into_iter()
            .find(|a| got[&Some(62)].paths.contains_key(a))
            .expect("両方の音高に同じ綴りがある");
        assert_eq!(
            got[&Some(55)].paths.get(&a),
            Some(&root.join(format!("audio/{low}_1.wav")))
        );
        assert_eq!(
            got[&Some(62)].paths.get(&a),
            Some(&root.join(format!("audio/{high}_1.wav")))
        );
    }

    /// 試唱が鳴らす綴りは、書き出しが配る綴りから、鳴らさない語尾を除いたもの。
    /// 違いは用途（`UseContext`）だけが持つ。
    #[test]
    fn 試唱の綴りは書き出しの綴りから語尾を除いたもの() {
        use koeru_model::materials::{self, Delivery};

        let mut s = opened("cvvc", "cvvc", &[57]);
        let rows: Vec<String> = s
            .rows_with_takes()
            .expect("引ける")
            .into_iter()
            .map(|r| r.row_id)
            .take(12)
            .collect();
        for r in &rows {
            s.seed_material_for_test(r).expect("置ける");
        }

        let rules = s.current_rules().expect("引ける");
        let preset = s.current_preset().expect("引ける");
        let catalog = koeru_model::target::Catalog::new(&rules, preset.method, preset.set);
        let exported: BTreeSet<String> = {
            let material = s
                .writer()
                .expect("書き手")
                .material_facts()
                .expect("引ける");
            materials::resolve(
                &material.facts,
                UseContext::Export {
                    delivery: Delivery::AsRecorded,
                },
                |_| Role::Cv,
            )
            .used()
            .map(|c| c.key.1.clone())
            .collect()
        };
        let previewed: BTreeSet<String> = by_tone(&mut s)
            .remove(&None)
            .expect("単音階の鍵は None")
            .paths
            .into_keys()
            .collect();

        let endings: BTreeSet<String> = exported
            .iter()
            .filter(|a| Role::of(&catalog, a) == Role::Ending)
            .cloned()
            .collect();
        assert!(!endings.is_empty(), "語尾を録ってある前提");
        assert!(
            exported.iter().any(|a| Role::of(&catalog, a) == Role::Vc),
            "渡りも録ってある前提"
        );
        assert!(previewed.is_disjoint(&endings), "試唱は語尾を鳴らさない");
        let expected: BTreeSet<String> = exported.difference(&endings).cloned().collect();
        assert_eq!(previewed, expected, "語尾以外は書き出しと同じ");

        // 画面が出す書き出しの数と、解決が配ると決めた数が一致する。
        let state = s.package_state().expect("引ける");
        assert_eq!(state.alias_count, exported.len(), "配る綴りは解決のとおり");
    }
}

/// マスターの内容の指紋（`DEC-PLT-053`）が、試唱の素材の鍵とジョブの入力の印になる。
///
/// `Studio::open` はアライナを読むので、Kaldi を組んである OS だけで走る。
#[cfg(all(test, target_os = "macos", not(koeru_force_unsupported_backend)))]
mod master_digest_tests {
    use super::*;

    fn opened(tag: &str) -> (Studio, PathBuf) {
        let root = std::env::temp_dir().join(format!("koeru-digest-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("指紋").expect("作れる");
        studio.open_project(id).expect("開ける");
        (studio, root)
    }

    /// 次に録る行へ素材を置き、そのテイクの ID と WAV の場所を返す。
    fn seed(studio: &mut Studio) -> (i32, PathBuf) {
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        let take = studio.seed_material_for_test(&row).expect("素材を置ける");
        let path = studio
            .opened()
            .expect("開いている")
            .dir()
            .audio_dir()
            .join(format!("{row}_1.wav"));
        (take, path)
    }

    /// テイクの WAV の、プロジェクトの中での場所。
    fn rel_of(studio: &mut Studio, take: i32) -> String {
        studio
            .writer()
            .expect("書き手")
            .take(take)
            .expect("読める")
            .expect("テイクがある")
            .rel_path
    }

    fn overwrite(path: &std::path::Path, amplitude: f32) {
        std::fs::remove_file(path).expect("消せる");
        let mut part = koeru_audio::wav::PartialTake::create(path, MASTER_RATE_HZ).expect("作れる");
        part.write(&vec![amplitude; MASTER_RATE_HZ as usize])
            .expect("書ける");
        part.finalize().expect("確定できる");
    }

    /// 同じ中身は場所が違っても同じ鍵、中身が違えば別の鍵（`TR-SYN-02`）。
    #[test]
    fn 同じ中身は別の場所でも同じ鍵になり_中身が違えば別の鍵になる() {
        let (mut studio, root) = opened("material");
        let (_, a) = seed(&mut studio);
        let (_, b) = seed(&mut studio);
        let (_, c) = seed(&mut studio);
        assert_ne!(a, b, "前提: 場所が違う");
        // 試験の素材は同じ中身（一定振幅）で作られる。 c だけ、まだ誰も指紋を見ていない
        // うちに中身を変える。
        overwrite(&c, 0.9);

        let key = |studio: &mut Studio, p: &PathBuf| {
            studio
                .material_digest(Some(p))
                .expect("計算できる")
                .expect("素材がある")
        };
        let (ka, kb, kc) = (
            key(&mut studio, &a),
            key(&mut studio, &b),
            key(&mut studio, &c),
        );
        assert_eq!(ka, kb, "同じ中身は別のパスでも同じ鍵");
        assert_ne!(ka, kc, "中身が違えば別の鍵");
        assert_eq!(
            ka,
            koeru_core::master::digest_file(&a).expect("読める"),
            "鍵はファイルの中身の SHA-256"
        );
        assert_eq!(studio.material_digest(None).expect("素材なしは値"), None);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 台帳の外のファイルは、同じ場所で中身が変われば別の鍵になる。 更新時刻と大きさが
    /// 同じでも見落とさない（以前の鍵は場所・大きさ・更新時刻だった）。
    #[test]
    fn 台帳の外のファイルは中身だけで決まる() {
        let (mut studio, root) = opened("outside");
        let dir = root.join("outside");
        std::fs::create_dir_all(&dir).expect("作れる");
        let (x, y) = (dir.join("x.bin"), dir.join("y.bin"));
        std::fs::write(&x, b"AAAA").expect("書ける");
        std::fs::write(&y, b"AAAA").expect("書ける");
        let before = studio.material_digest(Some(&x)).expect("計算できる");
        assert_eq!(
            before,
            studio.material_digest(Some(&y)).expect("計算できる"),
            "別の場所の同じ中身"
        );

        // 大きさも更新時刻も変えずに、中身だけ変える。
        let mtime = std::fs::metadata(&x)
            .and_then(|m| m.modified())
            .expect("時刻");
        std::fs::write(&x, b"BBBB").expect("書ける");
        std::fs::File::options()
            .write(true)
            .open(&x)
            .and_then(|f| f.set_modified(mtime))
            .expect("時刻を戻せる");
        assert_eq!(
            std::fs::metadata(&x).map(|m| m.len()).expect("大きさ"),
            4,
            "前提: 大きさは同じ"
        );
        assert_ne!(
            before,
            studio.material_digest(Some(&x)).expect("計算できる"),
            "同じ場所・同じ大きさ・同じ時刻でも、中身が変われば別の鍵"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 台帳に記録した指紋は、確定の直後に1度だけ置かれ、そのあとはファイルを読み直さない。
    /// 列を足す前に録ったテイク（空のまま）は、要るときにファイルから計算して置く。
    #[test]
    fn 記録した指紋を使い_空なら要るときに埋める() {
        let (mut studio, root) = opened("seal");
        let (take, path) = seed(&mut studio);
        let rel = rel_of(&mut studio, take);
        let stored = |studio: &mut Studio| {
            studio
                .writer()
                .expect("書き手")
                .master_digest_of(take)
                .expect("読める")
        };
        // 空のまま（試験の素材は確定の経路を通らない）。
        assert_eq!(stored(&mut studio), None);

        // 確定の直後の記録。
        studio.seal_master_digest(take);
        let want = koeru_core::master::digest_file(&path).expect("読める");
        assert_eq!(stored(&mut studio), Some(want));
        assert_eq!(
            studio.master_stamp(take, &rel),
            want.to_hex(),
            "ジョブの入力の印は指紋の16進"
        );

        // 記録したあとは、ファイルが変わっても読み直さない（確定したマスターは変えない）。
        overwrite(&path, 0.9);
        assert_eq!(studio.master_digest(take, &rel).expect("引ける"), want);

        let _ = std::fs::remove_dir_all(&root);
    }

    /// 読めなかったことも入力の一部で、読めた印とは別になる。
    #[test]
    fn 指紋を作れなければ読めなかった印になる() {
        let (mut studio, root) = opened("unreadable");
        let (take, path) = seed(&mut studio);
        let rel = rel_of(&mut studio, take);
        std::fs::remove_file(&path).expect("消せる");
        let stamp = studio.master_stamp(take, &rel);
        assert!(stamp.starts_with("unreadable:"), "{stamp}");

        let _ = std::fs::remove_dir_all(&root);
    }
}

/// ガイドのテンポ（`TR-REC-23`、`DEC-REC-012`）が、フレーズのガイドとテイクの録音条件に届く。
///
/// `Studio::open` はアライナを読むので、Kaldi を組んである OS だけで走る。
#[cfg(all(test, target_os = "macos", not(koeru_force_unsupported_backend)))]
mod guide_tempo_tests {
    use super::*;

    fn opened(tag: &str) -> (Studio, PathBuf, Uuid) {
        let root = std::env::temp_dir().join(format!("koeru-tempo-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let mut studio = Studio::open(root.clone()).expect("ライブラリを開ける");
        let id = studio.create_project("テンポ").expect("作れる");
        studio.open_project(id).expect("開ける");
        (studio, root, id)
    }

    fn tempo(bpm: i32) -> GuideTempo {
        GuideTempo::new(bpm).expect("選べるテンポ")
    }

    /// 次に録る行へテイクを1つ置き、その ID を返す。
    fn record_next(studio: &mut Studio) -> i32 {
        let (row, _) = studio
            .progress()
            .expect("進み具合を引ける")
            .next_row
            .expect("次に録る行がある");
        studio.seed_material_for_test(&row).expect("素材を置ける")
    }

    fn take_tempo(studio: &Studio, take: i32) -> Option<i32> {
        studio
            .writer()
            .expect("書き手")
            .take_guide_tempo(take)
            .expect("読める")
    }

    #[test]
    fn 変えていなければ_120_bpm_で鳴らして記録する() {
        let (mut studio, root, _) = opened("default");
        assert_eq!(studio.guide_tempo().expect("読める").bpm(), 120);
        let spec = studio.phrase_guide(8).expect("組める");
        assert!((spec.beat_ms() - 500.0).abs() < 1e-9);

        let take = record_next(&mut studio);
        assert_eq!(take_tempo(&studio, take), Some(120));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn 変えたテンポでフレーズのガイドの拍が刻まれる() {
        let (mut studio, root, _) = opened("phrase");
        let before = guide::render(&studio.phrase_guide(4).expect("組める"), 60, MASTER_RATE_HZ);

        assert_eq!(
            studio.set_guide_tempo(tempo(90)).expect("変えられる"),
            GuideTempoChange::Set(tempo(90))
        );
        let spec = studio.phrase_guide(4).expect("組める");
        assert!((spec.beat_ms() - 60_000.0 / 90.0).abs() < 1e-9);
        assert_eq!(spec.moras, 4, "モーラ数は行の構造のまま");
        assert_eq!(
            spec.lead_in_ms,
            koeru_core::guide::DEFAULT_LEAD_IN_MS,
            "助走の長さは変えない"
        );
        let after = guide::render(&spec, 60, MASTER_RATE_HZ);
        assert_eq!(
            after,
            guide::render(&GuideSpec::phrase(tempo(90), 4), 60, MASTER_RATE_HZ)
        );
        assert!(after.len() > before.len(), "遅くした拍のぶん長くなる");

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn 録ったテイクは録ったときのテンポを持ち続ける() {
        let (mut studio, root, _) = opened("take");
        let first = record_next(&mut studio);
        let session = studio.test_session_id().expect("始めてある");

        studio.set_guide_tempo(tempo(90)).expect("変えられる");
        assert_ne!(
            studio.test_session_id().expect("始めてある"),
            session,
            "テンポを変えたらセッションを切る"
        );
        let second = record_next(&mut studio);

        assert_eq!(
            take_tempo(&studio, first),
            Some(120),
            "前のテイクは変えない"
        );
        assert_eq!(take_tempo(&studio, second), Some(90));

        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn テンポは音源に残り_開き直しても変わらない() {
        let (mut studio, root, id) = opened("reopen");
        studio.set_guide_tempo(tempo(150)).expect("変えられる");
        studio.open_project(id).expect("開き直せる");
        assert_eq!(studio.guide_tempo().expect("読める"), tempo(150));

        let _ = std::fs::remove_dir_all(&root);
    }
}
