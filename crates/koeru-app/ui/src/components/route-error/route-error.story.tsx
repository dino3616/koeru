import type { Meta, StoryObj } from "@storybook/react-vite";

import { RouteError } from ".";
import type { AppError } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

const meta = {
  title: "部品/RouteError",
  component: RouteError,
  args: { error: null },
} satisfies Meta<typeof RouteError>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Rustからの失敗: Story = {
  args: {
    error: {
      code: "app.no_project",
      class: "rejected",
      outcome: "not_committed",
      action: "meet_condition",
      message: "その音源が見つからなかった",
    } satisfies AppError,
  },
  render: (args) => withRouter(<RouteError {...args} />),
};

export const 素の例外: Story = {
  args: { error: new Error("読み込みに失敗した") },
  render: (args) => withRouter(<RouteError {...args} />),
};
