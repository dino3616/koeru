import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, mocked, userEvent, waitFor, within } from "storybook/test";

import { SettingsScreen } from ".";
import { api } from "~/lib/ipc";
import type { PrivacySettings } from "~/lib/ipc";
import { withRouter } from "~/lib/story-router";

/** 訊いていない。 製品の既定（`TR-PKG-54`）。 */
const defaults: PrivacySettings = {
  usage: "NOT_ASKED",
  crashReports: "NOT_ASKED",
  lastSentAt: null,
};

const meta = {
  title: "画面/SettingsScreen",
  component: SettingsScreen,
  parameters: { layout: "fullscreen" },
  // 画面は窓いっぱいに置くもの。 高さを持たない枠に入れると、中身の面が潰れる。
  decorators: [(Story) => <div className="h-screen">{Story()}</div>],
  beforeEach: () => {
    mocked(api.privacy).mockResolvedValue(defaults);
  },
} satisfies Meta<typeof SettingsScreen>;

export default meta;
type Story = StoryObj<typeof meta>;

const selectedTab = (canvas: ReturnType<typeof within>) =>
  canvas.getByRole("tab", { selected: true });

export const 既定: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=privacy"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole("heading", { name: "プライバシー" })).toBeVisible();
    await expect(selectedTab(canvas)).toHaveAccessibleName("プライバシー");
    await expect(canvas.getByText("既定はどちらもオフ")).toBeVisible();
    await expect(
      canvas.getByRole("switch", { name: "利用状況" }).getAttribute("aria-checked"),
    ).toBe("false");
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("false");
    await expect(canvas.getByText("まだ送っていません")).toBeVisible();
    await expect(canvas.getByRole("button", { name: "記録を開く…" })).toBeDisabled();
  },
};

export const 利用状況だけオン: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=privacy"),
  beforeEach: () => {
    mocked(api.privacy).mockResolvedValue({ ...defaults, usage: "GRANTED" });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const usage = await canvas.findByRole("switch", { name: "利用状況" });
    await expect(usage.getAttribute("aria-checked")).toBe("true");
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("false");
  },
};

export const 他のタブを選んだ: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=tools"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole("heading", { name: "外部ツール" })).toBeVisible();
    await expect(selectedTab(canvas)).toHaveAccessibleName("外部ツール");
    await expect(canvas.getByText("まだありません")).toBeVisible();
    // 中身を作っていないタブは、同意を読みにも行かない。
    await expect(canvas.queryByRole("switch")).toBeNull();
    await expect(api.privacy).not.toHaveBeenCalled();
  },
};

/** `tab` が無い経路は一般。 */
export const 既定のタブ: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole("heading", { name: "一般" })).toBeVisible();
    await expect(selectedTab(canvas)).toHaveAccessibleName("一般");
  },
};

export const 矢印でタブを移る: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=general"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const general = await canvas.findByRole("tab", { name: "一般" });
    general.focus();

    await userEvent.keyboard("{ArrowDown}");
    // 選びは URL が持つので、見えている選びが変わったなら URL が変わっている。
    await waitFor(() => expect(selectedTab(canvas)).toHaveAccessibleName("収録"));
    await expect(await canvas.findByRole("heading", { name: "収録" })).toBeVisible();
    await expect(canvas.getByRole("tab", { name: "収録" })).toHaveFocus();

    await userEvent.keyboard("{End}");
    await waitFor(() => expect(selectedTab(canvas)).toHaveAccessibleName("プライバシー"));
    await expect(await canvas.findByRole("switch", { name: "利用状況" })).toBeVisible();
  },
};

export const 切り替えを保存する: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=privacy"),
  beforeEach: () => {
    mocked(api.privacy).mockResolvedValue(defaults);
    mocked(api.choosePrivacyConsent).mockResolvedValue({
      outcome: {
        __typename: "PrivacyConsentChosen",
        privacy: { ...defaults, usage: "GRANTED" },
      },
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole("switch", { name: "利用状況" }));
    // 押したスイッチだけが運ばれる。 もう片方は動かさない。
    await expect(api.choosePrivacyConsent).toHaveBeenCalledTimes(1);
    await expect(api.choosePrivacyConsent).toHaveBeenCalledWith({
      subject: "USAGE",
      granted: true,
    });
    await waitFor(() =>
      expect(canvas.getByRole("switch", { name: "利用状況" }).getAttribute("aria-checked")).toBe(
        "true",
      ),
    );
    await expect(
      canvas.getByRole("switch", { name: "クラッシュ報告" }).getAttribute("aria-checked"),
    ).toBe("false");
  },
};

export const 保存できなかった: Story = {
  render: () => withRouter(<SettingsScreen />, "/settings?tab=privacy"),
  beforeEach: () => {
    mocked(api.privacy).mockResolvedValue(defaults);
    mocked(api.choosePrivacyConsent).mockResolvedValue({
      outcome: {
        __typename: "StorageProblem",
        code: "privacy.storage",
        class: "storage",
        commit: "not_committed",
        actions: ["retry_same"],
        message: "設定を保存できませんでした。もう一度お試しください。",
      },
    });
  },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await userEvent.click(await canvas.findByRole("switch", { name: "利用状況" }));
    await waitFor(() =>
      expect(canvas.getByRole("status")).toHaveTextContent("設定を保存できませんでした"),
    );
    // 保存されていないものを、保存されたように見せない。
    await expect(
      canvas.getByRole("switch", { name: "利用状況" }).getAttribute("aria-checked"),
    ).toBe("false");
  },
};
