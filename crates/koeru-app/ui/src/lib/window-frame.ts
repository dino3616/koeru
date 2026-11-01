import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect } from "react";

/**
 * 全画面のあいだ、`<html>` に `data-fullscreen` を付ける。
 *
 * 全画面では信号ボタンが消えるので、ツールバーが左に空けた幅（`traffic-lights`）が
 * 何も無い空きになる。 窓の大きさが変わるたびに問い直す——全画面へ出入りした
 * 知らせは別に無く、大きさの変化として届く。
 *
 * 信号ボタンを重ねていない窓では何もしない（`public/window-frame.js`）。
 */
export const useFullscreenMark = () => {
  useEffect(() => {
    const root = document.documentElement;
    if (root.dataset["windowFrame"] !== "overlay") return;
    const win = getCurrentWindow();
    let alive = true;

    const sync = async () => {
      const full = await win.isFullscreen();
      if (alive) root.toggleAttribute("data-fullscreen", full);
    };
    // 問えなければ空けたまま。 信号ボタンが残っている側に倒す。
    sync().catch(() => {});
    const unlisten = win.onResized(() => {
      sync().catch(() => {});
    });

    return () => {
      alive = false;
      unlisten.then((stop) => stop()).catch(() => {});
    };
  }, []);
};
