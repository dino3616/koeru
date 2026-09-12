import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Button } from "~/components/button";
import { voiceStyle } from "~/lib/voice-color";

/*
 * 配色の段を、実際に描いて測る（`TR-PLT-25`、`TR-PLT-28`）。
 *
 * Radix パッケージの値を読む自前のスクリプトで計算していたのを廃止して、
 * 実ブラウザに描いた色を測る形へ移した（`DEC-PLT-022`）——計算した値と、
 * 実際に画面へ出る色が食い違う余地が無くなる。
 *
 * ここが検査範囲の正本になる。 部品の story に出てこない組み合わせは、
 * ここに置かないと一度も測られない。段を使いはじめたら、ここへ足す。
 *
 * `components/` に置かない。 部品ではないので、`<名前>/index.tsx` の形に
 * 当てはまらない。見ているのは `globals.css` が出す段なので、その隣に置く。
 *
 * 段 9 / 10 は塗りに使わない。 明るい面で 4.5:1 に届かないので、
 * 塗りは段 11、その hover は段 12。ここにも出さない——出すと、
 * 使わないと決めた段の違反を毎回報告することになる。
 *
 * **cyan と jade はもう無い。** 色相を持ってよいのは声・red・amber だけ
 * （`docs/design/direction.md`）。押せるものの塗りもフォーカス環も無彩色。
 */
const meta = { title: "配色/段" } satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;

/** 字と面の組み合わせ。`src/` で使っている段を全部並べる。 */
const TEXT_ON_SURFACE = [
  { fg: "text-slate-12", bg: "bg-slate-1", label: "本文 / 地" },
  { fg: "text-slate-12", bg: "bg-slate-2", label: "本文 / 面" },
  { fg: "text-slate-12", bg: "bg-slate-3", label: "本文 / 部品" },
  { fg: "text-slate-12", bg: "bg-slate-4", label: "本文 / 部品 hover" },
  { fg: "text-slate-11", bg: "bg-slate-1", label: "薄い字 / 地" },
  { fg: "text-slate-11", bg: "bg-slate-2", label: "薄い字 / 面" },
  { fg: "text-slate-11", bg: "bg-slate-3", label: "薄い字 / 部品" },
  { fg: "text-slate-11", bg: "bg-slate-4", label: "薄い字 / 部品 hover" },
  { fg: "text-slate-1", bg: "bg-slate-11", label: "主ボタンの字 / 塗り" },
  { fg: "text-slate-1", bg: "bg-slate-12", label: "主ボタンの字 / 塗り hover" },
  { fg: "text-slate-1", bg: "bg-red-11", label: "危険ボタンの字 / 塗り" },
  { fg: "text-slate-1", bg: "bg-red-12", label: "危険ボタンの字 / 塗り hover" },
  { fg: "text-red-11", bg: "bg-red-3", label: "失敗の字 / 失敗の面" },
  { fg: "text-red-11", bg: "bg-slate-2", label: "失敗の字 / 面" },
  { fg: "text-red-11", bg: "bg-slate-3", label: "失敗の字 / 部品" },
] as const;

/*
 * 明暗の両方を1つの story に入れる。
 *
 * story を分けない。 Storybook の vitest 統合は既定の globals で1回ずつ
 * 走らせるので、`theme` を切り替えた story を別に置いても片方しか回らない。
 * Radix は段を `.light` / `.dark` のクラスに定義するので、入れ子にすれば
 * 同じ画面で両方を測らせられる。
 *
 * 片方だけ見ると、もう片方で 4.5:1 を割っていることに気づけない。
 */
export const 字と面: Story = {
  render: () => (
    <div className="flex gap-5">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
          <div className="flex flex-col gap-2">
            {TEXT_ON_SURFACE.map(({ fg, bg, label }) => (
              <p key={label} className={`${bg} ${fg} px-3 py-2 text-sm`}>
                {label}
              </p>
            ))}
          </div>
        </div>
      ))}
    </div>
  ),
};

