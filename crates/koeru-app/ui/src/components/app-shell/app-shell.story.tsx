import type { Meta, StoryObj } from "@storybook/react-vite";
import { type ComponentProps, type ReactNode, useState } from "react";
import { expect, userEvent, waitFor, within } from "storybook/test";

import { AppShell, ShellToolbarEnd, useSidebarCollapsedWhile } from ".";
import { 木, 棚_ひとつ, 棚_三つ, NAGI } from "./fixtures";
import { Glyph } from "./glyph";
import { withRouter } from "~/lib/story-router";

/*
 * 外枠を丸ごと出す。 中身の画面は、場の画面がするとおりに `<main>` と見出しを持つ。
 *
 * 窓の最小（960×640）と、既定より広い窓（1440×900）で出す。 960 で中身が潰れないこと
 * （`DEC-PLT-029`）は、中身の幅を測って確かめる。
 */

/** 場の画面の代わり。 `<main>` と見出しと、押せるものを1つ持つ。 */
const Screen = ({ children }: { children?: ReactNode }) => (
  <main className="flex h-full flex-col gap-4 p-8">
    <h1 className="text-xl font-semibold">音源のページ</h1>
    <button type="button" className="h-11 w-fit rounded-lg border border-line-strong px-5 text-sm">
      中身の操作
    </button>
    {children}
  </main>
);

const framed = (
  args: Omit<ComponentProps<typeof AppShell>, "children">,
  path: string,
  size = "h-160 w-240",
  content: ReactNode = <Screen />,
) => <div className={size}>{withRouter(<AppShell {...args}>{content}</AppShell>, path)}</div>;

const meta = {
  title: "シェル/AppShell",
  component: AppShell,
  parameters: { layout: "fullscreen" },
  args: {
    voices: 棚_三つ,
    selectedId: NAGI,
    subbanks: 木,
    children: null,
  },
  render: (args) => framed(args, `/voicebank?id=${args.selectedId ?? ""}`),
} satisfies Meta<typeof AppShell>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 音源が1つ。 */
export const 音源1つ: Story = {
  args: { voices: 棚_ひとつ },
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByRole("heading", { name: "音源のページ" });
  },
};

/** 音源が3つ。 1つを開いていて、その枠が棚の下に並ぶ。 */
export const 音源3つ_1つを開いた: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("list", { name: "Normal" });
    // 選んだ音源の声の色を、サイドバーの選択のアクセントに使う。
    const row = canvas.getByRole("link", { name: "ナギ" });
    await expect(getComputedStyle(row, "::before").backgroundColor).not.toBe("rgba(0, 0, 0, 0)");
  },
};

/** 声が1つも録れていない音源を選んだ。 選択は面と太さだけで伝わる。 */
export const 声の無い音源を選んだ: Story = {
  args: { selectedId: "33333333-3333-4333-8333-333333333333" },
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const row = await canvas.findByRole("link", { name: "新しい音源 2" });
    await expect(row.getAttribute("aria-current")).toBe("page");
    await expect(getComputedStyle(row, "::before").backgroundColor).toBe("rgba(0, 0, 0, 0)");
    await expect(getComputedStyle(row).backgroundColor).not.toBe("rgba(0, 0, 0, 0)");
  },
};

/** 畳んだ。 64 のアイコン列。 */
export const 畳んだ: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole("button", { name: "サイドバー" });
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");
    const nav = canvas.getByRole("navigation", { name: "音源と場" });
    await waitFor(() => expect(nav.getBoundingClientRect().width).toBe(64));
    await expect(canvas.getByRole("link", { name: "収録" })).toBeVisible();
  },
};

/** 新しい音源を選んだ。 */
export const 新しい音源を選んだ: Story = {
  args: { selectedId: undefined },
  render: (args) => framed(args, "/new"),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const fresh = await canvas.findByRole("link", { name: "新しい音源" });
    await expect(fresh.getAttribute("aria-current")).toBe("page");
    await expect(canvas.queryByRole("link", { name: "収録" })).toBeNull();
  },
};

/** 読んでいるあいだ。 外枠は先に出て、中身の画面は妨げられない。 */
export const 音源を読んでいる: Story = {
  args: { voices: null, subbanks: null, selectedId: undefined },
  play: async ({ canvasElement }) => {
    await within(canvasElement).findByRole("heading", { name: "音源のページ" });
  },
};

/** ランドマーク。 `<main>` はシェルが持たず、中身の画面が1つだけ持つ。 */
export const ランドマーク: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("heading", { name: "音源のページ" });
    await expect(canvas.getAllByRole("main")).toHaveLength(1);
    await expect(canvas.getAllByRole("banner")).toHaveLength(1);
    // 名前が違うので、領域移動で区別できる。
    await expect(canvas.getByRole("navigation", { name: "音源と場" })).toBeVisible();
    await expect(canvas.getByRole("navigation", { name: "いまの場所" })).toBeVisible();
    // `<main>` はシェルの中身の領域にあり、サイドバーやツールバーを包まない。
    const main = canvas.getByRole("main");
    await expect(main.contains(canvas.getByRole("banner"))).toBe(false);
    await expect(main.contains(canvas.getByRole("navigation", { name: "音源と場" }))).toBe(false);
  },
};

/**
 * Tab は サイドバー → ツールバー → 中身 の順に進む。
 *
 * 見た目は上のツールバーが先だが、DOM はサイドバーが先（受入条件）。
 * 進めた先がどの領域かを記録し、連続する重複を畳んで順を見る。
 */
