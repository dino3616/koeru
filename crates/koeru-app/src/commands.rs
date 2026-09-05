//! Tauri のコマンド。判断はここに置かない。
//!
//! [`crate::studio::Studio`] を呼んで、結果を画面へ渡す形へ直すだけ。
//! 筋を足すときは `studio` 側に足す（GUI 無しで検査できる場所に置く）。
//!
//! # 待つものは `#[tauri::command(async)]` にする
//!
//! 素の `#[tauri::command]` はメインスレッドで走る。 Tauri のマクロは
//! 同期関数を `kind.block(...)` で呼び出しスレッドのまま実行するので、
//! そこで数秒かかると WebView ごと止まる——テイクの確定は
//! アライメントを含めて数秒かかる。
//!
//! `async` を付けると `spawn_blocking` に載る。 関数自体は同期のままで、
//! 本体を書き換えなくてよい。`std::sync::Mutex` の番人が await を跨がないので、
//! `tokio::sync::Mutex` へ替える必要も無い。
//!
//! 移すのは、録音・合成・ファイル・FFT・台帳（SQLite）に触るもの。
//! 残すのは即返るものだけで、いま5つある。
//!
//! - `stream_envelope` — 自前でスレッドを立てて番号を即返す。`Channel` を持つ
//! - `stop_envelope_stream` — `compare_exchange` 1回
//! - `preroll_ms` — 値を1つ読む
//! - `output_kind` — OS へ1回問い合わせる
//! - `pending_work` — 1秒ごとに引かれる。即返る
//!
//! 即返るものを移さない。 スレッドプールへの往復が増えるだけで、
//! 待ち数の表示のように短い間隔で引くものは、むしろ遅くなる。

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use tauri::State;
use tauri::ipc::Channel;

use crate::error::{AppError, Result};
use crate::studio::{Preflight, Progress, SpaceEstimate, Studio, TakeResult};

/// 件数を画面へ渡せる幅へ落とす。
///
/// 画面に出すのは録音リストの行数や待ち数で、`u32` に収まらない値は
/// そもそも扱えない。 それでも飽和させるのは、溢れて 0 に見えるより
/// 上限に張り付くほうが「多すぎる」と読めるため。
fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// 同じことを `u64` から。残量の見積りが行数を返す経路で使う。
fn count64(n: u64) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// アプリが持つ状態。1本の `Mutex` で囲む。
///
/// 音声のコールバックはこの `Mutex` を取らない（`Capture` の中はアトミック）。
/// ここが守るのは画面から来る操作の直列化だけ。
#[derive(Debug)]
pub struct AppState {
    studio: Mutex<Studio>,
    /// 波形の包絡（`TR-REC-43`）。あえて `studio` の外に置く。
    ///
    /// テイクの確定はアライメントを含めて数秒かかる。同じロックを通すと、
    /// その間ずっと波形が止まる。
    envelope: Arc<Mutex<Option<Arc<Mutex<crate::pump::Envelope>>>>>,
    /// 待っている仕事（`TR-SYN-33`）。これも `studio` の外。
    ///
    /// 待ち数がいちばん動くのはテイクの確定中で、そこが `studio` を握っている。
    /// 同じロックを通すと、出したい時間だけ止まる。
    pending: Arc<Mutex<Option<crate::workers::PendingHandle>>>,
    /// 送っている流れの世代。新しく始めると、古いものが自分で止まる。
    stream: Arc<AtomicU32>,
}

impl AppState {
    /// 状態を作る。
    #[must_use]
    pub fn new(studio: Studio) -> Self {
        Self {
            studio: Mutex::new(studio),
            envelope: Arc::new(Mutex::new(None)),
            pending: Arc::new(Mutex::new(None)),
            stream: Arc::new(AtomicU32::new(0)),
        }
    }
}

/// 波形を送る間隔（ミリ秒、`TR-REC-43`）。
const ENVELOPE_FRAME_MS: u64 = 50;

/// `Mutex` が毒されたときの失敗。
///
/// 毒されたら握り潰さない。 どこかのコマンドが panic した証拠で、
/// そのまま続けると壊れた状態の上で操作を重ねる。
fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Studio>> {
    state
        .studio
        .lock()
        .map_err(|_| AppError::new("app.poisoned", "内部状態が壊れている。開き直してほしい"))
}

/// 画面へ返すデバイス。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct DeviceView {
    /// 永続識別子（`TR-REC-03`）。同一性の判定はこれで行う。
    pub id: String,
    /// 表示名。一覧に出すためだけ。
    pub name: String,
}

/// 画面へ返すプロジェクト。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProjectView {
    /// 不変の識別子。
    pub id: String,
    /// 表示名。manifest が読めなければ `None`。
    pub display_name: Option<String>,
    pub method: Option<String>,
    /// 項目数。
    pub item_count: Option<u32>,
}

