import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "~/components/ui/select";

/*
 * 一覧から1つ選ぶ。
 *
 * `aria-label` を置かない。 置くと可視テキストを上書きして、
 * 選んでいるものが名前から消える。見出しを `aria-labelledby` で指す。
 */
const meta = {
  title: "部品/Select",
  component: Select,
} satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

const DEVICES = [
  { id: "d1", name: "MacBook Pro のマイク" },
  { id: "d2", name: "Scarlett Solo USB" },
];

export const 未選択: Story = {
  render: () => (
    <Select>
      <span id="device-label" className="text-sm text-slate-11">
        入力デバイス
      </span>
      <SelectTrigger aria-labelledby="device-label">
        <SelectValue placeholder="入力デバイスを選ぶ" />
      </SelectTrigger>
      <SelectContent>
        {DEVICES.map((d) => (
          <SelectItem key={d.id} value={d.id}>
            {d.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  ),
  play: async ({ canvasElement }) => {
    // 名前は見出しから来る。選んでいるものが名前を奪われていないこと。
    const trigger = canvasElement.querySelector("[aria-labelledby]");
    await expect(trigger?.getAttribute("aria-label")).toBeNull();
  },
};

export const 選択済み: Story = {
  render: () => (
    <Select value="d2">
      <span id="device-label-2" className="text-sm text-slate-11">
        入力デバイス
      </span>
      <SelectTrigger aria-labelledby="device-label-2">
        <SelectValue placeholder="入力デバイスを選ぶ" />
      </SelectTrigger>
      <SelectContent>
        {DEVICES.map((d) => (
          <SelectItem key={d.id} value={d.id}>
            {d.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  ),
};

/** 開いている最中は選び直させない（`arm_device` が走っている間）。 */
export const 塞いでいる: Story = {
  render: () => (
    <Select value="d1" disabled>
      <span id="device-label-3" className="text-sm text-slate-11">
        入力デバイス
      </span>
      <SelectTrigger aria-labelledby="device-label-3">
        <SelectValue placeholder="入力デバイスを選ぶ" />
      </SelectTrigger>
      <SelectContent>
        {DEVICES.map((d) => (
          <SelectItem key={d.id} value={d.id}>
            {d.name}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  ),
};
