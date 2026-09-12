/**
 * 待っていることを示す印（`docs/design/direction.md` の「息の抑揚」）。
 *
 * 回る円にしない。 返事を待っているあいだの印なので、
 * 進行の割合を示す形（円や棒）を置くと、進み具合を約束することになる——
 * 押してから鳴るまでの時間は素材の量で変わり、こちらには分からない。
 *
 * `aria-hidden` にする。 待ちを伝えるのは読み上げ領域の文言のほうで
 * （`TR-PLT-29`）、絵は目で見る人のためだけに置く。二重に読ませない。
 *
 * 止まった姿が正しい絵になるようにしてある。 `prefers-reduced-motion` では
 * `globals.css` が `animation` を潰すので、キーフレームではなく元の値が残る。
 * だから3本とも、動かないときは同じ濃さで並ぶ。
 *
 * 色は置く側から継ぐ。 段を焼き込まない——段 11 を書いていたので、
 * 段 11 の塗りを持つ `Button` の中では**塗りと同じ色になって消えていた**
 * （`確かめています` の中。`disabled:opacity-45` も掛かるので完全に見えない）。
 * 「押してから鳴るまでを無言にしない」（`TR-SYN-33`）の印が、
 * いちばん要る場所で出ていなかった。**踏んだ。**
 */
export const Breath = ({ size = "md" }: { size?: "sm" | "md" }) => {
  const height = size === "sm" ? 16 : 44;
  const width = size === "sm" ? 88 : 240;
  return (
    <svg viewBox="0 0 240 44" width={width} height={height} aria-hidden="true" fill="currentColor">
      <path className="koeru-breath" d="M8 10 C 70 3, 150 3, 232 10 C 150 17, 70 17, 8 10 Z" />
      <path
        className="koeru-breath [animation-delay:0.3s]"
        d="M40 22 C 90 17, 140 17, 200 22 C 140 27, 90 27, 40 22 Z"
      />
      <path
        className="koeru-breath [animation-delay:0.6s]"
        d="M24 34 C 80 29, 130 29, 176 34 C 130 39, 80 39, 24 34 Z"
      />
    </svg>
  );
};
