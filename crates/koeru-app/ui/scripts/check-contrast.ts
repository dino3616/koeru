/**
 * 配色のコントラストを検査する（TR-PLT-25、TR-PLT-28）。
 *
 * **globals.css の意味づけと、この表を1対1で保つ。**
 * 段を選び直したらここも直す。直さなければ、明暗どちらかで AA を割ったまま出る。
 *
 * 値は `@radix-ui/colors` の CSS から読む。**手で写さない。**
 * Radix の版が上がって段がずれたら、ここが落ちる。
 *
 * WCAG 2.2 Level AA:
 * - 文字は 4.5:1（大きい文字は 3:1）
 * - UI 部品と図形は 3:1 — **波形と境界線も対象**（TR-PLT-28）
 */

import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(import.meta.url);

type Theme = "light" | "dark";
type Scale = Record<Theme, Record<number, string>>;

const readScale = (name: string): Scale => {
  const out: Scale = { light: {}, dark: {} };
  for (const [file, theme] of [
    [`${name}.css`, "light"],
    [`${name}-dark.css`, "dark"],
  ] as const) {
    const path = require.resolve(`@radix-ui/colors/${file}`);
    // p3 のブロックは後ろにある。**16進の先頭ブロックだけを読む。**
    const head = readFileSync(path, "utf8").split("@supports")[0] ?? "";
    for (const m of head.matchAll(new RegExp(`--${name}-(\\d+):\\s*(#[0-9a-f]{6})`, "g"))) {
      const step = Number(m[1]);
      const hex = m[2];
      if (hex !== undefined) out[theme][step] = hex;
    }
  }
  return out;
};

const SCALES = {
  slate: readScale("slate"),
  cyan: readScale("cyan"),
  red: readScale("red"),
  jade: readScale("jade"),
  amber: readScale("amber"),
};

/** `"cyan-11"` のような指定を実際の色にする。 */
const resolve = (token: string, theme: Theme): string => {
  const [name, step] = token.split("-");
  const scale = SCALES[name as keyof typeof SCALES];
  if (scale === undefined) throw new Error(`知らない色 ${token}`);
  const hex = scale[theme][Number(step)];
  if (hex === undefined) throw new Error(`知らない段 ${token}`);
  return hex;
};

const luminance = (hex: string): number => {
  const raw = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16) / 255);
  const lin = raw.map((c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * (lin[0] ?? 0) + 0.7152 * (lin[1] ?? 0) + 0.0722 * (lin[2] ?? 0);
};

const ratio = (a: string, b: string): number => {
  const [la, lb] = [luminance(a), luminance(b)];
  return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
};

/** globals.css の意味づけ。**ここが正本の写し。** */
const TOKENS = {
  bg: "slate-1",
  surface: "slate-2",
  "surface-2": "slate-3",
  "surface-3": "slate-4",
  border: "slate-6",
  "border-strong": "slate-11",
  focus: "cyan-11",
  text: "slate-12",
  "text-dim": "slate-11",
  "accent-solid": "cyan-11",
  "accent-ink": "slate-1",
  "danger-solid": "red-11",
  "danger-ink": "slate-1",
  "danger-text": "red-11",
  "danger-surface": "red-3",
  "ok-fill": "jade-11",
  "quiet-fill": "slate-11",
  wave: "cyan-11",
  "wave-clip": "red-11",
  boundary: "amber-11",
  "boundary-surface": "amber-3",
} as const;

type TokenName = keyof typeof TOKENS;

/** 確かめる組み合わせ。`need` は WCAG が要る比。 */
const PAIRS: Array<{ what: string; fg: TokenName; bg: TokenName; need: number }> = [
  { what: "本文 / 地", fg: "text", bg: "bg", need: 4.5 },
  { what: "本文 / 面", fg: "text", bg: "surface", need: 4.5 },
  { what: "薄い字 / 地", fg: "text-dim", bg: "bg", need: 4.5 },
  { what: "薄い字 / 面", fg: "text-dim", bg: "surface", need: 4.5 },
  { what: "薄い字 / 面2", fg: "text-dim", bg: "surface-2", need: 4.5 },
  { what: "本文 / 面2", fg: "text", bg: "surface-2", need: 4.5 },
  { what: "主ボタンの字 / 塗り", fg: "accent-ink", bg: "accent-solid", need: 4.5 },
  { what: "危険ボタンの字 / 塗り", fg: "danger-ink", bg: "danger-solid", need: 4.5 },
  { what: "警告文 / 警告の面", fg: "danger-text", bg: "danger-surface", need: 4.5 },
  { what: "警告文 / 面", fg: "danger-text", bg: "surface", need: 4.5 },
  // ここから下は UI 部品と図形。**3:1**（TR-PLT-28 は波形と境界線も対象）。
  { what: "波形 / 面2", fg: "wave", bg: "surface-2", need: 3.0 },
  { what: "割れた波形 / 面2", fg: "wave-clip", bg: "surface-2", need: 3.0 },
  { what: "フォーカス環 / 地", fg: "focus", bg: "bg", need: 3.0 },
  { what: "フォーカス環 / 面", fg: "focus", bg: "surface", need: 3.0 },
  { what: "フォーカス環 / 面2", fg: "focus", bg: "surface-2", need: 3.0 },
  { what: "強い境界 / 面", fg: "border-strong", bg: "surface", need: 3.0 },
  { what: "メーターの良 / 面2", fg: "ok-fill", bg: "surface-2", need: 3.0 },
  { what: "メーターの割れ / 面2", fg: "danger-solid", bg: "surface-2", need: 3.0 },
  { what: "メーターの弱 / 面2", fg: "quiet-fill", bg: "surface-2", need: 3.0 },
  // 自動原音設定の境界（TR-ALN-33）。**線と、その線が乗る下地の両方。**
  { what: "境界線 / 面2", fg: "boundary", bg: "surface-2", need: 3.0 },
  { what: "境界線 / 切り出し域", fg: "boundary", bg: "boundary-surface", need: 3.0 },
  // **波形は切り出し域の上にも乗る。** 面2 だけ見ていると、そこで消える。
  { what: "波形 / 切り出し域", fg: "wave", bg: "boundary-surface", need: 3.0 },
];

let failed = 0;
const lines: string[] = [];

for (const theme of ["light", "dark"] as const) {
  lines.push(`── ${theme === "light" ? "明るい面" : "暗い面"} ──`);
  for (const p of PAIRS) {
    const got = ratio(resolve(TOKENS[p.fg], theme), resolve(TOKENS[p.bg], theme));
    const ok = got >= p.need;
    if (!ok) failed += 1;
    lines.push(
      `  ${ok ? "OK  " : "NG  "}${p.what.padEnd(22)}${got.toFixed(2)} : 1（要 ${p.need}）`,
    );
  }
}

process.stdout.write(`${lines.join("\n")}\n`);
if (failed > 0) {
  process.stderr.write(`\n**${failed} 件が WCAG 2.2 AA を割っている。**\n`);
  process.exit(1);
}
process.stdout.write("\nすべて WCAG 2.2 AA を満たす。\n");
