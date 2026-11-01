import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";

import { 木, NAGI } from "~/components/app-shell/fixtures";
import { ShellProvider } from "~/components/app-shell/shell-state";
import { SIDEBAR_ID } from "~/components/app-shell/sidebar";
import { withRouter } from "~/lib/story-router";

import { Toolbar } from ".";

/*
 * 上の外枠だけを出す。 窓の最小の幅（960）で、パンくずが右端を押し出さないかを見る。
 * 畳むボタンは store が要るので、シェルと同じ provider の中に置く。
 */
const framed = (args: ComponentProps<typeof Toolbar>, path: string, width = "w-240") => (
  <ShellProvider>
    {/* 畳むボタンが指す先。 サイドバーを持ち込まず、指す先の名前だけ置く。 */}
    <div id={SIDEBAR_ID} hidden />
    <div className={`relative h-toolbar bg-slate-1 ${width}`}>
      {withRouter(<Toolbar {...args} />, path)}
    </div>
  </ShellProvider>
);

const meta = {
  title: "シェル/Toolbar",
  component: Toolbar,
  parameters: { layout: "fullscreen" },
  args: { voice: { id: NAGI, name: "ナギ" }, subbanks: 木 },
  render: (args) => framed(args, `/voicebank?id=${NAGI}`),
} satisfies Meta<typeof Toolbar>;

export default meta;
type Story = StoryObj<typeof meta>;

const crumb = (canvasElement: HTMLElement) =>
  within(within(canvasElement).getByRole("navigation", { name: "いまの場所" }));

/** 音源のページ。 音源の名前がそのまま現在地で、場の名前を重ねない。 */
export const 音源のページ: Story = {
  play: async ({ canvasElement }) => {
    const here = await within(canvasElement).findByRole("navigation", { name: "いまの場所" });
    await expect(within(here).getByText("ナギ").getAttribute("aria-current")).toBe("page");
    await expect(within(here).queryByRole("combobox")).toBeNull();
    await expect(within(here).queryByText("音源のページ")).toBeNull();
  },
};

/** 収録。 音源 › 枠の選択 › 場。 枠の選択は見えている字を名前の一部に持つ。 */
export const 収録: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-normal-a3`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const trigger = await canvas.findByRole("combobox", { name: /サブバンク/ });
    await expect(trigger.textContent).toContain("Normal A3 CVVC");
    await expect(crumb(canvasElement).getByText("収録").getAttribute("aria-current")).toBe("page");
    // ツールバーの高さ（52px）に収まる。
    const header = canvas.getByRole("banner");
    await expect(trigger.getBoundingClientRect().height).toBeLessThan(
      header.getBoundingClientRect().height,
    );
  },
};

/** 枠を選ぶと、同じ場のまま枠だけ替わる。 元の枠の行は持ち越さない。 */
export const 枠を替える: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-normal-a3&row=r12`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const trigger = await canvas.findByRole("combobox", { name: /サブバンク/ });
    trigger.focus();
    await userEvent.keyboard("{Enter}");
    const option = await within(document.body).findByRole("option", { name: "Whisper A3 CVVC" });
    await userEvent.click(option);
    await waitFor(() => expect(trigger.textContent).toContain("Whisper A3 CVVC"));
    // 場は収録のまま。
    await expect(crumb(canvasElement).getByText("収録")).toBeVisible();
    // 開いたままだと、Radix が残りを `aria-hidden` にした状態で axe に当たる。
    await waitFor(() => expect(within(document.body).queryByRole("listbox")).toBeNull());
  },
};

/** 枠の選択が出るのは、枠で扱う場だけ。 */
export const 曲バンク: Story = {
  render: (args) => framed(args, `/songs?id=${NAGI}`),
  play: async ({ canvasElement }) => {
    const here = await within(canvasElement).findByRole("navigation", { name: "いまの場所" });
    await expect(within(here).queryByRole("combobox")).toBeNull();
    await expect(within(here).getByText("曲バンク").getAttribute("aria-current")).toBe("page");
    // 音源の名前は音源のページへ戻るリンクになる。
    await expect(within(here).getByRole("link", { name: "ナギ" }).getAttribute("href")).toContain(
      NAGI,
    );
  },
};

/** 枠のページ。 枠の選択が現在地で、場の名前は重ねない。 */
export const 枠のページ: Story = {
  render: (args) => framed(args, `/subbank?id=${NAGI}&subbank=sb-growl-a3`),
  play: async ({ canvasElement }) => {
    const trigger = await within(canvasElement).findByRole("combobox", { name: /サブバンク/ });
    await expect(trigger.textContent).toContain("Growl A3 単独音");
  },
};

/** 枠を選ぶ前の収録。 */
export const 枠を選ぶ前: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}`),
  play: async ({ canvasElement }) => {
    const trigger = await within(canvasElement).findByRole("combobox", { name: /サブバンク/ });
    await expect(trigger.textContent).toContain("サブバンクを選ぶ");
  },
};

/** 枠を読んでいるあいだは、選択を出さない。 */
export const 枠を読んでいる: Story = {
  args: { subbanks: null },
  render: (args) => framed(args, `/record?id=${NAGI}`),
};

/** 新しい音源。 音源が無いので、場の名前だけ。 */
export const 新しい音源: Story = {
  args: { voice: undefined, subbanks: null },
  render: (args) => framed(args, "/new"),
  play: async ({ canvasElement }) => {
    const here = await within(canvasElement).findByRole("navigation", { name: "いまの場所" });
    await expect(within(here).getByText("新しい音源").getAttribute("aria-current")).toBe("page");
  },
};

/** 音源の名前が長い。 パンくずが省略記号で縮み、右端と押し合わない。 */
export const 名前が長い: Story = {
  args: {
    voice: { id: NAGI, name: "とても長い名前の音源とても長い名前の音源とても長い名前の音源" },
  },
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-normal-a3`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("combobox", { name: /サブバンク/ });
    const header = canvas.getByRole("banner");
    // 幅の中に収まる。 はみ出すと横スクロールが生まれる。
    await expect(header.scrollWidth).toBeLessThanOrEqual(header.clientWidth);
  },
};

/** 戻る・進む。 履歴の先頭では戻れない。 */
export const 戻る_進む: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-normal-a3`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    /*
     * ボタンは移るたびに引き直す。 story のルータは経路ごとに別の部品を立てるので、
     * 経路が替わるとツールバーごと作り直され、前に掴んだ要素は DOM から外れている
     * （外れた要素を押しても何も起きない）。 本体ではツールバーはルータの外側にあり、
     * 作り直されない。
     */
    await expect(await canvas.findByRole("button", { name: "戻る" })).toBeDisabled();

    // パンくずの音源名から音源のページへ。 履歴が1つ積まれる。
    await userEvent.click(canvas.getByRole("link", { name: "ナギ" }));
    await waitFor(() => expect(canvas.queryByRole("combobox")).toBeNull());
    await waitFor(() => expect(canvas.getByRole("button", { name: "戻る" })).toBeEnabled());

    await userEvent.click(canvas.getByRole("button", { name: "戻る" }));
    await waitFor(() => expect(canvas.getByRole("combobox", { name: /サブバンク/ })).toBeVisible());

    await userEvent.click(canvas.getByRole("button", { name: "進む" }));
    await waitFor(() => expect(canvas.queryByRole("combobox")).toBeNull());
  },
};

/** 畳むボタン。 押すたびに `aria-expanded` が替わる。 名前は替えない。 */
export const 畳むボタン: Story = {
  play: async ({ canvasElement }) => {
    const toggle = await within(canvasElement).findByRole("button", { name: "サイドバー" });
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
  },
};
