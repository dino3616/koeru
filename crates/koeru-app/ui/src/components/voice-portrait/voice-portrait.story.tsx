import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";
import { fn } from "storybook/test";

import { VoicePortrait } from ".";

const 育っている = {
  display_name: "",
  method: "single",
  rows: 21,
  covered: 30,
  required: 102,
  singable_songs: 2,
  songs_in_bank: 4,
  // 五十音の行（内側から あ か が さ ざ た だ な は ば ぱ ま や ら わ）。
  rings: [
    { covered: 6, total: 6 },
    { covered: 8, total: 8 },
    { covered: 8, total: 8 },
    { covered: 8, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 3 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 8 },
    { covered: 0, total: 3 },
    { covered: 0, total: 8 },
    { covered: 0, total: 2 },
  ],
  color: { hue: 318, chroma: 0.6, lightness: 0.45 },
};

const meta = {
  title: "声/VoicePortrait",
  component: VoicePortrait,
  args: { state: 育っている, onListen: fn(), onStop: fn(), preparing: false },
} satisfies Meta<typeof VoicePortrait>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 育っている途中: Story = {};

export const 声を用意している: Story = { args: { preparing: true } };

/** まだ1音も無い。環も色も出ないが、「聴く」は押せる。 */
export const まだ録っていない: Story = {
  args: {
    state: {
      display_name: "",
      method: "single",
      rows: 21,
      covered: 0,
      required: 102,
      singable_songs: 0,
      songs_in_bank: 4,
      rings: [
        { covered: 0, total: 6 },
        { covered: 0, total: 8 },
      ],
      color: null,
    },
  },
  play: async ({ canvasElement }) => {
    // 押せなくしない。押して鳴らないことが返事になる。
    await expect(canvasElement.querySelector("button")?.hasAttribute("disabled")).toBe(false);
  },
};

export const 閉じた: Story = {
  args: {
    state: {
      display_name: "",
      method: "single",
      rows: 21,
      covered: 102,
      required: 102,
      singable_songs: 4,
      songs_in_bank: 4,
      rings: Array.from({ length: 15 }, () => ({ covered: 8, total: 8 })),
      color: { hue: 196, chroma: 0.4, lightness: 0.7 },
    },
  },
};

export const 曲が無い: Story = {
  args: { state: { ...育っている, singable_songs: 0, songs_in_bank: 0 } },
};
