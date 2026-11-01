import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, within } from "storybook/test";

import { boardPitch, boardWave } from "./fixture";
import { RecordTimeline, timelineCells, WAVE_BARS } from ".";

const PAPA = ["ぱ", "ぱ", "ぴ", "ぷ", "ぺ", "ぽ", "ん", "ば"];
const GAGA = ["が", "が", "ぎ", "ぐ", "げ", "ご", "ん", "ぱ"];

const meta = {
  title: "部品/RecordTimeline",
  component: RecordTimeline,
  // 中央の段の幅（1032px）に置く。
  decorators: [
    (Story) => (
      <div className="w-258 bg-bg p-4">
        <Story />
      </div>
    ),
  ],
  args: {
    moras: PAPA,
    beatMs: 500,
    takeMs: null,
    wave: boardWave(0),
    waveLabel: "いま入っている音の形",
    pitch: undefined,
    tone: 57,
    playhead: null,
  },
} satisfies Meta<typeof RecordTimeline>;

export default meta;
type Story = StoryObj<typeof meta>;

/** 録音中（S04-Record）。 尺の 49% にいて、いまのマスは 90% まで進んでいる。 */
export const 録音中: Story = {
  args: {
    takeMs: 0.49 * 5000 - 500,
    wave: boardWave(147),
    waveLabel: "入力の波形。録音中",
    playhead: { ms: 0.49 * 5000 - 500, recording: true },
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const beats = c.getByRole("img", { name: "ガイドBGM の拍" });
    // 助走1・モーラ8・末尾1。
    await expect(beats.children).toHaveLength(10);
    await expect(beats).toHaveTextContent(PAPA.join(""));
    const wave = c.getByRole("img", { name: "入力の波形。録音中" });
    await expect(wave.children).toHaveLength(WAVE_BARS);
    // 録っているあいだは F0 を流さない。 欄は目盛りと音名だけ。
    const pitch = c.getByRole("img", { name: /^音高の記録/ });
    await expect(pitch.querySelector("polyline")).toBeNull();
    await expect(pitch).toHaveTextContent("A#3A3G#3");
    await expect(canvasElement.textContent).toContain("5.0 s");
  },
};

/** 確定後（S04-Unlocked）。 全部のマスが済みで、F0 の線と再生位置の点が出る。 */
export const 確定後: Story = {
  args: {
    moras: GAGA,
    takeMs: 4000,
    wave: boardWave(266),
    waveLabel: "入力の波形。テイク 1",
    pitch: boardPitch(8),
    playhead: { ms: 0.89 * 5000 - 500, recording: false },
  },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const pitch = c.getByRole("img", { name: "音高の記録。 中央は A3" });
    await expect(pitch.querySelectorAll("polyline")).toHaveLength(8);
  },
};

/** テイクが無い待機中。 いま入っている音を欄いっぱいに流す。 */
export const 待機: Story = {
  args: { wave: boardWave(300).map((v) => (v === null ? 0.02 : v)) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("img", { name: "いま入っている音の形" }).children).toHaveLength(
      WAVE_BARS,
    );
  },
};

/** 解析がまだ。 欄は目盛りだけで、名前が言う。 */
export const 解析待ち: Story = {
  args: { takeMs: 4000, wave: boardWave(266), pitch: null },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(c.getByRole("img", { name: "音高の記録。 まだ解析しています" })).toBeVisible();
  },
};

/** テイクが行より長い。 末尾のマスを足して尺を広げる。 */
export const 長いテイク: Story = {
  args: { takeMs: 6200, wave: boardWave(266), pitch: boardPitch(8) },
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    await expect(timelineCells(8, 500, 6200)).toBe(14);
    await expect(c.getByRole("img", { name: "ガイドBGM の拍" }).children).toHaveLength(14);
    await expect(canvasElement.textContent).toContain("7.0 s");
  },
};
