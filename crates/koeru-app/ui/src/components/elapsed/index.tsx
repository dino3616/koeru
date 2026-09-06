import { useEffect, useState } from "react";

/**
 * 収録の経過秒。
 *
 * 収録中だけマウントする。 数えはじめはマウントの瞬間で、
 * 呼び出し側から時刻を渡さない——ref を描画中に読むことになるため。
 *
 * ここだけが再描画される。 秒を収録画面の直下に置くと、
 * 200ms ごとに TakeInspector や一覧を含む木が丸ごと再評価される。
 *
 * 読み上げには出さない。数字は `aria-hidden` にして、状態は別の live region が持つ。
 */
export const Elapsed = () => {
  const [seconds, setSeconds] = useState(0);

  useEffect(() => {
    const since = performance.now();
    const t = window.setInterval(() => {
      setSeconds(Math.floor((performance.now() - since) / 1000));
    }, 200);
    return () => window.clearInterval(t);
  }, []);

  return <span aria-hidden="true">（{seconds} 秒）</span>;
};
