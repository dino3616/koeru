/*
 * npm 側の依存ライセンスを検査する。
 *
 * Rust 側は `cargo deny check` が見るが、`node_modules` は誰も見ていなかった。
 * KOERU は AGPL-3.0-or-later なので、取り込めるものだけに限る。
 *
 * `deny.toml` と同じ方式にする。 許可リストを1箇所に置き、そこに無いものは通さない。
 * ディレクトリの中身から推測しない——推測すると、名乗らないパッケージが黙って通る。
 */
/*
 * ファイルの読みと走査は Bun の API を使う。 `node:fs` は Bun の互換層で、
 * ここは `bun run` で走るので直接 Bun を呼べる。
 */

/**
 * AGPL-3.0-or-later に取り込める識別子。
 *
 * BlueOak-1.0.0 は Blue Oak Model License 1.0.0。 許諾的で、特許条項を含む。
 * OSI 承認済み（2020年）。Storybook が引く glob / minimatch などが使っている。
 *
 * MPL-2.0 は §3.3 の secondary license 条項で GPL 系と両立する。
 * Python-2.0 と CC-BY-4.0 は許諾的で、FSF も GPL 互換としている
 * （CC-BY はソフトウェアには推奨されないが、`caniuse-lite` はデータ表）。
 */
const ALLOWED = new Set([
  "0BSD",
  "Apache-2.0",
  "Apache-2.0 OR MIT",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "BlueOak-1.0.0",
  "CC-BY-4.0",
  "CC0-1.0",
  "ISC",
  "MIT",
  "MIT OR Apache-2.0",
  "MPL-2.0",
  "Python-2.0",
  "Unlicense",
]);

/**
 * ライセンス欄を持たないことを承知で通す、親パッケージの一族。
 *
 * OS 別のバイナリは欄を持たないことがあり、条件は親が名乗っている。
 * 前置きで許す。 `@yuku-parser/binding-darwin-arm64` のように1つずつ挙げると、
 * 走らせた OS のぶんだけ通って他が落ちる——手元は darwin、CI は linux なので、
 * 手元で通ったものが CI で落ちる。一度そうなった。
 *
 * 親の名前で許す。 `@yuku-*` を丸ごと通すのではなく、
 * `binding-` が付いたものだけに限る。
 */
const NO_FIELD_OK: readonly { prefix: string; why: string }[] = [
  { prefix: "@yuku-codegen/binding-", why: "@yuku-codegen（MIT）の OS 別バイナリ" },
  { prefix: "@yuku-parser/binding-", why: "@yuku-parser（MIT）の OS 別バイナリ" },
];

// `pathname` にしない。空白や非 ASCII が `%20` のまま残る。
const ROOT = Bun.fileURLToPath(new URL("../node_modules", import.meta.url));

type Pkg = { name: string; license: string | null };

/*
 * インストール済みのすべての manifest を挙げる。
 *
 * 直下の2段だけを見ない。 版が衝突すると bun は入れ子の `node_modules` へ
 * 実体を置く。手元では 27 件あり、そこは丸ごと検査から漏れていた——
 * 版が違えばライセンスも違いうるので、漏れたぶんは素通りする。
 *
 * 名前と版で重複を落とす。 同じ実体が複数の場所に居ることはあるが、
 * 版が違えば別物として数える。
 */
const manifests = [...new Bun.Glob("**/package.json").scanSync({ cwd: ROOT, absolute: true })]
  /*
   * 配られていないものを外す。
   *
   * パッケージが自分の試験用に置いた固定物（`resolve/test/resolver/baz`）や、
   * 雛形（`vite-plus-*-template`）は、依存として解決されたものではない。
   * 数えると、実体の無いものにライセンスを求めることになる。
   *
   * 判定は「パスに `node_modules` 以外の段が挟まっているか」。
   * 本物の依存は必ず `node_modules/<名前>/package.json` の形で置かれ、
   * 入れ子でも `node_modules/…/node_modules/<名前>/package.json` になる。
   */
  .filter((path) => {
    const rel = path.slice(ROOT.length + 1, -"/package.json".length);
    const segments = rel.split("/");
    // スコープ付きは1段深い。`node_modules` で区切って、各区間を見る。
    for (const part of rel.split("node_modules/")) {
      const depth = part.split("/").filter((x) => x !== "").length;
      if (depth > 2) return false;
    }
    return segments.length > 0;
  })
  .sort();

const seen = new Set<string>();
const packages: Pkg[] = [];

for (const path of manifests) {
  let j: { name?: string; version?: string; license?: unknown; licenses?: unknown };
  try {
    j = (await Bun.file(path).json()) as typeof j;
  } catch {
    // 壊れた manifest（型定義だけの入れ物など）は数えない。
    continue;
  }
  // 名前を名乗らないものは package ではない。
  if (typeof j.name !== "string") continue;

  const key = `${j.name}@${j.version ?? "?"}`;
  if (seen.has(key)) continue;
  seen.add(key);

  // 古い形は `licenses: [{ type }]`。
  const legacy = Array.isArray(j.licenses)
    ? j.licenses
        .map((x) =>
          typeof x === "object" && x !== null ? String((x as { type?: unknown }).type) : String(x),
        )
        .join(" OR ")
    : null;
  const license = typeof j.license === "string" ? j.license : legacy;
  packages.push({ name: j.name, license });
}

const bad: string[] = [];
for (const p of packages) {
  if (p.license === null) {
    if (!NO_FIELD_OK.some(({ prefix }) => p.name.startsWith(prefix))) {
      bad.push(`${p.name}: ライセンス欄が無い`);
    }
    continue;
  }
  if (!ALLOWED.has(p.license)) bad.push(`${p.name}: ${p.license}`);
}

const counts = new Map<string, number>();
for (const p of packages) {
  const k = p.license ?? "(無し)";
  counts.set(k, (counts.get(k) ?? 0) + 1);
}

console.log(`── npm のライセンス ── ${packages.length} パッケージ`);
for (const [k, v] of [...counts].sort((a, b) => b[1] - a[1])) {
  console.log(`  ${String(v).padStart(4)}  ${k}`);
}

if (bad.length > 0) {
  console.error(`\n許可リストに無いライセンスが ${bad.length} 件。`);
  for (const b of bad) console.error(`  NG  ${b}`);
  console.error("\nAGPL-3.0-or-later に取り込めるか確認し、通すなら ALLOWED か");
  console.error("NO_FIELD_OK へ理由つきで足す。黙って通さない。");
  process.exit(1);
}
console.log("\nすべて AGPL-3.0-or-later に取り込める。");