/// 画面へ返す進み具合。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct ProgressView {
    /// 次に録る行の ID。
    pub next_row_id: Option<String>,
    /// 次に録る行の読み上げ文字列。
    pub next_row_text: Option<String>,
    /// 収録済み単位。
    pub covered: u32,
    /// 必要な単位。
    pub required: u32,
    /// 完成状態。
    pub coverage: String,
    /// 手渡し状態。完成判定はこれを見ない（`TR-PKG-33`）。
    pub handoff: String,
    /// いま歌える曲の数（`TR-RCL-19`）。カバレッジと常に両方出す。
    pub singable_songs: u32,
    /// バンクに入っている曲の数。0 でも成立する。
    pub songs_in_bank: u32,
}

impl From<Progress> for ProgressView {
    fn from(p: Progress) -> Self {
        let (id, text) = p.next_row.map_or((None, None), |(i, t)| (Some(i), Some(t)));
        Self {
            next_row_id: id,
            next_row_text: text,
            covered: count(p.covered),
            required: count(p.required),
            coverage: p.coverage.as_str().to_owned(),
            handoff: p.handoff.as_str().to_owned(),
            singable_songs: count(p.singable_songs),
            songs_in_bank: count(p.songs_in_bank),
        }
    }
}

/// 画面へ返す、いま流れている音の包絡（`TR-REC-43`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct EnvelopeView {
    /// 目盛りごとの min/max。畳まずにそのまま渡す。
    ///
    /// 割り切れない本数へ畳むと絵が揺れる。 画面は1本につき1列を描く。
    pub steps: Vec<(Finite, Finite)>,
    /// 排出しはじめてからのマスターの通算サンプル数。単調に増える。
    ///
    /// 画面が古い応答を捨てるための番号ではない。 Channel が順序を保証するので
    /// （`DEC-PLT-017`）、この経路に古い応答は届かない。進んでいないフレームを
    /// 送らないための比較に使う。
    ///
    /// TS では `number` にする。 JS の数値は f64 で、正確に持てるのは 2^53 まで。
    /// 44100 Hz でそこに届くのは 64 億年後なので、精度は落ちない。
    #[specta(type = f64)]
    pub position: u64,
}

/// 画面へ返す原音設定の1件（`TR-ALN-33`）。5値をそのまま渡す。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct OtoView {
    pub alias: String,
    #[specta(type = specta_typescript::Number)]
    pub offset_ms: f64,
    #[specta(type = specta_typescript::Number)]
    pub consonant_ms: f64,
    /// 負なら「offset からの長さ」、正なら「ファイル末尾からの距離」。
    #[specta(type = specta_typescript::Number)]
    pub cutoff_ms: f64,
    #[specta(type = specta_typescript::Number)]
    pub preutterance_ms: f64,
    #[specta(type = specta_typescript::Number)]
    pub overlap_ms: f64,
}

/// 画面へ返す「行と、その行のテイク」（`TR-REC-21`, `TR-RCL-25`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct RowTakesView {
    pub row_id: String,
    /// 読み上げる文字列。
    pub text: String,
    /// `unrecorded` / `recorded` / `needs_retake` / `excluded`。
    pub state: String,
    /// 世代順。非採用も含む——いつでも採用を戻せる（`TR-REC-21`）。
    pub takes: Vec<TakeSummaryView>,
    /// いま採用しているテイクの ID。
    pub adopted: Option<i32>,
}

/// 一覧に出すテイク1件。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TakeSummaryView {
    pub take_id: i32,
    /// 何本目か（1 始まり）。
    pub generation: i32,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: f64,
    /// 取りこぼしで自動的に無効にした（`TR-REC-07`）。
    pub invalid: bool,
}

impl From<koeru_core::db::RowTakes> for RowTakesView {
    fn from(r: koeru_core::db::RowTakes) -> Self {
        Self {
            row_id: r.row_id,
            text: r.text,
            state: r.state.as_str().to_owned(),
            takes: r
                .takes
                .into_iter()
                .map(|t| TakeSummaryView {
                    take_id: t.id,
                    generation: t.generation,
                    #[allow(
                        clippy::cast_precision_loss,
                        reason = "テイクの長さは表示用。桁は十分に収まる"
                    )]
                    duration_ms: t.frames as f64 * 1000.0
                        / f64::from(koeru_audio::wav::MASTER_RATE_HZ),
                    invalid: t.invalid,
                })
                .collect(),
            adopted: r.adopted,
        }
    }
}

/// 画面へ返すテイク。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct TakeView {
    /// 台帳の ID。
    pub take_id: i32,
    pub row_id: String,
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: f64,
    /// 絶対値の最大。`koeru_core::analysis::CLIP_THRESHOLD` 以上ならクリップ。
    #[specta(type = specta_typescript::Number)]
    pub peak: f32,
    /// 波形サムネイル（0〜255）。
    pub thumbnail: Vec<u8>,
    /// 原音設定が導けたか。
    pub has_oto: bool,
    /// 境界の確信度。
    pub confidence: Option<f64>,
    /// 取りこぼしの回数（`TR-REC-07`）。
    pub discontinuities: u32,
    /// 取りこぼしたので自動的に無効にした。 同じフレーズがもう一度出てくる。
    pub invalidated: bool,
    /// 押した瞬間より前から何ミリ秒ぶん遡れたか（`TR-REC-19`）。
    #[specta(type = specta_typescript::Number)]
    pub preroll_ms: f64,
    /// サンプルピーク（dBFS）。無音は `null`。
    pub peak_dbfs: Option<f64>,
    /// 発声の前に確保できた無音（ミリ秒、`TR-REC-38`）。
    #[specta(type = specta_typescript::Number)]
    pub leading_margin_ms: f64,
    /// 発声の後に確保できた無音（ミリ秒、`TR-REC-38`）。
    #[specta(type = specta_typescript::Number)]
    pub trailing_margin_ms: f64,
    /// 前後 300ms の無音マージンを確保できたか（`TR-REC-38`）。
    /// 足りなくてもテイクは有効。 事実を伝えるだけ。
    pub has_required_margins: bool,
}

