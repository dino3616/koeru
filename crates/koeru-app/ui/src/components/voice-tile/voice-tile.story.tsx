import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn } from "storybook/test";

import { VoiceTile } from ".";

const meta = {
  title: "声/VoiceTile",
  component: VoiceTile,
  args: { onOpen: fn() },
} satisfies Meta<typeof VoiceTile>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 育っている途中: Story = {
  args: {
    project: {
      id: "1",
      display_name: "ミナ",
      method: "single",
      item_count: 29,
      state: {
        display_name: "",
        method: "single",
        rows: 21,
        covered: 30,
        required: 102,
        singable_songs: 2,
        songs_in_bank: 4,
        rings: [
          { covered: 6, total: 6 },
          { covered: 8, total: 8 },
          { covered: 8, total: 8 },
          { covered: 8, total: 8 },
          { covered: 0, total: 8 },
        ],
        color: { hue: 318, chroma: 0.6, lightness: 0.45 },
      },
    },
  },
};

export const 閉じた: Story = {
  args: {
    project: {
      id: "2",
      display_name: "みなも",
      method: "single",
      item_count: 29,
      state: {
        display_name: "",
        method: "single",
        rows: 21,
        covered: 102,
        required: 102,
        singable_songs: 4,
        songs_in_bank: 4,
        rings: Array.from({ length: 15 }, () => ({ covered: 8, total: 8 })),
        color: { hue: 196, chroma: 0.5, lightness: 0.6 },
      },
    },
  },
};

export const 読めなかった: Story = {
  args: {
    project: { id: "3", display_name: null, method: null, item_count: null, state: null },
  },
};
