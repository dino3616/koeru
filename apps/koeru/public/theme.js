/*
 * OS の配色設定を、Radix Colors が要る `.dark` / `.light` クラスへ写す。
 *
 * **インラインではなく外部ファイルにしてある。** Tauri の CSP は
 * `script-src 'self'` なので、インラインスクリプトは通らない。
 *
 * `<head>` の中で defer なしに読ませて、最初の描画より前に決める。
 * ここが遅れると、暗い設定の人に一瞬白い画面が出る。
 */
(() => {
  const apply = () => {
    const chosen = localStorage.getItem("koeru.theme");
    const dark =
      chosen === "dark" ||
      (chosen !== "light" && window.matchMedia("(prefers-color-scheme: dark)").matches);
    const root = document.documentElement;
    root.classList.toggle("dark", dark);
    root.classList.toggle("light", !dark);
    root.style.colorScheme = dark ? "dark" : "light";
  };

  try {
    apply();
    // **OS 側の切り替えに追従する。** 収録の途中で日が暮れることがある。
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", apply);
  } catch {
    // localStorage も matchMedia も使えない環境では、既定（明るい面）のままでよい。
  }
})();
