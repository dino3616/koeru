import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn, mocked } from "storybook/test";

import { InputSetup } from "~/components/input-setup";
import { api } from "~/lib/ipc";

/*
 * 入力の面。マイクが使える状態かを、ここだけで見せる。
 *
 * 監視（波形とレベル）は常に見せ、設定（選択・校正・回り込み）は済んだら畳む。
 * 畳んだ状態と開いた状態の両方を出す。
 */
const meta = {
  title: "部品/InputSetup",
  component: InputSetup,
  args: {
    deviceId: "d1",
    guideMidi: 60,
    onDeviceChange: fn(),
    onStatus: fn(),
    onError: fn(),
    onLeakChecked: fn(),
  },
  beforeEach: () => {
    mocked(api.listDevices).mockResolvedValue([
      { id: "d1", name: "MacBook Pro のマイク" },
      { id: "d2", name: "Scarlett Solo USB" },
    ]);
    mocked(api.outputKind).mockResolvedValue("Headphones");
  },
} satisfies Meta<typeof InputSetup>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 選択済み: Story = {};

export const 未選択: Story = { args: { deviceId: undefined } };

/** 残量が足りない（`TR-REC-41`）。「足りません」だけでは判断できない。 */
export const 残量が足りない: Story = {
  beforeEach: () => {
    mocked(api.estimateSpace).mockResolvedValue({
      remaining_rows: 120,
      rows_that_fit: 34,
      sufficient: false,
      required_bytes: 4_200_000_000,
      available_bytes: 1_200_000_000,
    });
  },
};
