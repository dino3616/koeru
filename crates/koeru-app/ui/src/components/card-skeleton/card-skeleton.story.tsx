import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { CardSkeleton } from "~/components/card-skeleton";

/** `Suspense` の受け皿。中身が来る前に枠と見出しを出す。 */
const meta = {
  title: "部品/CardSkeleton",
  component: CardSkeleton,
  args: { title: "録れたもの一覧" },
} satisfies Meta<typeof CardSkeleton>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 読み込み中: Story = {
  play: async ({ canvasElement }) => {
    // 画面を見ていない人にも届くこと。`Spinner` は `aria-hidden`。
    await expect(canvasElement.querySelector("[role='status']")).not.toBeNull();
    await expect(canvasElement.querySelector("[aria-hidden='true']")).not.toBeNull();
  },
};
