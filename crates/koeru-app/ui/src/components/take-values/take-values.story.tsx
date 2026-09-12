import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { TakeValues } from ".";
import type { OtoView } from "~/lib/ipc";

const otos: OtoView[] = ["か", "き", "く", "け", "こ"].map((alias, i) => ({
  alias,
  offset_ms: 120 + i * 600,
  consonant_ms: 92,
  cutoff_ms: -420,
  preutterance_ms: 152,
  overlap_ms: 48,
}));

const meta = {
  title: "領域/TakeValues",
  component: TakeValues,
  args: {
    otos,
    selected: "き",
    durationMs: 3100,
    raw: false,
    onRaw: fn(),
  },
  decorators: [(Story) => <div className="w-[640px]">{Story()}</div>],
} satisfies Meta<typeof TakeValues>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 言い換えて出す: Story = {
  play: async ({ canvasElement }) => {
    // 内部表現の名前を出さない（`TR-REC-18`）。
    const text = canvasElement.textContent ?? "";
    for (const word of ["offset", "preutterance", "overlap", "cutoff", "oto"]) {
      await expect(text).not.toContain(word);
    }
  },
};

export const 数値で見る: Story = {
  args: { raw: true },
  play: async ({ canvasElement }) => {
    // 本人が切り替えたときだけ、元の名前と生値を出す。
    await expect(canvasElement.textContent).toContain("preutterance");
  },
};

export const 音が取れなかった: Story = { args: { otos: [], selected: null } };
