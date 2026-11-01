import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, waitFor, within } from "storybook/test";

import type { RowStatus } from "~/components/status-mark";
import type { RowTakesView, SongView } from "~/lib/ipc";
import { voiceStyle } from "~/lib/voice-color";

import { rowItem, rowStatus, songItem, songState } from "./from-views";
import { RecordSidebar, type SidebarRow, type SidebarSong } from ".";

const SONGS: SidebarSong[] = [
  { id: "s1", title: "さくらさくら", range: "A3–D4", state: { status: "ready" } },
  { id: "s2", title: "帰り道ワルツ", range: "G3–E4", state: { status: "fallback" } },
  { id: "s3", title: "ひかりのあと", range: "A3–F4", state: { status: "remaining", rows: 4 } },
  { id: "s4", title: "夜明けのテスト", range: "D4–A5", state: { status: "out_of_range" } },
];

const row = (id: string, text: string, status: RowStatus): SidebarRow => ({ id, text, status });

/** ボードの並び。 録ったものが先で、途中に要録り直しと除外が混ざる。 */
const ROWS: SidebarRow[] = [
  row("r1", "な に ぬ ね の ん は", "recorded"),
  row("r2", "や ゆ よ ん ま", "recorded"),
  row("r3", "ら り る れ ろ ん わ", "needs_retake"),
  row("r4", "ぱ ぱ ぴ ぷ ぺ ぽ ん ば", "unrecorded"),
  row("r5", "ざ ざ じ ず ぜ ぞ ん だ", "unrecorded"),
  row("r6", "ば ば び ぶ べ ぼ ん が", "unrecorded"),
  row("r7", "きゃ きゃ きゅ きぇ きょ ん ぎゃ", "unrecorded"),
  row("r8", "しゃ しゃ しゅ しぇ しょ ん じゃ", "excluded"),
  row("r9", "ふぁ ふぁ ふぃ ふぇ ふぉ ん ゔぁ", "unrecorded"),
];

const KANA = "あいうえおかきくけこさしすせそたちつてと".split("");

/** 長い一覧。 先頭の 93 行を録ったことにする（ボードの「93 / 131」に寄せた形）。 */
const manyRows = (count: number): SidebarRow[] =>
  Array.from({ length: count }, (_, i) => {
    const text = Array.from({ length: 8 }, (_, k) => KANA[(i + k) % KANA.length]).join(" ");
    const status: RowStatus =
      i < 93
        ? "recorded"
        : i % 29 === 0
          ? "needs_retake"
          : i % 41 === 0
            ? "excluded"
            : "unrecorded";
    return row(`m${i}`, text, status);
  });

const meta = {
  title: "部品/RecordSidebar",
  component: RecordSidebar,
  args: {
    songs: SONGS,
    rows: ROWS,
    selectedRowId: null,
    selectedSongId: null,
    onSelectRow: fn(),
    onSelectSong: fn(),
    onPlaySong: fn(),
  },
  decorators: [
    // 声の色は置く側の要素が与える。 実画面では音源の面がこの役をする。
    (Story) => (
      <div
        className="flex h-160 bg-bg"
        /* 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。 */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle({ hue: 168, chroma: 0.6, lightness: 0.5 })}
      >
        {Story()}
      </div>
    ),
  ],
} satisfies Meta<typeof RecordSidebar>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 途中。 曲は4つの状態が揃い、行は4つの状態が混ざる。 */
export const 途中: Story = {
  args: { selectedRowId: "r4" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);

    // 歌える曲だけに ▶ が出る。 歌える（代替ありを含む）は2つ。
    await expect(canvas.getAllByRole("button", { name: /を歌う$/ })).toHaveLength(2);
    await expect(canvas.getByText("2 / 4")).toBeInTheDocument();
    await expect(canvas.getByText("2 / 9")).toBeInTheDocument();
    await expect(canvas.getByText("あと 4 行")).toBeInTheDocument();

    // 選んだ行だけが選ばれている。 状態は名前に入る。
    const selected = canvas.getAllByRole("option", { selected: true });
    await expect(selected).toHaveLength(1);
    await expect(selected[0]).toHaveAccessibleName(/^ぱ ぱ ぴ ぷ ぺ ぽ ん ば\s*未収録$/);

    // 欠けを不足として書かない（`docs/reports/ux/direction.md`）。
    const text = canvasElement.textContent ?? "";
    for (const word of ["未達", "不足", "%"]) {
      await expect(text).not.toContain(word);
    }
  },
};

