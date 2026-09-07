import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { TakeGenerations } from ".";
import type { TakeSummaryView } from "~/lib/ipc";

const takes: TakeSummaryView[] = [
  { take_id: 1, generation: 1, peak: 0.68, duration_ms: 3020, invalid: false },
  { take_id: 2, generation: 2, peak: 0.71, duration_ms: 3100, invalid: false },
  { take_id: 3, generation: 3, peak: 0.65, duration_ms: 2940, invalid: false },
];

const meta = {
  title: "領域/TakeGenerations",
  component: TakeGenerations,
  args: {
    takes,
    adoptedId: 2,
    shownId: 2,
    onShow: fn(),
    onAdopt: fn(),
    onRetake: fn(),
    busy: false,
  },
  decorators: [(Story) => <div className="w-80">{Story()}</div>],
} satisfies Meta<typeof TakeGenerations>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 三回録った: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 行 ID を読み上げに入れない（`Q-REC-003`、`TR-REC-18`）。
     * 切り替えても被覆が変わらないことを、その場に書く（`TR-RCL-25`）。
     */
    await expect(canvasElement.textContent).toContain("取れる音は変わりません");
    const labels = [...canvasElement.querySelectorAll("[aria-label]")].map((e) =>
      e.getAttribute("aria-label"),
    );
    await expect(labels.some((l) => l?.includes("s0") === true)).toBe(false);
  },
};

export const 使えない回がある: Story = {
  args: {
    takes: [...takes, { take_id: 4, generation: 4, peak: 0.4, duration_ms: 1200, invalid: true }],
  },
};

export const 一回だけ: Story = { args: { takes: takes.slice(0, 1), adoptedId: 1, shownId: 1 } };

export const 収録中: Story = { args: { busy: true } };
