import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { SongKey } from ".";
import { api } from "~/lib/ipc";
import type { SongView } from "~/lib/ipc";

const song: SongView = {
  id: "s1",
  title: "さくらさくら",
  singability: "Complete",
  singable: true,
  covered: 24,
  required: 24,
  missing_units: 0,
  missing_rows: 0,
  seconds: 18.4,
  total_moras: 24,
  previewable: true,
  transpose: 0,
  recommended_transpose: 0,
  rescuing_tone: null,
};

const meta = {
  title: "領域/SongKey",
  component: SongKey,
  args: { song },
  beforeEach: () => {
    mocked(api.setSongTranspose).mockResolvedValue(null);
  },
  decorators: [(Story) => <div className="w-120">{Story()}</div>],
} satisfies Meta<typeof SongKey>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 届いている: Story = {
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("収録した高さの近くで鳴らせます");
    // 届いているときに移調を勧めない。
    await expect(canvasElement.textContent).not.toContain("そのキーにする");
  },
};

/**
 * 遠くても押せなくしない（`DEC-SYN-012`）。
 *
 * 基準の ±7 半音・二乗平均 4 半音に実測の裏付けが無い。
 * 出すのは「何が起きるか」と「どうすれば近づくか」。
 */
export const 遠いので道を出す: Story = {
  args: {
    song: { ...song, previewable: false, recommended_transpose: -12, rescuing_tone: "D4" },
  },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("引き伸ばすぶん、声が変わって聞こえます");
    // 2つの道を出す。キーを動かすか、次に作るとき音高を足すか。
    await expect(canvasElement.textContent).toContain("-12 半音にすると");
    await expect(canvasElement.textContent).toContain("D4");
    // 「歌えない」と言わない。
    await expect(canvasElement.textContent).not.toContain("歌えません");
  },
};

/** 当てるのは本人の操作だけ（`DEC-SYN-012`）。 */
export const 勧めたキーにする: Story = {
  args: { song: { ...song, previewable: false, recommended_transpose: -12 } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "そのキーにする" }));
    await waitFor(() => expect(api.setSongTranspose).toHaveBeenCalledWith("s1", -12));
  },
};

/** すでに動かしてあるときは、その値が出ている。 */
export const キーを動かしてある: Story = {
  args: { song: { ...song, transpose: -12, recommended_transpose: -12 } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByLabelText("この曲を動かす")).toHaveValue("-12");
  },
};
