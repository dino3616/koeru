import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { ToneProgressList } from ".";

const meta = {
  title: "部品/ToneProgressList",
  component: ToneProgressList,
} satisfies Meta<typeof ToneProgressList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 多音階: Story = {
  args: {
    byTone: [
      { tone: "G3", done: 312, total: 312 },
      { tone: "D4", done: 40, total: 312 },
      { tone: "A4", done: 0, total: 312 },
    ],
  },
  play: async ({ canvasElement }) => {
    // 足し合わせた1本にしない（`TR-RCL-26`）。音高の数だけ帯が出る。
    const meters = canvasElement.querySelectorAll("meter");
    await expect(meters).toHaveLength(3);
    // 値と範囲を持ち、語も並ぶ（`TR-PLT-29`）。
    await expect(meters[0]?.getAttribute("max")).toBe("312");
    await expect(canvasElement.textContent).toContain("312 / 312 行");
  },
};

/** まだ何も録っていない。 */
export const 手つかず: Story = {
  args: {
    byTone: [
      { tone: "C3", done: 0, total: 312 },
      { tone: "A3", done: 0, total: 312 },
      { tone: "E4", done: 0, total: 312 },
    ],
  },
};

/**
 * 単音階では出さない。
 *
 * 1本しかないところに内訳は無く、全体の進みと同じものを2度置くことになる。
 */
export const 単音階では出さない: Story = {
  args: { byTone: [{ tone: "A3", done: 10, total: 21 }] },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector("section")).toBeNull();
  },
};
