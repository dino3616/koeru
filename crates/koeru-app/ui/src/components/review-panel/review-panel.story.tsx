import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor } from "storybook/test";

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
  missing: 0,
  conflicting: 0,
  unestimated: 0,
  may_export: false,
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
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, may_export: true }),
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

export const 発声が見つからなかった行がある: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, missing: 2 }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("録り直すまで書き出せません"));
    // キューは空でも書き出させない（`INV-ALN-003` の趣旨）。
    const button = [...canvasElement.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("書き出す"),
    );
    await expect(button?.disabled).toBe(true);
  },
};

export const 録り直しを待っている音がある: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, unestimated: 2 }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("録り直しを待っている音"));
    // 上で「済んでいない音はありません」と言わない。
    await expect(canvasElement.textContent).not.toContain("済んでいない音はありません");
    /*
      件数から組み立て直さない。 `pending` は未推定を数えないので、
      ここで組み立てると「済んだ」と出して押させ、押すと断られる。
    */
    const button = [...canvasElement.querySelectorAll("button")].find((b) =>
      b.textContent?.includes("書き出す"),
    );
    await expect(button?.disabled).toBe(true);
  },
};

export const 文字コードを選べる: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, may_export: true }),
  play: async ({ canvasElement }) => {
    // 固定にすると、CP932 で表せない名前が1つあるだけで書き出せなくなる（`TR-ALN-21`）。
    const chips = await waitFor(() => {
      const found = [...canvasElement.querySelectorAll("button")].filter(
        (b) => b.textContent === "Shift-JIS" || b.textContent === "UTF-8",
      );
      if (found.length !== 2) throw new Error("まだ出ていない");
      return found;
    });
    const jis = chips.find((c) => c.textContent === "Shift-JIS");
    const utf = chips.find((c) => c.textContent === "UTF-8");
    // 既定は UTAU 本体互換の側。選択は `aria-pressed` で言う。
    await expect(jis?.getAttribute("aria-pressed")).toBe("true");
    await expect(utf?.getAttribute("aria-pressed")).toBe("false");
    if (utf !== undefined) await userEvent.click(utf);
    await waitFor(() => expect(utf?.getAttribute("aria-pressed")).toBe("true"));
  },
};

export const 別の回と同じ名前の音がある: Story = {
  beforeEach: () => summary({ pending: 0, estimated_seconds: 0, conflicting: 2 }),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("別の回と同じ名前の音"));
    // キューが片方を落とすので、確認が空でも書き出させない。
    await expect(canvasElement.textContent).not.toContain("済んでいない音はありません");
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
