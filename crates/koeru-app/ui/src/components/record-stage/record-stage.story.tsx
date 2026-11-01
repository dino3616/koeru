import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor, within } from "storybook/test";

import { RecordStage } from ".";
import { boardPitch } from "~/components/record-timeline/fixture";
import { TakeSwitch } from "~/components/take-switch";
import { api } from "~/lib/ipc";
import { voiceStyle } from "~/lib/voice-color";

/** ボードの行（ぱぱぴぷぺぽんば）のエイリアス 15 個。 */
const ALIASES = [
  "- ぱ",
  "ぱ",
  "a p",
  "a ぱ",
  "a ぴ",
  "i p",
  "i ぷ",
  "u p",
  "u ぺ",
  "e p",
  "e ぽ",
  "o n",
  "ん",
  "n b",
  "ば",
];

const points = Array.from({ length: 600 }, (_, i) => {
  const env = Math.exp(-((((i / 600) % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

const takeSwitch = (
  <TakeSwitch
    takes={[
      { id: 11, generation: 1, label: null, invalid: false },
      { id: 12, generation: 2, label: "ちょい弱め", invalid: false },
    ]}
    adopted={12}
    onAdopt={fn()}
    onRename={fn()}
    onNew={fn()}
  />
);

const meta = {
  title: "領域/RecordStage",
  component: RecordStage,
  parameters: { layout: "fullscreen" },
  decorators: [
    // 声の色は置く側が与える。 実画面ではシェルがこの役をする。 幅は 1440 の窓の中央の段。
    (Story) => (
      <div
        className="flex h-210 w-277 bg-bg"
        /* 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。 */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle({ hue: 168, chroma: 0.6, lightness: 0.5 })}
      >
        {Story()}
      </div>
    ),
  ],
  args: {
    voiceId: "v1",
    rowText: "ぱ ぱ ぴ ぷ ぺ ぽ ん ば",
    aliases: ALIASES,
    phase: "idle",
    count: null,
    beatMs: 500,
    take: null,
    pitch: undefined,
    tone: 57,
    ready: true,
    onRecord: fn(),
    onStop: fn(),
    onCancel: fn(),
    onPlay: fn(),
  },
  beforeEach: () => {
    mocked(api.streamEnvelope).mockResolvedValue(1);
    mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
    mocked(api.waveformWindow).mockResolvedValue(points);
  },
} satisfies Meta<typeof RecordStage>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 待機。 まだ録っていない行は、いま入っている音を出す。 */
export const 待機: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("img", { name: "ぱ ぱ ぴ ぷ ぺ ぽ ん ば" })).toBeVisible();
    await expect(c.getByRole("img", { name: "いま入っている音の形" })).toBeVisible();
    // 録っていないので聴くものも、戻す再生位置も無い。
    await expect(c.getByRole("button", { name: "再生" })).toBeDisabled();
    await expect(c.getByRole("button", { name: "再生ヘッドを先頭に戻す" })).toBeDisabled();

    // エイリアスは作った順に、契約が運んだ綴りのまま並ぶ。
    const list = c.getByRole("list", { name: "この行のエイリアス" });
    await expect(
      within(list)
        .getAllByRole("listitem")
        .map((li) => li.textContent),
    ).toEqual(ALIASES);

    // Tab だけで録るところまで届く。 ショートカットは持たない。
    await userEvent.tab();
    await expect(document.activeElement).toBe(c.getByRole("button", { name: "録る" }));
    await userEvent.keyboard("{Enter}");
    await expect(args.onRecord).toHaveBeenCalledOnce();
  },
};

/** 助走を数えている。 押すと取りやめる——まだ録っていない。 */
export const カウント: Story = {
  args: { phase: "countdown", count: 2 },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const button = c.getByRole("button", { name: "止める カウント 2" });
    await userEvent.click(button);
    await expect(args.onCancel).toHaveBeenCalledOnce();
    await expect(args.onStop).not.toHaveBeenCalled();
    await expect(c.getByRole("button", { name: "再生" })).toBeDisabled();
  },
};

/** 録音中（S04-Record）。 いま歌うモーラが拍で進み、再生位置の線が録る位置を指す。 */
export const 録音中: Story = {
  args: { phase: "recording", beatMs: 200, takes: takeSwitch },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const current = () => canvasElement.querySelector("span[data-state='current']")?.textContent;
    await waitFor(() => expect(current()).toBe("ぱ"));
    await waitFor(() => expect(current()).toBe("ぴ"), { timeout: 1000 });
    await expect(c.getByRole("img", { name: "入力の波形。録音中" })).toBeVisible();
    await userEvent.click(c.getByRole("button", { name: "止める" }));
    await expect(args.onStop).toHaveBeenCalledOnce();
  },
};

/** 確定させている。 録る的は押させない（`TR-REC-42`）。 */
export const 確かめている: Story = {
  args: { phase: "settling" },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole("button", { name: "録る" })).toBeDisabled();
  },
};

/** 録ったあと（S04-Unlocked）。 採用テイクの波形と F0、右上にテイクの切り替え。 */
export const 確定後: Story = {
  args: {
    take: { id: 12, durationMs: 4000, name: "ちょい弱め" },
    pitch: boardPitch(8),
    takes: takeSwitch,
  },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: "入力の波形。ちょい弱め" })).toBeVisible();
    // F0 の名前は音名で言う（`TR-REC-25`）。 Hz を出さない。
    await expect(c.getByRole("img", { name: "音高の記録。 中央は A3" })).toBeVisible();
    await userEvent.click(c.getByRole("button", { name: "再生" }));
    await expect(args.onPlay).toHaveBeenCalledOnce();
    await userEvent.click(c.getByRole("button", { name: "再生ヘッドを先頭に戻す" }));
  },
};

/** F0 の解析がまだ。 波形は出し、F0 の欄は目盛りだけで、名前が言う。 */
export const F0がまだ: Story = {
  args: { take: { id: 12, durationMs: 4000, name: "ちょい弱め" }, pitch: null },
  play: async ({ canvasElement }) => {
    await expect(
      within(canvasElement).getByRole("img", { name: "音高の記録。 まだ解析しています" }),
    ).toBeVisible();
  },
};

/** マイクを選んでいない。 録らせない。 */
export const マイク未選択: Story = {
  args: { ready: false },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole("button", { name: "録る" })).toBeDisabled();
  },
};

/** 枠の行を録り終えた。 */
export const 録る行が無い: Story = {
  args: { rowText: null, aliases: [] },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("この枠で録る行はもうありません。")).toBeVisible();
    await expect(c.getByRole("button", { name: "録る" })).toBeDisabled();
  },
};
