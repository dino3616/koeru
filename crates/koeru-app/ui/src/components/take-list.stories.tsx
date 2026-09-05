import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn, mocked } from "storybook/test";

import { TakeList } from "~/components/take-list";
import { api } from "~/lib/ipc";

/*
 * 録れたものの一覧（`TR-REC-21`、`TR-RCL-25`）。
 *
 * 世代を並べる。 録り直しは上書きせず積むので、採用中がどれかと、
 * 取りこぼしで無効になったものが見分けられる必要がある。
 */
const meta = {
  title: "部品/TakeList",
  component: TakeList,
  args: { revision: 0, busy: false, onRetake: fn(), onPlay: fn() },
} satisfies Meta<typeof TakeList>;

export default meta;
type Story = StoryObj<typeof meta>;

const ROWS = [
  {
    row_id: "s001",
    text: "あ い う え お",
    state: "Recorded",
    adopted: 2,
    takes: [
      { take_id: 1, generation: 1, duration_ms: 2100, invalid: false },
      { take_id: 2, generation: 2, duration_ms: 2340, invalid: false },
    ],
  },
  {
    row_id: "s002",
    text: "か き く け こ",
    state: "Recorded",
    adopted: 4,
    takes: [
      // 取りこぼしたテイクは自動で無効になる（`TR-REC-07`）。押せない。
      { take_id: 3, generation: 1, duration_ms: 1980, invalid: true },
      { take_id: 4, generation: 2, duration_ms: 2210, invalid: false },
    ],
  },
  { row_id: "s003", text: "さ し す せ そ", state: "Pending", adopted: null, takes: [] },
];

export const 世代が積まれている: Story = {
  beforeEach: () => {
    mocked(api.rowsWithTakes).mockResolvedValue(ROWS);
  },
};

/** 収録中は録り直しを出さない。二重に開かせないため。 */
export const 収録中: Story = {
  args: { busy: true },
  beforeEach: () => {
    mocked(api.rowsWithTakes).mockResolvedValue(ROWS);
  },
};

export const まだ何も無い: Story = {
  beforeEach: () => {
    mocked(api.rowsWithTakes).mockResolvedValue([]);
  },
};
