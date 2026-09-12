import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, waitFor } from "storybook/test";

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

/**
 * 音が届いていて、割れた回もある。
 *
 * **割れた回数は Rust が数えた値をそのまま出す。** 画面で通知を数えていた
 * ときは、1つの割れが窓に残っているあいだ約 30 回に膨らんでいた
 * （`TR-REC-16` の定義は3サンプル以上の連続）。ここが通れば、
 * 届いた値を足し込んでいないことが分かる。
 */
export const 割れた回がある: Story = {
  beforeEach: () => {
    mocked(api.stopEnvelopeStream).mockResolvedValue(undefined);
    mocked(api.streamEnvelope).mockImplementation((channel) => {
      // 受け口が繋がったあとで送る。 開く前に送ると誰も見ていない。
      setTimeout(() => {
        channel.onmessage({
          steps: Array.from({ length: 300 }, (_, i) => {
            const v = Math.sin(i / 6) * 0.9;
            return [-Math.abs(v), Math.abs(v)] as [number, number];
          }),
          position: 44_100,
          clipped_runs: 2,
        });
      }, 0);
      return Promise.resolve(1);
    });
  },
  play: async ({ canvasElement }) => {
    await waitFor(() => expect(canvasElement.textContent).toContain("2 回"));
    // 足し込んでいたら、通知の数だけ増えて 2 のままにはならない。
    await expect(canvasElement.textContent).not.toContain("3 回");
  },
};
