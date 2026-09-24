import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { api } from "~/lib/ipc";

import { DowngradeNotice } from ".";

const meta = {
  title: "部品/DowngradeNotice",
  component: DowngradeNotice,
} satisfies Meta<typeof DowngradeNotice>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 容量を書き出し前に出す（`TR-PKG-24`）。
 *
 * **素材の由来は出さない**（`DEC-RCL-015`）。 跨いだ収録セッションの数や
 * 期間から読めるのは「声が揃っていないかもしれない」だけで、
 * 検知しないと言いながら判断材料を置くことになる。
 */
export const 単独音へ降りられる: Story = {
  args: {
    downgrades: [{ method: "single", bytes: 288_358_400 }],
  },
  play: async ({ canvasElement }) => {
    // 「oto.ini 1ファイル分」ではないことが、容量の数で分かる。
    await expect(canvasElement.textContent).toContain("約 275 MB");
    // 声質に関与しない。推測材料も置かない。
    await expect(canvasElement.textContent).not.toContain("録った回");
    await expect(canvasElement.textContent).not.toContain("声の揃い");
    // 「出せます」と書いたら、出す口も置く（`TR-PKG-24`）。
    await expect(canvasElement.querySelector("button")?.textContent).toContain("単独音");
  },
};

/** 出したあとは、置き場所へ行ける（`TR-PKG-45`）。 */
export const 書き出したあと: Story = {
  args: {
    downgrades: [{ method: "single", bytes: 288_358_400 }],
  },
  beforeEach: () => {
    mocked(api.exportDowngrade).mockResolvedValue({
      seq: 3,
      archive_name: "000003-v1-single.zip",
      alias_count: 144,
      released_at: "2026-09-22T10:00:00Z",
    });
  },
  play: async ({ canvasElement }) => {
    canvasElement.querySelector("button")?.click();
    // 独立した ZIP になる（`TR-PKG-25`）。名前が元と違うことが見える。
    await waitFor(() => expect(canvasElement.textContent).toContain("000003-v1-single.zip"));
  },
};

/** 降りられる先が無ければ、何も置かない。 */
export const 降りられない: Story = {
  args: { downgrades: [] },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent?.trim()).toBe("");
  },
};