impl From<TakeResult> for TakeView {
    fn from(t: TakeResult) -> Self {
        Self {
            take_id: t.take_id,
            row_id: t.row_id,
            duration_ms: t.duration_ms,
            peak: t.peak,
            thumbnail: t.thumbnail,
            has_oto: t.oto.is_some(),
            confidence: t.confidence,
            discontinuities: count(t.discontinuities),
            invalidated: t.invalidated,
            preroll_ms: t.preroll_ms,
            peak_dbfs: t
                .metrics
                .peak_dbfs
                .is_finite()
                .then_some(t.metrics.peak_dbfs),
            leading_margin_ms: t.metrics.leading_margin_ms,
            trailing_margin_ms: t.metrics.trailing_margin_ms,
            has_required_margins: t.metrics.has_required_margins(),
        }
    }
}

/// 入力デバイスを挙げる。
#[tauri::command(async)]
#[specta::specta]
pub fn list_devices() -> Result<Vec<DeviceView>> {
    Ok(Studio::devices()?
        .into_iter()
        .map(|d| DeviceView {
            id: d.id.as_str().to_owned(),
            name: d.name.expose().to_owned(),
        })
        .collect())
}

/// ライブラリの中身を挙げる。
#[tauri::command(async)]
#[specta::specta]
pub fn list_projects(state: State<'_, AppState>) -> Result<Vec<ProjectView>> {
    Ok(lock(&state)?
        .projects()?
        .into_iter()
        .map(|(id, m)| ProjectView {
            id: id.to_string(),
            display_name: m.as_ref().map(|m| m.display_name.clone()),
            method: m.as_ref().map(|m| m.method.as_str().to_owned()),
            item_count: m.as_ref().map(|m| m.item_count),
        })
        .collect())
}

/// プロジェクトを作る。
#[tauri::command(async)]
#[specta::specta]
pub fn create_project(state: State<'_, AppState>, display_name: String) -> Result<String> {
    Ok(lock(&state)?.create_project(&display_name)?.to_string())
}

/// プロジェクトを開く。
#[tauri::command(async)]
#[specta::specta]
pub fn open_project(state: State<'_, AppState>, id: String) -> Result<ProgressView> {
    let (view, pending) = {
        let mut s = lock(&state)?;
        let uuid = id
            .parse()
            .map_err(|_| AppError::new("app.bad_id", "その識別子は読めない"))?;
        s.open_project(uuid)?;
        (ProgressView::from(s.progress()?), s.pending_handle())
    };
    // 待ち数の持ち手を、状態ロックの外へ出しておく（`TR-SYN-33`）。
    if let Ok(mut g) = state.pending.lock() {
        *g = Some(pending);
    }
    Ok(view)
}

/// いまの進み具合。
#[tauri::command(async)]
#[specta::specta]
pub fn progress(state: State<'_, AppState>) -> Result<ProgressView> {
    Ok(lock(&state)?.progress()?.into())
}

/// デバイスを選び、ストリームを開く。
///
/// 返るのは、OS 側の音声加工が残っているかどうか（`TR-REC-11`）。
#[tauri::command(async)]
#[specta::specta]
pub fn arm_device(state: State<'_, AppState>, device_id: String) -> Result<MicModeView> {
    let (mode, handle) = {
        let mut s = lock(&state)?;
        let mode = s.arm_device(&koeru_audio::DeviceId::new(device_id))?;
        (mode, s.envelope_handle())
    };
    // 包絡の持ち手を、状態ロックの外へ出しておく（`TR-REC-43`）。
    if let Ok(mut g) = state.envelope.lock() {
        *g = handle;
    }
    Ok(MicModeView::parse(mode.as_str()))
}

