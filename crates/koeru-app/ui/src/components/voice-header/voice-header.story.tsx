import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { VoiceHeader } from ".";

const meta = {
  title: "領域/VoiceHeader",
  component: VoiceHeader,
  args: {
    name: "ミナ",
    method: "single",
    deviceName: "MacBook Pro のマイク",
    tab: "sound",
    onTab: fn(),
    onBack: fn(),
  },
} satisfies Meta<typeof VoiceHeader>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 音: Story = {
  play: async ({ canvasElement }) => {
    // いまいる面は `aria-current` で言う。濃さだけにしない。
    const current = canvasElement.querySelectorAll("[aria-current='page']");
    await expect(current).toHaveLength(1);
  },
};

export const 曲: Story = { args: { tab: "songs" } };
export const 配り物: Story = { args: { tab: "package" } };
export const 設定: Story = { args: { tab: "settings" } };

export const マイクを選ぶ前: Story = { args: { deviceName: null } };
