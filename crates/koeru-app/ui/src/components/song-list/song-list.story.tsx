import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { SongList } from ".";
import type { SongView } from "~/lib/ipc";

const song = (
  id: string,
  title: string,
  singability: string,
  singable: boolean,
  missingUnits: number,
  missingRows: number,
): SongView => ({
  id,
  title,
  singability,
  singable,
  covered: 24 - missingUnits,
  required: 24,
  missing_units: missingUnits,
  missing_rows: missingRows,
  seconds: 18.4,
  total_moras: 24,
});

const songs = [
  song("s1", "かえるの合唱", "Complete", true, 0, 0),
  song("s2", "さくらさくら", "Unavailable", false, 12, 3),
  song("s3", "ふるさと", "Unavailable", false, 33, 8),
  song("s4", "よあけ", "WithFallback", true, 6, 2),
];

const meta = {
  title: "領域/SongList",
  component: SongList,
  args: { songs, preparingId: null, onSing: fn() },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof SongList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 三つの状態: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 欠けを不足として書かない（`docs/design/direction.md`）。
     * 「未達」「あと〇〇%」は出さない。
     */
    const text = canvasElement.textContent ?? "";
    for (const word of ["未達", "未収録", "%"]) {
      await expect(text).not.toContain(word);
    }
  },
};

export const 用意している: Story = { args: { preparingId: "s1" } };

export const 選べる: Story = { args: { onSelect: fn(), selectedId: "s2" } };

export const 曲が無い: Story = { args: { songs: [] } };
