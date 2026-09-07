import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked } from "storybook/test";

import { TakeWaveform } from ".";
import { api } from "~/lib/ipc";
import type { OtoView } from "~/lib/ipc";

/** 5音ぶんの区間。1行から5つ取れる（`TR-RCL-03`）。 */
const otos: OtoView[] = ["か", "き", "く", "け", "こ"].map((alias, i) => ({
  alias,
  offset_ms: 120 + i * 600,
  consonant_ms: 60,
  cutoff_ms: -420,
  preutterance_ms: 90,
  overlap_ms: 30,
}));

/** 上下対称でない包絡。実際の min/max を描いていることが分かる。 */
const points = Array.from({ length: 600 }, (_, i) => {
  const t = i / 600;
  const env = Math.exp(-(((t % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

const meta = {
  title: "領域/TakeWaveform",
  component: TakeWaveform,
  args: { takeId: 12, durationMs: 3100, peak: 0.71, otos, selected: "き" },
  decorators: [(Story) => <div className="w-[640px]">{Story()}</div>],
  beforeEach: () => {
    mocked(api.waveformWindow).mockResolvedValue(points);
  },
} satisfies Meta<typeof TakeWaveform>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 五音取れた: Story = {
  play: async ({ canvasElement }) => {
    // canvas は名前で中身を伝える。数と単位を含める（`TR-PLT-28`）。
    const label = canvasElement.querySelector("canvas")?.getAttribute("aria-label") ?? "";
    await expect(label).toContain("秒");
    await expect(label).toContain("%");
  },
};

export const 割れている: Story = { args: { peak: 1 } };

export const 音が取れなかった: Story = { args: { otos: [], selected: null } };