/** 全部録った。 全曲が歌える。 */
export const 全部録った: Story = {
  args: {
    songs: SONGS.slice(0, 2),
    rows: ROWS.map((r) => ({ ...r, status: "recorded" as const })),
    selectedRowId: "r9",
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("2 / 2")).toBeInTheDocument();
    await expect(canvas.getByText("9 / 9")).toBeInTheDocument();
  },
};

/** 歌える曲が無い。 ▶ は1つも出ない。 */
export const 歌える曲が無い: Story = {
  args: { songs: SONGS.slice(2) },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.queryAllByRole("button", { name: /を歌う$/ })).toHaveLength(0);
    await expect(canvas.getByText("0 / 2")).toBeInTheDocument();
  },
};

/** 行が 300 ある。 選んだ行は窓の外にあり、一覧の中だけが送られて見える位置に来る。 */
export const 行が300ある: Story = {
  args: { rows: manyRows(300), selectedRowId: "m200" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getAllByRole("option")).toHaveLength(300);

    const list = canvas.getByRole("listbox", { name: "行" });
    const [selected] = canvas.getAllByRole("option", { selected: true });
    await waitFor(async () => {
      const box = list.getBoundingClientRect();
      const at = selected?.getBoundingClientRect();
      await expect(at?.top).toBeGreaterThanOrEqual(box.top);
      await expect(at?.bottom).toBeLessThanOrEqual(box.bottom);
    });
    // 列の外（ページ）は送られていない。
    await expect(document.scrollingElement?.scrollTop).toBe(0);
  },
};

/** 曲で絞っている。 選んでいる曲は押されている。 曲の行に補助の字を出せる。 */
export const 曲を選んだ: Story = {
  args: {
    selectedSongId: "s3",
    songs: SONGS.map((s) => (s.id === "s1" ? { ...s, note: "0:12 / 1:48" } : s)),
  },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const pressed = canvas.getAllByRole("button", { pressed: true });
    await expect(pressed).toHaveLength(1);
    await expect(pressed[0]).toHaveAccessibleName(/ひかりのあと/);
    await expect(canvas.getByText("0:12 / 1:48")).toBeInTheDocument();

    // もう一度押すと絞りが外れる。
    await userEvent.click(pressed[0] as HTMLElement);
    await expect(args.onSelectSong).toHaveBeenCalledWith(null);
  },
};

/** 長い題と長い行。 折り返さずに省く。 列の幅は変わらない。 */
export const 長い字: Story = {
  args: {
    songs: [
      {
        id: "s1",
        title: "とても長い題の曲の名前がここに入ります",
        range: "A3–D4",
        state: { status: "fallback" },
      },
    ],
    rows: [
      row(
        "r1",
        "ふぁ ふぁ ふぃ ふぇ ふぉ ん ゔぁ ゔぃ ゔ ゔぇ ゔぉ きゃ きゃ きゅ きぇ きょ",
        "unrecorded",
      ),
    ],
  },
  play: async ({ canvasElement }) => {
    const sidebar = canvasElement.firstElementChild?.firstElementChild;
    await expect(sidebar?.getBoundingClientRect().width).toBe(260);
    await expect(canvasElement.scrollWidth).toBeLessThanOrEqual(canvasElement.clientWidth);
  },
};

