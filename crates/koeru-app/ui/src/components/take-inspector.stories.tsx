import type { Meta, StoryObj } from "@storybook/react-vite";
import { mocked } from "storybook/test";

import { TakeInspector } from "~/components/take-inspector";
import { api } from "~/lib/ipc";

/*
 * 録れたテイクの波形と原音設定（`TR-PLT-04`、`TR-ALN-33`）。
 *
 * 割れているテイクは色が変わる。 両方出す——`peak` で分岐するので、
 * 片方だけでは割れた側の色を測らない。
 */
const meta = {
  title: "部品/TakeInspector",
  component: TakeInspector,
  args: { takeId: 1, durationMs: 2400, peak: 0.72 },
  beforeEach: () => {
    mocked(api.otosOfTake).mockResolvedValue([
      {
        alias: "あ",
        offset_ms: 120,
        consonant_ms: 40,
        cutoff_ms: -900,
        preutterance_ms: 60,
        overlap_ms: 30,
      },
    ]);
    // 波形は 200 点の正弦波にする。実物に近い形が出れば、境界の線が読める。
    mocked(api.waveformWindow).mockResolvedValue(
      Array.from({ length: 200 }, (_, i) => {
        const a = Math.sin((i / 200) * Math.PI * 6) * 0.6;
        return [-Math.abs(a), Math.abs(a)] as [number, number];
      }),
    );
  },
} satisfies Meta<typeof TakeInspector>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 通常: Story = {};

/** 割れているテイク。波形の色が変わり、読み上げにも「割れている」が入る。 */
export const 割れている: Story = { args: { peak: 1 } };

export const 原音設定がまだ無い: Story = {
  beforeEach: () => {
    mocked(api.otosOfTake).mockResolvedValue([]);
  },
};
