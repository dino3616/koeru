import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, waitFor, within } from "storybook/test";

import { voiceStyle } from "~/lib/voice-color";

import { Button } from ".";

/*
 * 主・副・小の3種を、通常 / hover / 押せない / フォーカスの4状態で出す（`DEC-PLT-022`）。
 *
 * 状態ごとに story を分ける。 1つだけ出すと、残りの配色は axe に一度も当たらない。
 * hover とフォーカスは `play` で実際に起こす——擬似クラスを属性で偽装すると、
 * 本物の `:hover` / `:focus-visible` が当たったときの色を測らない。
 *
 * 押している状態（`active:`）は story にしない。 押したままにする手段が無く、
 * 面を1段進めるだけなので、段そのものを `styles/palette.story.tsx` が測る。
 */
const meta = {
  title: "部品/Button",
  component: Button,
  args: { children: "録る" },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

const buttonIn = (root: HTMLElement) => within(root).getByRole("button");

/**
 * hover で背景が変わることを見る。
 *
 * `storybook/test` の `userEvent.hover` は使わない。 あれは `mouseover` などの
 * イベントを送るだけで、ブラウザの `:hover` は立たない（試して、背景が変わらなかった）。
 * 本物のポインタを動かせるのは、試験を走らせる実ブラウザ側の `userEvent` だけ。
 * Storybook を手で開いたときには無いので、そこでは何もしない。
 *
 * 先に外す。 前の story で乗ったままの位置に本物のポインタが残っていると、
 * 「通常」の色として読んだものが既に hover の色になる。
 */
const hovered: NonNullable<Story["play"]> = async ({ canvasElement }) => {
  // Vitest の実ブラウザの試験だけにある印。 `import.meta.env.VITEST` は立たない。
  if (!("__vitest_browser__" in globalThis)) return;
  const { userEvent: pointer } = await import("vite-plus/test/browser");
  const button = buttonIn(canvasElement);
  await pointer.unhover(button);
  const rest = getComputedStyle(button).backgroundColor;
  await pointer.hover(button);
  await waitFor(() => expect(getComputedStyle(button).backgroundColor).not.toBe(rest));
};

/**
 * キーボードで到達したとき、フォーカス環が出ること（`TR-PLT-27`）。
 *
 * `.focus()` を直接呼ばない。 `:focus-visible` はキーボードの操作で来たときだけ当たるので、
 * Tab で渡して初めて本物の環が出る。
 */
const focused: NonNullable<Story["play"]> = async ({ canvasElement }) => {
  const button = buttonIn(canvasElement);
  await userEvent.tab();
  await expect(button).toHaveFocus();
  await expect(button.matches(":focus-visible")).toBe(true);
  await expect(getComputedStyle(button).outlineStyle).toBe("solid");
};

export const 主: Story = { args: { variant: "primary" } };
export const 主_hover: Story = { args: { variant: "primary" }, play: hovered };
export const 主_押せない: Story = { args: { variant: "primary", disabled: true } };
export const 主_フォーカス: Story = { args: { variant: "primary" }, play: focused };

export const 副: Story = { args: { variant: "secondary" } };
export const 副_hover: Story = { args: { variant: "secondary" }, play: hovered };
export const 副_押せない: Story = { args: { variant: "secondary", disabled: true } };
export const 副_フォーカス: Story = { args: { variant: "secondary" }, play: focused };

export const 小: Story = { args: { variant: "ghost" } };
export const 小_hover: Story = { args: { variant: "ghost" }, play: hovered };
export const 小_押せない: Story = { args: { variant: "ghost", disabled: true } };
export const 小_フォーカス: Story = { args: { variant: "ghost" }, play: focused };

/**
 * 声の色で塗った主ボタン。
 *
 * 上の「主」は声が無い音源で、slate 11 に落ちた姿。 声が届いたときの塗りと字は
 * こちらで axe に当てる。 声の帯ごとの網羅は `styles/palette.story.tsx`。
 */
export const 主_声の色: Story = {
  args: { variant: "primary" },
  decorators: [
    (Story) => (
      // 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。
      // oxlint-disable-next-line shadcn/no-inline-styles
      <div style={voiceStyle({ hue: 200, chroma: 0.5, lightness: 0.5 })}>
        <Story />
      </div>
    ),
  ],
};
export const 主_声の色_hover: Story = { ...主_声の色, play: hovered };

export const 危険: Story = { args: { variant: "danger", children: "止める" } };
export const 小さい: Story = { args: { variant: "secondary", size: "sm", children: "聴く" } };
