//! Tauri のコマンド。**判断はここに置かない。**
//!
//! [`crate::studio::Studio`] を呼んで、結果を画面へ渡す形へ直すだけ。
//! **筋を足すときは `studio` 側に足す**（GUI 無しで検査できる場所に置く）。

use std::sync::Mutex;

use serde::Serialize;
use tauri::State;

use crate::error::{AppError, Result};
use crate::studio::{Preflight, Progress, SpaceEstimate, Studio, TakeResult};

/// アプリが持つ状態。**1本の `Mutex` で囲む。**
///
/// 音声のコールバックはこの `Mutex` を取らない（`Capture` の中はアトミック）。
/// ここが守るのは画面から来る操作の直列化だけ。
#[derive(Debug)]
pub struct AppState {
    studio: Mutex<Studio>,
}

impl AppState {
    /// 状態を作る。
    #[must_use]
    pub const fn new(studio: Studio) -> Self {
        Self {
            studio: Mutex::new(studio),
        }
    }
}

/// `Mutex` が毒されたときの失敗。
///
/// **毒されたら握り潰さない。** どこかのコマンドが panic した証拠で、
/// そのまま続けると壊れた状態の上で操作を重ねる。
fn lock(state: &AppState) -> Result<std::sync::MutexGuard<'_, Studio>> {
    state
        .studio
        .lock()
        .map_err(|_| AppError::new("app.poisoned", "内部状態が壊れている。開き直してほしい"))
}

/// 画面へ返すデバイス。
#[derive(Debug, Clone, Serialize)]
pub struct DeviceView {
    /// 永続識別子（`TR-REC-03`）。**同一性の判定はこれで行う。**
    pub id: String,
    /// 表示名。**一覧に出すためだけ。**
    pub name: String,
}

/// 画面へ返すプロジェクト。
#[derive(Debug, Clone, Serialize)]
pub struct ProjectView {
    /// 不変の識別子。
    pub id: String,
    /// 表示名。**manifest が読めなければ `None`。**
    pub display_name: Option<String>,
    /// 方式。
    pub method: Option<String>,
    /// 項目数。
    pub item_count: Option<u32>,
}

/// 画面へ返す進み具合。
#[derive(Debug, Clone, Serialize)]
pub struct ProgressView {
    /// 次に録る行の ID。
    pub next_row_id: Option<String>,
    /// 次に録る行の読み上げ文字列。
    pub next_row_text: Option<String>,
    /// 収録済み単位。
    pub covered: usize,
    /// 必要な単位。
    pub required: usize,
    /// 完成状態。
    pub coverage: String,
    /// 手渡し状態。**完成判定はこれを見ない**（`TR-PKG-33`）。
    pub handoff: String,
    /// **いま歌える曲の数**（`TR-RCL-19`）。カバレッジと常に両方出す。
    pub singable_songs: usize,
    /// バンクに入っている曲の数。**0 でも成立する。**
    pub songs_in_bank: usize,
}

impl From<Progress> for ProgressView {
    fn from(p: Progress) -> Self {
        let (id, text) = p.next_row.map_or((None, None), |(i, t)| (Some(i), Some(t)));
        Self {
            next_row_id: id,
            next_row_text: text,
            covered: p.covered,
            required: p.required,
            coverage: format!("{:?}", p.coverage),
            handoff: format!("{:?}", p.handoff),
            singable_songs: p.singable_songs,
            songs_in_bank: p.songs_in_bank,
        }
    }
}

