import tailwindcss from "@tailwindcss/vite";
import { tanstackStart } from "@tanstack/react-start/plugin/vite";
import viteReact from "@vitejs/plugin-react";
// ここは `vite` から取る。`vite-plus` からではない。
//
// TanStack Start は `vite` の `isRunnableDevEnvironment` で環境を見分ける。
// `vite-plus` が再輸出する `createRunnableDevEnvironment` は**別のクラス**を作るので、
// そちらで作ると Start からは「走らせられない環境」に見え、
// middleware が入らず `/` が 404 になる。 実際にそうなった。
// oxlint-disable-next-line vite-plus/prefer-vite-plus-imports
import { createRunnableDevEnvironment } from "vite";
import { defineConfig, lazyPlugins } from "vite-plus";

/*
 * WebView 側の設定。**Rust と同じ crate の中に置いてある。**
 *
 * `koeru-app` はアプリケーション層で、Tauri のコマンドと画面は一体のもの。
 * 別のワークスペースに切ると、コマンドを1つ足すたびに2箇所を行き来することになる。
 *
 * 整形と lint の範囲はこのディレクトリの中だけ。 `docs/generated/` は FSL から
 * 決定論的に生成していて CI が drift を見ているし、`meta/` の TOML は check-meta が読む。
 * ここから外へ出ると、その両方を黙って書き換える。一度やった。
 */

// Tauri が繋ぐポート。 ../tauri.conf.json の devUrl と揃える。
const DEV_PORT = 1420;

/*
 * 生成物。整形も lint もしない。
 *
 * `routeTree.gen.ts` は TanStack Router、`bindings.gen.ts` は
 * Rust のコマンド定義（`DEC-PLT-019`）から出る。 型検査には要るので
 * リポジトリに置くが、手で直す対象ではない——直しても次の生成で消える。
 * 古くなっていないかは `cargo test -p koeru-app --test bindings` が見る。
 */
const GENERATED = ["src/routeTree.gen.ts", "src/lib/bindings.gen.ts"];

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
    /*
     * react と jsx-a11y は oxlint の既定でオフ。**入れる。**
     * このリポジトリは a11y を `TR-PLT-25` / `28` / `29` で要求しているので、
     * 手で見るものにしておく理由が無い。
     */
    plugins: ["typescript", "react", "jsx-a11y", "import", "promise"],
    rules: {
      "vite-plus/prefer-vite-plus-imports": "error",
      // 落ちるべきものは落とす。警告のままだと CI が素通りする。
      "react/set-state-in-effect": "error",
      "react/exhaustive-deps": "error",
      "jsx-a11y/alt-text": "error",
      "jsx-a11y/aria-props": "error",
      "jsx-a11y/role-has-required-aria-props": "error",
      /*
       * canvas には効かない。`<canvas role="img">` は仕様どおりの書き方で、
       * `<img>` へは置き換えられない（描画面が要る）。
       * 3つの canvas すべてがこれに当たるので、行ごとの例外ではなく規則を切る。
       */
      "jsx-a11y/prefer-tag-over-role": "off",
      /*
       * 汎用の見出し部品（`CardTitle`）は children を素通しするので、
       * 定義だけを見ると中身が無いように映る。呼び出し側では必ず文言が入る。
       */
      "jsx-a11y/heading-has-content": "off",
      // 型を緩める書き方を止める。いま違反0件なので、入れる費用が実質ゼロ。
      "typescript/no-explicit-any": "error",
      "typescript/no-non-null-assertion": "error",
      "typescript/switch-exhaustiveness-check": "error",
    },
    options: { typeAware: true, typeCheck: true },
  },
  /*
   * SSR 環境を「走らせられる」形で作る。
   *
   * TanStack Start の dev サーバは、`ssr` 環境の中でサーバ入口を実行して HTML を返す。
   * vite-plus の既定の `ssr` 環境はそれができない形なので、
   * **Start は黙って middleware を入れず、`/` が 404 になる**（実際になった）。
   *
   * Tauri の中にサーバは無いので SSR はしないが、dev で画面を出すのにこの環境が要る。
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
      // `node:url` を引かない。 `URL` は標準で、`pathname` で足りる。
      "~": new URL("./src", import.meta.url).pathname,
    },
  },
  // 外へ出ない。 処理はローカル完結で、声をサーバへ送らない。
  server: {
    port: DEV_PORT,
    strictPort: true,
    host: "127.0.0.1",
  },
  plugins:
    lazyPlugins(() => [
      tailwindcss(),
      tanstackStart({
        // SSR を持たない。 Tauri の中にサーバは無いので、
        // 起動時に配るのは殻だけにして、あとは全部クライアントで組む。
        spa: {
          enabled: true,
          prerender: {
            // Tauri は frontendDist の直下に index.html を求める。
            outputPath: "/index.html",
          },
        },
        // 画面ごとに殻を出しておく。
        // Tauri はファイルをそのまま配るので、URL に対応する html が無いと
        // 再読み込みで 404 になる。普段は画面遷移がクライアント側で完結するので
        // 表に出ないが、Cmd+R 一発で見える。
        pages: [{ path: "/" }, { path: "/record" }],
        // 黙って諦めさせない。
        // Start は `ssr` 環境が走らせられないと判断すると、middleware を入れずに戻る。
        // そうなると `/` が 404 になり、**画面が「Cannot GET /」だけになる**（実際になった）。
        // 明示的に立てておけば、同じことが起きたときに起動時点で理由付きで落ちる。
        vite: { installDevServerMiddleware: true },
      }),
      /*
       * React Compiler を通す（`DEC-PLT-018`）。
       *
       * `useMemo` / `useCallback` / `memo` を手で置かなくても、
       * コンパイラが読み取り専用の依存を見て等価な結果を出す。
       * 手で置くと、依存の書き漏らしが「たまに古い値で描く」形で出る——
       * lint は依存配列の中しか見ないので、置き忘れ自体は誰も言わない。
       *
       * 経路は Rolldown の babel preset。 Babel を全体に掛け直すのではなく、
       * この preset を通る分だけなので、oxc の変換はそのまま残る。
       */
      viteReact({ compiler: true }),
    ]) ?? [],
});

export default config;
