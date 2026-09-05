import { createRouter } from "@tanstack/react-router";

import { RouteError } from "~/components/route-error";
import { routeTree } from "~/routeTree.gen";

/**
 * ルータを組む。
 *
 * TanStack Start が起動時にここを呼ぶので、名前は `getRouter` で固定。
 */
export const getRouter = () =>
  createRouter({
    routeTree,
    // アプリなので、待ち時間を演出しない。 出せるものはすぐ出す。
    defaultPreload: "intent",
    scrollRestoration: false,
    // ルータが投げたものも受ける。 `ErrorBoundary` が拾うのは描画の例外だけで、
    // ルータの側で起きた失敗はその外を通る——どちらも白い画面にしない。
    defaultErrorComponent: RouteError,
  });

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}
