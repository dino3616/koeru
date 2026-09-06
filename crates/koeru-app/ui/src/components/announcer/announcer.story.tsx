import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Announcer } from "~/components/announcer";
import { withRouter } from "~/lib/story-router";

/*
 * 常設の読み上げ領域（`TR-PLT-29`）。
 *
 * 見えない。 目視では確かめられないので、`aria-live` が中身より先に
 * DOM へ居ることを機械で見る——文言と一緒に挿し込むと、支援技術が
 * 変化として拾えず読まれない。
 */
const meta = {
  title: "部品/Announcer",
  component: Announcer,
  render: () => withRouter(<Announcer />),
} satisfies Meta<typeof Announcer>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 常設されている: Story = {
  play: async ({ canvasElement }) => {
    const live = canvasElement.querySelector("[aria-live='polite']");
    await expect(live).not.toBeNull();
    // 文言ごと挿し込む形になっていないこと。領域は常に居る。
    await expect(live?.getAttribute("aria-atomic")).toBe("true");
  },
};