/// 有限だと分かっている小数。
///
/// specta は素の `f64` を `number | null` に写す。 JSON に NaN も無限も
/// 無いからで、それ自体は正しい。 だが標本数と固定レート（44100）から作る
/// 時間・位置・比は、NaN も無限も作れない。 返り値の位置には属性を書けないので、
/// 「有限だと分かっている」ことを型で言うために1枚被せる。
///
/// 中身はそのまま数値として送る（`serde(transparent)`）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, specta::Type)]
#[serde(transparent)]
pub struct Finite(#[specta(type = specta_typescript::Number)] pub f64);

impl From<f32> for Finite {
    fn from(v: f32) -> Self {
        Self(f64::from(v))
    }
}

impl From<f64> for Finite {
    fn from(v: f64) -> Self {
        Self(v)
    }
}

/*
 * 小数の型について。
 *
 * specta は `f32` / `f64` を `number | null` に写す。 JSON に NaN も
 * 無限も無く、serde はどちらも `null` にするので、これは正しい。
 *
 * ただし KOERU の時間・位置・比は、標本数と固定レート（44100）から作る値で、
 * NaN も無限も作れない。 そこには `Number` を付けて `number` にする——
 * 起きない `null` を画面で 20 箇所ぶん扱わせない。
 *
 * 本当に起きるのは校正の dBFS（無音で -inf）と確信度（推定しなければ無し）で、
 * そちらは `Option` のまま残す。 -inf は Rust 側で `None` に畳んである
 * ——`Some(-inf)` は JSON で `null` になり、`None` と見分けが付かない。
 */

/// OS 側の音声加工の状態（`TR-REC-11`）。
///
/// バックエンドの `MicrophoneMode` を写さず、ここで定義する。
/// 書いていない OS では変種が減るので、写すと生成する TS が
/// ビルドした OS で変わってしまう。 画面の型が OS で変わってはいけない。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub enum MicModeView {
    /// 加工なし。これが望ましい。
    Standard,
    /// 音声分離が入っている。
    VoiceIsolation,
    /// 広帯域。
    WideSpectrum,
    /// 判定できなかった。加工が無いことの根拠にはしない。
    Unknown,
}

/// ゲインをどこで触れるか（`TR-REC-14`）。
///
/// これもバックエンドから写さない。理由は [`MicModeView`] と同じ。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub enum GainControlView {
    /// ハードウェア側のボリューム。校正に使える。
    Hardware,
    /// ソフトウェア実装。校正に使わない（`TR-REC-14`）。
    Software,
    /// 読み書きできない。
    Unavailable,
}

impl GainControlView {
    fn parse(s: &str) -> Self {
        match s {
            "hardware" => Self::Hardware,
            "software" => Self::Software,
            _ => Self::Unavailable,
        }
    }
}

/// 出力先の種別（`TR-REC-24`）。
///
/// ドライバの自己申告なので、安全側の根拠にしない。
/// 回り込みは録音側でしか確かめられない。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub enum OutputKindView {
    /// ヘッドホンと申告している。装着されている保証は無い。
    Headphones,
    /// スピーカと申告している。
    Speakers,
    /// 判定できなかった。
    Unknown,
}

impl OutputKindView {
    fn parse(s: &str) -> Self {
        match s {
            "headphones" => Self::Headphones,
            "speakers" => Self::Speakers,
            _ => Self::Unknown,
        }
    }
}

impl MicModeView {
    fn parse(s: &str) -> Self {
        match s {
            "Standard" => Self::Standard,
            "VoiceIsolation" => Self::VoiceIsolation,
            "WideSpectrum" => Self::WideSpectrum,
            _ => Self::Unknown,
        }
    }
}

/// 画面へ返す残量の見積もり（`TR-REC-41`）。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub struct SpaceView {
    /// まだ録っていない行の数。
    pub remaining_rows: u32,
    /// その残量で録りきれる件数。「足りません」だけでは判断できない。
    pub rows_that_fit: u32,
    /// 残り全部を録りきれるか。
    pub sufficient: bool,
    /// 残り全部に要るバイト数。
    /// 必要なバイト数。
    ///
    /// TS では `number` にする。 f64 が正確に持てる 2^53 バイトは 9 PB で、そこへ届く前に
    /// 保存先が尽きる。バイト数は `u32` では足りない（4 GB で溢れる）。
    #[specta(type = f64)]
    pub required_bytes: u64,
    /// 保存先の空き。引けなければ `null`。
    /// 保存先の残量（バイト）。取れなければ `None`。
    #[specta(type = Option<f64>)]
    pub available_bytes: Option<u64>,
}

impl From<SpaceEstimate> for SpaceView {
    fn from(e: SpaceEstimate) -> Self {
        Self {
            remaining_rows: count64(e.remaining_rows),
            rows_that_fit: count64(e.rows_that_fit),
            sufficient: e.is_sufficient(),
            required_bytes: e.required_bytes,
            available_bytes: e.available_bytes,
        }
    }
}

/// 画面へ返す校正の結果（`TR-REC-14`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct CalibrationView {
    /// 決めたゲイン（0.0〜1.0）。触れなければ `null`。
    pub gain: Option<f32>,
    /// `hardware` / `software` / `unavailable`。
    ///
    /// `hardware` 以外では自動調整しない（`TR-REC-14`）。
    /// 画面は OS 設定での調整を1回だけ案内する。
    pub control: GainControlView,
    /// 最後に測ったピーク（dBFS）。無音は `null`。
    pub peak_dbfs: Option<f64>,
    /// 目標範囲（-12〜-6 dBFS）に入ったか。入らなくても収録には進める。
    pub settled: bool,
}