/// 画面へ返すテイク。
#[derive(Debug, Clone, Serialize)]
pub struct TakeView {
    /// 台帳の ID。
    pub take_id: i32,
    /// どの行か。
    pub row_id: String,
    /// 長さ（ミリ秒）。
    pub duration_ms: f64,
    /// 絶対値の最大。**1.0 に達していたらクリップ。**
    pub peak: f32,
    /// 波形サムネイル（0〜255）。
    pub thumbnail: Vec<u8>,
    /// 原音設定が導けたか。
    pub has_oto: bool,
    /// 境界の確信度。
    pub confidence: Option<f64>,
    /// 取りこぼしの回数（`TR-REC-07`）。
    pub discontinuities: usize,
    /// **取りこぼしたので自動的に無効にした。** 同じフレーズがもう一度出てくる。
    pub invalidated: bool,
    /// 押した瞬間より前から何ミリ秒ぶん遡れたか（`TR-REC-19`）。
    pub preroll_ms: f64,
    /// サンプルピーク（dBFS）。無音は `null`。
    pub peak_dbfs: Option<f64>,
    /// 発声の前に確保できた無音（ミリ秒、`TR-REC-38`）。
    pub leading_margin_ms: f64,
    /// 発声の後に確保できた無音（ミリ秒、`TR-REC-38`）。
    pub trailing_margin_ms: f64,
    /// 前後 300ms の無音マージンを確保できたか（`TR-REC-38`）。
    /// **足りなくてもテイクは有効。** 事実を伝えるだけ。
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
            discontinuities: t.discontinuities,
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
#[tauri::command]
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
#[tauri::command]
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
#[tauri::command]
pub fn create_project(state: State<'_, AppState>, display_name: String) -> Result<String> {
    Ok(lock(&state)?.create_project(&display_name)?.to_string())
}

/// プロジェクトを開く。
#[tauri::command]
pub fn open_project(state: State<'_, AppState>, id: String) -> Result<ProgressView> {
    let mut s = lock(&state)?;
    let uuid = id
        .parse()
        .map_err(|_| AppError::new("app.bad_id", "その識別子は読めない"))?;
    s.open_project(uuid)?;
    Ok(s.progress()?.into())
}

/// いまの進み具合。
#[tauri::command]
pub fn progress(state: State<'_, AppState>) -> Result<ProgressView> {
    Ok(lock(&state)?.progress()?.into())
}

/// デバイスを選び、ストリームを開く。
///
/// 返るのは、**OS 側の音声加工が残っているかどうか**（`TR-REC-11`）。
#[tauri::command]
pub fn arm_device(state: State<'_, AppState>, device_id: String) -> Result<String> {
    let mode = lock(&state)?.arm_device(&koeru_audio::DeviceId::new(device_id))?;
    Ok(format!("{mode:?}"))
}

/// 画面へ返す残量の見積もり（`TR-REC-41`）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct SpaceView {
    /// まだ録っていない行の数。
    pub remaining_rows: u64,
    /// **その残量で録りきれる件数。**「足りません」だけでは判断できない。
    pub rows_that_fit: u64,
    /// 残り全部を録りきれるか。
    pub sufficient: bool,
    /// 残り全部に要るバイト数。
    pub required_bytes: u64,
    /// 保存先の空き。**引けなければ `null`。**
    pub available_bytes: Option<u64>,
}

impl From<SpaceEstimate> for SpaceView {
    fn from(e: SpaceEstimate) -> Self {
        Self {
            remaining_rows: e.remaining_rows,
            rows_that_fit: e.rows_that_fit,
            sufficient: e.is_sufficient(),
            required_bytes: e.required_bytes,
            available_bytes: e.available_bytes,
        }
    }
}

/// 画面へ返す校正の結果（`TR-REC-14`）。
#[derive(Debug, Clone, Serialize)]
pub struct CalibrationView {
    /// 決めたゲイン（0.0〜1.0）。触れなければ `null`。
    pub gain: Option<f32>,
    /// `hardware` / `software` / `unavailable`。
    ///
    /// **`hardware` 以外では自動調整しない**（`TR-REC-14`）。
    /// 画面は OS 設定での調整を1回だけ案内する。
    pub control: String,
    /// 最後に測ったピーク（dBFS）。無音は `null`。
    pub peak_dbfs: Option<f64>,
    /// 目標範囲（-12〜-6 dBFS）に入ったか。**入らなくても収録には進める。**
    pub settled: bool,
}

/// 画面へ返す曲の状態（`TR-RCL-17`, `TR-RCL-19`, `TR-SYN-20`）。
#[derive(Debug, Clone, Serialize)]
pub struct SongView {
    /// 曲名。
    pub title: String,
    /// `Complete` / `WithFallback` / `Unavailable`（`TR-RCL-19`）。
    pub singability: String,
    /// 「歌える」に含めてよいか。
    pub singable: bool,
    /// 必要単位のうち収録済みの数。
    pub covered: usize,
    /// 必要単位の数。
    pub required: usize,
    /// **あと何項目録れば完全になるか**（`TR-SYN-20`）。
    ///
    /// **エイリアス名の一覧は返さない。** 出すのはこの数。
    pub missing_units: usize,
    /// 総モーラ数。
    pub total_moras: usize,
}

/// 画面へ返す書き出し前の関門（`TR-REC-16`, `TR-REC-32`）。
#[derive(Debug, Clone, Serialize)]
pub struct PreflightView {
    /// NFC へ直した名前の数。
    pub renamed_to_nfc: usize,
    /// それでも NFC でない名前。**残っていたら書き出さない。**
    pub non_nfc_names: Vec<String>,
    /// フルスケールに達している採用テイク（行 ID と回数）。
    ///
    /// **止めない。** 本人が承知のうえで配ることはありうる。
    pub clipped_takes: Vec<(String, u32)>,
    /// 書き出してよいか。
    pub may_export: bool,
}

