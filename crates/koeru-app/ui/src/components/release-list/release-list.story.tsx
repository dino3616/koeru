import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked } from "storybook/test";

import { ReleaseList } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/ReleaseList",
  component: ReleaseList,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  decorators: [(Story) => <div className="flex w-72 flex-col gap-3">{Story()}</div>],
} satisfies Meta<typeof ReleaseList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const つくったものがある: Story = {
  beforeEach: () => {
    mocked(api.releases).mockResolvedValue([
      {
        seq: 2,
        version: "v1.1",
        method: "single",
        alias_count: 102,
        validation: "passed",
        archive_name: "000002-v1.1.zip",
        released_at: "2026-09-18T09:20:00Z",
      },
      {
        seq: 1,
        version: "",
        method: "single",
        alias_count: 96,
        validation: "passed",
        archive_name: "000001-v1.0.zip",
        released_at: "2026-09-12T18:04:00Z",
      },
    ]);
  },
  play: async ({ canvasElement }) => {
    // 場所は出さない（`TR-PKG-45`）。名前だけを出し、置き場所は的で開かせる。
    await expect(canvasElement.textContent).not.toContain("/");
    await expect(canvasElement.querySelectorAll("button").length).toBe(2);
    // 本人が付けた札を出す。空なら、空だと言う。
    await expect(canvasElement.textContent).toContain("v1.1");
    await expect(canvasElement.textContent).toContain("呼び名なし");
  },
};

export const まだつくっていない: Story = {
  beforeEach: () => {
    mocked(api.releases).mockResolvedValue([]);
  },
};
