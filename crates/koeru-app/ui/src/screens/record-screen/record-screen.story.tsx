import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { RecordScreen } from ".";
import { AppShell } from "~/components/app-shell";
import { NAGI, 棚_三つ } from "~/components/app-shell/fixtures";
import { boardPitch, boardWave } from "~/components/record-timeline/fixture";
import { api } from "~/lib/ipc";
import type {
  GuideTempo,
  PitchTrack,
  ProgressView,
  RowTakesView,
  SongView,
  Subbank,
  TakeView,
} from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

const NORMAL = "1";
const WHISPER = "2";

const take = (id: number, generation: number, label: string | null = null) => ({
  take_id: id,
  generation,
  peak: 0.71,
  duration_ms: 3100,
  invalid: false,
  recorded_at: "2026-09-21T18:24:00Z",
  label,
});

const row = (
  id: string,
  subbank: string,
  text: string,
  takes: ReturnType<typeof take>[] = [],
): RowTakesView => ({
  row_id: id,
  subbank,
  text,
  state: takes.length > 0 ? "recorded" : "unrecorded",
  units: text.split(" ").length,
  moras: text.split(" ").length,
  risk_hard: 0,
  targets: [],
  takes,
  adopted: takes.at(-1)?.take_id ?? null,
});

const rows = [
  row("n1", NORMAL, "あ い う え お", [take(11, 1), take(12, 2, "ちょい明め")]),
  row("n2", NORMAL, "か き く け こ"),
  row("n3", NORMAL, "さ し す せ そ"),
  row("w1", WHISPER, "た ち つ て と"),
  row("w2", WHISPER, "な に ぬ ね の"),
];

const subbank = (id: string, voiceColor: string, done: number, total: number): Subbank => ({
  id,
  voiceColor,
  method: "CVVC",
  preset: null,
  tone: { midi: 57, name: "A3" },
  toneRange: { low: { midi: 50, name: "D3" }, high: { midi: 62, name: "D4" }, edited: false },
  progress: { recordedRows: done, rows: total, remainingSeconds: 60 },
  hasTakes: done > 0,
});

const progress: ProgressView = {
  next_row_id: "n2",
  next_row_text: "か き く け こ",
  covered: 5,
  required: 25,
  coverage: "InProgress",
  handoff: "NotExported",
  singable_songs: 0,
  songs_in_bank: 1,
  remaining_seconds: 240,
  remaining_rows: 4,
  by_tone: [],
};

const songs: SongView[] = [
  {
    id: "s1",
    title: "かえるの合唱",
    singability: "Unavailable",
    singable: false,
    covered: 5,
    required: 24,
    missing_units: 19,
    missing_rows: 3,
    seconds: 18.4,
    total_moras: 24,
    previewable: false,
    transpose: 0,
    recommended_transpose: 0,
    rescuing_tone: null,
    range: null,
  },
];

const tempo: GuideTempo = {
  bpm: 120,
  beatMs: 500,
  defaultBpm: 120,
  minBpm: 60,
  maxBpm: 180,
  stepBpm: 5,
  leadInMs: 1250,
};

const pitch: PitchTrack = {
  hop: 256,
  hz: Array.from({ length: 520 }, (_, i) =>
    i < 40 || i > 470 || i % 86 < 6 ? null : 220 * 2 ** (Math.sin(i / 60) / 12),
  ),
};

const settled: TakeView = {
  take_id: 21,
  row_id: "n2",
  duration_ms: 3100,
  peak: 0.6,
  thumbnail: [],
  has_oto: true,
  confidence: 0.9,
  discontinuities: 0,
  invalidated: false,
  preroll_ms: 500,
  peak_dbfs: -4,
  leading_margin_ms: 300,
  trailing_margin_ms: 300,
  has_required_margins: true,
  followup: null,
};

