import { type RefObject, useEffect, useRef } from "react";

/**
 * 画面に入ったとき、見出しへ焦点を移す。
 *
 * SPA の遷移はページ読み込みではないので、何もしないと焦点は前の画面で
 * 押したボタンの位置に残る。 キーボードだけで使っている人は、
 * 移動先の先頭ではなく途中から辿り直すことになる（`TR-PLT-29`）。
 *
 * 読み上げは `Announcer` が別に持つ。焦点の移動だけをここでする——
 * 焦点で読ませようとすると、見出しの文言と遷移の通知が二重に読まれる。
 *
 * 移す先は `tabIndex={-1}` を持たせた要素にする。持たせないと
 * `focus()` が効かない。`outline-none` を併せて置く——
 * 自分で押していないのに環が出ると、押した場所を見失う。
 */
export const useScreenFocus = (): RefObject<HTMLHeadingElement | null> => {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    ref.current?.focus();
  }, []);
  return ref;
};