/** キーボードだけで、曲を選び、行を選べる。 */
export const キーボードだけ: Story = {
  args: { selectedRowId: "r4" },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);

    // 曲: ▶ と曲は別のボタンで、Tab で順に届く。 Enter で押せる。
    await userEvent.tab();
    await expect(canvas.getByRole("button", { name: "さくらさくら を歌う" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await expect(args.onPlaySong).toHaveBeenCalledWith("s1");

    await userEvent.tab();
    await expect(canvas.getByRole("button", { name: /^さくらさくら\s*A3–D4/ })).toHaveFocus();
    await userEvent.keyboard(" ");
    await expect(args.onSelectSong).toHaveBeenCalledWith("s1");

    // 行: 曲の最後から Tab で、選んでいる行へ入る。 入る先は1つだけ。
    canvas.getByRole("button", { name: /^夜明けのテスト/ }).focus();
    await userEvent.tab();
    const options = canvas.getAllByRole("option");
    await expect(options[3]).toHaveFocus();
    await expect(canvasElement.querySelectorAll('[role="option"][tabindex="0"]')).toHaveLength(1);

    // 矢印は焦点だけを動かす。 選ぶのは Enter。
    await userEvent.keyboard("{ArrowDown}");
    await expect(options[4]).toHaveFocus();
    await expect(args.onSelectRow).not.toHaveBeenCalled();
    await userEvent.keyboard("{Enter}");
    await expect(args.onSelectRow).toHaveBeenLastCalledWith("r5");

    await userEvent.keyboard("{ArrowUp}{ArrowUp} ");
    await expect(args.onSelectRow).toHaveBeenLastCalledWith("r3");

    await userEvent.keyboard("{End}");
    await expect(options[8]).toHaveFocus();
    await userEvent.keyboard("{Home}");
    await expect(options[0]).toHaveFocus();

    // 一覧を出て戻ると、入る先は選んでいる行に戻る。 抜けるのに一覧の長さは要らない。
    await userEvent.tab();
    await expect(options[0]).not.toHaveFocus();
    await expect(options[0]?.getAttribute("tabindex")).toBe("-1");
    await expect(options[3]?.getAttribute("tabindex")).toBe("0");
  },
};

/** 曲も行も無い。 */
export const 空: Story = {
  args: { songs: [], rows: [] },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getByText("曲はまだありません。")).toBeInTheDocument();
    await expect(canvas.getByText("行はまだありません。")).toBeInTheDocument();
    await expect(canvas.queryAllByRole("option")).toHaveLength(0);
  },
};

const song = (over: Partial<SongView>): SongView => ({
  id: "s1",
  title: "さくらさくら",
  singability: "Complete",
  singable: true,
  covered: 24,
  required: 24,
  missing_units: 0,
  missing_rows: 0,
  seconds: 18.4,
  previewable: true,
  transpose: 0,
  recommended_transpose: 0,
  rescuing_tone: null,
  total_moras: 24,
  range: null,
  ...over,
});

/** `SongView` と `RowTakesView` からの写し。 部品の外に置いてあるので、ここで確かめる。 */
export const 写し: Story = {
  play: async () => {
    await expect(songState(song({}))).toEqual({ status: "ready" });
    await expect(songState(song({ singability: "WithFallback" }))).toEqual({ status: "fallback" });
    await expect(songState(song({ singability: "Unavailable", missing_rows: 4 }))).toEqual({
      status: "remaining",
      rows: 4,
    });
    // 音域の外は、足せば届く音高があること。 行数は見ない。
    await expect(
      songState(song({ singability: "Unavailable", missing_rows: 4, rescuing_tone: "D4" })),
    ).toEqual({ status: "out_of_range" });
    // 知らない語は歌えると言わない。
    await expect(songState(song({ singability: "Other", missing_rows: 1 }))).toEqual({
      status: "remaining",
      rows: 1,
    });

    await expect(songItem(song({}), "A3–D4")).toEqual({
      id: "s1",
      title: "さくらさくら",
      range: "A3–D4",
      state: { status: "ready" },
    });
    await expect(songItem(song({})).range).toBeNull();

    for (const status of ["unrecorded", "recorded", "needs_retake", "excluded"] as const) {
      await expect(rowStatus(status)).toBe(status);
    }
    await expect(rowStatus("something_else")).toBe("unrecorded");

    const view = {
      row_id: "r9",
      text: "か き く け こ",
      state: "needs_retake",
    } satisfies Pick<RowTakesView, "row_id" | "text" | "state">;
    await expect(rowItem(view)).toEqual({
      id: "r9",
      text: "か き く け こ",
      status: "needs_retake",
    });
  },
};
