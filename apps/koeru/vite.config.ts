import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
import { fileURLToPath } from "node:url";
import { defineConfig, lazyPlugins } from "vite-plus";

// **Tauri が繋ぐポート。** crates/koeru-app/tauri.conf.json の devUrl と揃える。
const DEV_PORT = 1420;

const config = defineConfig({
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
      }),
      viteReact(),
    ]) ?? [],
});

export default config;
