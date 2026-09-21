import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { RecordingOrder } from ".";

const meta = {
  title: "部品/RecordingOrder",
  component: RecordingOrder,
  args: {
    onChange: fn(),
    switching: false,
    hasSongs: true,
    remainingSeconds: 4200,
    measured: false,
  },
} satisfies Meta<typeof RecordingOrder>;

export default meta;
type Story = StoryObj<typeof meta>;

/** どちらでいるかを常に表示する（`TR-SYN-19`）。 */
export const 曲バンク優先: Story = {
  args: { mode: "SongBankFirst" },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("歌いたい曲から");
  },
};

export const 被覆効率: Story = {
  args: { mode: "CoverageEfficiency" },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("少ない行数で");
  },
};

/** 切り替えは可逆（`TR-SYN-19` の (b)）。 */
export const 切り替えられる: Story = {
  args: { mode: "CoverageEfficiency" },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: /歌いたい曲から/ }));
    await expect(args.onChange).toHaveBeenCalledWith("SongBankFirst");
  },
};

/**
 * 曲が無ければ、曲バンク優先へ戻す先が無い。
 *
 * 押せる的として出さない——押しても何も起きない。
 */
export const 曲が無い: Story = {
  args: { mode: "CoverageEfficiency", hasSongs: false },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector("button")).toBeNull();
  },
};

export const 切り替え中: Story = { args: { mode: "SongBankFirst", switching: true } };

/**
 * 実測が効くまでは見込みだと言う（`TR-RCL-10`）。
 *
 * 言わずに数だけ出すと、最初の数行で「あと3時間」と読まれて手が止まる。
 */
export const 見込みの残り時間: Story = {
  args: { mode: "SongBankFirst", remainingSeconds: 4200, measured: false },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("約 1 時間");
    await expect(canvasElement.textContent).toContain("いまは見込みです");
  },
};

/** 実測が溜まったら、そのことを言わない——数がそのまま答えになる。 */
export const 実測が効いている: Story = {
  args: { mode: "CoverageEfficiency", remainingSeconds: 1800, measured: true },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("約 30 分");
    await expect(canvasElement.textContent).not.toContain("いまは見込みです");
  },
};
