import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor, within } from "storybook/test";

import { LeakCard } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/LeakCard",
  component: LeakCard,
  args: { ready: true, midi: 60, onStatus: fn(), onChecked: fn() },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof LeakCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 確かめる前: Story = {};

export const 入っていない: Story = {
  beforeEach: () => {
    mocked(api.checkGuideLeak).mockResolvedValue({ correlation: 0.02, lag_ms: 0, leaking: false });
  },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole("button", { name: "確かめる" }));
    await waitFor(() => expect(canvasElement.textContent).toContain("入っていません"));
  },
};

export const 入っている: Story = {
  beforeEach: () => {
    mocked(api.checkGuideLeak).mockResolvedValue({ correlation: 0.61, lag_ms: 18, leaking: true });
  },
  play: async ({ canvasElement }) => {
    await userEvent.click(within(canvasElement).getByRole("button", { name: "確かめる" }));
    // 止めない。回り込んでいても録れることを、その場に書く。
    await waitFor(() => expect(canvasElement.textContent).toContain("このままでも録れます"));
  },
};
