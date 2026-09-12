import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked } from "storybook/test";

import { NewVoice } from ".";
import { api } from "~/lib/ipc";

const 単独音 = {
  id: "single",
  label: "単独音",
  summary: "1音ずつ、間をあけて読む",
  rows: 21,
  units: 102,
  seconds: 265,
  passes: 1,
  reach: "ゆっくりした曲が歌えます。",
  reading: "1音ずつ読むので、読み間違えにくい。",
};

const meta = {
  title: "領域/NewVoice",
  component: NewVoice,
  args: { onCreate: fn(), onClose: fn(), creating: false },
  beforeEach: () => {
    mocked(api.methodPresets).mockResolvedValue([単独音]);
  },
} satisfies Meta<typeof NewVoice>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 名前を打つ前: Story = {
  play: async ({ canvasElement }) => {
    // 名前が空のうちは作れない。押せる的として出しておくと、押して失敗する。
    const create = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent === "作る",
    );
    await expect(create?.hasAttribute("disabled")).toBe(true);
  },
};

export const 作っている: Story = { args: { creating: true } };
