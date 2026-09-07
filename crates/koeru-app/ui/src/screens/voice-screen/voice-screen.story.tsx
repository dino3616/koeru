import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { VoiceScreen } from ".";
import { api } from "~/lib/ipc";
import type { RowTakesView, SongView, VoiceStateView } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

const ID = "11111111-1111-4111-8111-111111111111";

const voice: VoiceStateView = {
  display_name: "ミナ",
  method: "single",
  rows: 21,
  covered: 30,
  required: 102,
  singable_songs: 2,
  songs_in_bank: 4,
  // 五十音の行（内側から あ か が さ ざ た だ な は ば ぱ ま や ら わ）。30 音ぶん。
  rings: [
    { covered: 6, total: 6 },
    { covered: 8, total: 8 },
    { covered: 8, total: 8 },
    { covered: 8, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 3 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 3 },
    { covered: 0, total: 8 },
    { covered: 0, total: 2 },
  ],
  color: { hue: 318, chroma: 0.6, lightness: 0.45 },
};

const row = (id: string, text: string, takes: number): RowTakesView => ({
  row_id: id,
  text,
  state: takes > 0 ? "recorded" : "unrecorded",
  units: text.split(" ").length,
  takes: Array.from({ length: takes }, (_, i) => ({
    take_id: i + 1,
    generation: i + 1,
    peak: 0.71,
    duration_ms: 3100,
    invalid: false,
  })),
  adopted: takes > 0 ? takes : null,
});

const rows = [
  row("s001", "あ い う え お", 2),
  row("s002", "か き く け こ", 1),
  row("s003", "が ぎ ぐ げ ご", 1),
  row("s004", "さ し す せ そ", 0),
  row("s005", "ざ じ ず ぜ ぞ", 0),
];

const songs: SongView[] = [
  {
    id: "s1",
    title: "かえるの合唱",
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
    singability: "Unavailable",
    singable: false,
    covered: 12,
    required: 24,
    missing_units: 12,
    missing_rows: 3,
    seconds: 36,
    total_moras: 24,
  },
];

/** 面が変わっても中央が動かないことを見るので、4つとも出す。 */
const 台帳 = () => {
  mocked(api.openProject).mockResolvedValue({
    next_row_id: "s004",
    next_row_text: "さ し す せ そ",
    covered: 30,
    required: 102,
    coverage: "InProgress",
    handoff: "NotExported",
    singable_songs: 2,
    songs_in_bank: 4,
  });
  mocked(api.progress).mockResolvedValue({
    next_row_id: "s004",
    next_row_text: "さ し す せ そ",
    covered: 30,
    required: 102,
    coverage: "InProgress",
    handoff: "NotExported",
    singable_songs: 2,
    songs_in_bank: 4,
  });
  mocked(api.voiceState).mockResolvedValue(voice);
  mocked(api.rowsWithTakes).mockResolvedValue(rows);
  mocked(api.songStatus).mockResolvedValue(songs);
  mocked(api.listDevices).mockResolvedValue([{ id: "builtin", name: "MacBook Pro のマイク" }]);
  // まだ一度も選んでいない音源（`TR-REC-03`）。「録る」は押せない姿で出る。
  mocked(api.chosenDevice).mockResolvedValue({ id: null, armed: false });
  mocked(api.autoAdvanceMs).mockResolvedValue(3000);
  mocked(api.pendingWork).mockResolvedValue(0);
  mocked(api.streamEnvelope).mockResolvedValue(1);
  mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
};

const meta = {
  title: "画面/VoiceScreen",
  component: VoiceScreen,
  parameters: { layout: "fullscreen" },
  beforeEach: 台帳,
} satisfies Meta<typeof VoiceScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 音: Story = {
  render: () => withRouter(<VoiceScreen />, `/voice?id=${ID}&tab=sound`),
  play: async ({ canvasElement }) => {
    // 進み具合と歌える曲を、どちらも隠さない（`TR-RCL-19`）。
    await waitFor(() => expect(canvasElement.textContent).toContain("102 音"));
    await expect(canvasElement.textContent).toContain("曲が歌える");
    // 準備は設定の面にある。収録の面に残すのは1行だけ（`DEC-PLT-024`）。
    await expect(canvasElement.textContent).not.toContain("入力レベル");
  },
};

export const 曲: Story = {
  render: () => withRouter(<VoiceScreen />, `/voice?id=${ID}&tab=songs`),
  beforeEach: () => {
    台帳();
    mocked(api.songPlan).mockResolvedValue({
      rows: [{ row_id: "s018", text: "ら り る れ ろ", units: 5 }],
      covers: 5,
      unreachable: 0,
      seconds: 12,
    });
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("歌わせたい曲"));
  },
};

export const 配り物: Story = {
  render: () => withRouter(<VoiceScreen />, `/voice?id=${ID}&tab=package`),
  beforeEach: () => {
    台帳();
    mocked(api.preflight).mockResolvedValue({
      renamed_to_nfc: 0,
      non_nfc_names: [],
      clipped_takes: [],
      may_export: true,
    });
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("書き出す前に見ること"));
  },
};

export const 設定: Story = {
  render: () => withRouter(<VoiceScreen />, `/voice?id=${ID}&tab=settings`),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("録るときの音"));
  },
};

/** 識別子が無いまま開かれたとき。落とさず、戻る道を出す。 */
export const どの声か分からない: Story = {
  render: () => withRouter(<VoiceScreen />, "/voice"),
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("声へ戻る");
  },
};