export const Tab順: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}&subbank=sb-normal-a3`),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    await canvas.findByRole("heading", { name: "音源のページ" });
    await canvas.findByRole("combobox", { name: /サブバンク/ });

    const region = (el: Element | null) => {
      if (el?.closest("#app-sidebar")) return "サイドバー";
      if (el?.closest("header")) return "ツールバー";
      if (el?.closest("main")) return "中身";
      return "外";
    };

    const seen: string[] = [];
    for (let i = 0; i < 60; i += 1) {
      await userEvent.tab();
      const now = region(document.activeElement);
      if (seen.at(-1) !== now) seen.push(now);
      if (now === "中身") break;
    }
    await expect(seen).toEqual(["サイドバー", "ツールバー", "中身"]);
  },
};

/** 窓の最小（960）。 中身が潰れず、サイドバーの分を除いた幅が残る。 */
export const 窓の最小: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const main = await canvas.findByRole("main");
    const nav = canvas.getByRole("navigation", { name: "音源と場" });
    await expect(nav.getBoundingClientRect().width).toBe(248);
    // 窓 960 から、サイドバー 248 と右の余白 12 と縁 2 を引いた幅が、そのまま中身になる。
    await expect(main.getBoundingClientRect().width).toBeGreaterThan(640);
    await expect(document.documentElement.scrollWidth).toBeLessThanOrEqual(
      document.documentElement.clientWidth,
    );
  },
};

/** 広い窓（1440）。 中身が余った幅を受ける。 */
export const 広い窓: Story = {
  render: (args) => framed(args, `/voicebank?id=${NAGI}`, "h-225 w-360"),
  play: async ({ canvasElement }) => {
    const main = await within(canvasElement).findByRole("main");
    await expect(main.getBoundingClientRect().width).toBeGreaterThan(1000);
  },
};

/** 棚の別の音源を選ぶと、検索引数 `id` がその音源へ移る。 */
export const 音源を替える: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const other = await canvas.findByRole("link", { name: "ハルカゼ" });
    await userEvent.click(other);
    await waitFor(() =>
      expect(canvas.getByRole("link", { name: "ハルカゼ" }).getAttribute("aria-current")).toBe(
        "page",
      ),
    );
  },
};

/*
 * 場の画面が頼む2つの口。
 *
 * `Holding` は収録のような場の画面。 描かれているあいだ、サイドバーを畳ませて、
 * ツールバーの右端へ道具を置く。 出入りは外のボタンで確かめる。
 */
const Holding = () => {
  useSidebarCollapsedWhile(true);
  return (
    <ShellToolbarEnd>
      <button
        type="button"
        className="inline-flex h-9 items-center gap-2 rounded-lg border border-line-strong px-3 text-sm"
      >
        <Glyph name="mic" />
        録音の道具
      </button>
    </ShellToolbarEnd>
  );
};

const Field = () => {
  const [here, setHere] = useState(false);
  return (
    <main className="flex h-full flex-col gap-4 p-8">
      <h1 className="text-xl font-semibold">場</h1>
      <button
        type="button"
        onClick={() => setHere((h) => !h)}
        className="h-11 w-fit rounded-lg border border-line-strong px-5 text-sm"
      >
        {here ? "場を出る" : "場へ入る"}
      </button>
      {here ? <Holding /> : null}
    </main>
  );
};

/** 場に入るあいだ畳み、出たら戻る。 ツールバーの右端は、場にいるあいだだけ出る。 */
export const 場が畳ませる: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}`, "h-160 w-240", <Field />),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole("button", { name: "サイドバー" });
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await expect(canvas.queryByRole("button", { name: "録音の道具" })).toBeNull();

    await userEvent.click(canvas.getByRole("button", { name: "場へ入る" }));
    await waitFor(() => expect(toggle.getAttribute("aria-expanded")).toBe("false"));
    // ツールバーの中に出る。 中身の領域ではない。
    const tool = await canvas.findByRole("button", { name: "録音の道具" });
    await expect(tool.closest("header")).not.toBeNull();

    await userEvent.click(canvas.getByRole("button", { name: "場を出る" }));
    await waitFor(() => expect(toggle.getAttribute("aria-expanded")).toBe("true"));
    await expect(canvas.queryByRole("button", { name: "録音の道具" })).toBeNull();
  },
};

/** 本人が畳んでいたら、場を出たあとも畳んだまま。 本人の選択を上書きしない。 */
export const 本人の選択に戻る: Story = {
  render: (args) => framed(args, `/record?id=${NAGI}`, "h-160 w-240", <Field />),
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const toggle = await canvas.findByRole("button", { name: "サイドバー" });

    // 本人が畳む。
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");

    await userEvent.click(canvas.getByRole("button", { name: "場へ入る" }));
    await waitFor(() => expect(canvas.getByRole("button", { name: "録音の道具" })).toBeVisible());
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");

    // 場を出ても、本人が畳んだまま。
    await userEvent.click(canvas.getByRole("button", { name: "場を出る" }));
    await waitFor(() => expect(canvas.queryByRole("button", { name: "録音の道具" })).toBeNull());
    await expect(toggle.getAttribute("aria-expanded")).toBe("false");

    // 本人が開く。 場に入って畳まされ、その場で開き直しても、出れば本人の選択（開）のまま。
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await userEvent.click(canvas.getByRole("button", { name: "場へ入る" }));
    await waitFor(() => expect(toggle.getAttribute("aria-expanded")).toBe("false"));
    await userEvent.click(toggle);
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
    await userEvent.click(canvas.getByRole("button", { name: "場を出る" }));
    await waitFor(() => expect(canvas.queryByRole("button", { name: "録音の道具" })).toBeNull());
    await expect(toggle.getAttribute("aria-expanded")).toBe("true");
  },
};
