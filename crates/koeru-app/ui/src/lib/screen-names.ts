/**
 * 刷新した画面の経路。 並びはサイドバーの場の順。
 *
 * 経路を足したらここに足す。 殻を先に出す一覧（`vite.config.ts`）もここから読む。
 * そこから漏れると、その画面で再読み込みしたときに 404 になる。
 */
export const STUDIO_PATHS = [
  "/voicebank",
  "/subbank",
  "/record",
  "/songs",
  "/review",
  "/distribute",
  "/released",
  "/settings",
  "/new",
] as const;

/**
 * 経路から画面の名前へ。内部表現をそのまま読ませない（`TR-PKG-45`）。
 *
 * 無い経路は、読み上げが `/record` のような経路の字をそのまま読む。
 */
export const SCREEN_NAMES: Record<string, string> = {
  "/": "声",
  "/voice": "音源",
  "/take": "録った回",
  "/voicebank": "音源のページ",
  "/subbank": "サブバンクのページ",
  "/record": "収録",
  "/songs": "曲バンク",
  "/review": "原音設定",
  "/distribute": "配布",
  "/released": "手渡せる状態",
  "/settings": "設定",
  "/new": "新しい音源",
} satisfies Record<"/" | "/voice" | "/take" | (typeof STUDIO_PATHS)[number], string>;