impl From<Preflight> for PreflightView {
    fn from(p: Preflight) -> Self {
        let may_export = p.may_export();
        Self {
            renamed_to_nfc: p.renamed_to_nfc,
            non_nfc_names: p.non_nfc_names,
            clipped_takes: p.clipped_takes,
            may_export,
        }
    }
}

/// 見えている範囲の波形（`TR-PLT-04`）。
///
/// **上下の組を画素数ぶん返す。** 読む量は画素数に比例し、範囲の広さには比例しない。
#[tauri::command]
pub fn waveform_window(
    state: State<'_, AppState>,
    take_id: i32,
    from_ms: f64,
    to_ms: f64,
    pixels: usize,
) -> Result<Vec<(f32, f32)>> {
    lock(&state)?.waveform_window(take_id, from_ms, to_ms, pixels)
}

/// 見えている範囲のスペクトログラム（`TR-PLT-04`）。
#[derive(Debug, Clone, Serialize)]
pub struct SpectrogramView {
    /// 列を並べた強度（0〜255）。
    pub bins: Vec<u8>,
    /// 列の数（時間方向）。
    pub columns: usize,
    /// 1列あたりの高さ（周波数方向）。
    pub rows: usize,
}

/// 見えている範囲のスペクトログラムを作る（`TR-PLT-04`）。
///
/// **素材ファイル全体を先に計算しない。**
#[tauri::command]
pub fn spectrogram_window(
    state: State<'_, AppState>,
    take_id: i32,
    from_ms: f64,
    to_ms: f64,
    columns: usize,
    rows: usize,
) -> Result<SpectrogramView> {
    let s = lock(&state)?.spectrogram_window(take_id, from_ms, to_ms, columns, rows)?;
    Ok(SpectrogramView {
        bins: s.bins,
        columns: s.columns,
        rows: s.rows,
    })
}

/// 画面へ返す試唱の結果（`TR-SYN-18`）。
#[derive(Debug, Clone, Serialize)]
pub struct SungSongView {
    /// 曲名。
    pub title: String,
    /// 鳴らすフレーズの数。
    pub phrases: usize,
    /// **鳴らせないので落としたフレーズの数。**
    ///
    /// 落とした位置には無音も別音も代替音も挿さない（`TR-SYN-18` (2)）。
    pub dropped_phrases: usize,
    /// 鳴らす長さ（ミリ秒）。
    pub duration_ms: f64,
}

/// 曲を歌わせる（`TR-SYN-01`〜`04`, `TR-SYN-18`）。
///
/// **先頭フレーズができた時点で鳴りはじめる。** 曲全体の合成を待たない。
#[tauri::command]
pub fn sing_song(state: State<'_, AppState>, index: usize) -> Result<SungSongView> {
    let s = lock(&state)?.sing_song(index)?;
    Ok(SungSongView {
        title: s.title,
        phrases: s.phrases,
        dropped_phrases: s.dropped_phrases,
        duration_ms: s.duration_ms,
    })
}

/// 書き出す前の関門（`TR-REC-16`, `TR-REC-32`）。
#[tauri::command]
pub fn preflight(state: State<'_, AppState>) -> Result<PreflightView> {
    Ok(lock(&state)?.preflight()?.into())
}

/// 全チャンネルを混ぜる（`TR-REC-06`）。
///
/// **全チャンネルに有意な信号があるときだけ選べる。**
#[tauri::command]
pub fn use_mixed_channels(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.use_mixed_channels()
}

/// 曲ごとの状態を、手が届く順に返す（`TR-RCL-17`）。
#[tauri::command]
pub fn song_status(state: State<'_, AppState>) -> Result<Vec<SongView>> {
    Ok(lock(&state)?
        .song_status()?
        .into_iter()
        .map(|s| SongView {
            title: s.title,
            singability: format!("{:?}", s.singability),
            singable: s.singability.is_singable(),
            covered: s.covered,
            required: s.required,
            missing_units: s.missing_units,
            total_moras: s.total_moras,
        })
        .collect())
}

/// UST を取り込む（`TR-RCL-12`）。**主経路はこれ。**
#[tauri::command]
pub fn import_ust(state: State<'_, AppState>, bytes: Vec<u8>, title: String) -> Result<String> {
    lock(&state)?.import_ust(&bytes, &title)
}

