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

const inState = (canvasElement: HTMLElement, state: string) =>
  canvasElement.querySelectorAll(`span[data-state='${state}']`);

/** 歌い始め。 先頭のモーラが声の色。 */
export const 先頭: Story = {
  args: { position: 0 },
  play: async ({ canvasElement }) => {
    const [first] = canvasElement.querySelectorAll("span[data-state]");
    await expect(first?.getAttribute("data-state")).toBe("current");
    await expect(inState(canvasElement, "current")).toHaveLength(1);
  },
};

/** 途中（S04）。 済んだものは退き、まだのものは字の色のまま。 */
export const 途中: Story = {
  play: async ({ canvasElement }) => {
    await expect(inState(canvasElement, "done")).toHaveLength(3);
    await expect(inState(canvasElement, "upcoming")).toHaveLength(4);
    const [done] = inState(canvasElement, "done");
    const [upcoming] = inState(canvasElement, "upcoming");
    await expect(done?.className).toContain("text-2");
    await expect(upcoming?.className).toContain("text-1");
  },
};

/** 行の最後。 まだのモーラが残らない。 */
export const 最後: Story = {
  args: { position: 7 },
  play: async ({ canvasElement }) => {
    await expect(inState(canvasElement, "upcoming")).toHaveLength(0);
  },
};

/** 8 拍の行は 72px の1行に収まる。 */
export const 一行: Story = {
  decorators: [(Story) => <div className="w-258">{Story()}</div>],
  play: async ({ canvasElement }) => {
    const moras = [...canvasElement.querySelectorAll("span[data-state]")];
    const tops = new Set(moras.map((m) => Math.round(m.getBoundingClientRect().top)));
    await expect(tops.size).toBe(1);
    await expect(getComputedStyle(moras[0] as Element).fontSize).toBe("72px");
  },
};

/** 長い行は字を1段下げ、1行に収める。 */
export const 長い行: Story = {
  args: {
    moras: ["きゃ", "き", "きゅ", "きぇ", "きょ", "ん", "ぎゃ", "ぎ", "ぎゅ", "ぎぇ", "ぎょ"],
    position: 4,
  },
  decorators: [(Story) => <div className="w-258">{Story()}</div>],
  play: async ({ canvasElement }) => {
    const moras = [...canvasElement.querySelectorAll("span[data-state]")];
    const tops = new Set(moras.map((m) => Math.round(m.getBoundingClientRect().top)));
    await expect(tops.size).toBe(1);
    await expect(getComputedStyle(moras[0] as Element).fontSize).toBe("48px");
  },
};

/**
 * 窓が 960px のとき。 左の列を引いた幅に長い行が収まらず、折る。
 *
 * 折れるのはモーラの境目だけ。 2字の拍（「きゃ」）が割れず、横にはみ出さない。
 */
export const 窓が960px: Story = {
  args: {
    moras: ["きゃ", "き", "きゅ", "きぇ", "きょ", "ん", "ぎゃ", "ぎ", "ぎゅ", "ぎぇ", "ぎょ"],
    position: 4,
  },
  decorators: [(Story) => <div className="w-150 border border-line">{Story()}</div>],
  play: async ({ canvasElement }) => {
    const row = canvasElement.querySelector("p");
    const moras = [...canvasElement.querySelectorAll("span[data-state]")];
    const tops = new Set(moras.map((m) => Math.round(m.getBoundingClientRect().top)));
    await expect(tops.size).toBeGreaterThan(1);
    // どの拍も1行のまま。 行の高さを超えたら、字の途中で割れている。
    const lineHeight = Number.parseFloat(getComputedStyle(moras[0] as Element).lineHeight);
    for (const m of moras) {
      await expect(m.getBoundingClientRect().height).toBeLessThanOrEqual(lineHeight + 1);
    }
    await expect(row?.scrollWidth).toBeLessThanOrEqual(row?.clientWidth ?? 0);
  },
};

/** 読み上げは行の全体を1回だけ。 位置は名前に入らず、`aria-live` も持たない。 */
export const 読み上げ: Story = {
  play: async ({ canvasElement }) => {
    const row = canvasElement.querySelector("[role='img']");
    await expect(row?.getAttribute("aria-label")).toBe("ぱ ぱ ぴ ぷ ぺ ぽ ん ば");
    await expect(canvasElement.querySelector("[aria-live]")).toBeNull();
    // 中身は見た目だけ。 ここが読まれると名前と二重になる。
    await expect(canvasElement.querySelector("p")?.getAttribute("aria-hidden")).toBe("true");
  },
};
