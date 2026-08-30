import { createRouter } from "@tanstack/react-router";

import { routeTree } from "~/routeTree.gen";

/**
 * ルータを組む。
 *
 * **TanStack Start が起動時にここを呼ぶ**ので、名前は `getRouter` で固定。
 */
export const getRouter = () =>
  createRouter({
    routeTree,
    // **アプリなので、待ち時間を演出しない。** 出せるものはすぐ出す。
    defaultPreload: "intent",
    scrollRestoration: false,
  });

declare module "@tanstack/react-router" {
  interface Register {
    router: ReturnType<typeof getRouter>;
  }
}
