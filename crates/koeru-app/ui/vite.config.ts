import { fileURLToPath } from "node:url";
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
    jsPlugins: [
      { name: "vite-plus", specifier: "vite-plus/oxlint-plugin" },
      { name: "shadcn", specifier: "@shadcn/lint" },
    ],
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
      /*
       * デザインシステムの規則を機械で見る。
       * テーマは `components.json` が指す `src/styles/globals.css` から読む
       * ——段の名前も自前のクラスも、そこを辿って認識される。
       *
       * いま違反が無いものだけ入れてある。`no-arbitrary-values` と
       * `no-inline-styles` は違反が残っているので、直す範囲を決めてから入れる。
       */
      "shadcn/no-unknown-classes": "error",
      "shadcn/no-raw-colors": "error",
      /*
       * 部品へ注入してよいクラス。 既定は「何も通さない」で、
       * 通すものを部品ごとに列挙する。
       *
       * `Button` に通すのは、置く側でなければ決められないものだけ——
       * 幅と、flex の中での振る舞い。 高さと内側の余白は通さない
       * （`TR-PLT-31` の操作対象の大きさを呼び出し側が壊せる）。
       *
       * **幅は語で許し、値では許さない。** `w-*` を開けると `w-0` が通り、
       * `cn` の後勝ちで部品側の幅が消えて、24 CSS ピクセルを割る的ができる。
       * 高さだけ塞いでも `TR-PLT-31` は守れない。`min-w-*` と `max-w-*` も
       * 同じ理由で通さない（`max-w-0`、`min-w-0`）。
       *
       * `flex-1` と `grow-*` / `shrink-*` は残す。 縮んでも min-content
       * ——文字と `px` の分——より下へは行かない。下限を外せるのは `min-w-0` で、
       * それは上で塞いである。
       */
      "shadcn/no-restyle": [
        "error",
        {
          contracts: [
            {
              pattern: "^Button$",
              allow: [
                "w-full",
                "w-fit",
                "w-auto",
                "flex-1",
                "grow-*",
                "shrink-*",
                "self-*",
                "order-*",
              ],
            },
          ],
        },
      ],
      "shadcn/require-static-classes": "error",
      "shadcn/no-inline-styles": "error",
      /*
       * 任意値は尺度で書けるなら書く。 `w-[640px]` は `w-160` と同じ値で、
       * 尺度から外れた値だけが残るようにしておかないと、
       * 「4px 刻みのどこか」が読む側に分からなくなる。
       *
       * ここに並ぶのは尺度が持っていない形。 段組みの `auto`、
       * 画面高に対する割合、字間、環の影、`koeru-breath` の遅れ。
       */
      "shadcn/no-arbitrary-values": [
        "error",
        {
          allow: [
            "grid-cols-[auto_1fr]",
            "max-h-[40vh]",
            "tracking-[0.18em]",
            "tracking-[0.22em]",
            "shadow-[inset_0_-2px_0_var(--slate-12)]",
            "[animation-delay:0.3s]",
            "[animation-delay:0.6s]",
          ],
        },
      ],
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
      /*
       * `pathname` で済ませない。
       *
       * チェックアウト先に空白や非 ASCII が入ると `%20` のまま返り、
       * Windows では `/C:/…` になる。どちらもファイル系の実体ではないので、
       * `~/…` の解決が静かに外れる。`fileURLToPath` が実体へ直す。
       *
       * ここだけ `node:url` を引く。 設定ファイルを読むのは vite-plus で、
       * Bun ではない——`Bun.fileURLToPath` は「Bun is not defined」で落ちる。
       */
      "~": fileURLToPath(new URL("./src", import.meta.url)),
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
        pages: [{ path: "/" }, { path: "/voice" }, { path: "/take" }],
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
