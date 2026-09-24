import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { NextPhrase } from ".";

const meta = {
  title: "領域/NextPhrase",
  component: NextPhrase,
  args: {
    text: "だ ぢ づ で ど",
    units: 5,
    recording: false,
    settling: false,
    continuous: false,
    ready: true,
    advanceMs: 3000,
    onStart: fn(),
    onStop: fn(),
    onContinuous: fn(),
    onPause: fn(),
  },
} satisfies Meta<typeof NextPhrase>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 待っている: Story = {};

export const 収録中: Story = { args: { recording: true } };

export const 確かめている: Story = {
  args: { settling: true },
  play: async ({ canvasElement }) => {
    // 確定の途中で次を始めさせない（`TR-REC-42`）。
    const start = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent?.trim() === "録る",
    );
    await expect(start).toBeUndefined();
  },
};

export const 続けて録っている: Story = { args: { continuous: true } };

export const マイクを選ぶ前: Story = { args: { ready: false } };

export const 全部読み終えた: Story = { args: { text: null, units: 0 } };

/**
 * モーラ境界を視覚的に区切る（`TR-RCL-07`）。
 *
 * 字間を空けるだけだと「きゃ」が2文字に見えて2拍で読まれる。
 */
export const モーラ境界を区切る: Story = {
  args: { text: "きゃ き きゅ きぇ きょ", units: 5 },
  play: async ({ canvasElement }) => {
    // 1拍ずつ箱に入る。空白で割った数と一致する。
    const boxes = canvasElement.querySelectorAll("p > span.rounded-lg");
    await expect(boxes).toHaveLength(5);
    await expect(boxes[0]?.textContent).toBe("きゃ");
  },
};

/** 読みにくい音が混ざることを1行で言う（`TR-RCL-07`）。スコアの数は出さない。 */
export const 読みにくい行: Story = {
  args: { text: "てゅ でょ ヴぃ", units: 3, risk: { hard: 3, moras: 3 } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("読みにくい音が混ざります");
  },
};
