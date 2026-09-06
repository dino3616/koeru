import type { Meta, StoryObj } from "@storybook/react-vite";
import { mocked } from "storybook/test";

import { LibraryScreen } from "~/screens/library-screen";
import { api } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

/*
 * 音源の一覧。最初に開く画面。
 *
 * 画面まるごとの story。 部品を1つずつ見ても、組み立てで壊れるものは
 * 見えない——`<main>` が1つあること、見出しの段、landmark の並びは
 * ここでしか axe に測らせられない。
 */
const meta = {
  title: "画面/一覧",
  component: LibraryScreen,
  parameters: { layout: "fullscreen" },
  render: () => withRouter(<LibraryScreen />),
} satisfies Meta<typeof LibraryScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 何もない: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockResolvedValue([]);
  },
};

export const いくつかある: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockResolvedValue([
      { id: "p1", display_name: "ことね", method: "single", item_count: 142 },
      { id: "p2", display_name: "みなも", method: "sequential", item_count: 318 },
      // 名前も方式も読めない音源。落とさず出す。
      { id: "p3", display_name: null, method: null, item_count: null },
    ]);
  },
};

export const 読み込みに失敗した: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockRejectedValue({
      kind: "app.library_unreadable",
      message: "ライブラリを開けませんでした",
    });
  },
};
