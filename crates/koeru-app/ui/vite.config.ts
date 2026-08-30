import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
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
      }),
      viteReact(),
    ]) ?? [],
});

export default config;