/*
 * 非テキストのコントラスト（`TR-PLT-28`）。
 *
 * axe は文字にしか当たらない。 波形・フォーカス環・境界・環は
 * 「文字ではない要素」なので `color-contrast` の対象外で、放っておくと
 * 3:1 を割っても CI は緑のまま（`DEC-PLT-022`）。
 *
 * 計算色から測る。 段の値を写さないので、Radix の版が上がって
 * 段がずれれば、そのまま比に出る。
 */

/** 3:1 が要る組み合わせ。段ではなく、実際に使っているクラスで書く。 */
const NON_TEXT = [
  { fg: "bg-slate-11", bg: "bg-slate-3", label: "波形 / 部品" },
  { fg: "bg-red-11", bg: "bg-slate-3", label: "割れた波形 / 部品" },
  { fg: "bg-slate-11", bg: "bg-slate-1", label: "波形 / 地" },
  { fg: "bg-slate-12", bg: "bg-slate-1", label: "フォーカス環 / 地" },
  { fg: "bg-slate-12", bg: "bg-slate-2", label: "フォーカス環 / 面" },
  { fg: "bg-slate-12", bg: "bg-slate-3", label: "フォーカス環 / 部品" },
  /*
   * 輪郭だけで的だと分かるものは段 11。
   *
   * 段 7 の輪郭は一覧に載せない。 面（段 2〜3）と字が的を示していて、
   * 輪郭は情報を持たない——`TR-PLT-28` が 3:1 を求めるのは、
   * 「その部品を識別するのに要る」視覚情報のほう。塗りを持たない的
   * （空いた席、まだ決めていない枠）だけが、輪郭で識別されている。
   */
  { fg: "bg-slate-11", bg: "bg-slate-1", label: "輪郭だけの的 / 地" },
  { fg: "bg-slate-11", bg: "bg-slate-2", label: "輪郭だけの的 / 面" },
  { fg: "bg-amber-11", bg: "bg-slate-2", label: "注意の印 / 面" },
  { fg: "bg-red-11", bg: "bg-slate-2", label: "危険の印 / 面" },
] as const;

/**
 * 計算済みの背景色を [0,1] の3値で取る。
 *
 * **記法を数えない。ブラウザに解決させる。** `getComputedStyle` が返すのは
 * 指定した色空間そのままで、`oklch(…)` で書いた色は `oklch(…)` のまま返る。
 * `rgb(…)` だけを読む正規表現で拾うと、**声の色が全部 0 として測られ、
 * 検査が通ってしまう**（踏んだ。明るい面だけが偶然 3:1 を超えて緑になった）。
 *
 * 1×1 の canvas へその色で塗り、塗れた画素を読む。どの記法で来ても、
 * 実際に画面へ出る色そのものが返る。
 */
const rgbOf = (el: Element): [number, number, number] => {
  const value = getComputedStyle(el).backgroundColor;
  const probe = document.createElement("canvas");
  probe.width = 1;
  probe.height = 1;
  const ctx = probe.getContext("2d", { willReadFrequently: true });
  if (ctx === null) return [0, 0, 0];
  ctx.fillStyle = value;
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b] = ctx.getImageData(0, 0, 1, 1).data;
  return [(r ?? 0) / 255, (g ?? 0) / 255, (b ?? 0) / 255];
};

/** WCAG の相対輝度。 */
const luminance = ([r, g, b]: [number, number, number]): number => {
  const lin = (c: number) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
};

const ratio = (a: Element, b: Element): number => {
  const [x, y] = [luminance(rgbOf(a)), luminance(rgbOf(b))];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};

/** 描いた組み合わせをまとめて測る。3:1 を割ったものを文字列で返す。 */
const failuresIn = (root: Element, labels: readonly string[]): string[] => {
  const out: string[] = [];
  for (const label of labels) {
    for (const fg of [...root.querySelectorAll(`[data-fg="${label}"]`)]) {
      const bg = fg.closest(`[data-bg="${label}"]`);
      if (bg === null) continue;
      const r = ratio(fg, bg);
      if (r < 3) out.push(`${label}: ${r.toFixed(2)}:1（要 3）`);
    }
  }
  return out;
};

