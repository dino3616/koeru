import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { Avatar, type AvatarSize } from ".";

/** 1×1 の赤い PNG。 絵の story に使う最小のもの（`package-image` の story と同じ）。 */
const RED_DOT =
  "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAIAAACQd1PeAAAADElEQVQI12P4z8AAAAMBAQAY3Y3bAAAAAElFTkSuQmCC";

const SIZES: readonly AvatarSize[] = [72, 40, 24];

/** 名前と、期待する頭文字。 */
const NAMES: readonly (readonly [string, string])[] = [
  // 英字は大文字にする。
  ["mina", "M"],
  ["éclair", "É"],
  // 結合文字（e + 結合アクセント）は1字として切る。
  ["écho", "É"],
  // 大文字にすると2字になるものはそのまま。
  ["ßeta", "ß"],
  // サロゲートペア。
  ["𠮷野", "𠮷"],
  // ZWJ で繋いだ絵文字は1字。
  ["👨‍👩‍👧 かぞく", "👨‍👩‍👧"],
  // 前の空白は数えない。
  ["  霜月", "霜"],
];

const meta = {
  title: "声/Avatar",
  component: Avatar,
  args: {
    name: "ナナ",
    size: 72,
    color: { hue: 318, chroma: 0.6, lightness: 0.45 },
  },
  // 3つの大きさを横に並べる。 story ごとに大きさを分けると、24 だけ測られない story ができる。
  render: (args) => (
    <div className="flex items-end gap-4">
      {SIZES.map((size) => (
        <Avatar key={size} {...args} size={size} />
      ))}
    </div>
  ),
} satisfies Meta<typeof Avatar>;

export default meta;
type Story = StoryObj<typeof meta>;

const faces = (canvasElement: HTMLElement) => [
  ...canvasElement.querySelectorAll("img, [role=img]"),
];

export const 画像: Story = {
  args: { image: RED_DOT },
  play: async ({ canvasElement }) => {
    const all = faces(canvasElement);
    await expect(all.map((el) => el.tagName)).toEqual(["IMG", "IMG", "IMG"]);
    for (const el of all) {
      // 名前は alt が言う。
      await expect(el.getAttribute("alt")).toBe("ナナ");
      // 正方形に切り抜く。 引き伸ばさない。
      await expect(getComputedStyle(el).objectFit).toBe("cover");
      // 絵には角丸をかけない（`DEC-PLT-062`）。
      await expect(getComputedStyle(el).borderTopLeftRadius).toBe("0px");
    }
  },
};

export const 頭文字: Story = {
  play: async ({ canvasElement }) => {
    const all = faces(canvasElement);
    await expect(all.map((el) => el.getBoundingClientRect().width)).toEqual([72, 40, 24]);
    for (const el of all) {
      // 名前は role="img" の名前が言い、字は最初の1文字。
      await expect(el.getAttribute("aria-label")).toBe("ナナ");
      await expect(el.textContent).toBe("ナ");
      // 角丸は一辺の 22%（割合のまま返る）。
      await expect(getComputedStyle(el).borderTopLeftRadius).toBe("22%");
    }
  },
};

export const 新: Story = {
  args: { color: null },
  play: async ({ canvasElement }) => {
    const all = faces(canvasElement);
    for (const el of all) {
      // 見えている字が名前に含まれる。
      await expect(el.textContent).toBe("新");
      await expect(el.getAttribute("aria-label")).toBe("ナナ（新）");
      await expect(getComputedStyle(el).borderTopStyle).toBe("dashed");
    }
  },
};

/** 声が録れていなくても、本人が選んだ色があれば頭文字を塗る。 */
export const イメージカラー: Story = {
  args: { color: null, imageColor: "violet" },
  play: async ({ canvasElement }) => {
    const all = faces(canvasElement);
    for (const el of all) {
      await expect(el.textContent).toBe("ナ");
    }
  },
};

/** 頭文字は書記素で1字。 UTF-16 やコードポイントで切ると壊れる名前を並べる。 */
export const 頭文字の取り方: Story = {
  render: (args) => (
    <div className="flex items-end gap-4">
      {NAMES.map(([name]) => (
        <Avatar key={name} {...args} name={name} size={40} />
      ))}
    </div>
  ),
  play: async ({ canvasElement }) => {
    const got = faces(canvasElement).map((el) => el.textContent);
    await expect(got).toEqual(NAMES.map(([, initial]) => initial));
  },
};
