import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, fn, userEvent, within } from "storybook/test";

import { Segment, type SegmentOption } from ".";

type Method = "single" | "renri" | "cvvc";

const methods: SegmentOption<Method>[] = [
  { value: "single", label: "単独音" },
  { value: "renri", label: "連続音" },
  { value: "cvvc", label: "CVVC" },
];

/*
 * 選んだ値は部品の外が持つ。 story では state で受けて、
 * 呼ばれたことは spy でも見る。
 */
const Controlled = ({
  initial,
  options,
  onValueChange,
}: {
  initial: Method;
  options: SegmentOption<Method>[];
  onValueChange: (value: Method) => void;
}) => {
  const [value, setValue] = useState(initial);
  return (
    <Segment
      aria-label="方式"
      value={value}
      options={options}
      onValueChange={(next) => {
        setValue(next);
        onValueChange(next);
      }}
    />
  );
};

const meta = {
  title: "部品/Segment",
  component: Controlled,
  args: { initial: "cvvc", options: methods, onValueChange: fn() },
} satisfies Meta<typeof Controlled>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 通常: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const group = await c.findByRole("radiogroup", { name: "方式" });
    await expect(group).toBeTruthy();
    // 色だけで伝えない。選択は `aria-checked` と太さにも出す。
    const on = await c.findByRole("radio", { name: "CVVC" });
    await expect(on.getAttribute("aria-checked")).toBe("true");
    await expect(on.querySelector(".font-semibold:not(.invisible)")).not.toBeNull();
    await expect(
      (await c.findByRole("radio", { name: "単独音" })).getAttribute("aria-checked"),
    ).toBe("false");
  },
};

/** Tab ではグループに1回だけ入り、中は矢印で動く。動いた先が選ばれる。 */
export const フォーカス: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.tab();
    const cvvc = await c.findByRole("radio", { name: "CVVC" });
    await expect(document.activeElement).toBe(cvvc);

    // 選んでいない項目は Tab の順序に入らない。
    await expect((await c.findByRole("radio", { name: "単独音" })).tabIndex).toBe(-1);

    // 端で反対側へ回る。
    await userEvent.keyboard("{ArrowRight}");
    await expect(document.activeElement).toBe(await c.findByRole("radio", { name: "単独音" }));
    await expect(args.onValueChange).toHaveBeenLastCalledWith("single");

    await userEvent.keyboard("{ArrowRight}");
    await expect(args.onValueChange).toHaveBeenLastCalledWith("renri");

    await userEvent.keyboard("{ArrowUp}");
    await expect(args.onValueChange).toHaveBeenLastCalledWith("single");

    await userEvent.keyboard("{End}");
    await expect(args.onValueChange).toHaveBeenLastCalledWith("cvvc");
    await userEvent.keyboard("{Home}");
    await expect(args.onValueChange).toHaveBeenLastCalledWith("single");

    // 動いた先が選ばれ、Tab で入る先もそこに移る。
    await expect((await c.findByRole("radio", { name: "単独音" })).tabIndex).toBe(0);
    await expect((await c.findByRole("radio", { name: "CVVC" })).tabIndex).toBe(-1);
  },
};

/** 録ったあとの方式は選べない。矢印は押せない項目を飛ばす。 */
export const 押せない項目あり: Story = {
  args: {
    initial: "single",
    options: [
      { value: "single", label: "単独音" },
      { value: "renri", label: "連続音", disabled: true },
      { value: "cvvc", label: "CVVC" },
    ],
  },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.tab();
    await expect(document.activeElement).toBe(await c.findByRole("radio", { name: "単独音" }));

    await userEvent.keyboard("{ArrowRight}");
    await expect(document.activeElement).toBe(await c.findByRole("radio", { name: "CVVC" }));
    await expect(args.onValueChange).toHaveBeenLastCalledWith("cvvc");

    // 端からは押せない項目をまたいで回る。
    await userEvent.keyboard("{ArrowRight}");
    await expect(document.activeElement).toBe(await c.findByRole("radio", { name: "単独音" }));
    await expect(args.onValueChange).toHaveBeenCalledTimes(2);

    // 押せない項目は押しても選ばれない。
    await userEvent.click(await c.findByRole("radio", { name: "連続音" }));
    await expect(args.onValueChange).toHaveBeenCalledTimes(2);
  },
};

/** テイクの切り替え。 */
export const テイク: Story = {
  args: {
    initial: "single",
    options: [
      { value: "single", label: "テイク 1" },
      { value: "renri", label: "テイク 2" },
    ],
  },
};

/** 範囲の切り替え。 */
export const 範囲: Story = {
  args: {
    initial: "single",
    options: [
      { value: "single", label: "選択範囲" },
      { value: "renri", label: "曲全体" },
    ],
  },
};
