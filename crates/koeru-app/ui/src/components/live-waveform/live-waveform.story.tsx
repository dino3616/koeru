import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked } from "storybook/test";

import { LiveWaveform } from ".";
import { api } from "~/lib/ipc";

const meta = {
  title: "部品/LiveWaveform",
  component: LiveWaveform,
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
  beforeEach: () => {
    // 開く前は空で正しい。番号だけ返して、フレームは送らない。
    mocked(api.streamEnvelope).mockResolvedValue(1);
    mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
  },
} satisfies Meta<typeof LiveWaveform>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 開いた直後: Story = {
  play: async ({ canvasElement }) => {
    // canvas は読み上げに何も出さない。値はメーターが持つ（`TR-PLT-29`）。
    await expect(canvasElement.querySelector("canvas")?.getAttribute("role")).toBe("img");
    await expect(canvasElement.querySelector("meter")).not.toBeNull();
  },
};
