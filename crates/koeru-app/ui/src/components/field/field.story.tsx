import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Field } from ".";

const meta = {
  title: "部品/Field",
  component: Field,
  args: { id: "name", label: "名前", placeholder: "ミナ" },
} satisfies Meta<typeof Field>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement }) => {
    // 名札が `for` で結ばれている。`placeholder` は名前にならない。
    await expect(canvasElement.querySelector("label")?.getAttribute("for")).toBe("name");
  },
};

export const 説明つき: Story = {
  args: { hint: "あとから変えられます。絵文字や記号も使えます。" },
};

export const 打ってある: Story = { args: { defaultValue: "ミナ" } };
