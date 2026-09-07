import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { AppMark } from ".";

const meta = { title: "声/AppMark", component: AppMark } satisfies Meta<typeof AppMark>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement }) => {
    // 絵は読ませない。名前はワードマークの字が持つ。
    await expect(canvasElement.querySelector("svg")?.getAttribute("aria-hidden")).toBe("true");
  },
};
