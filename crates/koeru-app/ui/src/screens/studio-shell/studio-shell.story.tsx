import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { StudioShell } from ".";
import { ProjectGate } from "~/components/project-gate";
import { type ProjectView, type Subbank, api } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

const NAGI = "11111111-1111-4111-8111-111111111111";
const HARUKAZE = "22222222-2222-4222-8222-222222222222";

const voice = (id: string, name: string, hue: number): ProjectView => ({
  id,
  display_name: name,
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
    rings: [],
    color: { hue, chroma: 0.6, lightness: 0.5 },
  },
});

const subbank = (
  id: string,
  voiceColor: string | null,
  tone: string,
  method: Subbank["method"],
): Subbank => ({
  id,
  voiceColor,
  method,
  preset: null,
  tone: { midi: 57, name: tone },
  toneRange: {
    low: { midi: 57, name: tone },
    high: { midi: 69, name: tone },
    edited: false,
  },
  progress: { recordedRows: 0, rows: 131, remainingSeconds: 0 },
  hasTakes: false,
});

const OPENED = {
  next_row_id: "s003",
  next_row_text: "さ し す せ そ",
  covered: 10,
  required: 102,
  coverage: "InProgress",
  handoff: "NotExported",
  singable_songs: 0,
  songs_in_bank: 4,
  by_tone: [],
  remaining_seconds: 600,
  remaining_rows: 72,
};

const 棚 = () => {
  mocked(api.listProjects).mockResolvedValue([
    voice(NAGI, "ナギ", 175),
    voice(HARUKAZE, "ハルカゼ", 290),
  ]);
};

const 開く = () => {
  棚();
  mocked(api.openProject).mockResolvedValue(OPENED);
  mocked(api.subbanks).mockResolvedValue([
    subbank("sb-1", "Normal", "A3", "CVVC"),
    subbank("sb-2", "Whisper", "A3", "CVVC"),
  ]);
};

/** 場の画面の代わり。 `<main>` と見出しは場の画面が持つ。 */
const Screen = () => (
  <main className="h-full p-8">
    <h1 className="text-xl font-semibold">収録</h1>
  </main>
);

const shell = (id: string | undefined, path: string) =>
  withRouter(
    <StudioShell id={id}>
      <ProjectGate id={id}>
        <Screen />
      </ProjectGate>
    </StudioShell>,
    path,
  );

const meta = {
  title: "画面/StudioShell",
  component: StudioShell,
  parameters: { layout: "fullscreen" },
  args: { id: NAGI, children: null },
  render: () => <div className="h-160 w-240">{shell(NAGI, `/record?id=${NAGI}`)}</div>,
} satisfies Meta<typeof StudioShell>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 棚は実データ、木は音源を開いたあとに読んだ枠。 */
export const 音源を開いた: Story = {
  beforeEach: 開く,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("list", { name: "Normal" });
    await expect(canvas.getByRole("list", { name: "Whisper" })).toBeVisible();
    await expect(canvas.getByRole("link", { name: "ハルカゼ" })).toBeVisible();
    // 枠が木に出てから、音源を開いたのは1回だけ。
    await expect(api.openProject).toHaveBeenCalledTimes(1);
  },
};

/**
 * 外枠を畳んで開き直しても、音源を開き直さない。
 *
 * 木の読みが `openProjectQuery` を観測する側だと、木が付き直すたびに取り直しが走り、
 * 収録セッションが始め直しになる（`TR-REC-30`）。
 */
export const 畳んで開き直す: Story = {
  beforeEach: 開く,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("list", { name: "Normal" });
    const toggle = canvas.getByRole("button", { name: "サイドバー" });
    await userEvent.click(toggle);
    await userEvent.click(toggle);
    await canvas.findByRole("list", { name: "Normal" });
    await expect(api.openProject).toHaveBeenCalledTimes(1);
  },
};

/** 音源を開く途中。 枠はまだ読みに行かない。 外枠は先に出ている。 */
export const 開く途中: Story = {
  beforeEach: () => {
    棚();
    // 解決しない。 開く前に枠を引くと `app.no_project` になる。
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("link", { name: "ハルカゼ" });
    await expect(canvas.getByText("サブバンクを読んでいます")).toBeVisible();
    await expect(api.subbanks).not.toHaveBeenCalled();
  },
};

/** 枠を読めなかった。 外枠と棚は残る。 */
export const 枠を読めなかった: Story = {
  beforeEach: () => {
    開く();
    mocked(api.subbanks).mockRejectedValue(new Error("音源が開かれていません"));
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText("サブバンクを読めませんでした")).toBeVisible());
    await expect(canvas.getByRole("link", { name: "ハルカゼ" })).toBeVisible();
  },
};

/** 棚を読めなかった。 場の画面は出られる。 */
export const 棚を読めなかった: Story = {
  beforeEach: () => {
    mocked(api.listProjects).mockRejectedValue(new Error("一覧を読めませんでした"));
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await waitFor(() => expect(canvas.getByText("音源の一覧を読めませんでした")).toBeVisible());
  },
};

/** 音源を選ばない画面（新しい音源）。 開かず、枠も読まない。 */
export const 新しい音源: Story = {
  beforeEach: 棚,
  render: () => <div className="h-160 w-240">{shell(undefined, "/new")}</div>,
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const fresh = await canvas.findByRole("link", { name: "新しい音源" });
    await expect(fresh.getAttribute("aria-current")).toBe("page");
    await expect(api.openProject).not.toHaveBeenCalled();
    await expect(api.subbanks).not.toHaveBeenCalled();
  },
};
