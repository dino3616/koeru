import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { InputLevel } from ".";

const meta = {
  title: "部品/InputLevel",
  component: InputLevel,
  // 0.35 は約 −9.1 dBFS、0.5 は約 −6.0 dBFS。
  args: { level: 0.35, peak: 0.5, clipped: 0 },
  decorators: [(Story) => <div className="w-80">{Story()}</div>],
} satisfies Meta<typeof InputLevel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 普通: Story = {
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
    await expect(meter?.hasAttribute("high")).toBe(false);
    await expect(meter?.hasAttribute("optimum")).toBe(false);

    // 右端の値はピークで、バーの値ではない。 読み上げには、いまの値が届く。
    await expect(text).toContain("ピーク −6.0 dBFS");
    await expect(meter?.getAttribute("aria-valuetext")).toBe("−9.1 dBFS");
    await expect(text).toContain("縦線は直前のピーク。");
  },
};

export const 無音: Story = {
  args: { level: 0, peak: 0, clipped: 0 },
  play: async ({ canvasElement }) => {
    // 対数の 0 は無限に小さい。 数値でも、`<meter>` の値でも落ちない。
    await expect(canvasElement.textContent).toContain("ピーク −∞ dBFS");
    const meter = canvasElement.querySelector("meter");
    await expect(meter?.getAttribute("aria-valuetext")).toBe("−∞ dBFS");
  },
};

export const ピークが右端: Story = {
  args: { level: 1, peak: 1, clipped: 3 },
  play: async ({ canvasElement }) => {
    // フルスケールは `0.0`。 `-0.0` にならない。
    await expect(canvasElement.textContent).toContain("ピーク 0.0 dBFS");
    await expect(canvasElement.textContent).toContain("3 回");
  },
};
