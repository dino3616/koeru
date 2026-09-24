import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { ErrorBoundary } from ".";
import type { AppError } from "~/lib/ipc";

const meta = {
  title: "部品/ErrorBoundary",
  component: ErrorBoundary,
} satisfies Meta<typeof ErrorBoundary>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 例外を投げる子。描画のたびに落ちる。 */
const Broken = (): never => {
  throw {
    code: "app.poisoned",
    class: "internal",
    outcome: "unknown",
    action: "report",
    message: "内部状態が壊れている。開き直してほしい",
  } satisfies AppError;
};

export const 受け止めたところ: Story = {
  args: { children: <Broken /> },
  play: async ({ canvasElement }) => {
    // 割り込んで読ませる。画面が丸ごと入れ替わったことは待たせない。
    await expect(canvasElement.querySelector("[role='alert']")).not.toBeNull();
  },
};

export const 何も起きていないとき: Story = {
  args: { children: <p className="text-sm text-slate-12">中身がそのまま出る</p> },
};