export const 非テキストの比: Story = {
  render: () => (
    <div className="flex gap-5">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
          {NON_TEXT.map(({ fg, bg, label }) => (
            <div key={label} className={`${bg} mb-2 flex items-center gap-2 p-2`} data-bg={label}>
              <span className={`${fg} inline-block size-4 rounded`} data-fg={label} />
              <span className="text-xs text-slate-12">{label}</span>
            </div>
          ))}
        </div>
      ))}
    </div>
  ),
  play: async ({ canvasElement }) => {
    await expect(
      failuresIn(
        canvasElement,
        NON_TEXT.map((n) => n.label),
      ),
    ).toEqual([]);
  },
};

/*
 * 押せない状態の字（`TR-PLT-25`）。
 *
 * **合成した後の色を測る。** `Button` の押せない状態は `opacity-45` なので、
 * 段の値そのものは画面に出ない——`字と面` はどれも合成前の色を測っていて、
 * **押せない状態は一度も測られていなかった。**
 *
 * `opacity` は計算値の色に出ない。 `getComputedStyle` が返すのは重ねる前の色で、
 * 実際に見える色は「その色を親の面へ `opacity` の比で重ねたもの」。
 * だから重ねる側を自分で書く（[`over`]）。比は DOM から読むので、
 * `Button` 側の `opacity-45` を変えればこの検査も一緒に動く。
 *
 * WCAG 1.4.3 は押せないコントロールを対象外にしている。 だから
 * ここは 4.5:1 では測らず、床（[`DISABLED_FLOOR`]）だけを見る。
 * 「押せない」ことは伝わるべきだが、**何と書いてあるか読めないのは行き過ぎ**
 * ——初めて音源を開いた人が最初に見る「録る」がこの状態になる。
 */

/**
 * 押せない状態の字に許す下限。
 *
 * 1.8 にしてある。 明るい面の主ボタンが実測 1.95:1 で、これ以上落とすと
 * 語が判別できない。上げるなら `Button` の `opacity-45` を上げる側で。
 */
const DISABLED_FLOOR = 1.8;

/**
 * 押せない状態で出る組み合わせ。
 *
 * **本物の `Button` を描く。** クラスを写すと、`Button` 側の `opacity` を
 * 変えてもここは追従しない。`disabled` も本物である必要がある——
 * axe は押せないコントロールを対象外にするので、素の `<span>` で姿だけ真似ると
 * 「4.5:1 に足りない」で落ちる（そのとおりだが、ここで見たいのは床のほう）。
 */
const DISABLED = [
  { variant: "primary", label: "押せない 主" },
  { variant: "secondary", label: "押せない 副" },
  { variant: "ghost", label: "押せない 地" },
  { variant: "danger", label: "押せない 危険" },
] as const;

/** 重ねた後の色。`fg` を `bg` の上に `alpha` の比で置く。 */
const over = (
  fg: [number, number, number],
  bg: [number, number, number],
  alpha: number,
): [number, number, number] => [
  alpha * fg[0] + (1 - alpha) * bg[0],
  alpha * fg[1] + (1 - alpha) * bg[1],
  alpha * fg[2] + (1 - alpha) * bg[2],
];

/**
 * 計算済みの色を [0,1] の4値で取る。透明かどうかを残す。
 *
 * 透明な面は「面が無い」であって黒ではない。 `ghost` の面を黒として測ると、
 * 比が実際より大きく出る。
 */
const colorOf = (
  el: Element,
  prop: "color" | "backgroundColor",
): [number, number, number, number] => {
  const probe = document.createElement("canvas");
  probe.width = 1;
  probe.height = 1;
  const ctx = probe.getContext("2d", { willReadFrequently: true });
  if (ctx === null) return [0, 0, 0, 1];
  ctx.fillStyle = getComputedStyle(el)[prop];
  ctx.fillRect(0, 0, 1, 1);
  const [r, g, b, a] = ctx.getImageData(0, 0, 1, 1).data;
  return [(r ?? 0) / 255, (g ?? 0) / 255, (b ?? 0) / 255, (a ?? 0) / 255];
};

const contrast = (a: [number, number, number], b: [number, number, number]): number => {
  const [x, y] = [luminance(a), luminance(b)];
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
};

