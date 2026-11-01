import type { Meta, StoryObj } from "@storybook/react-vite";
import { useState } from "react";
import { expect, userEvent, within } from "storybook/test";

import { SettingsTabs } from ".";
import { SETTINGS_TABS, type SettingsTab } from "~/lib/settings-tabs";

/** 選びは URL が持つので、story では state に置き換える。 */
const Harness = ({ initial }: { initial: SettingsTab }) => {
  const [selected, setSelected] = useState(initial);
  const label = SETTINGS_TABS.find((t) => t.id === selected)?.label;
  return (
    <div className="h-96 w-3xl">
      <SettingsTabs selected={selected} onSelect={setSelected}>
        <p className="text-sm text-slate-12">{label}の中身</p>
      </SettingsTabs>
    </div>
  );
};

const meta = {
  title: "部品/SettingsTabs",
  component: Harness,
  args: { initial: "general" },
} satisfies Meta<typeof Harness>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 一般: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(canvas.getAllByRole("tab")).toHaveLength(SETTINGS_TABS.length);
    const general = canvas.getByRole("tab", { name: "一般" });
    await expect(general.getAttribute("aria-selected")).toBe("true");
    // 選んだタブの中身が、そのタブを名前にした面として出る。
    await expect(canvas.getByRole("tabpanel", { name: "一般" })).toBeVisible();
  },
};

export const プライバシー: Story = {
  args: { initial: "privacy" },
};

export const 矢印で動く: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const general = canvas.getByRole("tab", { name: "一般" });
    general.focus();

    await userEvent.keyboard("{ArrowDown}");
    const recording = canvas.getByRole("tab", { name: "収録" });
    await expect(recording.getAttribute("aria-selected")).toBe("true");
    // 焦点も付いてくる。 選んだだけで焦点が残ると、次の矢印が効かない。
    await expect(recording).toHaveFocus();
    await expect(canvas.getByRole("tabpanel", { name: "収録" })).toBeVisible();

    // Tab で入れるのは選んでいるタブだけ。
    await expect(general.getAttribute("tabindex")).toBe("-1");

    await userEvent.keyboard("{ArrowUp}{ArrowUp}");
    // 先頭の前は末尾へ回る。
    await expect(canvas.getByRole("tab", { name: "プライバシー" })).toHaveFocus();

    await userEvent.keyboard("{Home}");
    await expect(general).toHaveFocus();
    await userEvent.keyboard("{End}");
    await expect(canvas.getByRole("tab", { name: "プライバシー" })).toHaveFocus();
  },
};
