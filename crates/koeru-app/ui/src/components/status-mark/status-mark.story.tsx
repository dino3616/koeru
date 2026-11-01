import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect } from "storybook/test";

import { OriginMark, RowMark, SongMark } from ".";
import type { OriginStatus, RowStatus } from ".";
import { voiceStyle } from "~/lib/voice-color";

/*
 * 3系統の全状態を出す（`DEC-PLT-022`）。
 *
 * 1つだけ出すと、残りの配色は axe に一度も当たらない。 明暗の両方を入れ子で並べて
 * 1つの story で測る（`palette.story.tsx` と同じ形）。 声の色は祖先から届くので、
 * 明暗それぞれに `voiceStyle` を差す。
 */

const ROWS: readonly RowStatus[] = ["unrecorded", "recorded", "needs_retake", "excluded"];
const ORIGINS: readonly OriginStatus[] = ["auto_confirmed", "in_queue", "blocked", "pinned"];

/** 系統ごとの見出しと、並べる札。 札の一覧は `ul` の名前で引く。 */
const Systems = () => (
  <div className="flex flex-col gap-4">
    <ul aria-label="行の状態" className="flex flex-wrap gap-2">
      {ROWS.map((status) => (
        <li key={status}>
          <RowMark status={status} />
        </li>
      ))}
    </ul>
    <ul aria-label="曲の状態" className="flex flex-wrap gap-2">
      <li>
        <SongMark status="ready" />
      </li>
      <li>
        <SongMark status="fallback" />
      </li>
      <li>
        <SongMark status="remaining" rows={4} />
      </li>
      <li>
        <SongMark status="out_of_range" />
      </li>
    </ul>
    <ul aria-label="原音の状態" className="flex flex-wrap gap-2">
      {ORIGINS.map((status) => (
        <li key={status}>
          <OriginMark status={status} />
        </li>
      ))}
    </ul>
  </div>
);

const Themes = ({ grayscale }: { grayscale: boolean }) => (
  <div className={grayscale ? "flex gap-5 grayscale" : "flex gap-5"}>
    {(["light", "dark"] as const).map((theme) => (
      <div
        key={theme}
        className={`${theme} bg-slate-1 p-3`}
        /* 返すのは `--voice-*` だけ。別ファイルの関数なので読めない。 */
        // oxlint-disable-next-line shadcn/no-inline-styles
        style={voiceStyle({ hue: 170, chroma: 0.5, lightness: 0.5 })}
      >
        <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
        <Systems />
      </div>
    ))}
  </div>
);

const meta = {
  title: "部品/StatusMark",
  component: Systems,
} satisfies Meta<typeof Systems>;

export default meta;
type Story = StoryObj<typeof meta>;

/**
 * 全状態。
 *
 * 印は `aria-hidden` で、意味は字が持つ。 札の中身が字だけなら、読み上げで
 * 「丸」「三角」と二重にならない。 印は系統の中で1つずつ違い、字を消しても
 * 見分けがつく。
 */
export const 全状態: Story = {
  render: () => <Themes grayscale={false} />,
  play: async ({ canvasElement }) => {
    const lists = [...canvasElement.querySelectorAll("ul")];
    // 3系統 × 明暗。
    await expect(lists).toHaveLength(6);

    for (const list of lists) {
      const marks = [...list.querySelectorAll("li > span")];
      await expect(marks).toHaveLength(4);

      for (const mark of marks) {
        const glyphs = mark.querySelectorAll("svg");
        // 印は1つだけで、読み上げには出さない。
        await expect(glyphs).toHaveLength(1);
        await expect(glyphs[0]?.getAttribute("aria-hidden")).toBe("true");
        // 字が空でない。 印だけの札を作らない。
        await expect(mark.textContent?.trim().length).toBeGreaterThan(0);
      }

      // 印も字も、系統の中で重ならない。 片方だけで見分けられる。
      const shapes = marks.map((m) => m.querySelector("svg")?.innerHTML);
      const labels = marks.map((m) => m.textContent);
      await expect(new Set(shapes).size).toBe(marks.length);
      await expect(new Set(labels).size).toBe(marks.length);
    }
  },
};

/** 色を落として見る。 印と字と形（塗り・輪郭・破線）だけで、全状態が見分けられる。 */
export const 白黒: Story = {
  render: () => <Themes grayscale />,
};
