import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { ErrorBoundary } from "~/components/error-boundary";

/*
 * 描画中の例外を受け止める。
 *
 * 白い画面にしない。 これが無いと、例外が出た時点で React が木を丸ごと外し、
 * 何も出ないまま戻る手段も無くなる。収録の途中で起きうる。
 */
const meta = {
  title: "部品/ErrorBoundary",
  component: ErrorBoundary,
  args: { children: null },
} satisfies Meta<typeof ErrorBoundary>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 例外を投げる子。story のためだけに置く。 */
const Throws = (): never => {
  throw new Error("描画に失敗しました");
};

export const 受け止めた: Story = {
  args: { children: <Throws /> },
  play: async ({ canvasElement }) => {
    // やり直す手段がその場にあること。無いと戻れない。
    await expect(canvasElement.querySelectorAll("button").length).toBeGreaterThan(0);
    /*
     * 画面を見ていない人にも届くこと。
     *
     * ここは `Announcer` ごと置き換わった後なので、読み上げ領域は
     * 木に居ない。これが唯一の通知経路になる（`TR-PLT-29`）。
     */
    await expect(canvasElement.querySelector("[role='alert']")).not.toBeNull();
  },
};

export const 例外が無いとき: Story = {
  args: { children: <p className="text-sm text-slate-12">中身がそのまま出る</p> },
};
