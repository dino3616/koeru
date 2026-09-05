import type { Preview } from "@storybook/react-vite";

// 相対で書く。 `~` の別名は試験側のサーバへ届かない。
import "../src/styles/globals.css";

/*
 * すべての story に掛かる前提。
 *
 * 配色は明暗の両方で見る（`TR-PLT-25`）。 Radix は light を `:root, .light`、
 * dark を `.dark` に定義するので、`<html>` のクラスで切り替える。
 * 片方だけ見ると、もう片方で 4.5:1 を割っていることに気づけない。
 */
const preview: Preview = {
  parameters: {
    // 違反を見つけたら落とす。報告だけにすると、誰も見ない欄が増える。
    a11y: { test: "error" },
    layout: "centered",
  },
  globalTypes: {
    theme: {
      description: "配色",
      defaultValue: "light",
      toolbar: { icon: "mirror", items: ["light", "dark"], dynamicTitle: true },
    },
  },
  decorators: [
    (Story, context) => {
      const dark = context.globals["theme"] === "dark";
      document.documentElement.classList.toggle("dark", dark);
      document.documentElement.classList.toggle("light", !dark);
      // 面の上に置く。地の上だけで見ると、段 2 に載る字を一度も検査しない。
      document.body.className = "bg-slate-1 text-slate-12";
      return Story();
    },
  ],
};

export default preview;
