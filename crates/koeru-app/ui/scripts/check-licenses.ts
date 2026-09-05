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

const ROOT = new URL("../node_modules", import.meta.url).pathname;

type Pkg = { name: string; license: string | null };

/*
 * `node_modules/＊/package.json` と `node_modules/@＊/＊/package.json` を挙げる。
 *
 * 深さを2段までに切る。 入れ子の `node_modules`（版が衝突したときにできる）は
 * 数えない——同じパッケージを二度数えることになり、件数が実態と合わなくなる。
 */
const manifests = [
  ...new Bun.Glob("*/package.json").scanSync({ cwd: ROOT, absolute: true }),
  ...new Bun.Glob("@*/*/package.json").scanSync({ cwd: ROOT, absolute: true }),
].sort();

const packages: Pkg[] = [];
for (const path of manifests) {
  const j = (await Bun.file(path).json()) as {
    name?: string;
    license?: unknown;
    licenses?: unknown;
  };
  // 古い形は `licenses: [{ type }]`。
  const legacy = Array.isArray(j.licenses)
    ? j.licenses
        .map((x) =>
          typeof x === "object" && x !== null ? String((x as { type?: unknown }).type) : String(x),
        )
        .join(" OR ")
    : null;
  const license = typeof j.license === "string" ? j.license : legacy;
  packages.push({ name: j.name ?? path, license });
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
