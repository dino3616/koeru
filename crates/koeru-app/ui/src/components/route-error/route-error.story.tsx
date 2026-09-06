import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { RouteError } from "~/components/route-error";
import { withRouter } from "~/lib/story-router";

/*
 * ルータが投げた失敗の面。
 *
 * `ErrorBoundary` が拾うのは描画中の例外だけで、ルータ自身が起こした失敗は
 * その外を通る。 どちらの経路でも白い画面にしない。
 */
const meta = {
  title: "部品/RouteError",
  component: RouteError,
  args: { error: null },
  // ルータの文脈を与える。`useNavigate` を使うので、外では落ちる。
  render: ({ error }) => withRouter(<RouteError error={error} />),
} satisfies Meta<typeof RouteError>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Rustからの失敗: Story = {
  args: { error: { kind: "app.unknown_project", message: "その音源が見つかりません" } },
  play: async ({ canvasElement }) => {
    // 原因は `errorMessage` を通す。素の例外を出すとパスや音源名が漏れる。
    await expect(canvasElement.textContent).toContain("その音源が見つかりません");
    await expect(canvasElement.querySelectorAll("button").length).toBe(2);
  },
};

export const 素の例外: Story = { args: { error: new Error("読み込みに失敗しました") } };

/** 何が来ても文言になること。素のまま出さない。 */
export const 見知らぬもの: Story = {
  args: { error: { nope: 1 } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).toContain("予期しない失敗");
  },
};
