import { useEffect, useState } from "react";
import { useRouterState } from "@tanstack/react-router";

/**
 * 常設の読み上げ領域。
 *
 * live region は中身より先に DOM へ居る必要がある。
 * 文言と一緒に挿し込むと、支援技術が変化として拾えず読まれない。
 * だから空のまま置いておき、中身だけを差し替える。
 *
 * 画面遷移も同じ経路で伝える。SPA の遷移はページ読み込みではないので、
 * 何も言わないと「どこへ来たのか」が分からない（`TR-PLT-29`）。
 */
export const Announcer = () => {
  const path = useRouterState({ select: (s) => s.location.pathname });
  const [message, setMessage] = useState("");

  useEffect(() => {
    const name = path === "/" ? "音源の一覧" : path === "/record" ? "収録" : path;
    // 空 → 文言 の変化として拾わせる。同じ文言が続くと読まれないので一度空にする。
    // 画面遷移という外部の出来事に同期する。ここは effect の本来の用途。
    // oxlint-disable-next-line react/set-state-in-effect
    setMessage("");
    const t = window.setTimeout(() => setMessage(`${name}へ移動しました`), 80);
    return () => window.clearTimeout(t);
  }, [path]);

  return (
    <p aria-live="polite" aria-atomic="true" className="sr-only">
      {message}
    </p>
  );
};
