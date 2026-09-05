import type { Meta, StoryObj } from "@storybook/react-vite";

/*
 * 配色の段を、実際に描いて axe に測らせる（`TR-PLT-25`）。
 *
 * 以前は Radix パッケージの値を読む自前のスクリプト（`check-contrast.ts`）で
 * 計算していた。 廃止して、実ブラウザに描いた色を axe に測らせる形へ移した
 * （`DEC-PLT-022`）——計算した値と、実際に画面へ出る色が食い違う余地が無くなる。
 *
 * ここが検査範囲の正本になる。 部品の story に出てこない組み合わせは、
 * ここに置かないと一度も測られない。段を使いはじめたら、ここへ足す。
 *
 * 段 9 / 10 は塗りに使わない。 明るい面で 4.5:1 に届かないので、
 * 塗りは段 11、その hover は段 12（`DEC-PLT-015`）。ここにも出さない
 * ——出すと、使わないと決めた段の違反を毎回報告することになる。
 */
const meta = { title: "配色/段" } satisfies Meta;
export default meta;
type Story = StoryObj<typeof meta>;

/** 字と面の組み合わせ。`src/` で使っている段を全部並べる。 */
const TEXT_ON_SURFACE = [
  { fg: "text-slate-12", bg: "bg-slate-1", label: "本文 / 地" },
  { fg: "text-slate-12", bg: "bg-slate-2", label: "本文 / 面" },
  { fg: "text-slate-12", bg: "bg-slate-3", label: "本文 / 部品" },
  { fg: "text-slate-12", bg: "bg-slate-4", label: "本文 / 部品 hover" },
  { fg: "text-slate-11", bg: "bg-slate-1", label: "薄い字 / 地" },
  { fg: "text-slate-11", bg: "bg-slate-2", label: "薄い字 / 面" },
  { fg: "text-slate-11", bg: "bg-slate-3", label: "薄い字 / 部品" },
  { fg: "text-slate-11", bg: "bg-slate-4", label: "薄い字 / 部品 hover" },
  { fg: "text-slate-1", bg: "bg-cyan-11", label: "主ボタンの字 / 塗り" },
  { fg: "text-slate-1", bg: "bg-cyan-12", label: "主ボタンの字 / 塗り hover" },
  { fg: "text-slate-1", bg: "bg-red-11", label: "危険ボタンの字 / 塗り" },
  { fg: "text-slate-1", bg: "bg-red-12", label: "危険ボタンの字 / 塗り hover" },
  { fg: "text-red-11", bg: "bg-red-3", label: "警告文 / 警告の面" },
  { fg: "text-red-11", bg: "bg-slate-2", label: "警告文 / 面" },
  { fg: "text-jade-12", bg: "bg-slate-3", label: "成功の字 / 部品" },
] as const;

/*
 * 明暗の両方を1つの story で描く。
 *
 * story ごとに分けない。 Storybook の vitest 統合は既定の globals で
 * 1回ずつ走らせるので、`theme` を切り替えた story を別に用意しても
 * 片方しか回らない。 Radix は段を `.light` / `.dark` のクラスに定義するので、
 * 入れ子にすれば同じ画面で両方を測らせられる。
 *
 * 片方だけ見ると、もう片方で 4.5:1 を割っていることに気づけない。
 * 段 9 が明暗で同じ値になる色があるのが、まさにその例。
 */
/*
 * 明暗の両方を1つの story に入れる。
 *
 * story を分けない。 Storybook の vitest 統合は既定の globals で1回ずつ
 * 走らせるので、`theme` を切り替えた story を別に置いても片方しか回らない。
 * Radix は段を `.light` / `.dark` のクラスに定義するので、入れ子にすれば
 * 同じ画面で両方を測らせられる。
 *
 * 片方だけ見ると、もう片方で 4.5:1 を割っていることに気づけない。
 * 段 9 が明暗で同じ値になる色があるのが、まさにその例。
 */
export const 字と面: Story = {
  render: () => (
    <div className="flex gap-4">
      {(["light", "dark"] as const).map((theme) => (
        <div key={theme} className={`${theme} bg-slate-1 p-3`}>
          <p className="pb-2 text-sm text-slate-12">{theme === "light" ? "明るい面" : "暗い面"}</p>
          <div className="flex flex-col gap-1">
            {TEXT_ON_SURFACE.map(({ fg, bg, label }) => (
              <p key={label} className={`${bg} ${fg} px-3 py-2 text-sm`}>
                {label}
              </p>
            ))}
          </div>
        </div>
      ))}
    </div>
  ),
};

/*
 * 境界線とフォーカス環。
 *
 * 3:1 で足りる（`TR-PLT-28` の非テキスト）。 axe の `color-contrast` は
 * 文字にしか当たらないので、線の比は目視と `DEC-PLT-015` の記録で担保する。
 * ここに出すのは、段を選び直したときに見た目で気づけるようにするため。
 */
const Borders = () => (
  <div className="flex flex-col gap-3 bg-slate-2 p-4">
    <div className="border border-slate-6 p-3 text-sm text-slate-12">薄い境界 / 面</div>
    <div className="border border-slate-11 p-3 text-sm text-slate-12">強い境界 / 面</div>
    <button
      type="button"
      className="border border-cyan-11 p-3 text-sm text-slate-12 outline-2 outline-cyan-11"
    >
      フォーカス環 / 面
    </button>
  </div>
);

export const 境界とフォーカス: Story = { render: () => <Borders /> };
