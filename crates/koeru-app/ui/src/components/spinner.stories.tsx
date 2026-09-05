import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Spinner } from "~/components/spinner";
import { Button } from "~/components/ui/button";

/*
 * 待っていることを示す印。
 *
 * 塗りの上と面の上の両方で出す。 `border-current` で字の色を継ぐので、
 * 置いた場所によって色が変わる——片方だけ出すと、もう片方を測らない。
 */
const meta = {
  title: "部品/Spinner",
  component: Spinner,
} satisfies Meta<typeof Spinner>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 面の上: Story = {
  render: () => (
    <span className="flex items-center gap-2 bg-slate-2 p-3 text-sm text-slate-11">
      <Spinner />
      入力を確かめています
    </span>
  ),
};

export const ボタンの中: Story = {
  render: () => (
    <div className="flex gap-3">
      <Button variant="primary" disabled>
        <Spinner />
        確かめています
      </Button>
      <Button variant="danger" disabled>
        <Spinner />
        止めています
      </Button>
    </div>
  ),
  /*
   * 読み上げへ二重に出さない。
   *
   * 待ちを伝えるのは読み上げ領域の文言のほう（`TR-PLT-29`）。
   * 印にも名前が付くと、同じことが2回読まれる。
   */
  play: async ({ canvasElement }) => {
    const marks = canvasElement.querySelectorAll("[aria-hidden='true']");
    await expect(marks.length).toBe(2);
  },
};