export const 押せない状態: Story = {
  render: () => (
    <div className="flex gap-5">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
          {DISABLED.map(({ variant, label }) => (
            <div key={label} className="mb-2 bg-slate-2 p-2" data-surface={`${theme} ${label}`}>
              <Button variant={variant} disabled>
                {label}
              </Button>
            </div>
          ))}
        </div>
      ))}
    </div>
  ),
  play: async ({ canvasElement }) => {
    const low: string[] = [];
    for (const surface of [...canvasElement.querySelectorAll("[data-surface]")]) {
      const target = surface.querySelector("button");
      if (target === null) continue;
      const key = surface.getAttribute("data-surface") ?? "";

      const alpha = Number(getComputedStyle(target).opacity);
      const [sr, sg, sb] = colorOf(surface, "backgroundColor");
      const ground: [number, number, number] = [sr, sg, sb];

      // 面を持たない variant は、親の面がそのまま地になる。
      const [fr, fg, fb, fa] = colorOf(target, "backgroundColor");
      const fill = fa === 0 ? ground : over([fr, fg, fb], ground, alpha);

      const [tr, tg, tb] = colorOf(target, "color");
      const text = over([tr, tg, tb], ground, alpha);

      const r = contrast(text, fill);
      if (r < DISABLED_FLOOR) low.push(`${key}: ${r.toFixed(2)}:1（要 ${DISABLED_FLOOR}）`);
    }
    await expect(low).toEqual([]);
  },
};

/*
 * 声の色（`DEC-PLT-027`）。
 *
 * 色相は 110〜360 の 250 度ぶんを使い、彩度と明度は面ごとの幅へ写す。
 * 組み立ては `lib/voice-color.ts`——**ここでも同じ口を通す。**
 * 変数を手で並べると、色相ごとの彩度の上限を通らないまま測ることになる。
 *
 * ここで測るのは2つ。
 *
 * 1. 段 1〜3 のどの面に載せても 3:1 を満たすか（`TR-PLT-28` の非テキスト）
 * 2. sRGB を外れないか——外れると画面で丸められ、**声ごとの差がそこで消える**
 *
 * 端と中央だけを測る。 色相は 30 度刻み、彩度と明度は両端と中央。
 * 全点を測っても同じ結論になるが、story が重くなる。
 */

/** 測る色相。10 度刻みでは多すぎるので、30 度で刻む。 */
const HUES = Array.from({ length: 9 }, (_, i) => 110 + i * 30);

/** 彩度と明度の位置。両端と中央。 */
const POSITIONS = [0, 0.5, 1] as const;

/** 声を載せる面。環は地と面の両方に乗る。 */
const VOICE_ON = ["bg-slate-1", "bg-slate-2", "bg-slate-3"] as const;

export const 声の色: Story = {
  render: () => (
    <div className="flex gap-5">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
          <div className="flex flex-col gap-2">
            {VOICE_ON.map((surface) =>
              POSITIONS.map((position) => {
                const label = `${theme} ${surface} ${position}`;
                return (
                  <div key={label} className={`${surface} flex gap-1 p-2`} data-bg={label}>
                    {HUES.map((hue) => (
                      <span
                        key={hue}
                        data-fg={label}
                        className="koeru-voice-fill inline-block size-4 rounded"
                        style={voiceStyle({
                          hue,
                          chroma: position,
                          lightness: position,
                        })}
                      />
                    ))}
                  </div>
                );
              }),
            )}
          </div>
        </div>
      ))}
    </div>
  ),
  play: async ({ canvasElement }) => {
    const labels = (["light", "dark"] as const).flatMap((theme) =>
      VOICE_ON.flatMap((surface) => POSITIONS.map((p) => `${theme} ${surface} ${p}`)),
    );
    await expect(failuresIn(canvasElement, labels)).toEqual([]);

    /*
     * sRGB を外れていないか。
     *
     * 外れた色はブラウザが端へ丸める。 丸められた色は 0 か 255 に張り付くので、
     * 同じ位置で色相だけ変えたときに**同じ値の組が出る**。1つでも重なれば、
     * そこは声の差が消えている。
     */
    for (const label of labels) {
      const swatches = [...canvasElement.querySelectorAll(`[data-fg="${label}"]`)];
      const seen = new Set(swatches.map((s) => getComputedStyle(s).backgroundColor));
      await expect(seen.size).toBe(swatches.length);
    }
  },
};
