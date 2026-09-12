import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { LibraryScreen } from ".";
import { api } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

/**
 * 単独音・中核セットの五十音の行（内側から あ か が さ ざ た だ な は ば ぱ ま や ら わ）。
 *
 * **15 本で、録っても増えない。** 作り話の本数で試さない——12 本のつもりで
 * 組んでいたときは、外側の環が `viewBox` を突き抜けて四角く切り取られていた。
 */
const 五十音の行 = [6, 8, 8, 8, 8, 8, 3, 8, 8, 8, 8, 8, 3, 8, 2];

/** 内側から順に埋めた環。合計が `covered` になる。 */
const rings = (covered: number) => {
  let left = covered;
  return 五十音の行.map((total) => {
    const c = Math.min(total, Math.max(0, left));
    left -= c;
    return { covered: c, total };
  });
};

/** 育ち具合の違う4つ。環の埋まり方が並んで見える。 */
const 声たち = [
  {
    id: "11111111-1111-4111-8111-111111111111",
    display_name: "ミナ",
    method: "single",
    item_count: 29,
    state: {
      display_name: "",
      method: "single",
      rows: 29,
      covered: 30,
      required: 102,
      singable_songs: 2,
      songs_in_bank: 4,
      rings: rings(30),
      color: { hue: 318, chroma: 0.6, lightness: 0.45 },
    },
  },
  {
    id: "22222222-2222-4222-8222-222222222222",
    display_name: "そら",
    method: "single",
    item_count: 29,
    state: {
      display_name: "",
      method: "single",
      rows: 29,
      covered: 8,
      required: 102,
      singable_songs: 0,
      songs_in_bank: 4,
      rings: rings(8),
      color: { hue: 258, chroma: 0.3, lightness: 0.2 },
    },
  },
  {
    id: "33333333-3333-4333-8333-333333333333",
    display_name: "みなも",
    method: "single",
    item_count: 29,
    state: {
      display_name: "",
      method: "single",
      rows: 29,
      covered: 102,
      required: 102,
      singable_songs: 4,
      songs_in_bank: 4,
      rings: rings(102),
      color: { hue: 196, chroma: 0.5, lightness: 0.6 },
    },
  },
];

const meta = {
  title: "画面/LibraryScreen",
  component: LibraryScreen,
  parameters: { layout: "fullscreen" },
} satisfies Meta<typeof LibraryScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 三つの声: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockResolvedValue(声たち);
  },
  render: () => withRouter(<LibraryScreen />),
  play: async ({ canvasElement }) => {
    /*
     * 描けたことを確かめる。
     *
     * 画面の story に「出ているはずのもの」を1つ置く。 置かないと、
     * 描画で落ちて `ErrorBoundary` の面が出ていても axe は通る——
     * 失敗の面もアクセシブルに作ってあるので、**検査が緑のまま
     * 画面が壊れている**状態を作れる。踏んだ。
     */
    await waitFor(() => expect(canvasElement.textContent).toContain("ミナ"));
  },
};

/** まだ1つも無いとき。空いた席だけが並ぶ。 */
export const 何も無い: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockResolvedValue([]);
  },
  render: () => withRouter(<LibraryScreen />),
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("新しく作る"));
    // 「まだ何もありません」と書かない。席がそのまま入口になる。
    await expect(canvasElement.textContent).not.toContain("ありません");
  },
};

/** 台帳を読めなかった音源も席を残す。 */
export const 読めない音源がある: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockResolvedValue([
      ...声たち.slice(0, 1),
      {
        id: "44444444-4444-4444-8444-444444444444",
        display_name: null,
        method: null,
        item_count: null,
        state: null,
      },
    ]);
  },
  render: () => withRouter(<LibraryScreen />),
};

export const 読み込み中: Story = { render: () => withRouter(<LibraryScreen />) };
