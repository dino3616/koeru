import type { Meta, StoryObj } from "@storybook/react-vite";

import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from ".";

const meta = { title: "部品/Select", component: Select } satisfies Meta<typeof Select>;

export default meta;
type Story = StoryObj<typeof meta>;

/*
 * 名前は見えている字が持つ。
 *
 * `aria-label` を置くと可視テキストを上書きして、選んでいるものが
 * 名前から消える（`TR-PLT-29`）。見出しを `aria-labelledby` で指す。
 */
const Example = ({ disabled = false }: { disabled?: boolean }) => (
  <div className="flex w-72 flex-col gap-2">
    <span id="mic" className="text-xs text-slate-11">
      マイク
    </span>
    <Select disabled={disabled}>
      <SelectTrigger aria-labelledby="mic">
        <SelectValue placeholder="マイクを選ぶ" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="a">MacBook Pro のマイク</SelectItem>
        <SelectItem value="b">USB オーディオ</SelectItem>
      </SelectContent>
    </Select>
  </div>
);

export const 選ぶ前: Story = { render: () => <Example /> };
export const 押せない: Story = { render: () => <Example disabled /> };