const points = Array.from({ length: 600 }, (_, i) => {
  const env = Math.exp(-((((i / 600) % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

/** 外枠ごと出す。 サイドバーを畳むこととツールバーの右端は、外枠が無いと見えない。 */
const framed = (search: string) => (
  <div className="h-225 w-360">
    {withRouter(
      <AppShell
        voices={棚_三つ}
        selectedId={NAGI}
        subbanks={[
          { id: NORMAL, voiceColor: "Normal", tone: "A3", method: "CVVC" },
          { id: WHISPER, voiceColor: "Whisper", tone: "A3", method: "CVVC" },
        ]}
      >
        <RecordScreen />
      </AppShell>,
      `/record?id=${NAGI}${search}`,
    )}
  </div>
);

const meta = {
  title: "画面/RecordScreen",
  component: RecordScreen,
  parameters: { layout: "fullscreen" },
  render: () => framed(""),
  beforeEach: () => {
    mocked(api.progress).mockResolvedValue(progress);
    mocked(api.rowsWithTakes).mockResolvedValue(rows);
    mocked(api.songStatus).mockResolvedValue(songs);
    mocked(api.subbanks).mockResolvedValue([
      subbank(NORMAL, "Normal", 1, 3),
      subbank(WHISPER, "Whisper", 0, 2),
    ]);
    mocked(api.recordingOrder).mockImplementation(async (sb) => ({
      mode: "CoverageEfficiency",
      row_ids: sb === WHISPER ? ["w1", "w2"] : ["n2", "n3", "w1", "w2"],
    }));
    mocked(api.listDevices).mockResolvedValue([{ id: "mic-1", name: "内蔵マイク" }]);
    mocked(api.chosenDevice).mockResolvedValue({ id: "mic-1", armed: true, recording: false });
    mocked(api.guideTempo).mockResolvedValue(tempo);
    mocked(api.autoAdvanceMs).mockResolvedValue(3000);
    mocked(api.streamEnvelope).mockResolvedValue(1);
    mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
    mocked(api.waveformWindow).mockResolvedValue(points);
    mocked(api.takePitch).mockResolvedValue(pitch);
    mocked(api.startTake).mockResolvedValue("n2");
    mocked(api.startRetake).mockImplementation(async (r) => r);
    mocked(api.finishTake).mockResolvedValue(settled);
  },
} satisfies Meta<typeof RecordScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

const sidebarToggle = (canvasElement: HTMLElement) =>
  within(canvasElement).findByRole("button", { name: "サイドバー" });

/** 待機。 中央は次に録る行で、ツールバーの右端に録る道具が並ぶ。 */
export const 待機: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: "か き く け こ" })).toBeVisible();
    const tempoUp = c.getByRole("button", { name: "BPM を上げる" });
    await expect(tempoUp.closest("header")).not.toBeNull();
    await expect(tempoUp).toBeEnabled();
    await expect((await sidebarToggle(canvasElement)).getAttribute("aria-expanded")).toBe("true");
  },
};

/**
 * Tab と Enter だけで録って止める。 ショートカットは持たない。
 *
 * 数えているあいだと録っているあいだはサイドバーを畳み、テンポを変えさせない。
 * 止めたら確定して、次の行へ進む。
 */
export const ボタンだけで録る: Story = {
  beforeEach: () => {
    mocked(api.guideTempo).mockResolvedValue({ ...tempo, leadInMs: 300 });
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const record = await c.findByRole("button", { name: "録る" });
    record.focus();
    await userEvent.keyboard("{Enter}");

    await waitFor(() => expect(c.getByRole("button", { name: "止める" })).toBeVisible());
    await expect(api.startTake).toHaveBeenCalledWith(null);
    await expect(c.getByRole("button", { name: "BPM を上げる" })).toBeDisabled();
    await expect((await sidebarToggle(canvasElement)).getAttribute("aria-expanded")).toBe("false");

    // 止めるのも Tab で届くボタン。 確定したら台帳を読み直し、次の行へ進む。
    mocked(api.progress).mockResolvedValue({
      ...progress,
      next_row_id: "n3",
      next_row_text: "さ し す せ そ",
    });
    mocked(api.recordingOrder).mockResolvedValue({
      mode: "CoverageEfficiency",
      row_ids: ["n3", "w1", "w2"],
    });
    c.getByRole("button", { name: "止める" }).focus();
    await userEvent.keyboard("{Enter}");
    await waitFor(() => expect(api.finishTake).toHaveBeenCalledOnce());
    await expect(await c.findByRole("img", { name: "さ し す せ そ" })).toBeVisible();
    await waitFor(async () =>
      expect((await sidebarToggle(canvasElement)).getAttribute("aria-expanded")).toBe("true"),
    );
  },
};

/** 助走を数えている。 押すと取りやめ、録りはじめない。 */
export const カウント: Story = {
  beforeEach: () => {
    mocked(api.guideTempo).mockResolvedValue({ ...tempo, leadInMs: 60_000 });
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "録る" }));
    const counting = await c.findByRole("button", { name: /^止める カウント \d+$/ });
    await expect((await sidebarToggle(canvasElement)).getAttribute("aria-expanded")).toBe("false");
    await userEvent.click(counting);
    await expect(await c.findByRole("button", { name: "録る" })).toBeVisible();
    await expect(api.startTake).not.toHaveBeenCalled();
  },
};

