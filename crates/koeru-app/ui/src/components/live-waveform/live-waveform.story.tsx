import type { Meta, StoryObj } from "@storybook/react-vite";

import { LiveWaveform } from "~/components/live-waveform";

/*
 * いま入ってきている音の波形（`TR-REC-43`）。
 *
 * 中身は Channel から届く。 story では届かないので、絵は空のままになる
 * ——見たいのは、届く前でも枠と入力レベルが出て、名前が付いていること。
 * 動いている絵は実機で見る（`verify-koeru`）。
 */
const meta = {
  title: "部品/LiveWaveform",
  component: LiveWaveform,
} satisfies Meta<typeof LiveWaveform>;

export default meta;

export const 届く前: StoryObj<typeof meta> = {};
