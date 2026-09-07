import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { ItemList } from ".";
import type { RowTakesView } from "~/lib/ipc";

const row = (id: string, text: string, takes: number): RowTakesView => ({
  row_id: id,
  text,
  state: takes > 0 ? "recorded" : "unrecorded",
  units: text.split(" ").length,
  takes: Array.from({ length: takes }, (_, i) => ({
    take_id: i + 1,
    generation: i + 1,
    peak: 0.71,
    duration_ms: 3100,
    invalid: false,
  })),
  adopted: takes > 0 ? takes : null,
});

const rows = [
  row("s001", "あ い う え お", 2),
  row("s002", "か き く け こ", 1),
  row("s003", "が ぎ ぐ げ ご", 1),
  row("s004", "さ し す せ そ", 0),
  row("s005", "ざ じ ず ぜ ぞ", 0),
];

const meta = {
  title: "領域/ItemList",
  component: ItemList,
  args: { rows, nextRowId: "s004", onOpen: fn() },
} satisfies Meta<typeof ItemList>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 途中まで録れている: Story = {
  play: async ({ canvasElement }) => {
    /*
     * 行 ID を出さない（`Q-REC-003`、`TR-REC-18`）。
     * 見える字にも、読み上げの名前にも入れない。
     */
    await expect(canvasElement.textContent).not.toContain("s001");
    const labels = [...canvasElement.querySelectorAll("[aria-label]")].map((e) =>
      e.getAttribute("aria-label"),
    );
    await expect(labels.some((l) => l?.includes("s001") === true)).toBe(false);
  },
};

export const 一つも録っていない: Story = {
  args: { rows: rows.map((r) => ({ ...r, takes: [], adopted: null })), nextRowId: "s001" },
};

export const 全部録れている: Story = {
  args: { rows: rows.map((r) => row(r.row_id, r.text, 1)), nextRowId: null },
};
