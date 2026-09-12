import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn, mocked } from "storybook/test";

import { SongDetail } from ".";
import { api } from "~/lib/ipc";
import type { SongView } from "~/lib/ipc";

const song: SongView = {
  id: "s2",
  title: "さくらさくら",
  singability: "Unavailable",
  singable: false,
  covered: 12,
  required: 24,
  missing_units: 12,
  missing_rows: 3,
  seconds: 36,
  total_moras: 24,
};

const meta = {
  title: "領域/SongDetail",
  component: SongDetail,
  args: { voiceId: "11111111-1111-4111-8111-111111111111", song, onRecordFrom: fn() },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof SongDetail>;

export default meta;
type Story = StoryObj<typeof meta>;

export const あと三行: Story = {
  beforeEach: () => {
    mocked(api.songPlan).mockResolvedValue({
      rows: [
        { row_id: "s018", text: "ら り る れ ろ", units: 5 },
        { row_id: "s019", text: "りゃ りゅ りょ", units: 3 },
        { row_id: "s020", text: "わ を ん", units: 3 },
      ],
      covers: 11,
      unreachable: 0,
      seconds: 36,
    });
  },
};

export const 届かない音がある: Story = {
  beforeEach: () => {
    mocked(api.songPlan).mockResolvedValue({
      rows: [{ row_id: "s018", text: "ら り る れ ろ", units: 5 }],
      covers: 5,
      unreachable: 2,
      seconds: 12,
    });
  },
};

export const 全部録れている: Story = {
  beforeEach: () => {
    mocked(api.songPlan).mockResolvedValue({ rows: [], covers: 0, unreachable: 0, seconds: 0 });
  },
  args: { song: { ...song, singable: true, missing_units: 0, missing_rows: 0 } },
};
