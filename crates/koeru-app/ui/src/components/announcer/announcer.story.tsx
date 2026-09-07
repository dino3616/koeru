import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, waitFor } from "storybook/test";

import { Announcer } from ".";
import { withRouter } from "~/lib/story-router";

const meta = { title: "部品/Announcer", component: Announcer } satisfies Meta<typeof Announcer>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 一覧へ来た: Story = {
  render: () => withRouter(<Announcer />, "/"),
  play: async ({ canvasElement }) => {
    // 領域は空のまま先に居る。文言はあとから入る。
    const region = canvasElement.querySelector("[aria-live]");
    await expect(region).not.toBeNull();
    await waitFor(() => expect(region?.textContent).toContain("声"));
  },
};

export const 音源へ来た: Story = {
  render: () => withRouter(<Announcer />, "/voice"),
  play: async ({ canvasElement }) => {
    await waitFor(() =>
      expect(canvasElement.querySelector("[aria-live]")?.textContent).toContain("音源"),
    );
  },
};
