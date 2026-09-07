import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { TakeScreen } from ".";
import { api } from "~/lib/ipc";
import type { OtoView, RowTakesView } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

const ID = "11111111-1111-4111-8111-111111111111";

const otos: OtoView[] = ["か", "き", "く", "け", "こ"].map((alias, i) => ({
  alias,
  offset_ms: 120 + i * 600,
  consonant_ms: 92,
  cutoff_ms: -420,
  preutterance_ms: 152,
  overlap_ms: 48,
}));

const rows: RowTakesView[] = [
  {
    row_id: "s001",
    text: "あ い う え お",
    state: "recorded",
    units: 5,
    takes: [{ take_id: 9, generation: 1, peak: 0.6, duration_ms: 2900, invalid: false }],
    adopted: 9,
  },
  {
    row_id: "s002",
    text: "か き く け こ",
    state: "recorded",
    units: 5,
    takes: [
      { take_id: 10, generation: 1, peak: 0.68, duration_ms: 3020, invalid: false },
      { take_id: 11, generation: 2, peak: 0.71, duration_ms: 3100, invalid: false },
      { take_id: 12, generation: 3, peak: 0.65, duration_ms: 2940, invalid: false },
    ],
    adopted: 11,
  },
  {
    row_id: "s003",
    text: "さ し す せ そ",
    state: "unrecorded",
    units: 5,
    takes: [],
    adopted: null,
  },
];

const points = Array.from({ length: 600 }, (_, i) => {
  const t = i / 600;
  const env = Math.exp(-(((t % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

const 台帳 = () => {
  mocked(api.openProject).mockResolvedValue({
    next_row_id: "s003",
    next_row_text: "さ し す せ そ",
    covered: 10,
    required: 102,
    coverage: "InProgress",
    handoff: "NotExported",
    singable_songs: 0,
    songs_in_bank: 4,
  });
  mocked(api.rowsWithTakes).mockResolvedValue(rows);
  mocked(api.otosOfTake).mockResolvedValue(otos);
  mocked(api.waveformWindow).mockResolvedValue(points);
};

const meta = {
  title: "画面/TakeScreen",
  component: TakeScreen,
  parameters: { layout: "fullscreen" },
  beforeEach: 台帳,
} satisfies Meta<typeof TakeScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 三回録った行: Story = {
  render: () => withRouter(<TakeScreen />, `/take?id=${ID}&row=s002`),
  play: async ({ canvasElement }) => {
    // 描けたことを先に確かめる。無いものを数える検査は、失敗の面でも通る。
    await waitFor(() => expect(canvasElement.textContent).toContain("録った回"));
    // 行 ID を面に出さない（`Q-REC-003`、`TR-REC-18`）。
    await expect(canvasElement.textContent).not.toContain("s002");
  },
};

export const まだ録っていない行: Story = {
  render: () => withRouter(<TakeScreen />, `/take?id=${ID}&row=s003`),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("まだ録っていません"));
  },
};

export const どの回か分からない: Story = {
  render: () => withRouter(<TakeScreen />, "/take"),
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("声へ戻る");
  },
};
