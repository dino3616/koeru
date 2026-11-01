import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { RecordToolbar } from ".";
import type { GuideTempo } from "~/lib/ipc";

const tempo: GuideTempo = {
  bpm: 120,
  beatMs: 500,
  defaultBpm: 120,
  minBpm: 60,
  maxBpm: 180,
  stepBpm: 5,
  leadInMs: 1250,
};

const meta = {
  title: "部品/RecordToolbar",
  component: RecordToolbar,
  // ツールバーの高さ（52px）の中に置く。
  decorators: [
    (Story) => (
      <div className="flex h-13 items-center border-b border-line px-3">
        <Story />
      </div>
    ),
  ],
  args: {
    coverage: { done: 71, total: 100 },
    mode: "CoverageEfficiency",
    onModeChange: fn(),
    tempo,
    onTempoChange: fn(),
    devices: [
      { id: "mic-1", name: "内蔵マイク" },
      { id: "mic-2", name: "USB マイク" },
    ],
    deviceId: "mic-2",
    onDeviceChange: fn(),
    locked: false,
  },
} satisfies Meta<typeof RecordToolbar>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 待機中。 どれも変えられる。 */
export const 待機: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("71%")).toBeVisible();
    await expect(c.getByRole("combobox", { name: "録る順" })).toHaveTextContent("少ない行数で");
    await expect(c.getByRole("combobox", { name: "マイク" })).toHaveTextContent("USB マイク");
  },
};

/** テンポは刻みで上げ下げする。 数は Rust から受けた刻みで動く。 */
export const テンポ: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(c.getByRole("button", { name: "テンポを上げる" }));
    await expect(args.onTempoChange).toHaveBeenLastCalledWith(125);
    await userEvent.click(c.getByRole("button", { name: "テンポを下げる" }));
    await expect(args.onTempoChange).toHaveBeenLastCalledWith(115);
  },
};

/** 範囲の端では、その向きへは押せない（`DEC-REC-012`）。 */
export const テンポの端: Story = {
  args: { tempo: { ...tempo, bpm: 180, beatMs: 60_000 / 180 } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("button", { name: "テンポを上げる" })).toBeDisabled();
    await expect(c.getByRole("button", { name: "テンポを下げる" })).toBeEnabled();
  },
};

/** 録っているあいだは、どれも変えさせない（`TR-REC-30`）。 */
export const 録音中: Story = {
  args: { locked: true },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("button", { name: "テンポを上げる" })).toBeDisabled();
    await expect(c.getByRole("button", { name: "テンポを下げる" })).toBeDisabled();
    await expect(c.getByRole("combobox", { name: "録る順" })).toBeDisabled();
    await expect(c.getByRole("combobox", { name: "マイク" })).toBeDisabled();
  },
};

/** マイクが1つも無い。 選ぶ的を出さず、無いことを言う。 */
export const マイク無し: Story = {
  args: { devices: [], deviceId: undefined, coverage: { done: 0, total: 0 } },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("combobox", { name: "マイク" })).toHaveTextContent(
      "マイクがありません",
    );
    await expect(c.getByText("0%")).toBeVisible();
  },
};
