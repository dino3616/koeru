import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { PackageImage } from ".";
import { api } from "~/lib/ipc";

/** 1×1 の赤い PNG。story で絵を出すために、最小のものを埋め込む。 */
const RED_DOT = [
  137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 2, 0, 0,
  0, 144, 119, 83, 222, 0, 0, 0, 12, 73, 68, 65, 84, 8, 215, 99, 248, 207, 192, 0, 0, 3, 1, 1, 0,
  24, 221, 141, 219, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

const meta = {
  title: "部品/PackageImage",
  component: PackageImage,
  args: {
    voiceId: "11111111-1111-4111-8111-111111111111",
    slot: "icon" as const,
    label: "一覧に出る絵",
    hint: "正方形に切って、100×100 にして入れます。PNG か JPEG。",
  },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-3">{Story()}</div>],
} satisfies Meta<typeof PackageImage>;

export default meta;
type Story = StoryObj<typeof meta>;

export const まだ選んでいない: Story = {
  beforeEach: () => {
    mocked(api.packageIcon).mockResolvedValue(null);
  },
  play: async ({ canvasElement }) => {
    // 選んでいなければ「外す」を出さない。
    await expect(canvasElement.textContent).not.toContain("外す");
  },
};

export const 選んである: Story = {
  beforeEach: () => {
    mocked(api.packageIcon).mockResolvedValue(RED_DOT);
  },
  play: async ({ canvasElement }) => {
    // 絵には名前を付ける。装飾ではなく、選んだものの確認なので。
    // 読みは中断しないので（`useQuery`）、出るまで待つ。
    await waitFor(async () => {
      const alt = canvasElement.querySelector("img")?.getAttribute("alt");
      await expect(alt).toContain("一覧に出る絵");
    });
  },
};

export const 立ち絵: Story = {
  args: { slot: "portrait" as const, label: "立ち絵", hint: "OpenUtau でだけ出ます。" },
  beforeEach: () => {
    mocked(api.packagePortrait).mockResolvedValue(null);
  },
};
