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
