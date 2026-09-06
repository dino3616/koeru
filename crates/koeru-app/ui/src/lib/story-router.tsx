import {
  RouterProvider,
  createMemoryHistory,
  createRootRoute,
  createRoute,
  createRouter,
} from "@tanstack/react-router";
import type { ReactNode } from "react";

import { RouteError } from "~/components/route-error";

/*
 * story にルータの文脈を与える。
 *
 * `useNavigate` / `useSearch` / `useRouterState` を使う部品は、ルータの外では
 * `Cannot read properties of null` で落ちる。 story のためだけに、
 * 記憶上の履歴で最小のルータを組む。
 *
 * 本物の `routeTree` を使わない。 あれは `__root` から始まって
 * `theme.js` の読み込みや `Announcer` まで引き連れてくるので、
 * 1つの部品を見るには重すぎる。ここは経路の形だけを与える。
 */
export const withRouter = (children: ReactNode, path = "/") => {
  const root = createRootRoute();
  const index = createRoute({ getParentRoute: () => root, path: "/", component: () => children });
  const record = createRoute({
    getParentRoute: () => root,
    path: "/record",
    component: () => children,
    // 画面は `id` を検索引数で受ける。無いときの経路も story で出せるように、
    // 渡されたものをそのまま通す。
    validateSearch: (search: Record<string, unknown>) => ({
      id: search["id"] as string | undefined,
    }),
  });

  const router = createRouter({
    routeTree: root.addChildren([index, record]),
    history: createMemoryHistory({ initialEntries: [path] }),
    /*
     * 本物と同じ失敗の面を出す（`~/router`）。
     *
     * 渡さないと、ルータが自前の既定の面を描く。 あれは本体では一度も
     * 出ない絵なので、story で見ているものが本体と食い違う——
     * 実際、既定の面は赤字のコントラストが 3.89 で a11y の検査に落ちた。
     */
    defaultErrorComponent: RouteError,
  });

  return <RouterProvider router={router as never} />;
};
