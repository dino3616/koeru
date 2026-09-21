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
  BankSongView,
  CalibrationView,
  ChosenDeviceView,
  DeviceView,
  EnvelopeView,
  ExportedView,
  FindingView,
  GainControlView,
  ImportedSongView,
  LatencyView,
  LeakView,
  MethodPresetView,
  MicModeView,
  NoteView,
  OtoView,
  OutputKindView,
  PackageFileView,
  PackageSettingsView,
  PackageStateView,
  PlanRowView,
  PreflightView,
  ProgressView,
  ReviewItemView,
  ReviewSummaryView,
  ProjectView,
  ReleaseView,
  RingView,
  RowTakesView,
  SongSelection,
  SongPlanView,
  SongView,
  SpaceView,
  SpectrogramView,
  SungSongView,
  TakeSummaryView,
  TakeView,
  UnencodableView,
  VoiceStateView,
  VoiceView,
} from "~/lib/bindings.gen";

import type {
  AppError,
  EnvelopeView,
  MicModeView,
  PackageSettingsView,
  SongSelection,
} from "~/lib/bindings.gen";

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
  /** 選べる作り方（`TR-RCL-11`）。いまは単独音だけ。 */
  methodPresets: () => unwrap(commands.methodPresets()),
  /** 方式プリセットを選んで作る（`TR-RCL-01`）。 */
  createProject: ({ displayName, presetId }: { displayName: string; presetId: string }) =>
    unwrap(commands.createProject(displayName, presetId)),
  /** 表示名を変える（`DEC-PKG-007`）。空にはできない。 */
  renameProject: (id: string, displayName: string) =>
    unwrap(commands.renameProject(id, displayName)),
  openProject: (id: string) => unwrap(commands.openProject(id)),
  /** 開いている音源の環と色（`DEC-PLT-025`）。 */
  voiceState: () => unwrap(commands.voiceState()),
  progress: () => unwrap(commands.progress()),
  /** この音源で選ばれているマイク（`TR-REC-03`）。選択の持ち主は Rust 側。 */
  chosenDevice: () => unwrap(commands.chosenDevice()),
  /** OS 側の音声加工の状態（`TR-REC-11`）。`Standard` 以外は録った音が本人の声でなくなる。 */
  armDevice: (deviceId: string) => unwrap(commands.armDevice(deviceId)),
  probeInput: (ms: number) => unwrap(commands.probeInput(ms)),
  startTake: () => unwrap(commands.startTake()),
  /** 行を指定して録り直す（`TR-REC-21`）。既存のテイクは消えない。 */
  startRetake: (rowId: string) => unwrap(commands.startRetake(rowId)),
  rowsWithTakes: () => unwrap(commands.rowsWithTakes()),
  /** いまの録る順と、その並び（`TR-SYN-19`）。 */
  recordingOrder: () => unwrap(commands.recordingOrder()),
  /** 録る順を切り替える（`TR-SYN-19` の (b)）。可逆。 */
  setRecordingOrder: (mode: string) => unwrap(commands.setRecordingOrder(mode)),
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
  /** その曲を歌うために、あと録る行（`TR-RCL-17`）。 */
  songPlan: (id: string) => unwrap(commands.songPlan(id)),
  singSong: (id: string) => unwrap(commands.singSong(id)),
  pendingWork: () => unwrap(commands.pendingWork()),
  latencyReport: () => unwrap(commands.latencyReport()),
  preflight: () => unwrap(commands.preflight()),
  /** 確認の進み具合（`TR-ALN-25`）。 */
  reviewSummary: () => unwrap(commands.reviewSummary()),
  /** 採用テイクのエントリ全部を、確認待ちが先の順に（`TR-ALN-26`）。 */
  reviewQueue: () => unwrap(commands.reviewQueue()),
  /** 1件ずつ確認して確定させる（`REQ-ALN-008`）。 */
  confirmEntry: (alias: string) => unwrap(commands.confirmEntry(alias)),
  /** まとめて確認する（`REQ-ALN-010`）。個別確認をやめたあとだけ通る。 */
  confirmAllEntries: () => unwrap(commands.confirmAllEntries()),
  /** 録り直しに回す（`REQ-ALN-009`）。エントリを未推定へ戻すだけ。 */
  rerecordEntry: (alias: string) => unwrap(commands.rerecordEntry(alias)),
  /** 書き出し前の検証（`TR-ALN-20`）。直せるものを直す。 */
  validateOtos: () => unwrap(commands.validateOtos()),
  /**
   * `oto.ini` を書き出す（`TR-ALN-21`）。確認が残っている間は通らない。
   *
   * 文字コードを選べる。 既定の CP932 は UTAU 本体互換、UTF-8 は
   * OpenUtau など対応している受け手向け。
   */
  exportOtos: (encoding: OtoEncoding) => unwrap(commands.exportOtos(encoding)),
  /** モデルが変わって古くなった推定（`TR-ALN-29`）。 */
  staleTakes: () => unwrap(commands.staleTakes()),
  /** 同梱しているモデルのライセンス表記（`TR-ALN-31`）。 */
  modelNotice: () => unwrap(commands.modelNotice()),
  /** 配布に出す値（`PROFILE-M4`）。画像は別の口で取る。 */
  packageSettings: () => unwrap(commands.packageSettings()),
  setPackageSettings: (input: PackageSettingsView) => unwrap(commands.setPackageSettings(input)),
  /** 音源アイコンの元画像（`TR-PKG-07`）。`null` で外す。 */
  setPackageIcon: (bytes: number[] | null) => unwrap(commands.setPackageIcon(bytes)),
  setPackagePortrait: (bytes: number[] | null) => unwrap(commands.setPackagePortrait(bytes)),
  packageIcon: () => unwrap(commands.packageIcon()),
  packagePortrait: () => unwrap(commands.packagePortrait()),
  /** いま書き出せるか（`TR-PKG-49`）。 */
  packageState: () => unwrap(commands.packageState()),
  /** 配り物に入るもの（`TR-PKG-28` の同梱物）。 */
  packageContents: () => unwrap(commands.packageContents()),
  /** 書き出す（`REQ-PKG-105`）。ZIP と UAR の2つが出る（`DEC-PKG-010`）。 */
  exportPackage: () => unwrap(commands.exportPackage()),
  /** 書き出しの履歴（`TR-PKG-44`）。新しい順。 */
  releases: () => unwrap(commands.releases()),
  /** 書き出したものを、OS のファイルマネージャで見せる（`TR-PKG-45`）。 */
  revealRelease: (seq: number) => unwrap(commands.revealRelease(seq)),
  useMixedChannels: () => unwrap(commands.useMixedChannels()),
  /** UST / USTX を取り込む（`TR-RCL-12`）。USTX は1トラックが1曲になる。 */
  importSongs: (bytes: number[], fileName: string) => unwrap(commands.importSongs(bytes, fileName)),
  /** 曲の題を変える（`TR-RCL-12`）。 */
  renameSong: (id: string, title: string) => unwrap(commands.renameSong(id, title)),
  /** 曲のノート列（`TR-RCL-12`）。範囲を選ぶのに要る。 */
  songNotes: (id: string) => unwrap(commands.songNotes(id)),
  /** 選んだノート群から録音リストを詰め直す（`TR-RCL-16`）。 */
  repackForSelection: (selections: SongSelection[]) =>
    unwrap(commands.repackForSelection(selections)),
  /** 取り込んだ曲すべて（`TR-RCL-12`）。外した曲も並ぶ。 */
  allSongs: () => unwrap(commands.allSongs()),
  /** 曲をバンクから外す／戻す（`TR-RCL-12`）。曲そのものは消さない。 */
  setSongInBank: (id: string, inBank: boolean) => unwrap(commands.setSongInBank(id, inBank)),

  /*
   * ここから下は、位置引数のままだと取り違えても型が通るもの。
   * `takeId` / `midi` / `lengthMs` はどれも `number` で、並べ替えても気づけない。
   */

  /** そのテイクを、指定の音高で鳴らす（`TR-SYN-18`）。 */
  preview: ({ takeId, midi, lengthMs }: { takeId: number; midi: number; lengthMs: number }) =>
    unwrap(commands.preview(takeId, midi, lengthMs)),

  /** 個別確認をやめる（`REQ-ALN-010`）。上限を超えていなければ通らない。 */
  switchReviewMode: (mode: "batch" | "suggest_rerecord") => unwrap(commands.switchReviewMode(mode)),

  /** 5値のどれかを人が直す。その値だけを固定する（`TR-ALN-30`）。 */
  editOtoValue: ({ alias, slot, value }: { alias: string; slot: OtoSlot; value: number }) =>
    unwrap(commands.editOtoValue(alias, slot, value)),

  /** 固定を解いて自動へ戻す（`REQ-ALN-006`）。 */
  revertOtoValue: ({ alias, slot }: { alias: string; slot: OtoSlot }) =>
    unwrap(commands.revertOtoValue(alias, slot)),

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

/**
 * 5値のどれか（`TR-ALN-30`）。
 *
 * 綴りは Rust 側の `Slot` に合わせる。 生成物に出てこないのは、
 * コマンドが文字列で受けているため——ここで型を絞って、
 * 打ち間違いが実行時まで残らないようにする。
 */
export type OtoSlot = "offset" | "consonant" | "cutoff" | "preutterance" | "overlap";

/**
 * `oto.ini` の文字コード（`TR-ALN-21`）。
 *
 * 綴りは Rust 側の `TextEncoding::as_str` に合わせる。
 */
export type OtoEncoding = "cp932" | "utf8";

/** 画面に出す言い方。Rust の識別子をそのまま見せない。 */
export const micModeLabel = (m: MicModeView): string =>
  ({
    Standard: "加工なし",
    VoiceIsolation: "声を強調する処理",
    WideSpectrum: "周囲音を広く拾う処理",
    Unknown: "判定できません",
  })[m];
