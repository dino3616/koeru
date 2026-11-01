import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useState,
  useSyncExternalStore,
} from "react";
import { createPortal } from "react-dom";

import { createShellStore, type ShellStore } from "./shell-store";

/*
 * 場の画面がシェルへ頼む2つの口。
 *
 * 場の画面は `<main>` と見出しを持ち、シェルはその外側にある。 外側の状態を
 * 画面から触るので、props では渡せない（画面の親はルータで、シェルではない）。
 * context に store を置き、場の画面は hook と部品でだけ触れる。
 */
export const ShellContext = createContext<ShellStore | null>(null);

/** store を作って配る。 シェルが1つだけ置く。 */
export const ShellProvider = ({ children }: { children: ReactNode }) => {
  // 描画のたびに作ると、畳んだ状態が毎回消える。
  const [store] = useState(createShellStore);
  return <ShellContext value={store}>{children}</ShellContext>;
};

/** 畳むボタンの動き。 */
export const useSidebarToggle = (): (() => void) => {
  const store = useContext(ShellContext);
  return store?.toggle ?? noop;
};

/** ツールバーの右端の領域を store へ渡す ref。 */
export const useToolbarEndRef = (): ((element: HTMLElement | null) => void) => {
  const store = useContext(ShellContext);
  return store?.setToolbarEnd ?? noop;
};

/**
 * 畳んでいるか。 シェルの外では畳んでいない。
 *
 * サーバ側の描画（殻を先に出す段階）では畳まない。 本人の選択は
 * まだ何も無いので、開いた形が初期値になる。
 */
export const useSidebarCollapsed = (): boolean => {
  const store = useContext(ShellContext);
  return useSyncExternalStore(
    store?.subscribe ?? noopSubscribe,
    store?.isCollapsed ?? returnFalse,
    returnFalse,
  );
};

/**
 * 場の画面が呼ぶ。 `active` が真のあいだ、サイドバーを 64 の列へ畳む。
 *
 * 偽に戻るか、画面が消えたら、本人が選んでいた開閉へ戻す。 本人の選択は
 * 上書きしない——収録のあいだ畳んだからといって、本人が開いていたサイドバーが
 * 閉じたままになってはいけない。 畳まされているあいだに本人が開き直すのは通る
 * （その場だけで、場を離れれば捨てる）。
 *
 * シェルの外では何もしない。 場の画面だけを story に出すときに、シェルを用意させない。
 */
export const useSidebarCollapsedWhile = (active: boolean): void => {
  const store = useContext(ShellContext);
  useEffect(() => {
    if (!active || store === null) return;
    return store.hold();
  }, [active, store]);
};

/**
 * ツールバーの右端へ children を出す。 場の画面が描く。
 *
 * 描かれているあいだだけ出る。 場を移れば画面ごと消えるので、前の場の道具が
 * 残ることは無い。 シェルの外、またはツールバーがまだ描かれていないあいだは何も出ない。
 *
 * 置く場所は右端の領域で、窓の幅が 960 のときも他に取られない。 並べすぎると
 * パンくずが先に畳まれる（パンくずは省略記号で縮み、ここは縮まない）。
 */
export const ShellToolbarEnd = ({ children }: { children: ReactNode }) => {
  const store = useContext(ShellContext);
  const target = useSyncExternalStore(
    store?.subscribe ?? noopSubscribe,
    store?.getToolbarEnd ?? returnNull,
    returnNull,
  );
  return target === null ? null : createPortal(children, target);
};

const noop = () => {};
const noopSubscribe = () => noop;
const returnFalse = () => false;
const returnNull = () => null;
