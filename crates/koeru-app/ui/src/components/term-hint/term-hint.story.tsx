import type { Meta, StoryObj } from "@storybook/react-vite";
import { StrictMode } from "react";
import { expect, userEvent, within } from "storybook/test";

import { Term, TermScope } from ".";
import { TERMS, type TermEntry, type TermId } from "~/lib/terms";

/*
 * 初出・2回目・キーボードでフォーカスした状態を出す（`DEC-PLT-022`）。
 *
 * 説明は常に DOM にあるので、出ていないあいだの読み上げも同じ文を受ける。
 * ここでは「ⓘ の名前」「ⓘ の説明」「表の文と一致するか」を `play` で確かめる。
 * axe に規則が無いのは、この3つと、初出が移らないこと。
 */
const meta = {
  title: "部品/TermHint",
  component: Term,
  args: { term: "preutterance" },
  render: ({ term }) => (
    <TermScope>
      <p className="text-sm text-slate-12">
        <Term term={term} />
      </p>
    </TermScope>
  ),
} satisfies Meta<typeof Term>;

export default meta;
type Story = StoryObj<typeof meta>;

const hintOf = (canvasElement: HTMLElement, name: string) =>
  within(canvasElement).getByRole("button", { name });

/** 説明の全文。 面に出る字と、読み上げが受ける字は同じものを指す。 段ごとに空白で区切られる。 */
const textOf = (term: TermId) => {
  const entry: TermEntry = TERMS[term];
  return [entry.name, entry.summary, entry.note].filter((part) => part !== undefined).join(" ");
};

/**
 * 本物のポインタを ⓘ から外す。
 *
 * 前の story が動かしたポインタはその位置に残る（`button.story.tsx` の hover など）。
 * そこへ ⓘ が描かれると hover で説明が開き、「隠れている」が偽になる。 どのファイルが
 * 同じブラウザで先に走るかは実行ごとに変わるので、落ちたり落ちなかったりする。
 * 本物のポインタは試験の実ブラウザにしか無いので、Storybook を手で開いたときは何もしない。
 */
const unhover = async (target: Element) => {
  if (!("__vitest_browser__" in globalThis)) return;
  const { userEvent: pointer } = await import("vite-plus/test/browser");
  await pointer.unhover(target);
};

/** 初出。 塗りの ⓘ。 説明は隠れているが、読み上げには届く。 */
export const 初出: Story = {
  play: async ({ canvasElement }) => {
    const hint = hintOf(canvasElement, "先行発声の説明");
    await unhover(hint);
    await expect(hint.getAttribute("data-first")).toBe("true");
    await expect(hint).toHaveAccessibleDescription(textOf("preutterance"));
    const popup = canvasElement.querySelector("[role=tooltip]");
    await expect(popup?.hasAttribute("hidden")).toBe(true);
  },
};

/**
 * 2回目。 同じ用語は線だけの細い ⓘ になる。
 * 先に描いたほうが初出のまま動かないことも見る。
 */
export const 二回目: Story = {
  render: ({ term }) => (
    <TermScope>
      <p className="flex flex-col gap-2 text-sm text-slate-12">
        <Term term={term} />
        <Term term={term} />
      </p>
    </TermScope>
  ),
  play: async ({ canvasElement }) => {
    const hints = within(canvasElement).getAllByRole("button", { name: "先行発声の説明" });
    await expect(hints.map((hint) => hint.getAttribute("data-first"))).toEqual(["true", "false"]);
    // 薄くしても、同じ文を受ける。
    for (const hint of hints) {
      await expect(hint).toHaveAccessibleDescription(textOf("preutterance"));
    }
  },
};

/** 別の用語は別に数える。 2回目の数えが用語をまたいで移らない。 */
export const 用語ごとに数える: Story = {
  render: () => (
    <TermScope>
      <p className="flex flex-col gap-2 text-sm text-slate-12">
        <Term term="alias" />
        <Term term="cvvc" />
        <Term term="alias" />
      </p>
    </TermScope>
  ),
  play: async ({ canvasElement }) => {
    const first = (name: string) =>
      within(canvasElement)
        .getAllByRole("button", { name })
        .map((hint) => hint.getAttribute("data-first"));
    await expect(first("エイリアスの説明")).toEqual(["true", "false"]);
    await expect(first("CVVCの説明")).toEqual(["true"]);
  },
};

/** StrictMode は描画と副作用を二重に走らせる。 それでも初出は先に置いたほうのまま。 */
export const 二重描画でも初出は動かない: Story = {
  render: () => (
    <StrictMode>
      <TermScope>
        <p className="flex flex-col gap-2 text-sm text-slate-12">
          <Term term="multi_tone" />
          <Term term="multi_tone" />
          <Term term="multi_tone" />
        </p>
      </TermScope>
    </StrictMode>
  ),
  play: async ({ canvasElement }) => {
    const hints = within(canvasElement).getAllByRole("button", { name: "多音階の説明" });
    await expect(hints.map((hint) => hint.getAttribute("data-first"))).toEqual([
      "true",
      "false",
      "false",
    ]);
  },
};

/**
 * キーボードで届く。 Tab で ⓘ に着くと説明が出て、Esc で畳める。
 * 出ているあいだに axe が走るので、説明の面の配色もここで測られる。
 */
export const キーボードでフォーカス: Story = {
  play: async ({ canvasElement }) => {
    const hint = hintOf(canvasElement, "先行発声の説明");
    await unhover(hint);
    const popup = canvasElement.querySelector("[role=tooltip]");
    await userEvent.tab();
    await expect(hint).toHaveFocus();
    await expect(popup?.hasAttribute("hidden")).toBe(false);
    // 出ているときも、説明は ⓘ に結ばれたまま同じ文。
    await expect(hint).toHaveAccessibleDescription(textOf("preutterance"));
    await expect(popup?.getAttribute("id")).toBe(hint.getAttribute("aria-describedby"));
    await userEvent.keyboard("{Escape}");
    await expect(popup?.hasAttribute("hidden")).toBe(true);
    // Esc で畳んでも、フォーカスは ⓘ に残る（閉じ込めも、飛ばしもしない）。
    await expect(hint).toHaveFocus();
  },
};

/** 名前と説明が表の全用語で届く。 説明の字が欠けた用語は、ここで落ちる。 */
export const 全用語: Story = {
  render: () => (
    <TermScope>
      <ul className="flex flex-col gap-2 text-sm text-slate-12">
        {(Object.keys(TERMS) as TermId[]).map((term) => (
          <li key={term}>
            <Term term={term} />
          </li>
        ))}
      </ul>
    </TermScope>
  ),
  play: async ({ canvasElement }) => {
    for (const term of Object.keys(TERMS) as TermId[]) {
      const hint = hintOf(canvasElement, `${TERMS[term].name}の説明`);
      await expect(hint).toHaveAccessibleDescription(textOf(term));
    }
  },
};

/** 明暗の両方で ⓘ の配色を測る（`palette.story.tsx` と同じ形）。 */
export const 明暗: Story = {
  render: () => (
    <div className="flex flex-col gap-3">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <TermScope>
            <p className="flex flex-col gap-2 text-sm text-slate-12">
              <Term term="guide_bgm" />
              <Term term="guide_bgm" />
            </p>
          </TermScope>
        </div>
      ))}
    </div>
  ),
};
