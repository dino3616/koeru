import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { NoteField } from ".";

const meta = {
  title: "部品/NoteField",
  component: NoteField,
  args: { id: "terms", label: "使ってよい範囲", placeholder: "自由に使えます。" },
} satisfies Meta<typeof NoteField>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement }) => {
    // 名札が `for` で結ばれている。`placeholder` は名前にならない。
    await expect(canvasElement.querySelector("label")?.getAttribute("for")).toBe("terms");
  },
};

export const 説明つき: Story = {
  args: { hint: "書かなくても配れます。書いた分だけ説明書に載ります。" },
};

export const 六行: Story = { args: { lines: 6 } };

export const 十行: Story = { args: { lines: 10 } };

export const 打ってある: Story = {
  args: { defaultValue: "商用利用もできます。\nクレジットは任意です。", lines: 6 },
};
