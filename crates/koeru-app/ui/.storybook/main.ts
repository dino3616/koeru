import { fileURLToPath } from "node:url";
import type { StorybookConfig } from "@storybook/react-vite";

/*
 * Storybook の設定。
 *
 * story が各部品のアクセシビリティ検査の範囲を決める（`TR-PLT-25`）。
 * `addon-a11y` が axe-core を story ごとに当て、`addon-vitest` が
 * それを実ブラウザで CI から走らせる（`DEC-PLT-022`）。
 *
 * `addon-vitest` を `addons` に必ず載せる。 これが tester を配線する。
 * `vitest.story.config.ts` に plugin を書いただけでは足りず、
 * 外すと iframe が起動しないまま 60 秒で諦める。一度そうなった。
 *
 * ビルダは `vite-plus`。 Storybook 10 の optional peer は `^0.1.15 || ^0.2.0` で、
 * このリポジトリは 0.3.0。動くことを確かめて採っている。
 */
const config: StorybookConfig = {
  stories: ["../src/**/*.story.tsx"],
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
    /*
     * Rust 境界を story 用のものへ差し替える。
     *
     * Storybook に Tauri は無いので、`invoke` は必ず失敗する。
     * `~/lib/ipc.mock.ts` が代わりに読まれ、story が返り値を決める。
     *
     * `sb.mock` は使わない。 あれは対象のモジュールを変換して包むので、
     * `ipc.ts` の `export type … from` が値の再輸出として解決され、
     * 「`AppError` という輸出は無い」で落ちる。別名なら変換を通らない。
     *
     * `ipc.mock.ts` の中の `./ipc` は相対なので、この別名に当たらない。
     * だから本物を読める——循環しない。
     */
    resolve: {
      ...config.resolve,
      alias: {
        /*
         * 狭いほうを先に置く。 object 形式の alias は挿入順の前方一致で、
         * 最初に当たったものが勝つ。開発サーバは `~` を alias に入れて
         * 渡してくるので、後ろへ置くと `~/lib/ipc` は `~` に食われて
         * 本物の `ipc.ts` へ解決される。story は `mocked()` に素の関数を渡し、
         * `mockResolvedValue is not a function` で描画に失敗する。踏んだ。
         *
         * 試験側（`vitest.story.config.ts`）は `~` を alias に持たず
         * `tsconfigPaths` で解決するので、順序を間違えても当たっていた。
         * 検査は緑のまま、目視だけが死ぬ。
         */
        // `pathname` にしない。空白や非 ASCII、Windows のドライブ文字で外れる。
        // `node:url` を引くのは、設定を読むのが Bun ではないから。
        "~/lib/ipc": fileURLToPath(new URL("../src/lib/ipc.mock.ts", import.meta.url)),
        ...(config.resolve?.alias as Record<string, string> | undefined),
      },
    },
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
