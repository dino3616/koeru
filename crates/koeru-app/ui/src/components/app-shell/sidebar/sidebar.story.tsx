import type { Meta, StoryObj } from "@storybook/react-vite";
import type { ComponentProps } from "react";
import { expect, userEvent, within } from "storybook/test";

import { 木, 棚_ひとつ, 棚_三つ, NAGI, HARUKAZE } from "~/components/app-shell/fixtures";
import { withRouter } from "~/lib/story-router";

import { Sidebar } from ".";

/*
 * 左の外枠だけを出す。 窓の最小の高さ（640）で、棚と木と場の5つが収まるかを見る。
 * 畳んだ形も出す——絵だけの列でも、押せるものに名前が要る（`TR-PLT-28`）。
 */
const framed = (args: ComponentProps<typeof Sidebar>, path: string) => (
  // 窓の最小の高さ。 収まらなければ中でスクロールする。
  <div className="relative h-160 w-80 bg-slate-1">{withRouter(<Sidebar {...args} />, path)}</div>
);

const meta = {
  title: "シェル/Sidebar",
  component: Sidebar,
  parameters: { layout: "fullscreen" },
  args: {
    collapsed: false,
    voices: 棚_三つ,
    selectedId: NAGI,
    subbanks: 木,
  },
  render: (args) =>
    framed(args, args.selectedId === undefined ? "/new" : `/voicebank?id=${args.selectedId}`),
} satisfies Meta<typeof Sidebar>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 音源が1つ。 開いている。 */
export const 音源1つ: Story = {
  args: { voices: 棚_ひとつ },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await expect(await canvas.findByRole("link", { name: "ナギ" })).toBeVisible();
  },
};

/** 音源が3つ。 1つを開いていて、その枠が声色ごとに並ぶ。 */
export const 音源3つ_1つを開いた: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("link", { name: "ナギ" });
    // 木を開けるのは選んだ音源だけ。 ほかの音源に開閉の的は付かない。
    await expect(canvas.getAllByRole("button", { name: "サブバンク" })).toHaveLength(1);
    // 声色ごとにまとまり、まとまりの名前が読み上げに届く。
    for (const name of ["Normal", "Whisper", "Growl"]) {
      await expect(canvas.getByRole("list", { name })).toBeVisible();
    }
    await expect(
      within(canvas.getByRole("list", { name: "Whisper" })).getAllByRole("link"),
    ).toHaveLength(3);
  },
};

/** 木を閉じる。 閉じたあとも、開く的は残る。 */
export const 木を閉じた: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole("button", { name: "サブバンク" });
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");
    await expect(canvas.queryByRole("list", { name: "Normal" })).toBeNull();
    await userEvent.click(toggle);
    await expect(canvas.getByRole("list", { name: "Normal" })).toBeVisible();
  },
};

/** 畳んだ。 顔と場の絵だけで、名前は読み上げに残る。 */
export const 畳んだ: Story = {
  args: { collapsed: true },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("link", { name: "ナギ" });
    // 絵だけの列でも、押せるものの名前がある。
    for (const name of ["音源のページ", "収録", "曲バンク", "原音設定", "配布", "新しい音源"]) {
      await expect(canvas.getByRole("link", { name })).toBeVisible();
    }
    // 木は畳むと出さない。
    await expect(canvas.queryByRole("button", { name: "サブバンク" })).toBeNull();
    // 幅は 64。
    const nav = canvas.getByRole("navigation", { name: "音源と場" });
    await expect(nav.getBoundingClientRect().width).toBe(64);
  },
};

/** 新しい音源を選んだ。 音源が無いので、場の5つは出ない。 */
export const 新しい音源を選んだ: Story = {
  args: { selectedId: undefined },
  render: (args) => framed(args, "/new"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const fresh = await canvas.findByRole("link", { name: "新しい音源" });
    await expect(fresh.getAttribute("aria-current")).toBe("page");
    await expect(canvas.queryByRole("link", { name: "収録" })).toBeNull();
    // 棚のどれにも、選んでいる印は付かない。
    await expect(canvas.getByRole("link", { name: "ナギ" }).hasAttribute("aria-current")).toBe(
      false,
    );
  },
};

/** 選んでいる場と音源に `aria-current`。 場のページでは、その場が `page`。 */
export const 選んでいる場: Story = {
  render: (args) => framed(args, `/voicebank?id=${NAGI}`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const home = await canvas.findByRole("link", { name: "音源のページ" });
    await expect(home.getAttribute("aria-current")).toBe("page");
    await expect(canvas.getByRole("link", { name: "ナギ" }).getAttribute("aria-current")).toBe(
      "page",
    );
    await expect(canvas.getByRole("link", { name: "収録" }).hasAttribute("aria-current")).toBe(
      false,
    );
    // 音源を持ち回す。 落とすと、移った先で音源が分からなくなる。
    await expect(canvas.getByRole("link", { name: "収録" }).getAttribute("href")).toContain(NAGI);
    // 別の音源の行は、その音源の page へ移す。
    const other = canvas.getByRole("link", { name: "ハルカゼ" });
    await expect(other.getAttribute("href")).toContain(HARUKAZE);
    await expect(other.hasAttribute("aria-current")).toBe(false);
    // これから作る音源の画面で、前の音源を開かない。
    await expect(
      canvas.getByRole("link", { name: "新しい音源" }).getAttribute("href"),
    ).not.toContain(NAGI);
  },
};

/** 収録にいる。 枠を扱っていれば、木のその枠にも印が付く（`page` ではなく `true`）。 */
export const 収録で枠を扱っている: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-whisper-g4`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const record = await canvas.findByRole("link", { name: "収録" });
    await expect(record.getAttribute("aria-current")).toBe("page");
    // 音源は選んでいるが、その page にはいない。
    await expect(canvas.getByRole("link", { name: "ナギ" }).getAttribute("aria-current")).toBe(
      "true",
    );
    const whisper = within(canvas.getByRole("list", { name: "Whisper" })).getAllByRole("link");
    await expect(whisper.map((a) => a.getAttribute("aria-current"))).toEqual([null, "true", null]);
  },
};

/** 枠のページにいる。 その枠が `page`。 */
export const 枠のページ: Story = {
  render: (args) => framed(args, `/subbank?id=${NAGI}&subbank=sb-growl-c5`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const growl = within(await canvas.findByRole("list", { name: "Growl" })).getAllByRole("link");
    await expect(growl.map((a) => a.getAttribute("aria-current"))).toEqual([null, "page"]);
  },
};

/** 検索・コマンドは置くだけ。 押せないことが形で分かる。 */
export const 検索は押せない: Story = {
  play: async ({ canvasElement }) => {
    const search = await within(canvasElement).findByRole("button", { name: "検索・コマンド" });
    await expect(search).toBeDisabled();
  },
};

/** 音源の一覧を読んでいる。 */
export const 音源を読んでいる: Story = {
  args: { voices: null, selectedId: undefined, subbanks: null },
};

/** 音源は選んだが、枠をまだ読んでいる。 */
export const 枠を読んでいる: Story = { args: { subbanks: null } };

/** 枠がまだ無い。 */
export const 枠が無い: Story = { args: { subbanks: [] } };

export const 枠を読めなかった: Story = { args: { subbanks: null, subbanksFailed: true } };

export const 音源の一覧を読めなかった: Story = {
  args: { voices: null, voicesFailed: true, selectedId: undefined, subbanks: null },
};
