import type { Meta, StoryObj } from "@storybook/react-vite";

import { Button } from ".";

/*
 * variant は全部出す（`DEC-PLT-022`）。
 *
 * 1つだけ出すと、残りの配色は axe に一度も当たらない。
 * 押せない状態（`opacity-45` が掛かる）も出す。
 */
const meta = {
  title: "部品/Button",
  component: Button,
  args: { children: "録る" },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 主: Story = { args: { variant: "primary" } };
export const 副: Story = { args: { variant: "secondary" } };
export const 地: Story = { args: { variant: "ghost" } };
export const 危険: Story = { args: { variant: "danger", children: "止める" } };
export const 押せない: Story = { args: { variant: "primary", disabled: true } };
export const 小さい: Story = { args: { variant: "secondary", size: "sm", children: "聴く" } };
