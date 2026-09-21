import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { SongBank } from ".";
import { api } from "~/lib/ipc";
import type { BankSongView } from "~/lib/ipc";

const songs: BankSongView[] = [
  {
    id: "b1",
    title: "さくらさくら",
    notes: 14,
    in_bank: true,
    license: "パブリックドメイン",
  },
  {
    id: "b2",
    title: "New Project — 主旋律",
    notes: 96,
    in_bank: true,
    license: "不明（配布物には含めない）",
  },
  {
    id: "b3",
    title: "New Project — ハモリ",
    notes: 96,
    in_bank: false,
    license: "不明（配布物には含めない）",
  },
];

const meta = {
  title: "領域/SongBank",
  component: SongBank,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  beforeEach: () => {
    mocked(api.allSongs).mockResolvedValue(songs);
  },
  decorators: [(Story) => <div className="w-120">{Story()}</div>],
} satisfies Meta<typeof SongBank>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 持ち込んだ曲が並ぶ: Story = {
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("さくらさくら"));

    /*
      外した曲も一覧に残る（`TR-RCL-12`）。
      消すと、間違えて外した曲を戻す的がどこにも無くなる。
    */
    await expect(canvasElement.textContent).toContain("New Project — ハモリ");
    // 状態を字でも出す（`docs/design/direction.md`）。的の名前だけで言わない。
    await expect(canvasElement.textContent).toContain("外してあります");
    const labels = [...canvasElement.querySelectorAll("[aria-label]")].map((e) =>
      e.getAttribute("aria-label"),
    );
    await expect(labels).toContain("New Project — ハモリ を目標に戻す");
    await expect(labels).toContain("さくらさくら を目標から外す");
  },
};

/** 題はファイル名から採るので、そのままでは並べられないことがある。 */
export const 題を打ち直す: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const open = await canvas.findByLabelText("New Project — 主旋律 の題を変える");
    await userEvent.click(open);

    // 名札は見える字で置く（`TR-PLT-29`）。`placeholder` は名前にならない。
    const field = await canvas.findByLabelText("題");
    await expect(field).toHaveValue("New Project — 主旋律");

    // 変えていないうちは押せない。同じ題で書き戻さない。
    await expect(canvas.getByRole("button", { name: "変える" })).toBeDisabled();
  },
};

export const まだ一曲も無い: Story = {
  beforeEach: () => {
    mocked(api.allSongs).mockResolvedValue([]);
  },
  play: async ({ canvasElement }) => {
    // バンクが空でも成立する（`TR-RCL-12`）。欠けを失敗として描かない。
    await waitFor(() => expect(canvasElement.textContent).toContain("まだ1曲もありません"));
  },
};
