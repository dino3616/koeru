import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

import { PackageContents } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/PackageContents",
  component: PackageContents,
  args: { voiceId: "11111111-1111-4111-8111-111111111111" },
  decorators: [(Story) => <div className="flex w-72 flex-col gap-3">{Story()}</div>],
} satisfies Meta<typeof PackageContents>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 入るものがある: Story = {
  beforeEach: () => {
    mocked(api.packageContents).mockResolvedValue([
      { path: "character.txt", bytes: 48 },
      { path: "character.yaml", bytes: 132 },
      { path: "readme.txt", bytes: 620 },
      { path: "icon.bmp", bytes: 30_054 },
      { path: "oto.ini", bytes: 4_210 },
      { path: "s001.wav", bytes: 705_600 },
      { path: "s001_wav.frq", bytes: 22_080 },
      { path: "s002.wav", bytes: 705_600 },
      { path: "s002_wav.frq", bytes: 22_080 },
      { path: "s003.wav", bytes: 705_600 },
    ]);
  },
  play: async ({ canvasElement }) => {
    // 中断しない読みなので、出るまで待つ（`useQuery`）。
    await waitFor(async () => {
      // 全部は並べない。残りは数でまとめる。
      await expect(canvasElement.textContent).toContain("ほかに");
    });
  },
};

export const まだ何も無い: Story = {
  beforeEach: () => {
    mocked(api.packageContents).mockResolvedValue([]);
  },
};

export const いまは出せない: Story = {
  beforeEach: () => {
    // 組み立てが失敗しても、経路ごと落とさない（`TR-PKG-17` の直し途中）。
    mocked(api.packageContents).mockRejectedValue({
      kind: "text.unencodable",
      message: "この符号化で書けない文字がある",
    });
  },
  play: async ({ canvasElement }) => {
    await waitFor(async () => {
      await expect(canvasElement.textContent).toContain("いまは中身を出せません");
    });
  },
};