/// 画面へ返す曲の状態（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SongView {
    /// バンクの中でこの曲を指す識別子。位置ではなくこれで指す。
    pub id: String,
    pub title: String,
    /// `Complete` / `WithFallback` / `Unavailable`（`TR-RCL-19`）。
    pub singability: String,
    /// 「歌える」に含めてよいか。
    pub singable: bool,
    /// 必要単位のうち収録済みの数。
    pub covered: u32,
    /// 必要単位の数。
    pub required: u32,
    /// あと何項目録れば完全になるか（`TR-SYN-20`）。
    ///
    /// エイリアス名の一覧は返さない。 出すのはこの数。
    pub missing_units: u32,
    /// あと何行録れば完全になるか（`TR-RCL-16`, `TR-RCL-17`）。
    pub missing_rows: u32,
    /// その行を録るのに掛かる推定時間（秒、`TR-RCL-09`）。
    #[specta(type = specta_typescript::Number)]
    pub seconds: f64,
    /// 総モーラ数。
    pub total_moras: u32,
}

/// 画面へ返す書き出し前の関門（`TR-REC-16`, `TR-REC-32`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct PreflightView {
    /// NFC へ直した名前の数。
    pub renamed_to_nfc: u32,
    /// それでも NFC でない名前。残っていたら書き出さない。
    pub non_nfc_names: Vec<String>,
    /// フルスケールに達している採用テイク（行 ID と回数）。
    ///
    /// 止めない。 本人が承知のうえで配ることはありうる。
    pub clipped_takes: Vec<(String, u32)>,
    /// 書き出してよいか。
    pub may_export: bool,
}

impl From<Preflight> for PreflightView {
    fn from(p: Preflight) -> Self {
        let may_export = p.may_export();
        Self {
            renamed_to_nfc: count(p.renamed_to_nfc),
            non_nfc_names: p.non_nfc_names,
            clipped_takes: p.clipped_takes,
            may_export,
        }
    }
}

/// 背後で待っている仕事の数（`TR-SYN-33`, `TR-SYN-34`）。
///
/// 「録音終了 → 試唱ボタン活性化」の間に、無言の待ち時間を作らない。
/// 画面はこれを見て、進んでいることを出す。
#[tauri::command]
#[specta::specta]
pub fn pending_work(state: State<'_, AppState>) -> Result<u32> {
    // `studio` のロックを取らない（`TR-SYN-33`）。
    // 待ち数がいちばん動くのはテイクの確定中で、そこが `studio` を握っている。
    let handle = state
        .pending
        .lock()
        .map_err(|_| AppError::new("app.poisoned", "内部状態が壊れている。開き直してほしい"))?
        .clone();
    Ok(count(handle.map_or(0, |q| crate::workers::pending_of(&q))))
}

/// 試唱の待ち時間の実測（`TR-SYN-33`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct LatencyView {
    /// `First` / `Warm` / `Incremental` / `Replay`。
    pub case: String,
    /// 測った回数。
    pub count: u32,
    /// 中央値（ミリ秒）。
    pub median_ms: Option<u32>,
    /// その場面の目標（ミリ秒）。
    pub budget_ms: u32,
    /// 収まっているか。回数が少ないうちは `null`。
    pub meets: Option<bool>,
}

/// 試唱の待ち時間を返す（`TR-SYN-33`）。
#[tauri::command(async)]
#[specta::specta]
pub fn latency_report(state: State<'_, AppState>) -> Result<Vec<LatencyView>> {
    Ok(lock(&state)?
        .latency_report()
        .into_iter()
        .map(|r| LatencyView {
            case: r.case.as_str().to_owned(),
            count: count(r.count),
            median_ms: r.median_ms,
            budget_ms: r.budget_ms,
            meets: r.meets,
        })
        .collect())
}

/// 見えている範囲の波形（`TR-PLT-04`）。
///
/// 上下の組を画素数ぶん返す。 読む量は画素数に比例し、範囲の広さには比例しない。
#[tauri::command(async)]
#[specta::specta]
pub fn waveform_window(
    state: State<'_, AppState>,
    take_id: i32,
    from_ms: Finite,
    to_ms: Finite,
    pixels: u32,
) -> Result<Vec<(Finite, Finite)>> {
    Ok(lock(&state)?
        .waveform_window(take_id, from_ms.0, to_ms.0, pixels as usize)?
        .into_iter()
        .map(|(lo, hi)| (lo.into(), hi.into()))
        .collect())
}

/// 見えている範囲のスペクトログラム（`TR-PLT-04`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SpectrogramView {
    /// 列を並べた強度（0〜255）。
    pub bins: Vec<u8>,
    /// 列の数（時間方向）。
    pub columns: u32,
    /// 1列あたりの高さ（周波数方向）。
    pub rows: u32,
}

