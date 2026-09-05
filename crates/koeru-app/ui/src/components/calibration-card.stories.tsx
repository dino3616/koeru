import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn, mocked } from "storybook/test";

import { CalibrationCard } from "~/components/calibration-card";
import { api } from "~/lib/ipc";

/*
 * 入力レベルの校正（`TR-REC-14`）。
 *
 * ゲインをどこで触れるかで案内が変わる。 3つ全部出す——
 * `hardware` 以外では自動で動かさないので、その説明が読めるかを見る。
 */
const meta = {
  title: "部品/CalibrationCard",
  component: CalibrationCard,
  args: { ready: true, onStatus: fn() },
} satisfies Meta<typeof CalibrationCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 未実施: Story = {};

export const デバイス未選択: Story = { args: { ready: false } };

export const ハードウェアで調整できた: Story = {
  beforeEach: () => {
    mocked(api.calibrate).mockResolvedValue({
      gain: 0.62,
      control: "Hardware",
      peak_dbfs: -8.4,
      settled: true,
    });
  },
};

export const ソフトウェアなので触らない: Story = {
  beforeEach: () => {
    mocked(api.calibrate).mockResolvedValue({
      gain: 0.5,
      control: "Software",
      peak_dbfs: -22,
      settled: false,
    });
  },
};

export const 読み書きできない: Story = {
  beforeEach: () => {
    mocked(api.calibrate).mockResolvedValue({
      gain: null,
      control: "Unavailable",
      peak_dbfs: null,
      settled: false,
    });
  },
};
