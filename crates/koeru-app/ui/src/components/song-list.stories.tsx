import type { Meta, StoryObj } from "@storybook/react-vite";
import { mocked } from "storybook/test";

import { SongList } from "~/components/song-list";
import { api } from "~/lib/ipc";

/*
 * 歌える曲（`TR-RCL-19`）。
 *
 * 3つの状態を出す。 そのまま歌える・代替で歌える・まだ歌えない。
 * 代替ありは「歌えるが同じ音ではない」ことを伝える必要がある。
 */
const meta = {
  title: "部品/SongList",
  component: SongList,
  args: { revision: 0 },
  beforeEach: () => {
    mocked(api.pendingWork).mockResolvedValue(0);
  },
} satisfies Meta<typeof SongList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 三つの状態: Story = {
  beforeEach: () => {
    mocked(api.songStatus).mockResolvedValue([
      {
        id: "s1",
        title: "きらきら星",
        singability: "Complete",
        singable: true,
        covered: 24,
        required: 24,
        missing_units: 0,
        missing_rows: 0,
        seconds: 18.4,
        total_moras: 24,
      },
      {
        id: "s2",
        title: "さくらさくら",
        singability: "WithFallback",
        singable: true,
        covered: 19,
        required: 22,
        missing_units: 3,
        missing_rows: 1,
        seconds: 26.1,
        total_moras: 22,
      },
      {
        id: "s3",
        title: "夏の思い出",
        singability: "Unavailable",
        singable: false,
        covered: 8,
        required: 31,
        missing_units: 23,
        missing_rows: 5,
        seconds: 41,
        total_moras: 31,
      },
    ]);
  },
};

export const まだ1曲も無い: Story = {
  beforeEach: () => {
    mocked(api.songStatus).mockResolvedValue([]);
  },
};

/** 録った音の前処理が残っている（`TR-SYN-33`）。無言で待たせない。 */
export const 前処理を待っている: Story = {
  beforeEach: () => {
    mocked(api.songStatus).mockResolvedValue([]);
    mocked(api.pendingWork).mockResolvedValue(3);
  },
};

export const 読み込みに失敗した: Story = {
  beforeEach: () => {
    mocked(api.songStatus).mockRejectedValue({
      kind: "app.ledger_unreadable",
      message: "台帳を読めませんでした",
    });
  },
};
