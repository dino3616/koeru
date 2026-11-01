import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, userEvent, waitFor, within } from "storybook/test";

import { toneName } from "~/lib/tones";

import { BAR_WIDTH, pianoRollGeometry } from "./geometry";
import { PianoRoll, type PianoRollNote } from ".";

/** 4/4 の1小節（480 が4分音符）。 */
const BAR = 1920;

/** 音符を前から詰めて並べる。 休符は作らない。 */
const sequence = (
  parts: readonly { lyric: string; midi: number; ticks: number; missing?: boolean }[],
): PianoRollNote[] => {
  let start = 0;
  return parts.map((p) => {
    const note = { ...p, startTicks: start, missing: p.missing ?? false };
    start += p.ticks;
    return note;
  });
};

const SHORT = sequence([
  { lyric: "さ", midi: 69, ticks: 480 },
  { lyric: "く", midi: 69, ticks: 480 },
  { lyric: "ら", midi: 71, ticks: 960 },
  { lyric: "さ", midi: 69, ticks: 480 },
  { lyric: "く", midi: 69, ticks: 480 },
  { lyric: "ら", midi: 71, ticks: 960 },
  { lyric: "さ", midi: 71, ticks: 480 },
  { lyric: "き", midi: 72, ticks: 480 },
  { lyric: "わ", midi: 71, ticks: 480 },
  { lyric: "た", midi: 69, ticks: 480 },
  { lyric: "る", midi: 67, ticks: 960 },
  { lyric: "まで", midi: 64, ticks: 1920 },
]);

const KANA = "あいうえおかきくけこさしすせそたちつてと".split("");

/** 200 音符。 音階を行き来させ、長さは 4 種類を回す。 */
const LONG = sequence(
  Array.from({ length: 200 }, (_, i) => ({
    lyric: KANA[i % KANA.length] ?? "あ",
    // 三角波で 55〜76 を往復する。
    midi: 55 + Math.abs(((i * 3) % 42) - 21),
    ticks: [240, 480, 480, 960][i % 4] ?? 480,
  })),
);

const WITH_MISSING = sequence([
  { lyric: "ひ", midi: 67, ticks: 480 },
  { lyric: "か", midi: 69, ticks: 480, missing: true },
  { lyric: "り", midi: 71, ticks: 960 },
  { lyric: "の", midi: 69, ticks: 480 },
  { lyric: "あ", midi: 67, ticks: 480 },
  { lyric: "と", midi: 64, ticks: 960, missing: true },
  { lyric: "きゃ", midi: 66, ticks: 240, missing: true },
  { lyric: "ん", midi: 62, ticks: 1680 },
]);

const meta = {
  title: "部品/PianoRoll",
  component: PianoRoll,
  args: {
    label: "ひかりのあと",
    notes: SHORT,
    ticksPerBar: BAR,
    // サブバンクの音階（A3、G4）。
    micTones: [57, 67],
  },
  // 横幅は置く側の箱が決める。 story でも箱を与える。
  decorators: [(Story) => <div className="w-240">{Story()}</div>],
} satisfies Meta<typeof PianoRoll>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 短い曲。 窓より狭いので、枠は曲ぜんぶを囲う。 */
export const 短い曲: Story = {
  play: async ({ canvasElement }) => {
    // 図は隠して、同じ内容を表で読ませる。
    await expect(canvasElement.querySelectorAll("tbody tr")).toHaveLength(SHORT.length);
    await expect(canvasElement.querySelectorAll("svg[aria-hidden='true']").length).toBe(3);
    const c = within(canvasElement);
    await expect(c.getByRole("table")).toHaveAccessibleName(/録る音高は A3、G4/);
    // 鍵盤に C の音名と、マイクの印を付けた音階の名前が出る。
    const keys = canvasElement.querySelectorAll("svg")[1];
    await expect(keys?.textContent).toContain("C4");
    await expect(keys?.textContent).toContain("A3");
    await expect(keys?.textContent).toContain("G4");
  },
};

/** 長い曲。 横に送ると、ミニマップの枠と位置の値がついてくる。 */
export const 長い曲: Story = {
  args: { label: "夜明けのテスト", notes: LONG },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const slider = await c.findByRole("slider", { name: "見ている位置" });
    await expect(slider).toHaveAttribute("aria-valuenow", "1");

    // 矢印キーで1小節ずつ送る。
    slider.focus();
    await userEvent.keyboard("{ArrowRight}");
    await waitFor(() => expect(slider).toHaveAttribute("aria-valuenow", "2"));

    // 押したところへ送る。 真ん中を押せば、先頭から離れる。
    const box = slider.getBoundingClientRect();
    await userEvent.pointer({
      keys: "[MouseLeft]",
      target: slider,
      coords: { clientX: box.left + box.width / 2, clientY: box.top + box.height / 2 },
    });
    await waitFor(async () => {
      await expect(Number(slider.getAttribute("aria-valuenow"))).toBeGreaterThan(10);
    });

    await expect(canvasElement.querySelectorAll("tbody tr")).toHaveLength(200);
  },
};

/** 足りないエイリアスがある曲。 斜線と注意の色で言い、表にも「足りない」と出る。 */
export const 足りないエイリアスあり: Story = {
  args: { notes: WITH_MISSING },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByText("足りないエイリアス")).toBeVisible();
    // 色だけで伝えない。 表にも言葉で出す。
    await expect(c.getAllByRole("cell", { name: "足りない" })).toHaveLength(3);
    await expect(c.getAllByRole("cell", { name: "ある" })).toHaveLength(5);
  },
};

/** 図の上に重ねるものは、部品の座標で描ける（#456）。 */
export const 上に重ねる: Story = {
  args: {
    overlay: (g) => (
      <rect
        data-testid="overlay"
        x={g.tickToX(BAR)}
        y={g.midiToY(g.pitchRange.high)}
        width={g.tickToX(BAR)}
        height={g.height - g.midiToY(g.pitchRange.high)}
        className="fill-transparent stroke-slate-12"
      />
    ),
  },
  play: async ({ canvasElement }) => {
    const box = canvasElement.querySelector("[data-testid='overlay']");
    await expect(box?.getAttribute("x")).toBe(String(BAR_WIDTH));
  },
};

/** 座標の写像は往復できる。 重ねる側が逆引きに使う。 */
export const 座標の写像: Story = {
  play: async () => {
    const g = pianoRollGeometry({
      ticksPerBar: BAR,
      endTick: BAR * 3 + 1,
      pitchRange: { low: 55, high: 72 },
    });
    await expect(g.bars).toBe(4);
    await expect(g.xToTick(g.tickToX(1234))).toBeCloseTo(1234);
    await expect(g.yToMidi(g.midiToY(60) + 1)).toBe(60);
    await expect(toneName(60)).toBe("C4");
    await expect(toneName(57)).toBe("A3");
    await expect(toneName(61)).toBe("C#4");
    await expect(toneName(0)).toBe("C-1");
    await expect(toneName(127)).toBe("G9");
  },
};
