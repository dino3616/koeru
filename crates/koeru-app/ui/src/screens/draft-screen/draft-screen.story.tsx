import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";

import { DraftScreen } from ".";
import { withRouter } from "~/lib/story-router";

const ID = "11111111-1111-4111-8111-111111111111";

const meta = {
  title: "画面/DraftScreen",
  component: DraftScreen,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof DraftScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 収録: Story = {
  render: () => withRouter(<DraftScreen />, `/record?id=${ID}`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole("heading", { name: "収録" })).toBeVisible();
    // 移った先で音源が分からなくならないように、行き先が音源を持ち回す。
    const songs = canvas.getByRole("link", { name: "曲バンク" });
    await expect(songs.getAttribute("href")).toContain(ID);
    // これから作る音源の画面で、前の音源を開かない。
    const fresh = canvas.getByRole("link", { name: "新しい音源" });
    await expect(fresh.getAttribute("href")).not.toContain(ID);
  },
};

export const 音源なし: Story = {
  render: () => withRouter(<DraftScreen />, "/new"),
};
