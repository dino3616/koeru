import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { DowngradeNotice } from ".";

const meta = {
  title: "部品/DowngradeNotice",
  component: DowngradeNotice,
} satisfies Meta<typeof DowngradeNotice>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 容量と由来を書き出し前に出す（`TR-PKG-24`, `DEC-RCL-007`）。
 */
export const 単独音へ降りられる: Story = {
  args: {
    downgrades: [{ method: "single", bytes: 288_358_400, sessions: 4, span_days: 12 }],
  },
  play: async ({ canvasElement }) => {
    // 「oto.ini 1ファイル分」ではないことが、容量の数で分かる。
    await expect(canvasElement.textContent).toContain("約 275 MB");
    // 由来は事実だけ。「声質が揃っている」とは言わない。
    await expect(canvasElement.textContent).toContain("4 回");
    await expect(canvasElement.textContent).not.toContain("揃っています");
  },
};

/** 降りられる先が無ければ、何も置かない。 */
export const 降りられない: Story = {
  args: { downgrades: [] },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent?.trim()).toBe("");
  },
};
