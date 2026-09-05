import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked } from "storybook/test";

import { LeakCard } from "~/components/leak-card";
import { api } from "~/lib/ipc";

/*
 * ガイドの回り込みを確かめる面（`TR-REC-24`）。
 *
 * 3つの結果を全部出す。 漏れている・漏れていない・判定できない。
 * 判定できないことも正規の結果なので、それも出す。
 */
const meta = {
  title: "部品/LeakCard",
  component: LeakCard,
  args: { ready: true, midi: 60, onStatus: fn(), onChecked: fn() },
  beforeEach: () => {
    mocked(api.outputKind).mockResolvedValue("Headphones");
  },
} satisfies Meta<typeof LeakCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 未確認: Story = {};

export const デバイス未選択: Story = { args: { ready: false } };

export const 漏れていない: Story = {
  beforeEach: () => {
    mocked(api.checkGuideLeak).mockResolvedValue({
      leaking: false,
      correlation: 0.02,
      lag_ms: 0,
    });
  },
  play: async ({ canvasElement, userEvent }) => {
    await userEvent.click(canvasElement.querySelectorAll("button")[0] as HTMLElement);
    await expect(canvasElement.textContent).toContain("回り込み");
  },
};
