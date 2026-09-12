import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Breath } from ".";

const meta = { title: "部品/Breath", component: Breath } satisfies Meta<typeof Breath>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement }) => {
    // 二重に読ませない。文言は読み上げ領域が持つ（`TR-PLT-29`）。
    await expect(canvasElement.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  },
};

export const 小さい: Story = { args: { size: "sm" } };
