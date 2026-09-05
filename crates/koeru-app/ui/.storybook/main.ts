import type { StorybookConfig } from "@storybook/react-vite";

/*
 * Storybook の設定。
 *
 * story が各部品のアクセシビリティ検査の範囲を決める（`TR-PLT-25`）。
 * `addon-a11y` が axe-core を story ごとに当て、`addon-vitest` が
 * それを実ブラウザで CI から走らせる（`DEC-PLT-022`）。
 *
 * `addon-vitest` を `addons` に必ず載せる。 これが tester を配線する。
 * `vitest.stories.config.ts` に plugin を書いただけでは足りず、
 * 外すと iframe が起動しないまま 60 秒で諦める。一度そうなった。
 *
 * ビルダは `vite-plus`。 Storybook 10 の optional peer は `^0.1.15 || ^0.2.0` で、
 * このリポジトリは 0.3.0。動くことを確かめて採っている。
 */
const config: StorybookConfig = {
  stories: ["../src/**/*.stories.tsx"],
  addons: ["@storybook/addon-a11y", "@storybook/addon-vitest"],
  framework: { name: "@storybook/react-vite", options: {} },
  // 使う人向けの説明を書く場所ではない。部品の検査と目視のためだけに立てる。
  docs: { defaultName: "説明" },

  /*
   * TanStack のプラグインを外す。
   *
   * Storybook は自分の入口（`vite-inject-mocker-entry.js`）を足すので、
   * Start のマニフェスト生成が「入口が複数ある」と言って落ちる。
   * ここに要るのは部品を描くことだけで、ルーティングも SSR も関係が無い。
   *
   * 前置きは1つではない。 `tanstack-react-start:` `tanstack-start-core:`
   * `tanstack-start:` `tanstack-router:` `tanstack:` が混ざる。
   * 1つだけ弾くと、残りが同じ理由で落とす。
   *
   * 配列は入れ子になっている（`lazyPlugins` が包む）ので平らにしてから見る。
   *
   * 名前で外す。 `vite.config.ts` の側に Storybook 用の分岐を置くと、
   * アプリの設定が検査の都合で歪む。
   */
  viteFinal: (config) => ({
    ...config,
    plugins: (config.plugins ?? []).flat(9).filter((p) => {
      const name =
        p !== null && typeof p === "object" && "name" in p
          ? String((p as { name: unknown }).name)
          : "";
      return !name.startsWith("tanstack");
    }),
  }),
};

export default config;
