/*
 * すべての部品に story があることを検査する。
 *
 * 検査範囲は story の範囲そのもの（`DEC-PLT-022`）。 story を書き忘れた部品は
 * axe に一度も当たらないまま通る——「検査が緑」と「検査した」が食い違う。
 * 書き忘れを人の記憶に頼らない。
 *
 * 部品とみなすのは、大文字で始まる名前を輸出している `.tsx`。
 * `~/lib` の関数やフックは対象外——描くものではないので story を持てない。
 */
// `pathname` にしない。空白や非 ASCII が `%20` のまま残る。
const SRC = Bun.fileURLToPath(new URL("../src", import.meta.url));

/** story を持たなくてよいもの。理由を書いて足す。 */
const EXEMPT = new Map([
  ["src/routes/__root.tsx", "ルータの殻。`<html>` ごと出すので story にならない"],
  ["src/routes/index.tsx", "経路の宣言だけ。画面は screens 側の story が見る"],
  ["src/routes/record.tsx", "経路の宣言だけ。画面は screens 側の story が見る"],
  ["src/lib/story-router.tsx", "story のための道具。それ自身は部品ではない"],
]);

const files = [...new Bun.Glob("**/*.tsx").scanSync({ cwd: SRC })]
  .filter((f) => !f.endsWith(".story.tsx"))
  .sort();

const missing: string[] = [];
let checked = 0;

for (const rel of files) {
  const path = `src/${rel}`;
  if (EXEMPT.has(path)) continue;

  const source = await Bun.file(`${SRC}/${rel}`).text();
  // 大文字で始まる名前を輸出していなければ、描くものではない。
  if (!/export (const|class|function) [A-Z]/.test(source)) continue;

  checked += 1;
  /*
   * 部品は `<名前>/index.tsx`、story は `<名前>/<名前>.story.tsx`。
   *
   * `index.tsx` の隣に `index.story.tsx` を置かない。 Storybook の一覧も
   * エディタのタブも `index` だらけになって、どれがどれか分からなくなる。
   */
  const dir = rel.slice(0, rel.lastIndexOf("/"));
  const name = dir.slice(dir.lastIndexOf("/") + 1);
  const story = `${SRC}/${dir}/${name}.story.tsx`;
  if (!(await Bun.file(story).exists())) missing.push(path);
}

console.log(`── story ── 部品 ${checked} 件 / 免除 ${EXEMPT.size} 件`);

if (missing.length > 0) {
  console.error(`\nstory が無い部品が ${missing.length} 件。`);
  for (const m of missing) console.error(`  NG  ${m}`);
  console.error("\n`<名前>/<名前>.story.tsx` を置く。描かないものなら EXEMPT へ理由つきで足す。");
  process.exit(1);
}
console.log("\nすべての部品に story がある。");
