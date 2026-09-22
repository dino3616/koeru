import type { Meta, StoryObj } from "@storybook/react-vite";
import { expect, fn } from "storybook/test";

import { ReviewEntry } from ".";
import type { ReviewItemView } from "~/lib/ipc";

const item: ReviewItemView = {
  key: "か060",
  row_id: "s002",
  oto: {
    alias: "か",
    offset_ms: 412.5,
    consonant_ms: 96.25,
    cutoff_ms: -820,
    preutterance_ms: 62.5,
    overlap_ms: 31.25,
  },
  cause: "confidence.sharpness",
  confidence: 0.32,
  state: "in_queue",
  pinned: [],
};

const meta = {
  title: "部品/ReviewEntry",
  component: ReviewEntry,
  args: { item, adopted: true, individual: true, onConfirm: fn(), onRerecord: fn(), busy: false },
  decorators: [(Story) => <div className="w-96">{Story()}</div>],
} satisfies Meta<typeof ReviewEntry>;

export default meta;
type Story = StoryObj<typeof meta>;

export const 境界が曖昧: Story = {
  play: async ({ canvasElement }) => {
    // 成分の名前を出さない（`TR-ALN-26` (3)）。
    await expect(canvasElement.textContent).not.toContain("境界鋭さ");
    await expect(canvasElement.textContent).not.toContain("confidence");
    // 点数も出さない（`TR-SYN-20`）。
    await expect(canvasElement.textContent).not.toContain("0.32");
  },
};

export const ほかの回と違う: Story = {
  args: { item: { ...item, cause: "confidence.prior" } },
};

export const 音が割れている: Story = {
  args: { item: { ...item, cause: "confidence.acoustic" } },
};

export const 理由が分からない: Story = {
  args: { item: { ...item, cause: null } },
};

export const 直せない違反: Story = {
  args: { item: { ...item, state: "blocked", cause: null } },
  play: async ({ canvasElement }) => {
    // 直せないものを「これでよい」で通させない（`TR-ALN-20`）。
    await expect(canvasElement.textContent).not.toContain("これでよい");
    await expect(canvasElement.textContent).toContain("録り直す");
  },
};

export const まとめて確認へ切り替えたあと: Story = {
  args: { individual: false },
  play: async ({ canvasElement }) => {
    // 1件ずつ確定できるのは個別確認のときだけ（`REQ-ALN-008`）。
    await expect(canvasElement.textContent).not.toContain("これでよい");
  },
};

export const 確認が済んでいる: Story = {
  args: { item: { ...item, state: "auto_confirmed" } },
  play: async ({ canvasElement }) => {
    await expect(canvasElement.textContent).not.toContain("これでよい");
  },
};

export const 切り出しがまだ無い: Story = {
  args: { item: null },
};

export const 使っていない回を見ている: Story = {
  args: { adopted: false },
  play: async ({ canvasElement }) => {
    /*
      押す的を出さない。 Rust 側の鍵はエイリアスだけなので、
      押すと採用中の回が動く——見ている波形と食い違う。
    */
    await expect(canvasElement.textContent).not.toContain("これでよい");
    await expect(canvasElement.textContent).not.toContain("録り直す");
  },
};

export const 走っている間: Story = {
  args: { busy: true },
};
