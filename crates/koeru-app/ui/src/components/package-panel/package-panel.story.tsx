import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked } from "storybook/test";

import { PackagePanel } from ".";
import { api } from "~/lib/ipc";
import type { RowTakesView } from "~/lib/ipc";

const rows: RowTakesView[] = [
  {
    row_id: "s002",
    text: "か き く け こ",
    state: "recorded",
    units: 5,
    takes: [{ take_id: 1, generation: 1, peak: 0.71, duration_ms: 3100, invalid: false }],
    adopted: 1,
  },
];

const meta = {
  title: "領域/PackagePanel",
  component: PackagePanel,
  args: { voiceId: "11111111-1111-4111-8111-111111111111", rows, onOpenRow: fn() },
  decorators: [(Story) => <div className="flex w-96 flex-col gap-5">{Story()}</div>],
} satisfies Meta<typeof PackagePanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 引っかかるものが無い: Story = {
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue({
      renamed_to_nfc: 0,
      non_nfc_names: [],
      clipped_takes: [],
      may_export: true,
    });
  },
  play: async ({ canvasElement }) => {
    // 配ることを必須にしない（`TR-PKG-35`）。
    const text = canvasElement.textContent ?? "";
    for (const word of ["公開", "作者", "規約"]) {
      await expect(text).not.toContain(word);
    }
  },
};

export const 割れたテイクがある: Story = {
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue({
      renamed_to_nfc: 2,
      non_nfc_names: [],
      clipped_takes: [["s002", 4]],
      may_export: true,
    });
  },
  play: async ({ canvasElement }) => {
    // 行 ID ではなく読み上げ文字列で指す（`TR-REC-18`）。
    await expect(canvasElement.textContent).not.toContain("s002");
    await expect(canvasElement.textContent).toContain("か き く け こ");
  },
};

export const 書き出せない名前がある: Story = {
  beforeEach: () => {
    mocked(api.preflight).mockResolvedValue({
      renamed_to_nfc: 0,
      non_nfc_names: ["が"],
      clipped_takes: [],
      may_export: false,
    });
  },
};
