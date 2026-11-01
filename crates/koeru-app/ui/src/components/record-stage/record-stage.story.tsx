import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor, within } from "storybook/test";

import { RecordStage } from ".";
import { TakeSwitch } from "~/components/take-switch";
import { api } from "~/lib/ipc";
import type { PitchTrack } from "~/lib/ipc";
import { voiceStyle } from "~/lib/voice-color";

/** 「か き く け こ」を 120 BPM で。 1モーラ 500 ms に、前後の無声。 */
const pitch: PitchTrack = {
  hop: 256,
  hz: Array.from({ length: 520 }, (_, i) => {
    if (i < 40 || i > 470 || i % 86 < 6) return null;
    return 220 * 2 ** (Math.sin(i / 60) / 12);
  }),
};

const points = Array.from({ length: 600 }, (_, i) => {
  const env = Math.exp(-((((i / 600) % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

const meta = {
  title: "領域/RecordStage",
  component: RecordStage,
  parameters: { layout: "fullscreen" },
  decorators: [
    // 声の色は置く側が与える。 実画面ではシェルがこの役をする。
    (Story) => (
      <div
        className="flex h-200 w-270 flex-col p-8"
        /* 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。 */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle({ hue: 168, chroma: 0.6, lightness: 0.5 })}
      >
        {Story()}
      </div>
    ),
  ],
  args: {
    rowText: "か き く け こ",
    phase: "idle",
    count: null,
    beatMs: 500,
    take: null,
    pitch: undefined,
    ready: true,
    canGoBack: true,
    onRecord: fn(),
    onStop: fn(),
    onCancel: fn(),
    onBack: fn(),
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
    await expect(c.getByRole("img", { name: "か き く け こ" })).toBeVisible();
    await expect(c.getByRole("img", { name: "いま入っている音の形" })).toBeVisible();
    // 録っていないので聴くものが無い。
    await expect(c.getByRole("button", { name: "再生" })).toBeDisabled();

    // Tab だけで録るところまで届く。 ショートカットは持たない。
    await userEvent.tab();
    await expect(document.activeElement).toBe(c.getByRole("button", { name: "前の行" }));
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
    // 数えているあいだは前の行へも再生へも行かせない。
    await expect(c.getByRole("button", { name: "前の行" })).toBeDisabled();
  },
};

/** 録音中。 いま歌うモーラが拍で進む。 */
export const 録音中: Story = {
  args: { phase: "recording", beatMs: 200 },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const current = () => canvasElement.querySelector("span[data-state='current']")?.textContent;
    await waitFor(() => expect(current()).toBe("か"));
    await waitFor(() => expect(current()).toBe("き"), { timeout: 1000 });
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

/** 録ったあと。 採用テイクの波形と F0、右上にテイクの切り替え。 */
export const 確定後: Story = {
  args: {
    take: { id: 12, durationMs: 3100, peak: 0.71 },
    pitch,
    takes: (
      <TakeSwitch
        takes={[
          { id: 11, generation: 1, label: null, invalid: false },
          { id: 12, generation: 2, label: "ちょい明め", invalid: false },
        ]}
        adopted={12}
        onAdopt={fn()}
        onRename={fn()}
      />
    ),
  },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("img", { name: /^録れた音の形/ })).toBeVisible();
    // F0 の名前は音名で言う（`TR-REC-25`）。 Hz を出さない。
    const trace = c.getByRole("img", { name: /^音の高さの流れ/ });
    await expect(trace.getAttribute("aria-label")).toMatch(/[A-G]#?\d/);
    await userEvent.click(c.getByRole("button", { name: "再生" }));
    await expect(args.onPlay).toHaveBeenCalledOnce();
  },
};

/** F0 の解析がまだ。 波形は出し、F0 の場所は空けたまま言う。 */
export const F0がまだ: Story = {
  args: { take: { id: 12, durationMs: 3100, peak: 0.71 }, pitch: null },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByText("音の高さはまだ解析しています。")).toBeVisible();
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
  args: { rowText: null, canGoBack: false },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("この枠で録る行はもうありません。")).toBeVisible();
    await expect(c.getByRole("button", { name: "録る" })).toBeDisabled();
    await expect(c.getByRole("button", { name: "前の行" })).toBeDisabled();
  },
};
