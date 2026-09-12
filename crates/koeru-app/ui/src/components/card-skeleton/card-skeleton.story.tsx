import type { Meta, StoryObj } from "@storybook/react-vite";

import { CardSkeleton } from ".";

const meta = {
  title: "部品/CardSkeleton",
  component: CardSkeleton,
  args: { title: "録るもの" },
} satisfies Meta<typeof CardSkeleton>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {};
