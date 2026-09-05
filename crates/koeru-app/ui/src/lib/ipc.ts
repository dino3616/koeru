/*
 * Rust との境界。
 *
 * 型と呼び出し口は `bindings.gen.ts` が正本で、Rust のコマンド定義から
 * 生成している（`DEC-PLT-019`）。ここはその上に薄く被せる層で、
 * 持っているのは3つだけ。
 *
 * 1. 生成物の `{ status: "ok" | "error" }` を、投げる形へ剥がす
 * 2. 位置引数で取り違えやすいものを、オブジェクト引数に直す
 * 3. Rust の識別子を、画面に出す日本語へ直す
 *
 * 型を手で書かない。 書くと Rust 側と二重定義になり、片方だけが古くなる。
 */
import { Channel } from "@tauri-apps/api/core";

import { commands } from "~/lib/bindings.gen";

export { Channel };
export type {
  AppError,
  CalibrationView,
  DeviceView,
  EnvelopeView,
  GainControlView,
  LatencyView,
  LeakView,
  MicModeView,
  OtoView,
  OutputKindView,
  PreflightView,
  ProgressView,
  ProjectView,
  RowTakesView,
  SongView,
  SpaceView,
  SpectrogramView,
  SungSongView,
  TakeSummaryView,
  TakeView,
} from "~/lib/bindings.gen";

import type { AppError, EnvelopeView, MicModeView } from "~/lib/bindings.gen";

/** Rust 側の失敗かどうか。 */
export const isAppError = (e: unknown): e is AppError =>
  typeof e === "object" &&
  e !== null &&
  "kind" in e &&
  typeof (e as { kind: unknown }).kind === "string" &&
  "message" in e &&
  typeof (e as { message: unknown }).message === "string";

/** 画面に出す文言へ直す。素の例外をそのまま出さない。 */
export const errorMessage = (e: unknown): string => {
  if (isAppError(e)) return e.message;
  if (e instanceof Error) return e.message;
  // Tauri のランタイムは文字列で reject することがある（引数のデシリアライズ失敗、
  // 未登録のコマンド、Rust 側の panic）。落とすと原因が消える。
  if (typeof e === "string" && e !== "") return e;
  return "予期しない失敗が起きた";
};

/**
 * 生成物の結果型を、投げる形へ剥がす。
 *
 * 画面側の失敗の扱いは一律で `.catch(fail)` に集めてある。 呼ぶ側ごとに
 * `if (r.status === "error")` を書くと、同じ分岐が 30 箇所に散る。
 * 型が失われるわけではない——投げているのは `AppError` そのもので、
 * [`errorMessage`] と [`isAppError`] がそれを見る。
 */
const unwrap = async <T>(
  r: Promise<{ status: "ok"; data: T } | { status: "error"; error: AppError }>,
): Promise<T> => {
  const v = await r;
  if (v.status === "error") throw v.error;
  return v.data;
};

/**
 * 画面から呼ぶ入口。
 *
 * `commands` をそのまま使わないのは、剥がす層と、下の3つの
 * オブジェクト引数を挟むため。 それ以外は素通しにする。
 */
