import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
// **ここは `vite` から取る。`vite-plus` からではない。**
//
// TanStack Start は `vite` の `isRunnableDevEnvironment` で環境を見分ける。
// `vite-plus` が再輸出する `createRunnableDevEnvironment` は**別のクラス**を作るので、
// そちらで作ると Start からは「走らせられない環境」に見え、
// **middleware が入らず `/` が 404 になる。** 実際にそうなった。
// oxlint-disable-next-line vite-plus/prefer-vite-plus-imports
import { createRunnableDevEnvironment } from "vite";
import { defineConfig, lazyPlugins } from "vite-plus";

/*
 * WebView 側の設定。**Rust と同じ crate の中に置いてある。**
 *
 * `koeru-app` はアプリケーション層で、Tauri のコマンドと画面は一体のもの。
 * 別のワークスペースに切ると、コマンドを1つ足すたびに2箇所を行き来することになる。
 *
 * **整形と lint の範囲はこのディレクトリの中だけ。** `docs/generated/` は FSL から
 * 決定論的に生成していて CI が drift を見ているし、`meta/` の TOML は check-meta が読む。
 * ここから外へ出ると、その両方を黙って書き換える。**一度やった。**
 */

// **Tauri が繋ぐポート。** ../tauri.conf.json の devUrl と揃える。
const DEV_PORT = 1420;

/*
 * TanStack Router が書き出すルート木。
 *
 * **生成物なので整形も lint もしない。** 型検査には要るのでリポジトリに置くが、
 * 手で直す対象ではない（ファイル自身の先頭にもそう書いてある）。
 */
const GENERATED = ["src/routeTree.gen.ts"];

const config = defineConfig({
  staged: {
    "*.{ts,tsx,css,json}": "vp check --fix",
  },
  fmt: {
    ignorePatterns: GENERATED,
  },
  lint: {
    ignorePatterns: GENERATED,
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: { "vite-plus/prefer-vite-plus-imports": "error" },
    options: { typeAware: true, typeCheck: true },
  },
  /*
   * **SSR 環境を「走らせられる」形で作る。**
   *
   * TanStack Start の dev サーバは、`ssr` 環境の中でサーバ入口を実行して HTML を返す。
   * vite-plus の既定の `ssr` 環境はそれができない形なので、
   * **Start は黙って middleware を入れず、`/` が 404 になる**（実際になった）。
   *
   * Tauri の中にサーバは無いので SSR はしないが、**dev で画面を出すのにこの環境が要る。**
   */
  environments: {
    ssr: {
      dev: {
        createEnvironment: (name, config) => createRunnableDevEnvironment(name, config),
      },
    },
  },
  resolve: {
    tsconfigPaths: true,
    alias: {
      "~": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // **外へ出ない。** 処理はローカル完結で、声をサーバへ送らない。
  server: {
    port: DEV_PORT,
    strictPort: true,
    host: "127.0.0.1",
  },
  plugins:
    lazyPlugins(() => [
      tailwindcss(),
      tanstackStart({
        // **SSR を持たない。** Tauri の中にサーバは無いので、
        // 起動時に配るのは殻だけにして、あとは全部クライアントで組む。
        spa: {
          enabled: true,
          prerender: {
            // Tauri は frontendDist の直下に index.html を求める。
            outputPath: "/index.html",
          },
        },
        // **画面ごとに殻を出しておく。**
        // Tauri はファイルをそのまま配るので、URL に対応する html が無いと
        // 再読み込みで 404 になる。**普段は画面遷移がクライアント側で完結するので
        // 表に出ないが、Cmd+R 一発で見える。**
        pages: [{ path: "/" }, { path: "/record" }],
        // **黙って諦めさせない。**
        // Start は `ssr` 環境が走らせられないと判断すると、middleware を入れずに戻る。
        // そうなると `/` が 404 になり、**画面が「Cannot GET /」だけになる**（実際になった）。
        // 明示的に立てておけば、同じことが起きたときに起動時点で理由付きで落ちる。
        vite: { installDevServerMiddleware: true },
      }),
      viteReact(),
    ]) ?? [],
});

export default config;
