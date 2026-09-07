import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Card } from ".";

const meta = {
  title: "部品/Card",
  component: Card,
} satisfies Meta<typeof Card>;

export default meta;
type Story = StoryObj<typeof meta>;

/*
 * axe が見ないものだけを `play` に書く。
 *
 * 見出しの段の順序は `heading-order` が見るので書かない。
 * 「名前があると landmark になる」は axe に規則が無い。
 */
export const 名前つき: Story = {
  args: { title: "録るもの", children: <p className="text-sm text-slate-12">あ い う え お</p> },
  play: async ({ canvasElement }) => {
    const section = canvasElement.querySelector("section");
    await expect(section?.getAttribute("aria-labelledby")).not.toBeNull();
  },
};

export const 名前なし: Story = {
  args: { children: <p className="text-sm text-slate-12">名前が無い領域は landmark にしない</p> },
  play: async ({ canvasElement }) => {
    const section = canvasElement.querySelector("section");
    await expect(section?.getAttribute("aria-labelledby")).toBeNull();
  },
};

export const 入れ子: Story = {
  args: {
    title: "この声の設定",
    children: (
      <Card title="録るときの音">
        <p className="text-sm text-slate-12">MacBook Pro のマイク</p>
      </Card>
    ),
  },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector("h2")).not.toBeNull();
    await expect(canvasElement.querySelector("h3")).not.toBeNull();
  },
};
