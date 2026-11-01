import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, userEvent, within } from "storybook/test";

import { PrivacyPanel } from ".";

const meta = {
  title: "領域/PrivacyPanel",
  component: PrivacyPanel,
  parameters: { layout: "padded" },
  args: {
    settings: { usage: "NOT_ASKED", crashReports: "NOT_ASKED", lastSentAt: null },
    onChoose: fn(),
    problem: null,
  },
} satisfies Meta<typeof PrivacyPanel>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 既定: Story = {
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByRole("switch", { name: "利用状況" }).getAttribute("aria-checked"),
    ).toBe("false");
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("false");
    await expect(canvas.getByText("まだ送っていません")).toBeVisible();

    // 開けない理由が字で出ていて、ボタンの説明としても引かれる。
    const open = canvas.getByRole("button", { name: "記録を開く…" });
    await expect(open).toBeDisabled();
    await expect(open).toHaveAccessibleDescription("送った内容を読む画面はまだありません");

    // 切り替えは1つの単位だけを運ぶ。
    await userEvent.click(canvas.getByRole("switch", { name: "クラッシュ報告" }));
    await expect(args.onChoose).toHaveBeenCalledTimes(1);
    await expect(args.onChoose).toHaveBeenCalledWith("CRASH_REPORTS", true);
  },
};

export const 利用状況だけオン: Story = {
  args: { settings: { usage: "GRANTED", crashReports: "NOT_ASKED", lastSentAt: null } },
  play: async ({ canvasElement, args }) => {
    const canvas = within(canvasElement);
    const usage = canvas.getByRole("switch", { name: "利用状況" });
    await expect(usage.getAttribute("aria-checked")).toBe("true");
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("false");
    await userEvent.click(usage);
    await expect(args.onChoose).toHaveBeenCalledTimes(1);
    await expect(args.onChoose).toHaveBeenCalledWith("USAGE", false);
  },
};

/** 撤回したものも、送らないので切れて見える。 */
export const 撤回した: Story = {
  args: { settings: { usage: "REVOKED", crashReports: "GRANTED", lastSentAt: null } },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(
      canvas.getByRole("switch", { name: "利用状況" }).getAttribute("aria-checked"),
    ).toBe("false");
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("true");
  },
};

export const 送った記録がある: Story = {
  args: {
    settings: { usage: "GRANTED", crashReports: "NOT_ASKED", lastSentAt: "2026-09-21T18:24:00Z" },
  },
  play: async ({ canvasElement }) => {
    const time = within(canvasElement).getByText(/2026/);
    await expect(time.tagName).toBe("TIME");
    await expect(time.getAttribute("datetime")).toBe("2026-09-21T18:24:00Z");
  },
};

export const 保存できなかった: Story = {
  args: { problem: "設定を保存できませんでした。もう一度お試しください。" },
  play: async ({ canvasElement }) => {
    await expect(within(canvasElement).getByRole("status")).toHaveTextContent(
      "保存できませんでした",
    );
  },
};
