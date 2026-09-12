import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { PendingWork } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "部品/PendingWork",
  component: PendingWork,
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof PendingWork>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 待っている: Story = {
  beforeEach: () => {
    mocked(api.pendingWork).mockResolvedValue(3);
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("残り 3 件"));
  },
};

export const 何も待っていない: Story = {
  beforeEach: () => {
    mocked(api.pendingWork).mockResolvedValue(0);
  },
  play: async ({ canvasElement }) => {
    // 領域は常に置く。挿し込むと、支援技術が変化として拾えない。
    await expect(canvasElement.querySelector("[aria-live]")).not.toBeNull();
  },
};