/// 見えている範囲のスペクトログラムを作る（`TR-PLT-04`）。
///
/// 素材ファイル全体を先に計算しない。
#[tauri::command(async)]
#[specta::specta]
pub fn spectrogram_window(
    state: State<'_, AppState>,
    take_id: i32,
    from_ms: Finite,
    to_ms: Finite,
    columns: u32,
    rows: u32,
) -> Result<SpectrogramView> {
    let s = lock(&state)?.spectrogram_window(
        take_id,
        from_ms.0,
        to_ms.0,
        columns as usize,
        rows as usize,
    )?;
    Ok(SpectrogramView {
        bins: s.bins,
        columns: count(s.columns),
        rows: count(s.rows),
    })
}

/// 画面へ返す試唱の結果（`TR-SYN-18`）。
#[derive(Debug, Clone, Serialize, specta::Type)]
pub struct SungSongView {
    pub title: String,
    pub phrases: u32,
    /// 鳴らせないので落としたフレーズの数。
    ///
    /// 落とした位置には無音も別音も代替音も挿さない（`TR-SYN-18` (2)）。
    pub dropped_phrases: u32,
    /// 鳴らす長さ（ミリ秒）。
    #[specta(type = specta_typescript::Number)]
    pub duration_ms: f64,
}

/// 曲を歌わせる（`TR-SYN-01`〜`04`, `TR-SYN-18`）。
///
/// 先頭フレーズができた時点で鳴りはじめる。 曲全体の合成を待たない。
#[tauri::command(async)]
#[specta::specta]
pub fn sing_song(state: State<'_, AppState>, id: String) -> Result<SungSongView> {
    let s = lock(&state)?.sing_song(&id)?;
    Ok(SungSongView {
        title: s.title,
        phrases: count(s.phrases),
        dropped_phrases: count(s.dropped_phrases),
        duration_ms: s.duration_ms,
    })
}

/// 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
#[tauri::command(async)]
#[specta::specta]
pub fn preflight(state: State<'_, AppState>) -> Result<PreflightView> {
    Ok(lock(&state)?.preflight()?.into())
}

/// 全チャンネルを混ぜる（`TR-REC-06`）。
///
/// 全チャンネルに有意な信号があるときだけ選べる。
#[tauri::command(async)]
#[specta::specta]
pub fn use_mixed_channels(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.use_mixed_channels()
}

/// 曲ごとの状態を、手が届く順に返す（`TR-RCL-17`）。
#[tauri::command(async)]
#[specta::specta]
pub fn song_status(state: State<'_, AppState>) -> Result<Vec<SongView>> {
    Ok(lock(&state)?
        .song_status()?
        .into_iter()
        .map(|s| SongView {
            id: s.id,
            title: s.title,
            singability: s.singability.as_str().to_owned(),
            singable: s.singability.is_singable(),
            covered: count(s.covered),
            required: count(s.required),
            missing_units: count(s.missing_units),
            missing_rows: count(s.missing_rows),
            seconds: s.seconds,
            total_moras: count(s.total_moras),
        })
        .collect())
}

/// UST を取り込む（`TR-RCL-12`）。主経路はこれ。
#[tauri::command(async)]
#[specta::specta]
pub fn import_ust(state: State<'_, AppState>, bytes: Vec<u8>, title: String) -> Result<String> {
    lock(&state)?.import_ust(&bytes, &title)
}

/// 曲をバンクから外す／戻す（`TR-RCL-12`）。曲そのものは消さない。
#[tauri::command(async)]
#[specta::specta]
pub fn set_song_in_bank(state: State<'_, AppState>, id: String, in_bank: bool) -> Result<()> {
    lock(&state)?.set_song_in_bank(&id, in_bank)
}

/// 次のフレーズへ進むまでの長さ（ミリ秒、`TR-REC-20`）。
///
/// 単独音はガイドを使わないので固定長。 発話の検出結果を条件にしない。
#[tauri::command]
#[specta::specta]
pub const fn auto_advance_ms() -> Finite {
    Finite(koeru_core::guide::AUTO_ADVANCE_MS)
}

/// 出力がどこへ出ているらしいか（`TR-REC-24`）。
///
/// 一次の足切りでしかない。 ドライバの自己申告なので、
/// 実際の回り込みは [`check_guide_leak`] が録った音で確かめる。
#[tauri::command]
#[specta::specta]
pub fn output_kind() -> Result<OutputKindView> {
    Ok(OutputKindView::parse(Studio::output_kind().as_str()))
}

/// ガイドを鳴らしながら録って、回り込みを確かめる（`TR-REC-24`）。
///
/// これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。
#[tauri::command(async)]
#[specta::specta]
pub fn check_guide_leak(state: State<'_, AppState>, midi: i32) -> Result<LeakView> {
    let c = lock(&state)?.check_guide_leak(midi)?;
    Ok(LeakView {
        correlation: c.correlation,
        lag_ms: c.lag_ms,
        leaking: c.leaking,
    })
}

/// 音高を鳴らす（`TR-REC-23` の音高提示）。
///
/// 回り込みが確かめられていなければ鳴らさない。
#[tauri::command(async)]
#[specta::specta]
pub fn play_pitch(state: State<'_, AppState>, midi: i32) -> Result<()> {
    lock(&state)?.play_pitch(midi)
}

