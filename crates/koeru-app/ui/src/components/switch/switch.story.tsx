import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { Switch } from ".";

/*
 * 切っている・入っている・押せないを全部出す（`DEC-PLT-022`）。
 * 1つだけ出すと、残りの軌道と縁の配色は axe に一度も当たらない。
 */
const meta = {
  title: "部品/Switch",
  component: Switch,
  args: {
    label: "利用状況",
    description: "画面への到達・離脱した場所",
    checked: false,
    onCheckedChange: fn(),
  },
} satisfies Meta<typeof Switch>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 切っている: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const control = canvas.getByRole("switch", { name: "利用状況" });
    await expect(control.getAttribute("aria-checked")).toBe("false");
    // 説明は名前に混ざらず、説明として読まれる。
    await expect(control).toHaveAccessibleDescription("画面への到達・離脱した場所");
    // Space でも切り替わる。
    control.focus();
    await userEvent.keyboard(" ");
    await expect(args.onCheckedChange).toHaveBeenCalledWith(true);
  },
};

export const 入っている: Story = {
  args: { checked: true },
  play: async ({ canvasElement, args }) => {
    const control = within(canvasElement).getByRole("switch", { name: "利用状況" });
    await expect(control.getAttribute("aria-checked")).toBe("true");
    await userEvent.click(control);
    await expect(args.onCheckedChange).toHaveBeenCalledWith(false);
  },
};

export const 説明なし: Story = {
  render: ({ checked, onCheckedChange }) => (
    <Switch label="クラッシュ報告" checked={checked} onCheckedChange={onCheckedChange} />
  ),
};

export const 押せない: Story = {
  args: { disabled: true },
};
