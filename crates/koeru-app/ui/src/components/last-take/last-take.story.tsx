import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked } from "storybook/test";

import { LastTake } from ".";
import { api } from "~/lib/ipc";
import type { OtoView, TakeView } from "~/lib/ipc";

/** 1行から5音取れた（`TR-RCL-03`）。 */
const otos: OtoView[] = ["た", "ち", "つ", "て", "と"].map((alias, i) => ({
  alias,
  offset_ms: 120 + i * 600,
  consonant_ms: 92,
  cutoff_ms: -420,
  preutterance_ms: 152,
  overlap_ms: 48,
}));

/** 上下対称でない包絡。実際の min/max を描いていることが分かる。 */
const points = Array.from({ length: 600 }, (_, i) => {
  const t = i / 600;
  const env = Math.exp(-(((t % 0.2) - 0.1) ** 2) / 0.002);
  return [-env * 0.8, env] as [number, number];
});

const thumbnail = Array.from({ length: 512 }, (_, i) => {
  const t = i / 512;
  return Math.round(255 * Math.exp(-((t - 0.45) ** 2) / 0.02));
});

const take: TakeView = {
  take_id: 12,
  row_id: "s006",
  duration_ms: 3100,
  peak: 0.71,
  thumbnail,
  has_oto: true,
  confidence: 0.82,
  discontinuities: 0,
  invalidated: false,
  preroll_ms: 500,
  peak_dbfs: -3.0,
  leading_margin_ms: 420,
  trailing_margin_ms: 380,
  has_required_margins: true,
};

const meta = {
  title: "領域/LastTake",
  component: LastTake,
  args: {
    take,
    rowText: "た ち つ て と",
    units: 5,
    busy: false,
    onListen: fn(),
    onRetake: fn(),
    onOpen: fn(),
  },
  decorators: [(Story) => <div className="w-80">{Story()}</div>],
  beforeEach: () => {
    mocked(api.otosOfTake).mockResolvedValue(otos);
    mocked(api.waveformWindow).mockResolvedValue(points);
  },
} satisfies Meta<typeof LastTake>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 録れた: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 評価語を出さない（`DEC-REC-008`）。測った値だけを出す。
     * 単位も省かない（`docs/design/direction.md`）。
     */
    const text = canvasElement.textContent ?? "";
    for (const word of ["小さすぎ", "歪ん", "ちょうどよい", "うまく"]) {
      await expect(text).not.toContain(word);
    }
    await expect(text).toContain("秒");
  },
};

export const 声を見つけられなかった: Story = {
  args: { take: { ...take, has_oto: false, confidence: null } },
  beforeEach: () => {
    // 切り出しが1つも取れていないので、帯は出ない。
    mocked(api.otosOfTake).mockResolvedValue([]);
    mocked(api.waveformWindow).mockResolvedValue(points);
  },
};

export const とぎれた: Story = {
  args: { take: { ...take, invalidated: true, discontinuities: 3, has_oto: false } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector("[role='alert']")).not.toBeNull();
  },
};

export const 収録中: Story = { args: { busy: true } };