/** 録音中に入ってきた。 Rust が録っているので「止める」から始まる。 */
export const 録音中: Story = {
  beforeEach: () => {
    mocked(api.chosenDevice).mockResolvedValue({ id: "mic-1", armed: true, recording: true });
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("button", { name: "止める" })).toBeVisible();
    await expect(c.getByRole("button", { name: "BPM を下げる" })).toBeDisabled();
    await expect(c.getByRole("combobox", { name: "入力デバイス" })).toBeDisabled();
    await waitFor(async () =>
      expect((await sidebarToggle(canvasElement)).getAttribute("aria-expanded")).toBe("false"),
    );
  },
};

/** 録った行を選ぶと、採用テイクの波形と F0、テイクの切り替えが出る。 名前も変えられる。 */
export const 確定後: Story = {
  render: () => framed("&row=n1"),
  beforeEach: () => {
    mocked(api.renameTake).mockImplementation(async ({ take: t, label }) => ({
      outcome: { __typename: "TakeRenamed", take: { ...take(t, 2), label } },
    }));
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: /^音高の記録。 中央は / })).toBeVisible();
    await expect(c.getByRole("img", { name: "入力の波形。ちょい明め" })).toBeVisible();
    await expect(c.getByRole("radio", { name: "ちょい明め" }).getAttribute("aria-checked")).toBe(
      "true",
    );

    await userEvent.click(c.getByRole("button", { name: "ちょい明め の名前を変える" }));
    const field = c.getByRole("textbox");
    await userEvent.clear(field);
    await userEvent.type(field, "明るめ{Enter}");
    await expect(api.renameTake).toHaveBeenCalledWith({ take: 12, label: "明るめ" });
  },
};

/*
 * S04 のボードと同じ値の場面。 ボードと重ねて撮り、差が DEC-PLT-062 の差だけかを見る。
 *
 * ナギの Normal A3 CVVC、行は 93 / 131、いまの行は「ぱぱぴぷぺぽんば」でエイリアスは 15 個、
 * テイクは「テイク 1」と「ちょい弱め」、BPM 120、マイクは USB Audio CODEC。
 */
const BOARD_ROWS = [
  "な な に ぬ ね の ん は",
  "や や ゆ よ ん ま",
  "ら ら り る れ ろ ん わ",
  "ぱ ぱ ぴ ぷ ぺ ぽ ん ば",
  "ざ ざ じ ず ぜ ぞ ん だ",
  "ば ば び ぶ べ ぼ ん が",
  "が が ぎ ぐ げ ご ん ぱ",
  "きゃ きゃ きゅ きぇ きょ ん ぎゃ",
  "しゃ しゃ しゅ しぇ しょ ん じゃ",
  "ちゃ ちゃ ちゅ ちぇ ちょ ん にゃ",
  "ひゃ ひゃ ひゅ ひぇ ひょ ん びゃ",
  "みゃ みゃ みゅ みぇ みょ ん りゃ",
  "ふぁ ふぁ ふぃ ふぇ ふぉ ん ゔぁ",
  "つぁ つぁ つぃ つぇ つぉ ん てぃ",
  "うぃ うぃ うぇ うぉ ん いぇ",
  "でぃ でぃ どぅ でゅ ん とぅ",
];

const BOARD_ALIASES = [
  "- ぱ",
  "ぱ",
  "a p",
  "ぴ",
  "i p",
  "ぷ",
  "u p",
  "ぺ",
  "e p",
  "ぽ",
  "o n",
  "ん",
  "n b",
  "ば",
  "a -",
];

const boardRows = (): RowTakesView[] => {
  const listed = BOARD_ROWS.map((text, i) => {
    const r = row(`b${i}`, NORMAL, text, i < 3 ? [take(100 + i, 1)] : []);
    return i === 3
      ? {
          ...r,
          targets: BOARD_ALIASES.map((a) => ({ rendered_alias: a })),
          takes: [
            { ...take(11, 1), duration_ms: 4000 },
            { ...take(12, 2, "ちょい弱め"), duration_ms: 4000 },
          ],
          adopted: 12,
        }
      : r;
  });
  // 一覧の下は見えない。 数をボードに合わせるだけの行（録ったものを 90 行）。
  const rest = Array.from({ length: 131 - listed.length }, (_, i) =>
    row(`x${i}`, NORMAL, "あ い う え お", i < 90 ? [take(1000 + i, 1)] : []),
  );
  return [...listed, ...rest];
};

