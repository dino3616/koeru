/*
 * OS の配色設定を、Radix Colors が要る `.dark` / `.light` クラスへ写す。
 *
 * インラインではなく外部ファイルにしてある。 Tauri の CSP は
 * `script-src 'self'` なので、インラインスクリプトは通らない。
 *
 * `<head>` の中で defer なしに読ませて、最初の描画より前に決める。
 * ここが遅れると、暗い設定の人に一瞬白い画面が出る。
 *
 * CSS 側に退避経路を置かない。 Radix の段は `.dark` に定義されているので、
 * `prefers-color-scheme` だけで色を差し替えるには段の値を手で写すことになる——
 * 写しは必ず片方だけ古くなる（`AGENTS.md` の禁止事項6）。
 * したがってここが唯一の切り替え経路であり、だからこそ下の順序が重要になる。
 */
(() => {
  const root = document.documentElement;

  /* OS の設定。保存された選択より弱い。 */
  const prefersDark = () => {
    try {
      return window.matchMedia("(prefers-color-scheme: dark)").matches;
    } catch {
      // `matchMedia` が無い環境。明るい面を既定にする。
      return false;
    }
  };

  /**
   * 保存された選択。まだ書く画面は無い（切り替え UI は未実装）。
   *
   * `localStorage` は、プライベートウィンドウや権限を絞った WebView で
   * 読むだけで例外を投げることがある。 そこで落ちても
   * OS の設定への追従は続けたいので、ここだけで閉じて捕まえる。
   */
  const saved = () => {
    try {
      return localStorage.getItem("koeru.theme");
    } catch {
      return null;
    }
  };

  const apply = () => {
    const chosen = saved();
    const dark = chosen === "dark" || (chosen !== "light" && prefersDark());
    root.classList.toggle("dark", dark);
    root.classList.toggle("light", !dark);
    root.style.colorScheme = dark ? "dark" : "light";
  };

  apply();

  // OS 側の切り替えに追従する。 収録の途中で日が暮れることがある。
  // `apply()` が投げても登録まで来るように、ここは独立して捕まえる。
  try {
    window.matchMedia("(prefers-color-scheme: dark)").addEventListener("change", apply);
  } catch {
    // 追従できない環境。初回の判定だけで進む。
  }
})();
