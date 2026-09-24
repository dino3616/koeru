import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor, within } from "storybook/test";

import { TonePicker } from ".";
import { api } from "~/lib/ipc";

/** C1〜B7 の 84 半音（`TR-RCL-06`）。story では要るところだけ。 */
const NAMES = ["C3", "E3", "G3", "A3", "C4", "D4", "E4", "A4"] as const;
const MIDI = [48, 52, 55, 57, 60, 62, 64, 69] as const;

const 台帳 = () => {
  mocked(api.toneOptions).mockResolvedValue(
    MIDI.map((midi, i) => ({ midi, name: NAMES[i] ?? "" })),
  );
  mocked(api.toneSuggestions).mockResolvedValue([
    { label: "1本で始める", why: "まず1本録って、足りなければ後で作り直します。", midi: [57] },
    {
      label: "女声の目安",
      why: "高い曲も低い曲も無理なく歌えます。録る量は3倍。",
      midi: [55, 62, 69],
    },
    {
      label: "男声の目安",
      why: "高い曲も低い曲も無理なく歌えます。録る量は3倍。",
      midi: [48, 57, 64],
    },
  ]);
};

const meta = {
  title: "部品/TonePicker",
  component: TonePicker,
  args: { tones: [57], onChange: fn() },
  beforeEach: 台帳,
  decorators: [(Story) => <div className="w-120">{Story()}</div>],
} satisfies Meta<typeof TonePicker>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 一本から始める: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("A3"));
    // MIDI 番号を画面に出さない（`TR-REC-25`）。
    await expect(canvasElement.textContent).not.toContain("57");
    // 最後の1本は外させない。0 本のプロジェクトは作れない。
    await expect(
      [...canvasElement.querySelectorAll("[aria-label]")].map((e) => e.getAttribute("aria-label")),
    ).not.toContain("A3 を外す");
  },
};

export const 三本選んでいる: Story = {
  args: { tones: [55, 62, 69] },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("G3"));
    // 録る量が本数に比例することを1行で言う（`TR-RCL-26`）。
    await expect(canvasElement.textContent).toContain("3 本なら 3 周");
  },
};

/** 本数も音高も本人が決める（`TR-RCL-01`）。間隔で咎めない。 */
export const 狭い間隔でも咎めない: Story = {
  args: { tones: [60, 62] },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("C4"));
    await expect(canvasElement.textContent).not.toContain("警告");
    await expect(canvasElement.textContent).not.toContain("狭");
  },
};

/** 推奨は出すが、選択は止めない（`TR-RCL-06`）。 */
export const 推奨から始められる: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: /女声の目安/ }));
    await expect(args.onChange).toHaveBeenCalledWith([55, 62, 69]);
  },
};
