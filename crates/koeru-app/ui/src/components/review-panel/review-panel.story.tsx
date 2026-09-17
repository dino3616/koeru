import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { ReviewPanel } from ".";
import { api } from "~/lib/ipc";
import type { ReviewSummaryView } from "~/lib/ipc";

const base: ReviewSummaryView = {
  mode: "individual",
  pending: 12,
  blocked: 0,
  estimated_seconds: 120,
  budget_seconds: 300,
  exceeds_budget: false,
  allows_skipping: false,
  reach: "reach.undeclared",
  exported: false,
};

/** その要約を返させる。既定は「呼ばれたら待ち続ける」なので、毎回置く。 */
const summary = (over: Partial<ReviewSummaryView> = {}) => {
  mocked(api.reviewSummary).mockResolvedValue({ ...base, ...over });
};

const meta = {
  title: "領域/ReviewPanel",
  component: ReviewPanel,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  decorators: [(Story) => <div className="flex w-160 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof ReviewPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 上限の中: Story = {
  beforeEach: () => summary(),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("2 分"));
    // 超えていないうちは切り替えを出さない（`INV-ALN-004`）。
    await expect(canvasElement.textContent).not.toContain("まとめて確認する");
  },
};

export const 上限を超えた: Story = {
  beforeEach: () => summary({ pending: 48, estimated_seconds: 480, exceeds_budget: true }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("まとめて確認する"));
  },
};

export const まとめて確認へ切り替えたあと: Story = {
  beforeEach: () =>
    summary({ mode: "batch", pending: 48, estimated_seconds: 480, exceeds_budget: true }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("残りをまとめて確認する"));
  },
};

export const 直せない違反がある: Story = {
  beforeEach: () => summary({ pending: 3, blocked: 2, estimated_seconds: 30 }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("録り直すと直ります"));
  },
};

export const 確認が済んだ: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0 }),
  play: async ({ canvasElement }) => {
    // 済んでいれば書き出せる（`INV-ALN-003` の裏）。
    await waitFor(async () => {
      const button = [...canvasElement.querySelectorAll("button")].find((b) =>
        b.textContent?.includes("書き出す"),
      );
      await expect(button?.disabled).toBe(false);
    });
  },
};

export const 確認が残っている間は書き出せない: Story = {
  beforeEach: () => summary(),
  play: async ({ canvasElement }) => {
    await waitFor(async () => {
      const button = [...canvasElement.querySelectorAll("button")].find((b) =>
        b.textContent?.includes("書き出す"),
      );
      await expect(button?.disabled).toBe(true);
    });
  },
};

export const 書き出し済み: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, exported: true }),
};

export const 確認が空にならない作り方: Story = {
  beforeEach: () => summary({ allows_skipping: true, reach: "reach.never_empty" }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("まとめて引き受けて"));
  },
};
