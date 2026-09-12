import type { Meta, StoryObj } from "@storybook/react-vite";
import { fn, mocked } from "storybook/test";

import { DeviceCard } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "領域/DeviceCard",
  component: DeviceCard,
  args: { deviceId: undefined, armed: false, onDeviceChange: fn(), onArmed: fn(), onStatus: fn() },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
  beforeEach: () => {
    mocked(api.listDevices).mockResolvedValue([
      { id: "builtin", name: "MacBook Pro のマイク" },
      { id: "usb", name: "USB オーディオ" },
    ]);
    mocked(api.streamEnvelope).mockResolvedValue(1);
    mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
  },
} satisfies Meta<typeof DeviceCard>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 選ぶ前: Story = {};

export const 選んだあと: Story = { args: { deviceId: "builtin", armed: true } };

/**
 * 選ばれているが、まだ開いていない。
 *
 * 起動し直した直後の姿（`TR-REC-03`）。 選択は音源に残るが、
 * ストリームは閉じている——波形を出さない。
 */
export const 選ばれているが開いていない: Story = {
  args: { deviceId: "builtin", armed: false },
};
