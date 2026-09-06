import { HeadContent, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";

import { QueryClientProvider, QueryErrorResetBoundary } from "@tanstack/react-query";

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
            <Outlet />
          </ErrorBoundary>
        )}
      </QueryErrorResetBoundary>
    </QueryClientProvider>
  ),
});