/// 曲をバンクから外す／戻す（`TR-RCL-12`）。**曲そのものは消さない。**
#[tauri::command]
pub fn set_song_in_bank(state: State<'_, AppState>, id: String, in_bank: bool) -> Result<()> {
    lock(&state)?.set_song_in_bank(&id, in_bank)
}

/// 次のフレーズへ進むまでの長さ（ミリ秒、`TR-REC-20`）。
///
/// **単独音はガイドを使わないので固定長。** 発話の検出結果を条件にしない。
#[tauri::command]
pub const fn auto_advance_ms() -> f64 {
    koeru_core::guide::AUTO_ADVANCE_MS
}

/// 出力がどこへ出ているらしいか（`TR-REC-24`）。
///
/// **一次の足切りでしかない。** ドライバの自己申告なので、
/// 実際の回り込みは [`check_guide_leak`] が録った音で確かめる。
#[tauri::command]
pub fn output_kind() -> Result<String> {
    Ok(Studio::output_kind().as_str().to_owned())
}

/// ガイドを鳴らしながら録って、回り込みを確かめる（`TR-REC-24`）。
///
/// **これを置かないと、全テイクにガイドが混入した音源が完成に到達しうる。**
#[tauri::command]
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
/// **回り込みが確かめられていなければ鳴らさない。**
#[tauri::command]
pub fn play_pitch(state: State<'_, AppState>, midi: i32) -> Result<()> {
    lock(&state)?.play_pitch(midi)
}

/// 画面へ返す回り込みの検査結果（`TR-REC-24`）。
#[derive(Debug, Clone, Copy, Serialize)]
pub struct LeakView {
    /// 見つかった相関（0.0〜1.0）。
    pub correlation: f64,
    /// そのときの遅れ（ミリ秒）。**参考値。**
    pub lag_ms: f64,
    /// 回り込んでいるとみなすか。
    pub leaking: bool,
}

/// 残量を見積もる（`TR-REC-41`）。
#[tauri::command]
pub fn estimate_space(state: State<'_, AppState>) -> Result<SpaceView> {
    Ok(lock(&state)?.estimate_space()?.into())
}

/// 入力レベルを校正する（`TR-REC-14`）。
///
/// **関門にしない。** 収束しなくても収録に進める。
#[tauri::command]
pub fn calibrate(state: State<'_, AppState>, seconds: f64) -> Result<CalibrationView> {
    let c = lock(&state)?.calibrate(seconds)?;
    Ok(CalibrationView {
        gain: c.gain,
        control: c.control,
        peak_dbfs: c.peak_dbfs.is_finite().then_some(c.peak_dbfs),
        settled: c.settled,
    })
}

/// 保存してある校正と、いまのゲインの差（`TR-REC-15`）。
///
/// **勝手に戻さない。** 差があることを返すだけ。
#[tauri::command]
pub fn gain_drift(state: State<'_, AppState>) -> Result<Option<(f32, f32)>> {
    lock(&state)?.gain_drift()
}

/// 保存してあるゲインへ戻す（`TR-REC-15`）。**本人が選んだときだけ呼ぶ。**
#[tauri::command]
pub fn restore_saved_gain(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.restore_saved_gain()
}

/// 入力が届いているかを確かめる（`TR-REC-17`）。
///
/// **権限が無いと macOS は無音を返す。** 成否ではなくピークを見る。
#[tauri::command]
pub fn probe_input(state: State<'_, AppState>, ms: u64) -> Result<f32> {
    lock(&state)?.probe_input(ms)
}

/// いま録るべき行の収録を始める。
#[tauri::command]
pub fn start_take(state: State<'_, AppState>) -> Result<String> {
    lock(&state)?.start_take()
}

/// 収録を止めて、テイクを確定させる。
#[tauri::command]
pub fn finish_take(state: State<'_, AppState>) -> Result<TakeView> {
    Ok(lock(&state)?.finish_take()?.into())
}

/// 収録済みのテイクを、指定した音高で鳴らす。
#[tauri::command]
pub fn preview(
    state: State<'_, AppState>,
    take_id: i32,
    midi: i32,
    length_ms: f64,
) -> Result<usize> {
    lock(&state)?.preview(take_id, midi, length_ms)
}

/// プリロールがどれだけ溜まっているか（ミリ秒、`TR-REC-19`）。
#[tauri::command]
pub fn preroll_ms(state: State<'_, AppState>) -> Result<u64> {
    Ok(lock(&state)?.preroll_ms())
}

/// 鳴らしている音を止める。
#[tauri::command]
pub fn stop_preview(state: State<'_, AppState>) -> Result<()> {
    lock(&state)?.stop_preview();
    Ok(())
}
