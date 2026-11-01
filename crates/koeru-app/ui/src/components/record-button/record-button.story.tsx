import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { RecordButton } from ".";

/*
 * 4つとも出す（`DEC-PLT-022`）。 出さない状態の配色は axe に一度も当たらない。
 * 押せない状態は `opacity-45` が掛かるので、これも並べる。
 */
const meta = {
  title: "部品/RecordButton",
  component: RecordButton,
  args: { state: "idle", onPress: fn() },
} satisfies Meta<typeof RecordButton>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 待機: Story = {
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector("button");
    await expect(button?.getAttribute("aria-label")).toBe("録る");
  },
};

export const カウント: Story = {
  args: { state: "countdown", count: 2 },
  play: async ({ canvasElement }) => {
    // 見える字は数字。 名前に含まれていないと axe が落とす。
    const button = canvasElement.querySelector("button");
    await expect(button?.getAttribute("aria-label")).toBe("止める カウント 2");
    // 読み上げは常設の領域が持つ。
    await expect(button?.textContent).toBe("2");
    await expect(canvasElement.querySelector("[aria-live]")?.textContent).toBe("2");
  },
};

export const 録音中: Story = {
  args: { state: "recording" },
  play: async ({ canvasElement }) => {
    const button = canvasElement.querySelector("button");
    await expect(button?.getAttribute("aria-label")).toBe("止める");
    // カウントの読み上げは、カウントが終われば空に戻る。
    await expect(canvasElement.querySelector("[aria-live]")?.textContent).toBe("");
  },
};

export const 押せない: Story = {
  args: { state: "idle", disabled: true },
  play: async ({ canvasElement }) => {
    // 確定の途中で次を始めさせない（`TR-REC-42`）。
    await expect(canvasElement.querySelector("button")?.disabled).toBe(true);
  },
};
