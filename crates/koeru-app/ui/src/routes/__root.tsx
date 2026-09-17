import { HeadContent, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";

import { QueryClientProvider, QueryErrorResetBoundary } from "@tanstack/react-query";

import { AnimateView } from "motion/react-animate-view";
import { useReducedMotion } from "motion/react";
import type { Transition } from "motion/react";

import { Announcer } from "~/components/announcer";
import { ErrorBoundary } from "~/components/error-boundary";
import type { ReactNode } from "react";

import { createQueryClient } from "~/lib/query-client";
import globalsCss from "~/styles/globals.css?url";

/*
 * 1つだけ作る。
 *
 * 描画のたびに作ると、キャッシュが毎回空になって取り直しが止まらない。
 * このアプリは窓が1つなので、モジュールの寿命でよい。
 */
const queryClient = createQueryClient();

const RootDocument = ({ children }: { children: ReactNode }) => (
  /*
   * `lang="ja"` を必ず置く（WCAG 3.1.1）。読み上げの言語がこれで決まる。
   *
   * `suppressHydrationWarning` は `public/theme.js` のため。
   * あれは最初の描画より前に `class` と `color-scheme` を書き換えるので、
   * サーバが出した殻と必ず食い違う。 食い違いは意図したもので、
   * ここで黙らせないと毎回コンソールに出る。`<html>` の1枚だけに掛かる。
   */
  <html lang="ja" suppressHydrationWarning>
    <head>
      <HeadContent />
    </head>
    <body>
      {children}
      <Scripts />
    </body>
  </html>
);

/*
 * 画面の入れ替わりの量（`DEC-PLT-032`）。
 *
 * 160ms は「入れ替わった」と分かる下限側に寄せてある。 延ばすと、押してから画面が
 * 使えるまでが実際に延びる——遷移中の画面は静止画で、押しても反応しない。
 *
 * 緩急は CSS の `ease` を数で写したもの。 同じ曲線が Motion の名前つきの緩急に無い。
 */
const screenTransition: Transition = { duration: 0.16, ease: [0.25, 0.1, 0.25, 1] };

/*
 * 画面が入れ替わったことを、入れ替わりそのもので伝える。
 * 声の並び・声・テイクはどれも全面が差し替わるので、切り替えだけだと
 * 「押せたのか」「別の画面なのか」が一瞬読めない。
 *
 * 名前を固定する。 中身が入れ替わっても同じ名前なら、React は消滅と出現ではなく
 * 1つの領域の変化として扱い、前後を同じ層で重ねて溶かす。名前を外すと層が2つに
 * 割れ、重なっているあいだ地の色が 25% 覗く（実測、`DEC-PLT-032`）。
 *
 * 入る側は動かせない。 名前を保つと `AnimateView` の区分では update で、そこへ
 * 渡した値は消える側の層にしか当たらない。入る側を動かすには層を割ることになり、
 * それは上の理由で採らない。既定の溶かし込みに、時間と緩急だけを付け替える。
 *
 * ルータ側の `viewTransition` は使わない（既定で off のまま）。あちらは
 * `document.startViewTransition` を直接叩くので、React が持つ木の更新と二重に走る。
 */
const Screen = () => {
  /*
   * 動きを減らす設定なら、遷移を始めない（`TR-PLT-33`）。
   *
   * CSS では止められない。 `AnimateView` は疑似要素を WAAPI で動かすので、
   * `::view-transition-*` に `animation: none` を当てても効かない。実測した。
   *
   * 時間を詰めるのではなく、包むのをやめる。 詰めても静止画は一度挟まる。
   */
  const reduceMotion = useReducedMotion();
  if (reduceMotion) return <Outlet />;

  return (
    <AnimateView name="screen" transition={screenTransition}>
      <Outlet />
    </AnimateView>
  );
};

export const Route = createRootRoute({
  head: () => ({
    meta: [
      { charSet: "utf-8" },
      { name: "viewport", content: "width=device-width, initial-scale=1" },
      { title: "KOERU" },
    ],
    links: [{ rel: "stylesheet", href: globalsCss }],
    // 最初の描画より前に配色を決める。 遅れると暗い設定の人に白い画面が一瞬出る。
    // インラインではなく外部ファイル: Tauri の CSP は script-src 'self'。
    scripts: [{ src: "/theme.js" }],
  }),
  shellComponent: RootDocument,
  component: () => (
    <QueryClientProvider client={queryClient}>
      {/*
        読み上げ領域は境界の外に置く。 内側に入れると、描画で例外が出たときに
        領域ごと外れて挿し直しになり、支援技術が変化として拾えなくなる。
      */}
      <Announcer />
      {/*
        「やり直す」で問い合わせの失敗も消す。
        `reset` を渡さないと、描き直した先で同じ失敗をもう一度読んで
        即座に同じ例外が飛ぶ——押しても画面が変わらない。
      */}
      <QueryErrorResetBoundary>
        {({ reset }) => (
          <ErrorBoundary onReset={reset}>
            <Screen />
          </ErrorBoundary>
        )}
      </QueryErrorResetBoundary>
    </QueryClientProvider>
  ),
});
