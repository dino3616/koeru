import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, waitFor } from "storybook/test";

import { CalibrationCard } from "~/components/calibration-card";
import { api } from "~/lib/ipc";

/*
 * 入力レベルの校正（`TR-REC-14`）。
 *
 * ゲインをどこで触れるかで案内が変わる。 3つ全部出す——
 * `Hardware` 以外では自動で動かさないので、その説明が読めるかを見る。
 *
 * モックを置くだけでは足りない。 `calibrate` は押されて初めて走るので、
 * 置いただけの story は「未実施」と同じ絵を描き、結果の欄も配色も
 * 一度も検査されない。`play` で実際に押して、結果が出るまで待つ。
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
  args: { expected: "-8.4 dBFS" } as never,
  play: async ({ canvasElement, userEvent, args }) => {
    const expected = (args as unknown as { expected: string }).expected;
    const run = canvasElement.querySelectorAll("button")[0];
    await userEvent.click(run as HTMLElement);
    // 結果が描かれるまで待つ。押しただけでは、まだ「未実施」の絵。
    // 結果固有の文言まで見る。`dl` の有無だけだと、別の結果でも通ってしまう。
    await waitFor(async () => {
      await expect(canvasElement.textContent).toContain(expected);
    });
  },
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
  args: { expected: "ソフトウェア" } as never,
  play: async ({ canvasElement, userEvent, args }) => {
    const expected = (args as unknown as { expected: string }).expected;
    const run = canvasElement.querySelectorAll("button")[0];
    await userEvent.click(run as HTMLElement);
    // 結果が描かれるまで待つ。押しただけでは、まだ「未実施」の絵。
    // 結果固有の文言まで見る。`dl` の有無だけだと、別の結果でも通ってしまう。
    await waitFor(async () => {
      await expect(canvasElement.textContent).toContain(expected);
    });
  },
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
  args: { expected: "—" } as never,
  play: async ({ canvasElement, userEvent, args }) => {
    const expected = (args as unknown as { expected: string }).expected;
    const run = canvasElement.querySelectorAll("button")[0];
    await userEvent.click(run as HTMLElement);
    // 結果が描かれるまで待つ。押しただけでは、まだ「未実施」の絵。
    // 結果固有の文言まで見る。`dl` の有無だけだと、別の結果でも通ってしまう。
    await waitFor(async () => {
      await expect(canvasElement.textContent).toContain(expected);
    });
  },
  beforeEach: () => {
    mocked(api.calibrate).mockResolvedValue({
      gain: null,
      control: "Unavailable",
      peak_dbfs: null,
      settled: false,
    });
  },
};
