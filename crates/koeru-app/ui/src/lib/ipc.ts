import { invoke } from "@tauri-apps/api/core";

/**
 * Rust 側の失敗。
 *
 * `kind` は送信層へ載せてよい固定文字列、`message` は画面に出す日本語。
 * **`message` を外へ送らない**（パスや音源名が入りうる）。
 */
export type AppError = {
  kind: string;
  message: string;
};

/** Rust 側の失敗かどうか。 */
export const isAppError = (e: unknown): e is AppError =>
  typeof e === "object" && e !== null && "kind" in e && "message" in e;

/** 画面に出す文言へ直す。**素の例外をそのまま出さない。** */
export const errorMessage = (e: unknown): string => {
  if (isAppError(e)) return e.message;
  if (e instanceof Error) return e.message;
  return "予期しない失敗が起きた";
};

export type DeviceView = { id: string; name: string };

export type ProjectView = {
  id: string;
  display_name: string | null;
  method: string | null;
  item_count: number | null;
};

export type ProgressView = {
  next_row_id: string | null;
  next_row_text: string | null;
  covered: number;
  required: number;
  coverage: "Incomplete" | "AwaitingOto" | "Complete";
  handoff: "NotExported" | "Exported";
  /** いま歌える曲の数（TR-RCL-19）。カバレッジと常に両方出す。 */
  singable_songs: number;
  /** バンクに入っている曲の数。0 でも成立する。 */
  songs_in_bank: number;
};

/** 曲の状態（TR-RCL-17、TR-RCL-19、TR-SYN-20）。 */
export type SongView = {
  title: string;
  singability: "Complete" | "WithFallback" | "Unavailable";
  singable: boolean;
  covered: number;
  required: number;
  /** あと何項目録れば完全になるか。**エイリアス名の一覧は返らない。** */
  missing_units: number;
  /** あと何行録れば完全になるか（TR-RCL-16、TR-RCL-17）。 */
  missing_rows: number;
  /** その行を録るのに掛かる推定時間（秒）。 */
  seconds: number;
  total_moras: number;
};

export type TakeView = {
  take_id: number;
  row_id: string;
  duration_ms: number;
  peak: number;
  thumbnail: number[];
  has_oto: boolean;
  confidence: number | null;
  discontinuities: number;
  /** 取りこぼしたので自動的に無効にした（TR-REC-07）。同じフレーズがもう一度出てくる。 */
  invalidated: boolean;
  /** 押した瞬間より前から何ミリ秒ぶん遡れたか（TR-REC-19）。 */
  preroll_ms: number;
  /** サンプルピーク（dBFS）。無音は null。 */
  peak_dbfs: number | null;
  leading_margin_ms: number;
  trailing_margin_ms: number;
  /** 前後 300ms の無音マージンを確保できたか（TR-REC-38）。 */
  has_required_margins: boolean;
};

/** 残量の見積もり（TR-REC-41）。 */
export type SpaceView = {
  remaining_rows: number;
  /** その残量で録りきれる件数。「足りません」だけでは判断できない。 */
  rows_that_fit: number;
  sufficient: boolean;
  required_bytes: number;
  available_bytes: number | null;
};

/** 校正の結果（TR-REC-14）。 */
export type CalibrationView = {
  gain: number | null;
  /**
   * hardware / software / unavailable。
   * hardware 以外では自動調整しない。OS 設定での調整を1回だけ案内する。
   */
  control: "hardware" | "software" | "unavailable";
  peak_dbfs: number | null;
  /** 目標範囲（-12〜-6 dBFS）に入ったか。入らなくても収録には進める。 */
  settled: boolean;
};

/** 回り込みの検査結果（TR-REC-24）。 */
export type LeakView = {
  correlation: number;
  /** 参考値。判定に使うのは相関の大きさ。 */
  lag_ms: number;
  leaking: boolean;
};

/** 試唱の結果（TR-SYN-18）。 */
export type SungSongView = {
  title: string;
  phrases: number;
  /** 鳴らせないので落としたフレーズの数。落とした位置には何も挿さない。 */
  dropped_phrases: number;
  duration_ms: number;
};

export const api = {
  listDevices: () => invoke<DeviceView[]>("list_devices"),
  listProjects: () => invoke<ProjectView[]>("list_projects"),
  createProject: (displayName: string) => invoke<string>("create_project", { displayName }),
  openProject: (id: string) => invoke<ProgressView>("open_project", { id }),
  progress: () => invoke<ProgressView>("progress"),
  armDevice: (deviceId: string) => invoke<string>("arm_device", { deviceId }),
  probeInput: (ms: number) => invoke<number>("probe_input", { ms }),
  startTake: () => invoke<string>("start_take"),
  finishTake: () => invoke<TakeView>("finish_take"),
  preview: (takeId: number, midi: number, lengthMs: number) =>
    invoke<number>("preview", { takeId, midi, lengthMs }),
  stopPreview: () => invoke<void>("stop_preview"),
  prerollMs: () => invoke<number>("preroll_ms"),
  estimateSpace: () => invoke<SpaceView>("estimate_space"),
  calibrate: (seconds: number) => invoke<CalibrationView>("calibrate", { seconds }),
  gainDrift: () => invoke<[number, number] | null>("gain_drift"),
  restoreSavedGain: () => invoke<void>("restore_saved_gain"),
  autoAdvanceMs: () => invoke<number>("auto_advance_ms"),
  outputKind: () => invoke<"headphones" | "speakers" | "unknown">("output_kind"),
  checkGuideLeak: (midi: number) => invoke<LeakView>("check_guide_leak", { midi }),
  playPitch: (midi: number) => invoke<void>("play_pitch", { midi }),
  songStatus: () => invoke<SongView[]>("song_status"),
  singSong: (index: number) => invoke<SungSongView>("sing_song", { index }),
  pendingWork: () => invoke<number>("pending_work"),
  waveformWindow: (takeId: number, fromMs: number, toMs: number, pixels: number) =>
    invoke<[number, number][]>("waveform_window", { takeId, fromMs, toMs, pixels }),
  spectrogramWindow: (
    takeId: number,
    fromMs: number,
    toMs: number,
    columns: number,
    rows: number,
  ) =>
    invoke<{ bins: number[]; columns: number; rows: number }>("spectrogram_window", {
      takeId,
      fromMs,
      toMs,
      columns,
      rows,
    }),
  importUst: (bytes: number[], title: string) => invoke<string>("import_ust", { bytes, title }),
  setSongInBank: (id: string, inBank: boolean) => invoke<void>("set_song_in_bank", { id, inBank }),
};
