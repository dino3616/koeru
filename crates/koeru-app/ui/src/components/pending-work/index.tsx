import { useEffect, useState } from "react";

import { api } from "~/lib/ipc";

/**
 * 背後で待っている仕事（`TR-SYN-33`、`TR-SYN-34`）。
 *
 * 「録音終了 → 歌わせられる」の間に、無言の待ち時間を作らない。
 * 初回は前処理を含むので、中央値の目標が6秒ある——何も出ないまま
 * 6秒待たされると、壊れたと思われる。
 *
 * 問い合わせに載せない。 返ってきてから次を予約する形を自分で書く
 * ——`refetchInterval` も `setInterval` も、1回が間隔より長くかかったときの
 * 振る舞いを自分で決められない。待ち数がいちばん動くのはテイクの確定中で、
 * そこがいちばん詰まる時間でもある。
 */
export const PendingWork = () => {
  const [pending, setPending] = useState(0);

  useEffect(() => {
    let alive = true;
    let timer = 0;
    const tick = () => {
      api
        .pendingWork()
        .then((n) => {
          if (alive) setPending(n);
        })
        // 読めなくても画面は成り立つ。0 として黙る。
        .catch(() => {
          if (alive) setPending(0);
        })
        .finally(() => {
          if (alive) timer = window.setTimeout(tick, 400);
        });
    };
    tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  /*
   * 領域は常に置き、中身だけを差し替える。 文言と一緒に挿し込むと、
   * 支援技術が変化として拾えず読まれない（`TR-PLT-29`）。
   */
  return (
    <p aria-live="polite" aria-atomic="true" className="text-center text-xs text-slate-11">
      {pending > 0
        ? `録った音を整えています（残り ${pending} 件）。いま歌わせても鳴りますが、少し待ちます。`
        : ""}
    </p>
  );
};
