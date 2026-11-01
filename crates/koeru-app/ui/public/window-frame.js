/*
 * macOS の窓は、信号ボタンをツールバーの行に重ねる（`tauri.macos.conf.json` の Overlay）。
 * ツールバーがその幅を左に空けられるよう、`<html>` に印を付ける。
 *
 * 最初の描画より前に決める。 遅れると、ツールバーの中身が信号ボタンの下から右へ跳ぶ。
 * インラインにしないのは `theme.js` と同じ理由（CSP が `script-src 'self'`）。
 *
 * Tauri の外（Storybook、ブラウザ）では付けない。 信号ボタンが無いところで空けると、
 * 左に何も無い空きが残る。 Windows と Linux は OS の枠のままなので、こちらも付けない。
 */
(() => {
  if (!("__TAURI_INTERNALS__" in window)) return;
  if (!/Macintosh/.test(navigator.userAgent)) return;
  document.documentElement.dataset.windowFrame = "overlay";
})();
