import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { InputLevel } from ".";

const meta = {
  title: "部品/InputLevel",
  component: InputLevel,
  args: { peak: 0.62, clipped: 0 },
  decorators: [(Story) => <div className="w-80">{Story()}</div>],
} satisfies Meta<typeof InputLevel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 入っている: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 判定語を出さない（`DEC-REC-008`、`Q-REC-004`）。
     * 区分も持たない——`low` / `high` / `optimum` があると、
     * 支援技術に「よい範囲かどうか」が届く。
     */
    const text = canvasElement.textContent ?? "";
    for (const word of ["ちょうど", "小さすぎ", "大きすぎ"]) {
      await expect(text).not.toContain(word);
    }
    const meter = canvasElement.querySelector("meter");
    await expect(meter?.hasAttribute("low")).toBe(false);
    await expect(meter?.hasAttribute("optimum")).toBe(false);
  },
};

export const 静か: Story = { args: { peak: 0.02 } };
export const 割れた: Story = { args: { peak: 1, clipped: 3 } };
