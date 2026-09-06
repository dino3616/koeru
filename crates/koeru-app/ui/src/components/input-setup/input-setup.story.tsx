import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, waitFor } from "storybook/test";

import { useState } from "react";

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

/*
 * 残量が足りない（`TR-REC-41`）。「足りません」だけでは判断できない。
 *
 * `deviceId` を渡すだけでは出ない。 残量は `choose()` の中で引くので、
 * 実際に選ばせないとモックは呼ばれず、警告も描かれない。
 */
export const 残量が足りない: Story = {
  /*
   * `deviceId` は親が持つ。 story でも持ち主を用意しないと、選んでも
   * `deviceId` が変わらず、警告を出す条件（`space` の取得）まで進まない。
   */
  render: (args) => {
    const [deviceId, setDeviceId] = useState<string | undefined>(undefined);
    return <InputSetup {...args} deviceId={deviceId} onDeviceChange={setDeviceId} />;
  },
  play: async ({ canvasElement, userEvent }) => {
    // 設定は畳まれていない（回り込み未確認なので）。デバイスを選ぶ。
    const trigger = canvasElement.querySelector("[aria-labelledby='device-label']");
    await userEvent.click(trigger as HTMLElement);
    const option = await waitFor(() => {
      const el = document.body.querySelector("[role='option']");
      if (el === null) throw new Error("選択肢が出ていない");
      return el;
    });
    await userEvent.click(option as HTMLElement);
    // まず選択が反映されること。ここで止まるなら、選ぶ操作が届いていない。
    await waitFor(async () => {
      await expect(document.body.textContent).toContain("MacBook");
    });
    await waitFor(
      async () => {
        await expect(document.body.textContent).toContain("件までしか録れません");
      },
      { timeout: 3000 },
    );
  },
  /*
   * 鎖の全部を答えさせる。
   *
   * 選ぶと `armDevice` → `probeInput` → `estimateSpace` と続く。
   * 既定は「待ち続ける」なので、途中を答えないとそこで止まり、
   * 残量の警告まで進まない。
   */
  beforeEach: () => {
    mocked(api.armDevice).mockResolvedValue("Standard");
    mocked(api.probeInput).mockResolvedValue(0.4);
    mocked(api.estimateSpace).mockResolvedValue({
      remaining_rows: 120,
      rows_that_fit: 34,
      sufficient: false,
      required_bytes: 4_200_000_000,
      available_bytes: 1_200_000_000,
    });
  },
};
