import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Chip } from ".";

const meta = {
  title: "部品/Chip",
  component: Chip,
  args: { children: "まだ録っていない" },
} satisfies Meta<typeof Chip>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 選ばれている: Story = {
  args: { pressed: true },
  play: async ({ canvasElement }) => {
    // 色だけで伝えない。選択は `aria-pressed` にも出す。
    await expect(canvasElement.querySelector("button")?.getAttribute("aria-pressed")).toBe("true");
  },
};

export const 選ばれていない: Story = { args: { pressed: false } };
