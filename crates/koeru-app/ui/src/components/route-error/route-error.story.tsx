import type { Meta, StoryObj } from "@storybook/react-vite";

import { RouteError } from ".";
import { withRouter } from "~/lib/story-router";

const meta = {
  title: "部品/RouteError",
  component: RouteError,
  args: { error: null },
} satisfies Meta<typeof RouteError>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Rustからの失敗: Story = {
  args: { error: { kind: "app.no_project", message: "その音源が見つからなかった" } },
  render: (args) => withRouter(<RouteError {...args} />),
};

export const 素の例外: Story = {
  args: { error: new Error("読み込みに失敗した") },
  render: (args) => withRouter(<RouteError {...args} />),
};
