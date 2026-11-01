/*
 * シェルが持つ状態。 React の外に置き、`useSyncExternalStore` で読む。
 *
 * サイドバーを畳ませる場の画面は、描かれているあいだだけ畳ませたい。 それを
 * effect の中の `setState` で書くと、「畳ませる」と「本人が選んだ開閉」が
 * 同じ state に混ざって、戻すときにどちらが本人の選択だったか分からなくなる。
 * 数えるものと選んだものを別に持てば、戻すのは数が 0 になることだけで済む。
 */

export type ShellStore = {
  subscribe: (listener: () => void) => () => void;
  /** 畳んでいるか。 */
  isCollapsed: () => boolean;
  /** 畳むボタン。 場の画面に畳まされているあいだは、そのあいだだけ開き直せる。 */
  toggle: () => void;
  /** 場の画面が畳ませる。 返す関数で手放す。 */
  hold: () => () => void;
  /** ツールバーの右端の領域。 描かれていなければ `null`。 */
  setToolbarEnd: (element: HTMLElement | null) => void;
  getToolbarEnd: () => HTMLElement | null;
};

export const createShellStore = (): ShellStore => {
  const listeners = new Set<() => void>();
  const notify = () => {
    for (const listener of listeners) listener();
  };

  /** 本人が選んだ開閉。 畳まされているあいだも触らない。 */
  let chosen = false;
  /** 畳ませている場の画面の数。 場の移り変わりで、出る側と入る側が一瞬重なる。 */
  let holds = 0;
  /** 畳まされているあいだに、本人が開き直したか。 手放したら捨てる。 */
  let reopened = false;
  let toolbarEnd: HTMLElement | null = null;

  return {
    subscribe: (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    isCollapsed: () => (holds > 0 ? !reopened : chosen),
    toggle: () => {
      if (holds > 0) reopened = !reopened;
      else chosen = !chosen;
      notify();
    },
    hold: () => {
      holds += 1;
      // 入ってきた場は畳んだところから始める。 前の場で開き直した分を持ち越さない。
      if (holds === 1) reopened = false;
      notify();
      let released = false;
      return () => {
        // StrictMode の二重の後始末で数が負にならないように。
        if (released) return;
        released = true;
        holds -= 1;
        if (holds === 0) reopened = false;
        notify();
      };
    },
    setToolbarEnd: (element) => {
      if (toolbarEnd === element) return;
      toolbarEnd = element;
      notify();
    },
    getToolbarEnd: () => toolbarEnd,
  };
};
