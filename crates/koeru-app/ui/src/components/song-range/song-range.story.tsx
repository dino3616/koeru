import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { SongRange } from ".";

const notes = [
  { lyric: "さ", tone: "A4", ticks: 480 },
  { lyric: "く", tone: "A4", ticks: 480 },
  { lyric: "ら", tone: "B4", ticks: 960 },
  { lyric: "さ", tone: "A4", ticks: 480 },
  { lyric: "く", tone: "A4", ticks: 480 },
  { lyric: "ら", tone: "B4", ticks: 960 },
];

const meta = {
  title: "部品/SongRange",
  component: SongRange,
  args: { notes, onRepack: fn(), building: false },
} satisfies Meta<typeof SongRange>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 何も選ばなければ曲ぜんぶ（`TR-RCL-12`）。 */
export const 選ぶ前: Story = {
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("曲ぜんぶ");
  },
};

/** 連続した拍は1つの範囲に畳む。飛びは別の範囲になる（`TR-RCL-12`）。 */
export const 飛んだ範囲を選ぶ: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const beats = await c.findAllByRole("button", { pressed: false });
    // 0,1 と 4 を選ぶ。
    await userEvent.click(beats[0] as HTMLElement);
    await userEvent.click(beats[1] as HTMLElement);
    await userEvent.click(beats[4] as HTMLElement);
    await userEvent.click(await c.findByRole("button", { name: "ここを歌えるようにする" }));
    await expect(args.onRepack).toHaveBeenCalledWith([
      { from: 0, to: 2 },
      { from: 4, to: 5 },
    ]);
  },
};

export const 作っている: Story = { args: { building: true } };
