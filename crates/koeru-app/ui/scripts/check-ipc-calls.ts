/*
 * `api` の呼び出しを、問い合わせと指示の口に閉じ込める（`DEC-PLT-023`）。
 *
 * 見るのは1つだけ。 `api.…()` に `.then` / `.catch` / `.finally` を
 * 繋いでいないか。繋いでいたら、その場で「走っている最中か」「結果」
 * 「失敗した」を state で持ち直しているということで、どれか1つを
 * 消し忘れる形になる——成功したのに前の赤字が残る、という不具合が実際に出た。
 *
 * `queryFn` と `mutationFn` の中では繋がない。 約束をそのまま返せばよく、
 * 状態は TanStack Query が持つ。だから「繋いでいる＝手で持ち直している」で
 * ほぼ言い切れて、構文木を組まずに見つけられる。
 *
 * `await` は見ない。 順序が要る手続き（収録の状態機械）は `await` で書くのが
 * 自然で、そこは state を持ち直していない。
 */
// `pathname` にしない。空白や非 ASCII が `%20` のまま残る。
const SRC = Bun.fileURLToPath(new URL("../src", import.meta.url));

/** 繋いでよいもの。理由を書いて足す。 */
const EXEMPT = new Map([
  [
    "src/components/live-waveform/index.tsx",
    "Channel の開閉（`DEC-PLT-017`）。問い合わせではないので、取り直しも重複排除も意味を持たない",
  ],
  [
    "src/components/take-inspector/index.tsx",
    "描画要求の幅を canvas の実測から決めるので、鍵を描画前に作れない。順序は世代番号で捨てる",
  ],
  [
    "src/components/song-list/index.tsx",
    "待ち数の予約（`DEC-PLT-017`）。返ってきてから次を予約する形を、間隔ではなく完了で回す",
  ],
]);

/** `api.name(` の引数を閉じる位置を返す。 */
const closeOf = (text: string, open: number): number => {
  let depth = 0;
  for (let i = open; i < text.length; i += 1) {
    const c = text[i];
    if (c === "(") depth += 1;
    else if (c === ")") {
      depth -= 1;
      if (depth === 0) return i;
    }
  }
  return -1;
};

const files = [...new Bun.Glob("**/*.{ts,tsx}").scanSync({ cwd: SRC })]
  .filter((f) => !f.endsWith(".story.tsx"))
  // 境界そのものと、story 用の差し替えは対象外。
  .filter((f) => f !== "lib/ipc.ts" && f !== "lib/ipc.mock.ts")
  .sort();

const found: string[] = [];
let scanned = 0;

for (const rel of files) {
  const path = `src/${rel}`;
  const source = await Bun.file(`${SRC}/${rel}`).text();
  if (!source.includes("api.") && !/\bapi\s*\n/.test(source)) continue;
  scanned += 1;
  if (EXEMPT.has(path)) continue;

  // `api.foo(` と、改行で折り返した `api\n  .foo(` の両方。
  for (const m of source.matchAll(/\bapi\s*(?:\n\s*)?\.\s*([a-zA-Z][\w]*)\s*\(/g)) {
    const open = m.index + m[0].length - 1;
    const close = closeOf(source, open);
    if (close < 0) continue;
    const after = source.slice(close + 1).match(/^\s*\.\s*(then|catch|finally)\b/);
    if (after === null) continue;
    const line = source.slice(0, m.index).split("\n").length;
    found.push(`${path}:${line}  api.${m[1]} … .${after[1]}`);
  }
}

console.log(`── ipc ── \`api\` を呼ぶファイル ${scanned} 件 / 免除 ${EXEMPT.size} 件`);

if (found.length > 0) {
  console.error(`\n\`api\` に約束を繋いでいる箇所が ${found.length} 件。`);
  for (const f of found) console.error(`  NG  ${f}`);
  console.error(
    "\n押して走るものは `useMutation`、読みは `~/lib/queries` へ。" +
      "\nどちらでもないなら EXEMPT へ理由つきで足す。",
  );
  process.exit(1);
}
console.log("\n`api` はすべて問い合わせと指示の口を通っている。");
