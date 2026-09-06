import tailwindcss from "@tailwindcss/vite";
import { storybookTest } from "@storybook/addon-vitest/vitest-plugin";
// 試験の設定は `vite-plus/test/config` から取る。 `vite-plus` の
// `defineConfig` は Vite の設定用で、`test.browser` の型を持たない。
import { defineConfig } from "vite-plus/test/config";
// vite-plus が再輸出しているものを使う。 `@vitest/browser-playwright` を
// 直接引くと、`vp check --fix` がこちらの import を自動で足して識別子が衝突し、
// 設定が読めなくなる——tester が起動しないまま 60 秒で諦める。踏んだ。
import { playwright } from "vite-plus/test/browser-playwright";

export default defineConfig({
  plugins: [tailwindcss(), storybookTest({ configDir: ".storybook" })],
  resolve: { tsconfigPaths: true },
  optimizeDeps: { include: ["react", "react/jsx-dev-runtime", "react-dom", "react-dom/client"] },
  test: {
    name: "storybook",
    browser: {
      enabled: true,
      headless: true,
      // provider は実体を渡す。 文字列は 4.1 で受け付けなくなっている。
      provider: playwright(),
      instances: [{ browser: "chromium" }],
    },
  },
});
