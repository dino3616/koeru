import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn, mocked, userEvent, within } from "storybook/test";

import { NewVoice } from ".";
import { api } from "~/lib/ipc";

const 単独音 = {
  id: "single",
  label: "単独音",
  summary: "1音ずつ、間をあけて読む",
  rows: 21,
  units: 102,
  seconds: 265,
  passes: 1,
  reach: "ゆっくりした曲が歌えます。",
  reading: "1音ずつ読むので、読み間違えにくい。",
};

const 連続音 = {
  id: "sequential",
  label: "連続音",
  summary: "続けて読む。音のつながりが滑らかになる",
  rows: 312,
  units: 1176,
  seconds: 4140,
  passes: 1,
  reach: "言葉のつながりが自然になります。",
  reading: "続けて読みます。行は短めです。",
};

const 多音階連続音 = {
  id: "multi-pitch-sequential",
  label: "多音階連続音",
  summary: "続けて読むのを、高さを変えて何周かする",
  rows: 936,
  units: 3528,
  seconds: 12420,
  passes: 3,
  reach: "高い曲も低い曲も、無理なく歌えます。",
  reading: "続けて読みます。行は短めです。",
};

const meta = {
  title: "領域/NewVoice",
  component: NewVoice,
  args: { onCreate: fn(), onClose: fn(), creating: false },
  beforeEach: () => {
    mocked(api.methodPresets).mockResolvedValue([単独音, 連続音, 多音階連続音]);
  },
} satisfies Meta<typeof NewVoice>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 名前を打つ前: Story = {
  play: async ({ canvasElement }) => {
    // 名前が空のうちは作れない。押せる的として出しておくと、押して失敗する。
    const create = [...canvasElement.querySelectorAll("button")].find(
      (b) => b.textContent === "作る",
    );
    await expect(create?.hasAttribute("disabled")).toBe(true);
  },
};

/**
 * 先頭が選ばれている（`TR-RCL-11`）。
 *
 * Rust 側が所要時間の短い順に並べるので、いちばん軽いものが初期値になる。
 * どれも選ばれていない状態を作らない——作るボタンが押せないまま止まる。
 */
export const 作り方を選ぶ: Story = {
  play: async ({ canvasElement }) => {
    const c = within(canvasElement);
    const radios = await c.findAllByRole("radio");
    await expect(radios).toHaveLength(3);
    await expect((radios[0] as HTMLInputElement).checked).toBe(true);

    // 選び直せる。 矢印キーでも行き来できることは役割が担保する。
    await userEvent.click(radios[2] as HTMLInputElement);
    await expect((radios[2] as HTMLInputElement).checked).toBe(true);
    await expect((radios[0] as HTMLInputElement).checked).toBe(false);
  },
};

/** 名前と作り方の両方が揃って初めて作れる。 */
export const 選んで作る: Story = {
  play: async ({ canvasElement, args }) => {
    const c = within(canvasElement);
    await userEvent.type(await c.findByLabelText("名前"), "ミナ");
    await userEvent.click((await c.findAllByRole("radio"))[1] as HTMLInputElement);
    await userEvent.click(await c.findByRole("button", { name: "作る" }));
    await expect(args.onCreate).toHaveBeenCalledWith("ミナ", "sequential");
  },
};

export const 作っている: Story = { args: { creating: true } };
