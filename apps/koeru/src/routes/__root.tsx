import { HeadContent, Outlet, Scripts, createRootRoute } from "@tanstack/react-router";
import type { ReactNode } from "react";

import globalsCss from "~/styles/globals.css?url";

const RootDocument = ({ children }: { children: ReactNode }) => (
  // **`lang="ja"` を必ず置く**（WCAG 3.1.1）。読み上げの言語がこれで決まる。
  <html lang="ja">
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
    // **最初の描画より前に配色を決める。** 遅れると暗い設定の人に白い画面が一瞬出る。
    // インラインではなく外部ファイル: Tauri の CSP は script-src 'self'。
    scripts: [{ src: "/theme.js" }],
  }),
  shellComponent: RootDocument,
  component: Outlet,
});
