import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { api } from "~/lib/ipc";

import { PresampNotice } from ".";

const meta = {
  title: "部品/PresampNotice",
  component: PresampNotice,
  args: { voiceId: "v1" },
  decorators: [(Story) => <div className="w-160">{Story()}</div>],
} satisfies Meta<typeof PresampNotice>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 戻したことと、書き換えられた中身の行き先を伝える（`DEC-SYN-013`）。
 *
 * 置き場所は出さない（`TR-PKG-45`）。 名前だけ。
 */
export const 戻した: Story = {
  beforeEach: () => {
    mocked(api.presampNotice).mockResolvedValue("presamp-1a2b3c4d.ini");
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("presamp-1a2b3c4d.ini"));
    await expect(canvasElement.textContent).not.toContain("/");
    await expect(canvasElement.querySelector("button")?.textContent).toBe("分かった");
  },
};

/** 戻していなければ何も出さない。 */
export const 戻していない: Story = {
  beforeEach: () => {
    mocked(api.presampNotice).mockResolvedValue(null);
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(api.presampNotice).toHaveBeenCalled());
    await expect(canvasElement.textContent).toBe("");
  },
};
