import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, waitFor } from "storybook/test";

import { AlignCard } from ".";
import { api } from "~/lib/ipc";

const NOTICE = `# 同梱しているモデルと辞書

## MFA Japanese acoustic model

- 用途: 強制アライメント
- 出所: https://huggingface.co/MontrealCorpusTools/japanese_mfa
- ライセンス: CC-BY-4.0
- 帰属表示: Montreal Forced Aligner contributors
`;

const meta = {
  title: "領域/AlignCard",
  component: AlignCard,
  args: {
    voiceId: "11111111-1111-4111-8111-111111111111",
    onOpenRow: fn(),
    textOf: (rowId: string) => ({ s001: "あ い う え お", s003: "が ぎ ぐ げ ご" })[rowId] ?? rowId,
  },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof AlignCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 今のモデルで作られている: Story = {
  beforeEach: () => {
    mocked(api.staleTakes).mockResolvedValue([]);
    mocked(api.modelNotice).mockResolvedValue(NOTICE);
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("いま入っているモデル"));
  },
};

export const 前の版のモデルで作られたものがある: Story = {
  beforeEach: () => {
    mocked(api.staleTakes).mockResolvedValue(["s001", "s003"]);
    mocked(api.modelNotice).mockResolvedValue(NOTICE);
  },
  play: async ({ canvasElement }) => {
    // 行 ID を出さない（`DEC-REC-009`）。読み上げの名前にも入れない。
    await waitFor(() => expect(canvasElement.textContent).toContain("あ い う え お"));
    await expect(canvasElement.textContent).not.toContain("s001");
    // 黙って作り直さない（`TR-ALN-29`）。そのままでも使えると書く。
    await expect(canvasElement.textContent).toContain("そのままでも使えます");
  },
};

export const 帰属表示を開く: Story = {
  beforeEach: () => {
    mocked(api.staleTakes).mockResolvedValue([]);
    mocked(api.modelNotice).mockResolvedValue(NOTICE);
  },
  play: async ({ canvasElement }) => {
    const toggle = await waitFor(() => {
      const b = [...canvasElement.querySelectorAll("button")].find(
        (x) => x.textContent === "同梱しているモデル",
      );
      if (b === undefined) throw new Error("まだ出ていない");
      return b;
    });
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");
    await userEvent.click(toggle);
    await waitFor(() => expect(canvasElement.textContent).toContain("CC-BY-4.0"));
  },
};
