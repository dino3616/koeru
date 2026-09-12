import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor, within } from "storybook/test";

import { CalibrationCard } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/CalibrationCard",
  component: CalibrationCard,
  args: { ready: true, onStatus: fn() },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof CalibrationCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 合わせる前: Story = {
  play: async ({ canvasElement }) => {
    // 関門にしない。合わせなくても進めることを、その場に書く。
    await expect(canvasElement.textContent).toContain("合わせなくても");
  },
};

export const マイクを選ぶ前: Story = { args: { ready: false } };

export const KOERUから動かせないマイク: Story = {
  beforeEach: () => {
    mocked(api.calibrate).mockResolvedValue({
      gain: null,
      control: "Software",
      peak_dbfs: -14.2,
      settled: false,
    });
  },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole("button", { name: "合わせる" }));
    // 自動調整しない相手には、どこを触ればよいかを書く（`TR-REC-14`）。
    await waitFor(() => expect(canvasElement.textContent).toContain("システム設定"));
  },
};
