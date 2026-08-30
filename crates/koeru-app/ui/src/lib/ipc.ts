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
};
