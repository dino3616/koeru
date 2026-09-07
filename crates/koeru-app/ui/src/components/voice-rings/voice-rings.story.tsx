import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { VoiceRings } from ".";

const meta = {
  title: "声/VoiceRings",
  component: VoiceRings,
  args: {
    label: "録れた音の形。102 音のうち 30 音",
    color: { hue: 318, chroma: 0.7, lightness: 0.5 },
  },
  // 一辺は置く側の箱が決める。story でも箱を与える。
  decorators: [(Story) => <div className="size-60">{Story()}</div>],
} satisfies Meta<typeof VoiceRings>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 途中まで録れた声。閉じていない環と、まだ1本も無い環が混ざる。 */
export const 育っている途中: Story = {
  args: {
    rings: [
      { covered: 6, total: 6 },
      { covered: 8, total: 8 },
      { covered: 5, total: 8 },
      { covered: 1, total: 8 },
      { covered: 0, total: 8 },
      { covered: 0, total: 8 },
      { covered: 0, total: 3 },
      { covered: 0, total: 8 },
    ],
  },
  play: async ({ canvasElement }) => {
    // 欠けに軌道を置かない。1本も録れていない環は描かない。
    await expect(canvasElement.querySelectorAll("circle")).toHaveLength(4);
  },
};

/**
 * 単独音の中核セットの実寸。
 *
 * 五十音の行は 15 本。 **録っても本数は増えない**——増えるのは閉じ具合だけ。
 * これで `viewBox` からはみ出さないことを見る（固定の間隔にしていたときは
 * 外側が突き抜けて、**環が四角く切り取られて出た**）。
 */
export const 閉じた: Story = {
  args: {
    label: "録れた音の形。102 音すべて",
    rings: Array.from({ length: 15 }, () => ({ covered: 5, total: 5 })),
  },
  play: async ({ canvasElement }) => {
    const svg = canvasElement.querySelector("svg");
    const box = svg?.getAttribute("viewBox")?.split(" ").map(Number) ?? [0, 0, 0, 0];
    const half = (box[2] ?? 0) / 2;
    const stroke = Number(svg?.querySelector("g")?.getAttribute("stroke-width") ?? 0);
    for (const c of canvasElement.querySelectorAll("circle")) {
      await expect(Number(c.getAttribute("r")) + stroke / 2).toBeLessThan(half);
    }
  },
};

/** まだ1つも録れていない音源。色も形も無い。 */
export const まだ無い: Story = {
  args: {
    label: "まだ録っていません",
    color: null,
    rings: [
      { covered: 0, total: 6 },
      { covered: 0, total: 5 },
    ],
  },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("circle")).toHaveLength(0);
  },
};

export const 伸びる: Story = {
  args: {
    grow: true,
    rings: [
      { covered: 6, total: 6 },
      { covered: 2, total: 5 },
    ],
  },
};
