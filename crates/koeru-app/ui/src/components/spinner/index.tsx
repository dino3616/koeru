/**
 * 待っていることを示す印。
 *
 * `aria-hidden` にする。 待ちを伝えるのは読み上げ領域の文言のほうで
 * （`TR-PLT-29`）、絵は目で見る人のためだけに置く。二重に読ませない。
 *
 * `animate-spin` は `prefers-reduced-motion` で止まる（`globals.css` が
 * `animation-duration` を潰す）。止まっても、押せないことは `disabled` が伝える。
 */
export const Spinner = () => (
  <span
    aria-hidden="true"
    className="size-4 animate-spin rounded-full border-2 border-current border-t-transparent"
  />
);
