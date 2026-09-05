import type { Meta, StoryObj } from "@storybook/react-vite";

import { Button } from "~/components/ui/button";

/*
 * 押せるもの。
 *
 * variant と size の全組み合わせを出す。 axe がここでコントラストを測るので、
 * 出していない組み合わせは一度も検査されない——`variant` を足したら story も足す。
 */
const meta = {
  title: "部品/Button",
  component: Button,
  args: { children: "録る" },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Primary: Story = { args: { variant: "primary" } };
export const Secondary: Story = { args: { variant: "secondary" } };
export const Ghost: Story = { args: { variant: "ghost" } };
export const Danger: Story = { args: { variant: "danger", children: "止める" } };

/** 44px を下回らない（`TR-PLT-28` の対象サイズ）。 */
export const Sizes: Story = {
  render: () => (
    <div className="flex items-center gap-3">
      <Button size="md">md</Button>
      <Button size="lg">lg</Button>
      <Button size="icon" aria-label="閉じる">
        ✕
      </Button>
    </div>
  ),
};

/**
 * 押せない状態。
 *
 * `opacity-45` を掛けるので、そのままでは字のコントラストが落ちる。
 * 実際に測らせるために出す。
 */
export const Disabled: Story = {
  render: () => (
    <div className="flex items-center gap-3">
      <Button variant="primary" disabled>
        録る
      </Button>
      <Button variant="danger" disabled>
        止める
      </Button>
      <Button variant="secondary" disabled>
        やめる
      </Button>
    </div>
  ),
};
