import type { Meta, StoryObj } from "@storybook/react-vite";
import { type ComponentProps, useState } from "react";
import { expect, fn, userEvent, within } from "storybook/test";

import { LaneKeyboard } from ".";
import { keySpan } from "./geometry";
import { toneName } from "~/lib/tones";

type Props = ComponentProps<typeof LaneKeyboard>;

/**
 * 窓と選びを story の中で持つ。 部品は両方とも置く側が制御するので、
 * これが無いと、押しても送っても何も変わらない。
 */
const Lane = ({ view: initialView, selected: initialSelected, onToggle, ...rest }: Props) => {
  const [view, setView] = useState(initialView);
  const [selected, setSelected] = useState(initialSelected);
  return (
    <LaneKeyboard
      {...rest}
      view={view}
      onViewChange={(next) => {
        setView(next);
        rest.onViewChange(next);
      }}
      selected={selected}
      onToggle={(midi) => {
        setSelected((now) => (now.includes(midi) ? now.filter((m) => m !== midi) : [...now, midi]));
        onToggle(midi);
      }}
    />
  );
};

const C1 = 24;
const B7 = 107;
const C3 = 48;
const C6 = 84;
const A3 = 57;

const meta = {
  title: "部品/LaneKeyboard",
  component: LaneKeyboard,
  render: (args) => <Lane {...args} />,
  args: {
    label: "Normal CVVC",
    range: [C1, B7],
    view: [C3, C6],
    selected: [],
    onViewChange: fn(),
    onToggle: fn(),
  },
  decorators: [(Story) => <div className="w-180">{Story()}</div>],
} satisfies Meta<typeof LaneKeyboard>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 見えている字は C だけ。 名前は読み上げが持つ。 */
export const 何も選んでいない: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("toolbar", { name: "Normal CVVC の鍵盤" })).toBeVisible();
    await expect(c.getByRole("button", { name: "A3 を足す" })).toBeVisible();
    // MIDI 番号を画面に出さない（`TR-REC-25`）。
    await expect(canvasElement.textContent).not.toContain("57");
  },
};

export const 三つ選んだ: Story = {
  args: { selected: [57, 67, 72] },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    // 選んだ鍵は名前が見える。 操作は「外す」に変わる。
    await expect(c.getByRole("button", { name: "A3 を外す" })).toHaveTextContent("A3");
    await expect(c.getByRole("button", { name: "G4 を外す" })).toBeVisible();
    await expect(c.getByRole("button", { name: "C5 を外す" })).toBeVisible();
    await expect(c.getByRole("button", { name: "B3 を足す" })).toBeVisible();
  },
};

/** 黒鍵も選べる。 名前は `#` で書く。 */
export const 黒鍵を選んだ: Story = {
  args: { selected: [58, 63] },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("button", { name: "A#3 を外す" })).toBeVisible();
    await expect(c.getByRole("button", { name: "D#4 を外す" })).toBeVisible();
  },
};

export const 作成済みあり: Story = {
  args: { selected: [57, 67, 72], created: [57, 67] },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    // 下線は絵だけなので、名前にも出す。
    await expect(c.getByRole("button", { name: "A3 を外す（作成済み）" })).toBeVisible();
    await expect(c.getByRole("button", { name: "C5 を外す" })).toBeVisible();
  },
};

/** 同じ組がもうある音階。 押せないが、矢印では止まる。 */
export const 選べない鍵あり: Story = {
  args: { selected: [57], locked: [52, 55, 60] },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    const e3 = c.getByRole("button", { name: "E3（選べない）" });
    await expect(e3).toHaveAttribute("aria-disabled", "true");
    await userEvent.click(e3);
    await expect(args.onToggle).not.toHaveBeenCalled();
    // 選べない鍵のとなりは、いつもどおり押せる。
    await userEvent.click(c.getByRole("button", { name: "A3 を外す" }));
    await expect(args.onToggle).toHaveBeenCalledTimes(1);
    await expect(args.onToggle).toHaveBeenCalledWith(A3);
  },
};

/** 範囲の右端。 これ以上は送れない。 */
export const 窓の右端: Story = {
  args: { view: [83, B7], selected: [107] },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("button", { name: "Normal CVVC の鍵盤を右へ送る" })).toBeDisabled();
    await userEvent.click(c.getByRole("button", { name: "Normal CVVC の鍵盤を左へ送る" }));
    // 窓の幅は変えずに、1オクターブ戻る。
    await expect(args.onViewChange).toHaveBeenCalledTimes(1);
    await expect(args.onViewChange).toHaveBeenCalledWith([71, 95]);
  },
};