/// 画面へ返す回り込みの検査結果（`TR-REC-24`）。
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
pub struct LeakView {
    /// 見つかった相関（0.0〜1.0）。
    #[specta(type = specta_typescript::Number)]
    pub correlation: f64,
    /// そのときの遅れ（ミリ秒）。参考値。
    #[specta(type = specta_typescript::Number)]
    pub lag_ms: f64,
    /// 回り込んでいるとみなすか。
    pub leaking: bool,
}

/// 残量を見積もる（`TR-REC-41`）。
#[tauri::command(async)]
#[specta::specta]
pub fn estimate_space(state: State<'_, AppState>) -> Result<SpaceView> {
    Ok(lock(&state)?.estimate_space()?.into())
}

/// 入力レベルを校正する（`TR-REC-14`）。
///
/// 関門にしない。 収束しなくても収録に進める。
#[tauri::command(async)]
#[specta::specta]
pub fn calibrate(state: State<'_, AppState>, seconds: Finite) -> Result<CalibrationView> {
    let c = lock(&state)?.calibrate(seconds.0)?;
    Ok(CalibrationView {
        gain: c.gain,
        control: GainControlView::parse(&c.control),
        peak_dbfs: c.peak_dbfs.is_finite().then_some(c.peak_dbfs),
        settled: c.settled,
    })
}

/// 保存してある校正と、いまのゲインの差（`TR-REC-15`）。
///
/// 勝手に戻さない。 差があることを返すだけ。
#[tauri::command(async)]
#[specta::specta]
pub fn gain_drift(state: State<'_, AppState>) -> Result<Option<(Finite, Finite)>> {
    Ok(lock(&state)?
        .gain_drift()?
        .map(|(a, b)| (a.into(), b.into())))
}

/// 保存してあるゲインへ戻す（`TR-REC-15`）。本人が選んだときだけ呼ぶ。
#[tauri::command(async)]
#[specta::specta]
pub fn restore_saved_gain(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.restore_saved_gain()
}

/// 入力が届いているかを確かめる（`TR-REC-17`）。
///
/// 権限が無いと macOS は無音を返す。 成否ではなくピークを見る。
#[tauri::command(async)]
#[specta::specta]
pub fn probe_input(state: State<'_, AppState>, ms: u32) -> Result<Finite> {
    lock(&state)?.probe_input(u64::from(ms)).map(Finite::from)
}

/// いま録るべき行の収録を始める。
#[tauri::command(async)]
#[specta::specta]
pub fn start_take(state: State<'_, AppState>) -> Result<String> {
    lock(&state)?.start_take()
}

/// 行を指定して録り直す（`TR-REC-21`, `TR-RCL-25`, `TR-ALN-27`）。
///
/// 既存のテイクを消さない。 世代を1つ足して積み、採用を新しい方へ切り替える。
#[tauri::command(async)]
#[specta::specta]
pub fn start_retake(state: State<'_, AppState>, row_id: String) -> Result<String> {
    lock(&state)?.start_take_for(&row_id)
}

/// 全部の行と、そのテイク（`TR-REC-21`, `TR-RCL-25`）。
#[tauri::command(async)]
#[specta::specta]
pub fn rows_with_takes(state: State<'_, AppState>) -> Result<Vec<RowTakesView>> {
    Ok(lock(&state)?
        .rows_with_takes()?
        .into_iter()
        .map(Into::into)
        .collect())
}

/// 採用テイクを切り替える（`TR-RCL-25`）。
///
/// カバレッジは変わらない。 変わるのは原音設定の値だけ。
#[tauri::command(async)]
#[specta::specta]
pub fn adopt_take(state: State<'_, AppState>, row_id: String, take_id: i32) -> Result<()> {
    lock(&state)?.adopt_take(&row_id, take_id)
}

/// そのテイクの原音設定を、エイリアスごとに引く（`TR-ALN-33`）。
#[tauri::command(async)]
#[specta::specta]
pub fn otos_of_take(state: State<'_, AppState>, take_id: i32) -> Result<Vec<OtoView>> {
    Ok(lock(&state)?
        .otos_of_take(take_id)?
        .into_iter()
        .map(|(alias, o)| OtoView {
            alias,
            offset_ms: o.offset_ms,
            consonant_ms: o.consonant_ms,
            cutoff_ms: o.cutoff_ms,
            preutterance_ms: o.preutterance_ms,
            overlap_ms: o.overlap_ms,
        })
        .collect())
}

