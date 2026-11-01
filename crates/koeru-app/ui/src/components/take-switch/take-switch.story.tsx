import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { TakeSwitch, type TakeChoice } from ".";

const takes: TakeChoice[] = [
  { id: 11, generation: 1, label: null, invalid: false },
  { id: 12, generation: 2, label: "ちょい明め", invalid: false },
];

const meta = {
  title: "部品/TakeSwitch",
  component: TakeSwitch,
  args: { takes, adopted: 12, onAdopt: fn(), onRename: fn(), onNew: fn() },
} satisfies Meta<typeof TakeSwitch>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 名前の無いテイクは「テイク N」。 採用しているものが選ばれている。 */
export const 二本: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(
      (await c.findByRole("radio", { name: "ちょい明め" })).getAttribute("aria-checked"),
    ).toBe("true");
    await expect(c.getByRole("radio", { name: "テイク 1" }).getAttribute("aria-checked")).toBe(
      "false",
    );
  },
};

/** 選んだテイクを採用に回す。 矢印で動いた先が選ばれる。 */
export const 切り替える: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("radio", { name: "テイク 1" }));
    await expect(args.onAdopt).toHaveBeenLastCalledWith(11);
  },
};

/** 名前を変える。 Tab と打鍵だけで届く。 前後の空白は落とす。 */
export const 名前を変える: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "ちょい明め の名前を変える" }));
    const field = c.getByRole("textbox", { name: /ちょい明め の新しい名前/ });
    await expect(field).toHaveValue("ちょい明め");
    await userEvent.clear(field);
    await userEvent.type(field, "  明るめ  {Enter}");
    await expect(args.onRename).toHaveBeenLastCalledWith(12, "明るめ");
    // 決めたら欄を畳む。
    await expect(c.queryByRole("textbox")).toBeNull();
  },
};

/** 空にすると名前を消す。 既定の「テイク N」へ戻る。 */
export const 名前を消す: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "ちょい明め の名前を変える" }));
    await userEvent.clear(c.getByRole("textbox"));
    await userEvent.click(c.getByRole("button", { name: "決める" }));
    await expect(args.onRename).toHaveBeenLastCalledWith(12, null);
  },
};

/** 録っているあいだは切り替えも名前の変更もさせない。 */
export const 押せない: Story = {
  args: { disabled: true },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    for (const radio of await c.findAllByRole("radio")) {
      await expect(radio).toBeDisabled();
    }
    await expect(c.getByRole("button", { name: "ちょい明め の名前を変える" })).toBeDisabled();
  },
};

/** 取りこぼしで無効にしたテイクは選べない（`TR-REC-07`）。 */
export const 無効のテイク: Story = {
  args: {
    takes: [...takes, { id: 13, generation: 3, label: null, invalid: true }],
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(await c.findByRole("radio", { name: "テイク 3" })).toBeDisabled();
  },
};

/**
 * ⋮ は選んでいる項目の右端に重なる。 DOM では群の外にあり、`radiogroup` の子は
 * `radio` だけ。 選ぶテイクを替えると付いていく。
 */
export const 名前の印: Story = {
  args: { adopted: 11 },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const group = await c.findByRole("radiogroup", { name: "テイク" });
    await expect([...group.children].every((e) => e.getAttribute("role") === "radio")).toBe(true);
    const kebab = c.getByRole("button", { name: "テイク 1 の名前を変える" });
    const box = c.getByRole("radio", { name: "テイク 1" }).getBoundingClientRect();
    const at = kebab.getBoundingClientRect();
    await expect(at.right).toBeLessThanOrEqual(box.right);
    await expect(at.left).toBeGreaterThanOrEqual(box.left);
  },
};

/** ＋ はこの行をもう一度録る。 */
export const 新しいテイク: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(await c.findByRole("button", { name: "新しいテイク" }));
    await expect(args.onNew).toHaveBeenCalled();
  },
};

/** テイクが無ければ何も出さない。 */
export const 無し: Story = {
  args: { takes: [], adopted: null },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).queryByRole("radiogroup")).toBeNull();
  },
};
