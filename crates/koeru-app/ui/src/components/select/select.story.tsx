import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, within } from "storybook/test";

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

/*
 * パンくずの中の選択（`size="sm"`）。
 *
 * 見える見出しが無いので、名前は見えない見出しと、選んでいる字そのものの
 * 2つで作る（`aria-labelledby` に自分の `id` も入れる）。 `aria-label` だけにすると、
 * 選んでいるものが名前から消える（`TR-PLT-29`）。
 */
const Breadcrumb = ({
  disabledItem = false,
  defaultValue,
}: {
  disabledItem?: boolean;
  defaultValue?: string;
}) => (
  <nav
    aria-label="いまいる場所"
    className="flex h-toolbar w-120 items-center gap-2 border-b border-slate-6 px-4 text-md text-slate-11"
  >
    <span>ナギ</span>
    <span aria-hidden="true">›</span>
    <span id="subbank-name" className="sr-only">
      サブバンク
    </span>
    <Select defaultValue={defaultValue}>
      <SelectTrigger id="subbank" size="crumb" aria-labelledby="subbank-name subbank">
        <SelectValue placeholder="サブバンクを選ぶ" />
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="normal-a3">Normal A3 CVVC</SelectItem>
        <SelectItem value="normal-g4" disabled={disabledItem}>
          Normal G4 CVVC
        </SelectItem>
        <SelectItem value="whisper-c5">Whisper C5 連続音</SelectItem>
      </SelectContent>
    </Select>
    <span aria-hidden="true">›</span>
    <span>収録</span>
  </nav>
);

export const パンくず: Story = {
  render: () => <Breadcrumb defaultValue="normal-a3" />,
  play: async ({ canvasElement }) => {
    const trigger = within(canvasElement).getByRole("combobox", { name: /サブバンク/ });
    await expect(trigger.textContent).toContain("Normal A3 CVVC");
    // ツールバーの高さ（52px）に収まる。
    const toolbar = canvasElement.querySelector("nav");
    await expect(trigger.getBoundingClientRect().height).toBeLessThan(
      toolbar?.getBoundingClientRect().height ?? 0,
    );
  },
};

export const パンくず選ぶ前: Story = { render: () => <Breadcrumb /> };

/** 押せない項目は開いても選べない。キーボードで開いて確かめる。 */
export const パンくず押せない項目あり: Story = {
  render: () => <Breadcrumb defaultValue="normal-a3" disabledItem />,
  play: async ({ canvasElement }) => {
    await userEvent.tab();
    const trigger = within(canvasElement).getByRole("combobox");
    await expect(document.activeElement).toBe(trigger);
    await userEvent.keyboard("{Enter}");
    const item = await within(document.body).findByRole("option", { name: "Normal G4 CVVC" });
    await expect(item.getAttribute("aria-disabled")).toBe("true");
    // 開いたままだと、Radix が残りを `aria-hidden` にした状態で axe に当たる。
    await userEvent.keyboard("{Escape}");
  },
};

/** キーボードで入ったときの見た目。 */
export const パンくずフォーカス: Story = {
  render: () => <Breadcrumb defaultValue="normal-a3" />,
  play: async ({ canvasElement }) => {
    await userEvent.tab();
    await expect(document.activeElement).toBe(within(canvasElement).getByRole("combobox"));
  },
};
