import type { Meta, StoryObj } from "@storybook/react-vite";
import { mocked } from "storybook/test";

import { RecordScreen } from "~/screens/record-screen";
import { api } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

/*
 * 収録画面。縦切りの本体。
 *
 * 画面まるごとの story。 カードが4枚積まれた状態で、見出しの段が飛んで
 * いないか、landmark に名前が付いているかを axe に測らせる——
 * 部品を1つずつ見ても、積んだときの段は見えない。
 */
const PROGRESS = {
  next_row_id: "s003",
  next_row_text: "さ し す せ そ",
  covered: 18,
  required: 142,
  coverage: "Incomplete",
  handoff: "NotExported",
  singable_songs: 1,
  songs_in_bank: 3,
};

const meta = {
  title: "画面/収録",
  component: RecordScreen,
  parameters: { layout: "fullscreen" },
  render: () => withRouter(<RecordScreen />, "/record?id=p1"),
  beforeEach: () => {
    mocked(api.openProject).mockResolvedValue(PROGRESS);
    mocked(api.progress).mockResolvedValue(PROGRESS);
    mocked(api.autoAdvanceMs).mockResolvedValue(3000);
    mocked(api.listDevices).mockResolvedValue([{ id: "d1", name: "MacBook Pro のマイク" }]);
    mocked(api.outputKind).mockResolvedValue("Headphones");
    mocked(api.songStatus).mockResolvedValue([]);
    mocked(api.pendingWork).mockResolvedValue(0);
    mocked(api.rowsWithTakes).mockResolvedValue([]);
  },
} satisfies Meta<typeof RecordScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 収録前: Story = {};

/** 全部録れた。「まだ読めていない」と混ぜない——開いた直後に出してはいけない。 */
export const 全部録れた: Story = {
  beforeEach: () => {
    const done = { ...PROGRESS, next_row_id: null, next_row_text: null, covered: 142 };
    mocked(api.openProject).mockResolvedValue(done);
    mocked(api.progress).mockResolvedValue(done);
  },
};

/** 識別子が無いまま開かれた（履歴から直接来たとき）。落とさず戻る道を出す。 */
export const 音源が選ばれていない: Story = {
  render: () => withRouter(<RecordScreen />, "/record"),
};
