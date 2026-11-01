/**
 * 設定のタブ。 並びは左の列の並び。
 *
 * `id` は経路の検索引数 `tab` の値（`routes/_studio/settings.tsx`）。
 * タブを足すときは、経路の `picklist` にも同じ綴りを足す。
 */
export const SETTINGS_TABS = [
  { id: "general", label: "一般" },
  { id: "recording", label: "収録" },
  { id: "export", label: "書き出し" },
  { id: "tools", label: "外部ツール" },
  { id: "display", label: "表示" },
  { id: "privacy", label: "プライバシー" },
] as const;

export type SettingsTab = (typeof SETTINGS_TABS)[number]["id"];

/**
 * 検索引数から読んだ値がタブか。
 *
 * 画面は経路を `strict: false` で読むので、`tab` の型は経路の外から見ると
 * 何でもありうる。 狭めてから使う。
 */
export const isSettingsTab = (value: unknown): value is SettingsTab =>
  SETTINGS_TABS.some((t) => t.id === value);
