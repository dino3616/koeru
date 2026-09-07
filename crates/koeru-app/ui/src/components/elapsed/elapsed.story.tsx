import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Elapsed } from ".";

const meta = { title: "部品/Elapsed", component: Elapsed } satisfies Meta<typeof Elapsed>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 名前に入れない。 押せるものの中に置くので、毎秒読み上げが
     * 書き換わると追えなくなる（`TR-PLT-29`）。
     */
    await expect(canvasElement.querySelector("span")?.getAttribute("aria-hidden")).toBe("true");
  },
};
