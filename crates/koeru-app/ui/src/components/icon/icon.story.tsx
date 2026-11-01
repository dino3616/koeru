import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { ICON_NAMES, Icon } from ".";

const meta = {
  title: "部品/Icon",
  component: Icon,
  args: { name: "mic", size: 20 },
} satisfies Meta<typeof Icon>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 1つ。 字の色で描かれ、読み上げには出ない。 */
export const 一つ: Story = {
  render: (args) => (
    <span className="text-1">
      <Icon {...args} />
    </span>
  ),
  play: async ({ canvasElement }) => {
    const svg = canvasElement.querySelector("svg");
    await expect(svg?.getAttribute("aria-hidden")).toBe("true");
    await expect(svg?.getAttribute("width")).toBe("20");
  },
};

/** 全部。 名前を添えて並べる。 */
export const 全部: Story = {
  render: () => (
    <ul className="grid grid-cols-6 gap-4 text-1">
      {ICON_NAMES.map((name) => (
        <li key={name} className="flex flex-col items-center gap-2 text-xs text-2">
          <span className="text-1">
            <Icon name={name} size={20} />
          </span>
          {name}
        </li>
      ))}
    </ul>
  ),
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelectorAll("svg")).toHaveLength(ICON_NAMES.length);
  },
};
