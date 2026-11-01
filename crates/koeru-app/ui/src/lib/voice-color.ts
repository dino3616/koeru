import type { CSSProperties } from "react";

import type { VoiceView } from "~/lib/ipc";

/*
 * 声の色を、面ごとに組み立てて CSS へ渡す（`DEC-PLT-027`）。
 *
 * # Rust が返すのは位置だけ
 *
 * 色相（度）と、彩度・明度の 0〜1 の位置。 幅は面が決める——暗い面と明るい面で
 * 同じ明度を使うと片方で地に沈むし（環は非テキストの図なので 3:1 が要る、
 * `TR-PLT-28`）、明るい面で sRGB に収まる彩度は暗い面より狭い。
 *
 * # 幅をここに置く理由
 *
 * **彩度の上限が色相ごとに違うので、幅と上限を同じ場所に置くしかない。**
 * 上限は「その明度・その色相で sRGB に収まる最大の彩度」で、明度が分からないと
 * 計算できない。CSS には「この色相で収まる最大の彩度」を問う式が無いので、
 * `globals.css` に幅を置いたままでは上限を色相ごとにできなかった。
 *
 * 以前は全色相で一律の上限（明 0.072 / 暗 0.10）だった。 いちばん狭い色相
 * （210 度付近）に合わせた値なので、そこは正しい——**明るい面では
 * それが灰色に見える彩度で、6音源を並べても色の違いがほとんど読めなかった**
 * （`EVID-PLT-002`）。色相ごとの上限にすると、150 度は 0.12、318 度は 0.19 まで
 * 出せる。コントラストは全域で 5:1 を超えたままなので、余裕はここにあった。
 *
 * # 両方の面ぶんを渡す
 *
 * `theme.js` はクラスを付け替えるだけなので、片方だけ焼き込むと切り替えに
 * 追従しない。明暗2つの色を変数として渡し、どちらを使うかは `globals.css` が選ぶ
 * （環は `.dark`、`voice` の役割は `light-dark()`）。
 *
 * # 色が無いときは変数を置かない
 *
 * まだ1つも録れていない音源に声は無い。 そのとき環は1本も描かれないので、
 * 色を決める必要も無い。既定値で塗ると「灰色の声」という状態を作ってしまう。
 *
 * # 本人が選んだイメージカラーが勝つ
 *
 * 11色の対応は `DEC-PLT-062`。 段 11 を Radix から引くだけなので、明暗は
 * Radix の `.dark` がそのまま選ぶ。どちらの帯も通らない。
 */

/** 明度と彩度の幅。 */
type Band = { lightnessLo: number; lightnessHi: number; chromaLo: number };

/**
 * 環の帯。面ごとの明度と彩度の幅。
 *
 * 明度は実測で選んだ。 段 1〜3 のどの面に載せても 3:1 を満たす帯
 * （`TR-PLT-28` の非テキスト）。測っているのは `styles/palette.story.tsx`。
 *
 * 彩度の下端だけ絶対値で持つ。 上端は色相ごとに [`maxChroma`] から出す。
 */
const SURFACES = {
  light: { lightnessLo: 0.42, lightnessHi: 0.58, chromaLo: 0.04 },
  dark: { lightnessLo: 0.62, lightnessHi: 0.8, chromaLo: 0.05 },
} as const satisfies Record<string, Band>;

/**
 * 字を載せる塗りの帯。 主ボタンの塗りは環と別の帯を持つ（`DEC-PLT-062`）。
 *
 * 環の帯のままだと、`voice-ink` の字が 4.5:1 に届かない。 明るい面では上端の
 * 明るい色、暗い面では下端の紫で割る。 色相と、帯の中の位置は環と同じものを使う。
 * 測っているのは `styles/palette.story.tsx`。
 */
const FILLS = {
  light: { lightnessLo: 0.42, lightnessHi: 0.52, chromaLo: 0.04 },
  dark: { lightnessLo: 0.66, lightnessHi: 0.8, chromaLo: 0.05 },
} as const satisfies Record<string, Band>;

/** 本人が選べるイメージカラー。 Radix の色相の名前で持つ。 */
export const IMAGE_COLORS = [
  "lime",
  "green",
  "teal",
  "cyan",
  "sky",
  "blue",
  "indigo",
  "violet",
  "purple",
  "plum",
  "pink",
] as const;

