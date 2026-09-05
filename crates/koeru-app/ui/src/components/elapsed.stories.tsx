import type { Meta, StoryObj } from "@storybook/react-vite";

import { Elapsed } from "~/components/elapsed";

/** 収録中の経過秒。押せるものの名前には入れない（名前が毎秒変わる）。 */
const meta = {
  title: "部品/Elapsed",
  component: Elapsed,
} satisfies Meta<typeof Elapsed>;

export default meta;

export const 既定: StoryObj<typeof meta> = {};