/** MIDI の全域（C-1〜G9）の左端。 負のオクターブの名前も出る。 */
export const 窓の左端: Story = {
  args: { range: [0, 127], view: [0, 24], selected: [0] },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("button", { name: "Normal CVVC の鍵盤を左へ送る" })).toBeDisabled();
    await expect(c.getByRole("button", { name: "C-1 を外す" })).toHaveTextContent("C-1");
    await userEvent.click(c.getByRole("button", { name: "Normal CVVC の鍵盤を右へ送る" }));
    await expect(args.onViewChange).toHaveBeenCalledTimes(1);
    await expect(args.onViewChange).toHaveBeenCalledWith([12, 36]);
  },
};

/** 範囲の外の鍵（窓が範囲より広い）は押せない。 */
export const 範囲の外は押せない: Story = {
  args: { range: [A3, 72], view: [C3, 72] },
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.click(c.getByRole("button", { name: "G3（選べない）" }));
    await expect(args.onToggle).not.toHaveBeenCalled();
  },
};

/**
 * キーボードだけで鍵を1つ選べる。
 *
 * 鍵盤へは Tab で1回だけ入り、矢印で鍵を動き、Enter で押す。
 * 鍵が1つずつ Tab を取ると、収録の前に 40 回押すことになる。
 */
export const キーボードだけで選ぶ: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    // 先頭は左へ送るボタン。 その次の Tab で鍵盤へ入る。
    await userEvent.tab();
    await expect(c.getByRole("button", { name: "Normal CVVC の鍵盤を左へ送る" })).toHaveFocus();
    await userEvent.tab();
    const toolbar = c.getByRole("toolbar");
    await expect(toolbar.contains(canvasElement.ownerDocument.activeElement)).toBe(true);
    // 窓の左端（C3）に入る。 そこから A3 まで 9 半音。
    await expect(c.getByRole("button", { name: "C3 を足す" })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight>9/}");
    await expect(c.getByRole("button", { name: "A3 を足す" })).toHaveFocus();
    await userEvent.keyboard("{Enter}");
    await expect(args.onToggle).toHaveBeenCalledTimes(1);
    await expect(args.onToggle).toHaveBeenCalledWith(A3);
    // 押すと名前が変わる。 焦点は鍵に残る。
    await expect(c.getByRole("button", { name: "A3 を外す" })).toHaveFocus();
    // Space でも外れる。
    await userEvent.keyboard(" ");
    await expect(c.getByRole("button", { name: "A3 を足す" })).toHaveFocus();
    // 鍵盤の中の鍵は Tab を取らない。 次の Tab は右へ送るボタン。
    await userEvent.tab();
    await expect(c.getByRole("button", { name: "Normal CVVC の鍵盤を右へ送る" })).toHaveFocus();
    // 戻ると、最後にいた鍵へ入る。
    await userEvent.tab({ shift: true });
    await expect(c.getByRole("button", { name: "A3 を足す" })).toHaveFocus();
  },
};

/** 選んだ鍵があれば、Tab はそこへ入る。 端は Home / End。 */
export const 選んだ鍵へ入る: Story = {
  args: { selected: [67] },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await userEvent.tab();
    await userEvent.tab();
    await expect(c.getByRole("button", { name: "G4 を外す" })).toHaveFocus();
    await userEvent.keyboard("{End}");
    await expect(c.getByRole("button", { name: `${toneName(C6)} を足す` })).toHaveFocus();
    await userEvent.keyboard("{ArrowRight}");
    // 窓の外へは矢印で出ない。
    await expect(c.getByRole("button", { name: "C6 を足す" })).toHaveFocus();
    await userEvent.keyboard("{Home}");
    await expect(c.getByRole("button", { name: "C3 を足す" })).toHaveFocus();
  },
};

/** 帯のような重ねるものが、鍵と同じ座標で描ける。 */
export const 層を重ねる: Story = {
  args: {
    overlay: (
      <div
        className="absolute inset-y-0 left-1/4 w-1/4 border-2 border-slate-12"
        data-testid="band"
      />
    ),
  },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByTestId("band")).toBeVisible();
    // 窓が C3〜C6（白鍵 22）のとき、C4 は左から 7 本目。
    const { left, width } = keySpan(60, [C3, C6]);
    await expect(left).toBeCloseTo(7 / 22);
    await expect(width).toBeCloseTo(1 / 22);
    // 黒鍵は白鍵との境目を中心に、白鍵より細い。
    const black = keySpan(61, [C3, C6]);
    await expect(black.left + black.width / 2).toBeCloseTo(8 / 22);
    await expect(black.width).toBeLessThan(width);
  },
};
