import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Card } from "~/components/ui/card";

/*
 * ひとまとまりの領域。
 *
 * 入れ子にしたときに見出しの段が1つずつ下がることを、ここで目にも機械にも見せる。
 * 段を飛ばすと、見出しだけを辿る移動が壊れる。
 */
const meta = {
  title: "部品/Card",
  component: Card,
} satisfies Meta<typeof Card>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 名前つき: Story = {
  args: { title: "マイク", children: "中身" },
  /*
   * 名前が付いて landmark になっていること。
   *
   * axe には「`<section>` に名前を付けろ」という規則が無い（`region` は
   * ページ全体を見る規則で、部品1つの story では常に違反する）。
   * `Card` の存在理由そのものなので、ここで固定する。
   */
  play: async ({ canvasElement }) => {
    const section = canvasElement.querySelector("section");
    await expect(section).not.toBeNull();
    const id = section?.getAttribute("aria-labelledby");
    await expect(id).not.toBeNull();
    await expect(canvasElement.querySelector(`#${id ?? ""}`)?.textContent).toBe("マイク");
  },
};

/** 名前を渡さないと landmark にならない。領域として扱わせたくないときだけ。 */
export const 名前なし: Story = {
  args: { children: "中身" },
  play: async ({ canvasElement }) => {
    await expect(
      canvasElement.querySelector("section")?.getAttribute("aria-labelledby"),
    ).toBeNull();
  },
};

/*
 * 段は入れ子の深さが決める。
 *
 * axe の `heading-order` が飛びを見るが、「1つずつ下がる」ことまでは見ない。
 * 段を props で渡す形へ戻したときに気づけるよう、ここで固定する。
 */
export const 入れ子: Story = {
  play: async ({ canvasElement }) => {
    await expect(canvasElement.querySelector("h2")?.textContent).toBe("マイク");
    await expect(canvasElement.querySelector("h3")?.textContent).toBe("入力レベル");
  },
  render: () => (
    <Card title="マイク">
      <p className="mt-3 text-sm text-slate-11">下から入力デバイスを選んでください。</p>
      {/* `className` は渡せない。余白は置く側が持つ（`DEC-PLT-020`）。 */}
      <div className="mt-4">
        <Card title="入力レベル">
          <p className="text-sm text-slate-11">ちょうどよい</p>
        </Card>
      </div>
    </Card>
  ),
};
