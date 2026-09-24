import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, within } from "storybook/test";

import { SongTarget } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/SongTarget",
  component: SongTarget,
  args: { voiceId: "v1", songId: "s1", title: "さくらさくら" },
  beforeEach: () => {
    mocked(api.songNotes).mockResolvedValue([
      { lyric: "さ", tone: "A4", ticks: 480 },
      { lyric: "く", tone: "A4", ticks: 480 },
      { lyric: "ら", tone: "B4", ticks: 960 },
    ]);
    mocked(api.repackForSelection).mockResolvedValue(2);
  },
} satisfies Meta<typeof SongTarget>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 拍を選ぶ: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findAllByRole("button", { pressed: false })).toHaveLength(3);
  },
};

/** 選んだ範囲を渡して、足した行数を返す（`TR-RCL-16`）。 */
export const 歌えるようにする: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const beats = await c.findAllByRole("button", { pressed: false });
    await userEvent.click(beats[0] as HTMLElement);
    await userEvent.click(await c.findByRole("button", { name: "ここを歌えるようにする" }));
    await expect(api.repackForSelection).toHaveBeenCalledWith([
      { song_id: "s1", ranges: [{ from: 0, to: 1 }] },
    ]);
    await expect(await c.findByText(/行を足しました/)).toBeTruthy();
  },
};
