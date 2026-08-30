import { defineConfig } from "vite-plus";

/*
 * リポジトリ全体の TypeScript / CSS 検査。
 *
 * **見るのは apps/ と packages/ だけ。**
 * このリポジトリの正本は Markdown と TOML に多くあり（docs/、meta/、specs/）、
 * そこには別の規律が掛かっている。
 *
 * - `docs/generated/` は FSL から決定論的に生成する。**整形すると drift 検出が落ちる**
 * - `meta/` の TOML は `cargo xtask check-meta` が読む。配列の畳み方が変わると差分が濁る
 * - `crates/` は `cargo fmt` の担当
 *
 * **範囲を絞らないと、これらを黙って書き換える。実際に一度やった。**
 */
const OUTSIDE_THE_FRONTEND = [
  "docs/**",
  "meta/**",
  "specs/**",
  "crates/**",
  "xtask/**",
  ".agents/**",
  ".claude/**",
  ".github/**",
  "target/**",
  "*.md",
];

const config = defineConfig({
  staged: {
    "{apps,packages}/**/*.{ts,tsx,css,json}": "vp check --fix",
  },
  fmt: {
    ignorePatterns: OUTSIDE_THE_FRONTEND,
  },
  lint: {
    ignorePatterns: OUTSIDE_THE_FRONTEND,
    jsPlugins: [{ name: "vite-plus", specifier: "vite-plus/oxlint-plugin" }],
    rules: { "vite-plus/prefer-vite-plus-imports": "error" },
    options: { typeAware: true, typeCheck: true },
  },
});

export default config;
