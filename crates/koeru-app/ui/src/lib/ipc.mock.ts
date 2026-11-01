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
/*
 * 型はまとめて通す。
 *
 * `export type *` にする。 素の `export *` だと `AppError` などが値として
 * 解決されて「そんな輸出は無い」で落ちるが、型だけと明示すれば通る。
 * 1つずつ書き出すと、Rust 側に型を足すたびにここへ足すのを忘れ、
 * story だけが「そんな型は無い」で落ちる。
 */
export type * from "./ipc";

/** 既定は「呼ばれたら黙って待ち続ける」。story が明示したものと、下の `answers` だけ答える。 */
const pending = () => new Promise<never>(() => {});

type Api = typeof actual.api;

/** 同意の既定。 どちらも訊いておらず、まだ送っていない（`TR-PKG-54`）。 */
const privacy: actual.PrivacySettings = {
  usage: "NOT_ASKED",
  crashReports: "NOT_ASKED",
  lastSentAt: null,
};

/**
 * 配布物の既定。 増やさない人が見るのは、枠をすべて入れた「すべて」が1つだけ
 * （`DEC-PKG-018`）。 まだ書き出しておらず、最初の版が下書きにある。
 */
const everything: actual.Distribution = {
  id: "1",
  name: "すべて",
  archiveName: "Koeru",
  subbanks: [],
  uncovered: [],
  terms: null,
  draftVersion: "1.0.0",
  releases: [],
  rowsChangedSinceRelease: null,
};

/**
 * 待たずに答える口。 製品の既定がそのまま絵になるものだけを置く。
 *
 * 選んだ同意は既定から1つだけ動かして返す。 訊く前のオフは撤回にならないので、
 * オフを選んでも `NOT_ASKED` のまま。
 */
const answers: Partial<Api> = {
  distributions: async () => [everything],
  privacy: async () => privacy,
  choosePrivacyConsent: async ({ subject, granted }) => {
    const state: actual.ConsentState = granted ? "GRANTED" : "NOT_ASKED";
    return {
      outcome: {
        __typename: "PrivacyConsentChosen",
        privacy:
          subject === "USAGE" ? { ...privacy, usage: state } : { ...privacy, crashReports: state },
      },
    };
  },
};

/**
 * 本物の口をすべて `fn()` に置き換える。
 *
 * 名前を1つずつ書かない。 コマンドを足すたびにここへ足すのを忘れ、
 * story だけが「そんな関数は無い」で落ちる。
 */
export const api = Object.fromEntries(
  Object.keys(actual.api).map((k) => [
    k,
    fn(answers[k as keyof Api] ?? pending).mockName(`api.${k}`),
  ]),
) as unknown as Api;
