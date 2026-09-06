import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { LevelMeter } from "~/components/level-meter";
import { CLIP_THRESHOLD, TOO_QUIET } from "~/lib/levels";

/*
 * 入力レベル。
 *
 * 3つの状態を全部出す。 `<meter>` の色は `::-webkit-meter-*` が引き受けるので、
 * 状態ごとに違う色が載る——1つだけ出すと、残り2つの色を一度も測らない。
 */
const meta = {
  title: "部品/LevelMeter",
  component: LevelMeter,
} satisfies Meta<typeof LevelMeter>;

export default meta;
type Story = StoryObj<typeof meta>;

export const ちょうどよい: Story = {
  args: { peak: 0.5 },
  /*
   * 値と範囲が読めること（`TR-PLT-29`）。
   *
   * `<meter>` を選んだ理由そのもの。 `role="meter"` を付けた `div` では
   * 「よい範囲かどうか」まで支援技術に出せない。axe には値の有無を
   * 見る規則が無いので、ここで固定する。
   */
  play: async ({ canvasElement }) => {
    const meter = canvasElement.querySelector("meter");
    await expect(meter).not.toBeNull();
    await expect(meter?.getAttribute("value")).toBe("50");
    await expect(meter?.getAttribute("min")).toBe("0");
    await expect(meter?.getAttribute("max")).toBe("100");
    // 色だけで伝えない（`TR-PLT-28`）。語も並ぶ。
    await expect(canvasElement.textContent).toContain("ちょうどよい");
  },
};
export const 小さすぎる: Story = {
  args: { peak: TOO_QUIET / 2 },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("小さすぎる");
  },
};
export const 割れている: Story = {
  args: { peak: CLIP_THRESHOLD },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("割れている");
  },
};