export const api = {
  ...commands,

  listDevices: () => unwrap(commands.listDevices()),
  listProjects: () => unwrap(commands.listProjects()),
  createProject: (displayName: string) => unwrap(commands.createProject(displayName)),
  openProject: (id: string) => unwrap(commands.openProject(id)),
  progress: () => unwrap(commands.progress()),
  /** OS 側の音声加工の状態（`TR-REC-11`）。`Standard` 以外は録った音が本人の声でなくなる。 */
  armDevice: (deviceId: string) => unwrap(commands.armDevice(deviceId)),
  probeInput: (ms: number) => unwrap(commands.probeInput(ms)),
  startTake: () => unwrap(commands.startTake()),
  /** 行を指定して録り直す（`TR-REC-21`）。既存のテイクは消えない。 */
  startRetake: (rowId: string) => unwrap(commands.startRetake(rowId)),
  rowsWithTakes: () => unwrap(commands.rowsWithTakes()),
  /** 採用テイクを切り替える（`TR-RCL-25`）。カバレッジは変わらない。 */
  adoptTake: (rowId: string, takeId: number) => unwrap(commands.adoptTake(rowId, takeId)),
  finishTake: () => unwrap(commands.finishTake()),
  stopPreview: () => unwrap(commands.stopPreview()),
  /** そのテイクの原音設定を、エイリアスごとに引く（`TR-ALN-33`）。 */
  otosOfTake: (takeId: number) => unwrap(commands.otosOfTake(takeId)),
  /** 録れたものをそのまま鳴らす（`TR-REC-43`）。合成を通さない。 */
  playTake: (takeId: number) => unwrap(commands.playTake(takeId)),
  prerollMs: () => unwrap(commands.prerollMs()),
  estimateSpace: () => unwrap(commands.estimateSpace()),
  calibrate: (seconds: number) => unwrap(commands.calibrate(seconds)),
  gainDrift: () => unwrap(commands.gainDrift()),
  restoreSavedGain: () => unwrap(commands.restoreSavedGain()),
  /*
   * 失敗しない3つ。 生成側も結果型で包んでいないので、そのまま素通しにする。
   * `unwrap` に通すと、包まれていないものを剥がそうとして型が合わない。
   */
  autoAdvanceMs: () => commands.autoAdvanceMs(),
  /** いま入ってきている音の包絡を送らせる（`TR-REC-43`）。Channel を使う理由は `DEC-PLT-017`。 */
  streamEnvelope: (onFrame: Channel<EnvelopeView>) => commands.streamEnvelope(onFrame),
  /** 止める相手を名指しする。 番号を渡さないと、新しい流れを殺しうる。 */
  stopEnvelopeStream: (generation: number) => commands.stopEnvelopeStream(generation),
  outputKind: () => unwrap(commands.outputKind()),
  checkGuideLeak: (midi: number) => unwrap(commands.checkGuideLeak(midi)),
  playPitch: (midi: number) => unwrap(commands.playPitch(midi)),
  songStatus: () => unwrap(commands.songStatus()),
  singSong: (id: string) => unwrap(commands.singSong(id)),
  pendingWork: () => unwrap(commands.pendingWork()),
  latencyReport: () => unwrap(commands.latencyReport()),
  preflight: () => unwrap(commands.preflight()),
  useMixedChannels: () => unwrap(commands.useMixedChannels()),
  importUst: (bytes: number[], title: string) => unwrap(commands.importUst(bytes, title)),
  setSongInBank: (id: string, inBank: boolean) => unwrap(commands.setSongInBank(id, inBank)),

  /*
   * ここから下は、位置引数のままだと取り違えても型が通るもの。
   * `takeId` / `midi` / `lengthMs` はどれも `number` で、並べ替えても気づけない。
   */

  /** そのテイクを、指定の音高で鳴らす（`TR-SYN-18`）。 */
  preview: ({ takeId, midi, lengthMs }: { takeId: number; midi: number; lengthMs: number }) =>
    unwrap(commands.preview(takeId, midi, lengthMs)),

  waveformWindow: ({
    takeId,
    fromMs,
    toMs,
    pixels,
  }: {
    takeId: number;
    fromMs: number;
    toMs: number;
    pixels: number;
  }) => unwrap(commands.waveformWindow(takeId, fromMs, toMs, pixels)),

  spectrogramWindow: ({
    takeId,
    fromMs,
    toMs,
    columns,
    rows,
  }: {
    takeId: number;
    fromMs: number;
    toMs: number;
    columns: number;
    rows: number;
  }) => unwrap(commands.spectrogramWindow(takeId, fromMs, toMs, columns, rows)),
};

/** 画面に出す言い方。Rust の識別子をそのまま見せない。 */
export const micModeLabel = (m: MicModeView): string =>
  ({
    Standard: "加工なし",
    VoiceIsolation: "声を強調する処理",
    WideSpectrum: "周囲音を広く拾う処理",
    Unknown: "判定できません",
  })[m];