const boardSong = (over: Partial<SongView> & Pick<SongView, "id" | "title">): SongView => ({
  ...(songs[0] as SongView),
  singability: "Complete",
  singable: true,
  covered: 24,
  required: 24,
  missing_rows: 0,
  rescuing_tone: null,
  ...over,
});

const tone = (midi: number, name: string) => ({ midi, name });

const BOARD_SONGS: SongView[] = [
  boardSong({
    id: "s1",
    title: "さくらさくら",
    range: { low: tone(57, "A3"), high: tone(62, "D4") },
  }),
  boardSong({
    id: "s2",
    title: "帰り道ワルツ",
    range: { low: tone(55, "G3"), high: tone(64, "E4") },
  }),
  boardSong({
    id: "s3",
    title: "ひかりのあと",
    singability: "Unavailable",
    singable: false,
    covered: 16,
    missing_rows: 4,
    range: { low: tone(57, "A3"), high: tone(65, "F4") },
  }),
  boardSong({
    id: "s4",
    title: "夜明けのテスト",
    singability: "Unavailable",
    singable: false,
    covered: 10,
    missing_rows: 6,
    rescuing_tone: "D4",
    range: { low: tone(62, "D4"), high: tone(69, "A5") },
  }),
];

/** テイクの波形。 ボードの棒（2マス目から）が同じ高さで出る点の列。 */
const boardPoints = (pixels: number) => {
  const bars = boardWave(266);
  return Array.from({ length: pixels }, (_, j) => {
    const amp = bars[j + 30] ?? 0.02;
    return [-amp, amp] as [number, number];
  });
};

export const ボード: Story = {
  render: () => framed(`&subbank=${encodeURIComponent(JSON.stringify(NORMAL))}`),
  beforeEach: () => {
    mocked(api.rowsWithTakes).mockResolvedValue(boardRows());
    mocked(api.songStatus).mockResolvedValue(BOARD_SONGS);
    mocked(api.subbanks).mockResolvedValue([subbank(NORMAL, "Normal", 93, 131)]);
    mocked(api.recordingOrder).mockResolvedValue({ mode: "SongBankFirst", row_ids: ["b3"] });
    mocked(api.listDevices).mockResolvedValue([{ id: "usb", name: "USB Audio CODEC" }]);
    mocked(api.chosenDevice).mockResolvedValue({ id: "usb", armed: true, recording: false });
    mocked(api.waveformWindow).mockImplementation(async ({ pixels }) => boardPoints(pixels));
    mocked(api.takePitch).mockResolvedValue(boardPitch(8));
    mocked(api.songPlan).mockResolvedValue({
      rows: ["b3", "b4", "b5", "b6"].map((row_id) => ({ row_id, text: "", units: 1 })),
      covers: 4,
      unreachable: 0,
      seconds: 0,
    });
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: "ぱ ぱ ぴ ぷ ぺ ぽ ん ば" })).toBeVisible();
    await userEvent.click(c.getByRole("button", { name: /^ひかりのあと/ }));
    await waitFor(() =>
      expect(c.getAllByRole("option", { name: /ひかりのあと に要る行$/ })).toHaveLength(4),
    );
    await expect(c.getByText("71%")).toBeVisible();
    await expect(c.getByRole("list", { name: "この行のエイリアス" }).children).toHaveLength(15);
  },
};

/** 枠を選ぶと、その枠の行だけを並べ、その枠の録る順から録る（`DEC-RCL-018`）。 */
export const 枠を選んだ: Story = {
  // 枠の識別子は数字だけの文字列。 ルータは数に読めるものを引用符で包んで運ぶ。
  render: () => framed(`&subbank=${encodeURIComponent(JSON.stringify(WHISPER))}`),
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: "た ち つ て と" })).toBeVisible();
    await expect(api.recordingOrder).toHaveBeenCalledWith(WHISPER);
    const list = c.getByRole("listbox");
    await expect(within(list).getAllByRole("option")).toHaveLength(2);
    // カバレッジは枠の行で数える。
    await expect(c.getByText("0%")).toBeVisible();

    await userEvent.click(c.getByRole("button", { name: "録る" }));
    // 助走（1250 ms）を数えてから録りはじめる。
    await waitFor(() => expect(api.startTake).toHaveBeenCalledWith(WHISPER), { timeout: 3000 });
  },
};
