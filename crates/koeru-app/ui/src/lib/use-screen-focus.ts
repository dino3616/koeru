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
 *
 * **描画の中で焦点を動かさない。** effect からそのまま `focus()` を呼ぶと、
 * 同じ木の中に解決待ちの `Suspense` があるとき React が
 * 「Should not already be working.」で落ちる——焦点の移動が同期の
 * イベントを起こし、それが描画の途中に戻ってくる。画面が丸ごと
 * 失敗の面に置き換わるので、**気づきにくいのは見た目だけ**で、
 * 実害は画面が出ないこと。踏んだ。
 *
 * 1拍あとへ送る。 人には分からない遅れで、描画からは外れる。
 */
export const useScreenFocus = (): RefObject<HTMLHeadingElement | null> => {
  const ref = useRef<HTMLHeadingElement>(null);
  useEffect(() => {
    const t = window.setTimeout(() => ref.current?.focus(), 0);
    return () => window.clearTimeout(t);
  }, []);
  return ref;
};
