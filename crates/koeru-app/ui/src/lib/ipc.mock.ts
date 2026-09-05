/*
 * story 用の Rust 境界。
 *
 * Storybook には Tauri が無いので、`invoke` は必ず失敗する。
 * `.storybook/preview.ts` の `sb.mock` がこのファイルを本物の代わりに読ませ、
 * story ごとに返り値を差し替える（`mocked(api).progress.mockResolvedValue(...)`）。
 *
 * `export *` にしない。 型の再輸出まで値として解決されて、
 * `AppError` が無いと言われる。値と型を書き分ける。
 */
import { fn } from "storybook/test";

import * as actual from "./ipc";

// 判断を持たないものは本物をそのまま使う。ここを差し替える理由が無い。
export { errorMessage, isAppError, micModeLabel } from "./ipc";

/**
 * `Channel` は差し替える。
 *
 * 本物は `window.__TAURI_INTERNALS__.transformCallback` を呼ぶので、
 * Tauri の無いところで `new Channel()` すると即落ちる。
 * 受け口だけを持つ形にして、`onmessage` を story が呼べるようにする。
 */
export class Channel<T> {
  onmessage: (message: T) => void = () => {};

  /** 本物は Rust が採番する。story では番号を持つ意味が無い。 */
  id = 0;
}
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
} from "./ipc";

/** 既定は「呼ばれたら黙って待ち続ける」。story が明示したものだけ答える。 */
const pending = () => new Promise<never>(() => {});

/**
 * 本物の口をすべて `fn()` に置き換える。
 *
 * 名前を1つずつ書かない。 コマンドを足すたびにここへ足すのを忘れ、
 * story だけが「そんな関数は無い」で落ちる。
 */
export const api = Object.fromEntries(
  Object.keys(actual.api).map((k) => [k, fn(pending).mockName(`api.${k}`)]),
) as unknown as typeof actual.api;