/// いま入ってきている音の包絡を送り続ける（`TR-REC-43`）。
///
/// `invoke` で引かせず Channel で送る理由は `DEC-PLT-017`。
///
/// この関数が固有に守っているのは2つ。`studio` のロックを取らないので、
/// テイクの確定（アライメントを含む）のあいだも波形が止まらない。
/// 進んでいないフレームは送らないので、同じ絵を描き直させない。
#[tauri::command]
#[specta::specta]
pub fn stream_envelope(state: State<'_, AppState>, on_frame: Channel<EnvelopeView>) -> u32 {
    // 世代を1つ進める。 前の流れは次の目覚めで自分から止まる。
    let generation = state.stream.fetch_add(1, Ordering::SeqCst) + 1;
    let envelope = Arc::clone(&state.envelope);
    let stream = Arc::clone(&state.stream);

    std::thread::spawn(move || {
        let mut last = 0_u64;
        while stream.load(Ordering::SeqCst) == generation {
            let handle = envelope.lock().ok().and_then(|g| g.clone());
            if let Some(e) = handle {
                let (steps, position) = e.lock().map_or_else(|_| (Vec::new(), 0), |g| g.sample());
                // 「進んだか」ではなく「変わったか」で見る。
                // マイクを選び直すと `Pump` が作り直され、通算は 0 へ戻る。
                // 進んだかだけで見ていると、そこから二度と送らなくなる。
                if position != last {
                    last = position;
                    let steps = steps
                        .into_iter()
                        .map(|(lo, hi)| (lo.into(), hi.into()))
                        .collect();
                    if on_frame.send(EnvelopeView { steps, position }).is_err() {
                        // 画面が居なくなった。騒がずに畳む。
                        break;
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(ENVELOPE_FRAME_MS));
        }
    });
    generation
}

/// 波形を送るのをやめる（`TR-REC-43`）。
///
/// 止める相手を名指しする。 ただ世代を進めるだけにすると、
/// 画面が作り直されたときに新しい流れを殺してしまう——
/// 「古いのを止める」と「新しいのを始める」は非同期に飛ぶので、
/// 後者が先に着くことがある。そのときに数を進めると、生きているほうが止まる。
#[tauri::command]
#[specta::specta]
pub fn stop_envelope_stream(state: State<'_, AppState>, generation: u32) {
    let _ = state.stream.compare_exchange(
        generation,
        generation + 1,
        Ordering::SeqCst,
        Ordering::SeqCst,
    );
}

/// 録れたものをそのまま鳴らす（`TR-REC-43`）。合成を通さない。
///
/// 返すのは長さ（ミリ秒）。
#[tauri::command(async)]
#[specta::specta]
pub fn play_take(state: State<'_, AppState>, take_id: i32) -> Result<Finite> {
    lock(&state)?.play_take(take_id).map(Finite::from)
}

/// 収録を止めて、テイクを確定させる。
#[tauri::command(async)]
#[specta::specta]
pub fn finish_take(state: State<'_, AppState>) -> Result<TakeView> {
    Ok(lock(&state)?.finish_take()?.into())
}

/// 収録済みのテイクを、指定した音高で鳴らす。
#[tauri::command(async)]
#[specta::specta]
pub fn preview(
    state: State<'_, AppState>,
    take_id: i32,
    midi: i32,
    length_ms: Finite,
) -> Result<u32> {
    Ok(count(lock(&state)?.preview(take_id, midi, length_ms.0)?))
}

/// プリロールがどれだけ溜まっているか（ミリ秒、`TR-REC-19`）。
#[tauri::command]
#[specta::specta]
pub fn preroll_ms(state: State<'_, AppState>) -> Result<u32> {
    Ok(count64(lock(&state)?.preroll_ms()))
}

/// 鳴らしている音を止める。
#[tauri::command(async)]
#[specta::specta]
pub fn stop_preview(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.stop_preview();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 溢れたら上限に張り付く。0 に巻き戻らない。
    ///
    /// `as u32` で書くと、`u32::MAX + 1` 件が 0 件になる。 画面には
    /// 「あと0件」と出て、全部録れたのと見分けが付かなくなる。
    #[test]
    fn 件数は溢れても巻き戻らない() {
        assert_eq!(count(0), 0);
        assert_eq!(count(1234), 1234);
        assert_eq!(count(u32::MAX as usize), u32::MAX);
        assert_eq!(count(u32::MAX as usize + 1), u32::MAX);
        assert_eq!(count(usize::MAX), u32::MAX);

        assert_eq!(count64(0), 0);
        assert_eq!(count64(u64::from(u32::MAX) + 1), u32::MAX);
        assert_eq!(count64(u64::MAX), u32::MAX);
    }

    /// 境界の enum は、知らない綴りを取りこぼさず既定へ落とす。
    ///
    /// 落とすこと自体は正しい。 落ちたことに気づけるかは
    /// `tests/offline.rs` の `境界のenumがバックエンドの綴りを網羅している` が見る。
    #[test]
    fn 境界のenumが綴りを写す() {
        assert!(matches!(
            MicModeView::parse("VoiceIsolation"),
            MicModeView::VoiceIsolation
        ));
        assert!(matches!(
            MicModeView::parse("Standard"),
            MicModeView::Standard
        ));
        assert!(matches!(
            MicModeView::parse("なにか別のもの"),
            MicModeView::Unknown
        ));

        assert!(matches!(
            GainControlView::parse("software"),
            GainControlView::Software
        ));
        assert!(matches!(
            GainControlView::parse("なにか別のもの"),
            GainControlView::Unavailable
        ));

        assert!(matches!(
            OutputKindView::parse("headphones"),
            OutputKindView::Headphones
        ));
        assert!(matches!(
            OutputKindView::parse("なにか別のもの"),
            OutputKindView::Unknown
        ));
    }
}