export type ImageColor = (typeof IMAGE_COLORS)[number];

/**
 * sRGB に収まる最大の彩度に対して、どこまで使うか。
 *
 * 端まで使わない。 ブラウザの丸めと、面の合成で端を越えると
 * 画面で潰れ、**声ごとの差がそこで消える**。
 */
const GAMUT_MARGIN = 0.9;

/** 彩度の探索範囲の上端。OKLCH でここより外に sRGB は無い。 */
const CHROMA_CEILING = 0.4;

/** 探索の刻み。これ以下は画面に出る色として区別が付かない。 */
const CHROMA_STEP = 0.001;

/**
 * その明度・その色相で sRGB に収まる最大の彩度。
 *
 * OKLCH → OKLab → 線形 sRGB を自分で解く。 ブラウザに訊く手もあるが、
 * `getComputedStyle` は丸めた後の値を返すので「収まっているか」が分からない
 * ——収まらない色は端に張り付いた形で返ってくる。
 *
 * 二分探索で詰める。 収まる／収まらないは彩度に対して単調なので、
 * 端から順に試す必要は無い。
 */
const maxChroma = (lightness: number, hue: number): number => {
  const rad = (hue * Math.PI) / 180;
  const inGamut = (chroma: number): boolean => {
    const a = chroma * Math.cos(rad);
    const b = chroma * Math.sin(rad);
    // OKLab → 線形 sRGB（Björn Ottosson の行列）。
    const l = (lightness + 0.396_337_777_4 * a + 0.215_803_757_3 * b) ** 3;
    const m = (lightness - 0.105_561_345_8 * a - 0.063_854_172_8 * b) ** 3;
    const s = (lightness - 0.089_484_177_5 * a - 1.291_485_548 * b) ** 3;
    const r = 4.076_741_662_1 * l - 3.307_711_591_3 * m + 0.230_969_929_2 * s;
    const g = -1.268_438_004_6 * l + 2.609_757_401_1 * m - 0.341_319_396_5 * s;
    const bl = -0.004_196_086_3 * l - 0.703_418_614_7 * m + 1.707_614_701 * s;
    return [r, g, bl].every((v) => v >= 0 && v <= 1);
  };

  if (inGamut(CHROMA_CEILING)) return CHROMA_CEILING;
  let lo = 0;
  let hi = CHROMA_CEILING;
  while (hi - lo > CHROMA_STEP) {
    const mid = (lo + hi) / 2;
    if (inGamut(mid)) lo = mid;
    else hi = mid;
  }
  return lo;
};

/** 帯1つぶんの色を組み立てる。 */
const colorOn = (band: Band, color: VoiceView): string => {
  const { lightnessLo, lightnessHi, chromaLo } = band;
  const lightness = lightnessLo + (lightnessHi - lightnessLo) * color.lightness;
  // 下端を割らない。 いちばん狭い色相では、上限が下端より低くなりうる。
  const chromaHi = Math.max(chromaLo, maxChroma(lightness, color.hue) * GAMUT_MARGIN);
  const chroma = chromaLo + (chromaHi - chromaLo) * color.chroma;
  return `oklch(${lightness.toFixed(4)} ${chroma.toFixed(4)} ${color.hue})`;
};

/**
 * 声の色を当てる要素へ渡す style。 その要素の下が音源1つぶんになる。
 *
 * イメージカラーが無いときも `--voice-image` を打ち消す。 打ち消さないと、
 * 外側の音源が選んだ色を受け継いで、別の音源がその色で塗られる。
 */
export const voiceStyle = (
  color: VoiceView | null,
  image: ImageColor | null = null,
): CSSProperties =>
  ({
    ...(color === null
      ? {}
      : {
          "--voice-light": colorOn(SURFACES.light, color),
          "--voice-dark": colorOn(SURFACES.dark, color),
          "--voice-fill-light": colorOn(FILLS.light, color),
          "--voice-fill-dark": colorOn(FILLS.dark, color),
        }),
    "--voice-image": image === null ? "initial" : `var(--${image}-11)`,
  }) as CSSProperties;
