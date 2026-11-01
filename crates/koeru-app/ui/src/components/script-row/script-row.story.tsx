import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { ScriptRow } from ".";
import { voiceStyle } from "~/lib/voice-color";

const meta = {
  title: "部品/ScriptRow",
  component: ScriptRow,
  args: {
    moras: ["ぱ", "ぱ", "ぴ", "ぷ", "ぺ", "ぽ", "ん", "ば"],
    position: 3,
    beats: { count: 8, current: 3 },
  },
  decorators: [
    // 声の色は置く側の要素が与える。 実画面では音源の面がこの役をする。
    (Story) => (
      <div
        /* 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。 */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle({ hue: 168, chroma: 0.6, lightness: 0.5 })}
      >
        {Story()}
      </div>
    ),
  ],
} satisfies Meta<typeof ScriptRow>;

export default meta;
type Story = StoryObj<typeof meta>;

const current = (canvasElement: HTMLElement) =>
  canvasElement.querySelectorAll("span[data-state='current']");

/** 歌い始め。 先頭のモーラが声の色で、拍の点も先頭が今。 */
export const 先頭: Story = {
  args: { position: 0, beats: { count: 8, current: 0 } },
  play: async ({ canvasElement }) => {
    const [first] = canvasElement.querySelectorAll("span[data-state]");
    await expect(first?.getAttribute("data-state")).toBe("current");
    await expect(current(canvasElement)).toHaveLength(2);
  },
};

/** 途中。 済んだものと、まだのものが分かれる。 */
export const 途中: Story = {
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("span[data-state='done']")).toHaveLength(6);
    await expect(canvasElement.querySelectorAll("span[data-state='upcoming']")).toHaveLength(8);
  },
};

/** 行の最後。 まだのモーラも点も残らない。 */
export const 最後: Story = {
  args: { position: 7, beats: { count: 8, current: 7 } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("span[data-state='upcoming']")).toHaveLength(0);
  },
};

/** ガイド BGM が無いとき。 点の行ごと出さない。 */
export const 拍なし: Story = {
  args: { beats: null },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("span.rounded-full")).toHaveLength(0);
    await expect(canvasElement.querySelectorAll("p")).toHaveLength(1);
  },
};

/** 点の数はモーラの数と別。 いまの拍がまだ始まっていなければ全部が未。 */
export const 拍が始まる前: Story = {
  args: { position: -1, beats: { count: 4, current: null } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("span[data-state='current']")).toHaveLength(0);
    await expect(canvasElement.querySelectorAll("span.rounded-full")).toHaveLength(4);
  },
};

/**
 * 窓が 960px のとき。 サイドバー（248px）を引いた 712px が行に残る。
 *
 * 折れるのはモーラの境目だけ。 2字の拍（「きゃ」）が割れず、
 * 横にはみ出さない。
 */
export const 窓が960px: Story = {
  args: {
    moras: ["きゃ", "き", "きゅ", "きぇ", "きょ", "ん", "ぎゃ", "ぎ", "ぎゅ", "ぎぇ", "ぎょ"],
    position: 4,
    beats: { count: 11, current: 4 },
  },
  decorators: [(Story) => <div className="w-178 border border-line">{Story()}</div>],
  play: async ({ canvasElement }) => {
    const row = canvasElement.querySelector("p");
    const moras = [...canvasElement.querySelectorAll("p:first-of-type > span")];
    // 折り返している（1行に収まる幅ではない）。
    const tops = new Set(moras.map((m) => Math.round(m.getBoundingClientRect().top)));
    await expect(tops.size).toBeGreaterThan(1);
    // どの拍も1行のまま。 行の高さを超えたら、字の途中で割れている。
    const lineHeight = Number.parseFloat(getComputedStyle(moras[0] as Element).lineHeight);
    for (const m of moras) {
      await expect(m.getBoundingClientRect().height).toBeLessThanOrEqual(lineHeight + 1);
    }
    // 横にはみ出さない。
    await expect(row?.scrollWidth).toBeLessThanOrEqual(row?.clientWidth ?? 0);
  },
};

/** 読み上げは行の全体を1回だけ。 位置も拍も名前に入らず、`aria-live` も持たない。 */
export const 読み上げ: Story = {
  play: async ({ canvasElement }) => {
    const row = canvasElement.querySelector("[role='img']");
    await expect(row?.getAttribute("aria-label")).toBe("ぱ ぱ ぴ ぷ ぺ ぽ ん ば");
    await expect(canvasElement.querySelector("[aria-live]")).toBeNull();
    // 中身は見た目だけ。 ここが読まれると名前と二重になる。
    for (const part of canvasElement.querySelectorAll("p")) {
      await expect(part.getAttribute("aria-hidden")).toBe("true");
    }
  },
};
