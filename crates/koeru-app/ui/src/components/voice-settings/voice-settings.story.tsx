import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { VoiceSettings } from ".";

const meta = {
  title: "領域/VoiceSettings",
  component: VoiceSettings,
  args: {
    id: "11111111-1111-4111-8111-111111111111",
    name: "ミナ",
    method: "single",
    rows: 21,
    onRenamed: fn(),
  },
  // 置く側の領域の中に入る部品なので、story でも同じ枠に入れて見る。
  decorators: [
    (Story) => (
      <div className="flex w-96 flex-col gap-3 rounded-xl border border-slate-7 bg-slate-2 p-5">
        {Story()}
      </div>
    ),
  ],
} satisfies Meta<typeof VoiceSettings>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 名前を変える前: Story = {
  play: async ({ canvasElement }) => {
    // 変えていないうちは押せない。押しても何も起きない的を出さない。
    const button = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent === "名前を変える",
    );
    await expect(button?.hasAttribute("disabled")).toBe(true);
  },
};

export const 作り方が読めない: Story = { args: { method: null } };
