---
name: react-conventions
description: KOERU のフロントエンド（React + TanStack Start + Tailwind）の規約。tailwind-variants の使い方と tailwind-merge を使わない理由、className を props で受けないこと、部品の粒度と状態の置き場所、Rust との境界（生成した bindings）、アクセシビリティと配色の段を定める。tsx / ts ファイルの追加・編集、部品の切り出し、スタイルの当て方、IPC の呼び出し、a11y の対応、PR レビューのときに使う。
---

# KOERU — React の規約

コードの好みではなく、破ると後から回復しにくいものだけを書く。

コメントの書き方は [writing-comments](../writing-comments/SKILL.md)。検証は [verify-koeru](../verify-koeru/SKILL.md)。

## スタイル

### `className` を props で受けない

部品の見た目は部品が持つ。外から差し込めるようにすると、同じ部品が呼ばれた場所ごとに違う姿になり、部品が守っているはずの条件を呼び出し側が黙って壊せる（`Button` の高さは `TR-PLT-28` の対象サイズ）。

```tsx
type ButtonProps = Omit<ComponentProps<"button">, "className"> & VariantProps<typeof button>;
```

見た目を変える必要があるなら、variant を足す。余白は置く側が `flex` / `gap` で持つ——`<LiveWaveform className="mt-3" />` と書かない。

### tailwind-merge を使わない

`~/lib/tv` の `tv` は `createTV({ twMerge: false })` で作ってある。`clsx` も `tailwind-merge` も依存に無い。

畳む必要があるのは「外から `className` で上書きされる」部品だけで、上のとおり受け取らない。受け取らないなら衝突は起きず、畳む処理は毎回の描画で空回りする。

それ以上に、畳みに頼ると衝突が黙って解決される。どちらが勝つかは tailwind-merge の分類表が決めるので、Tailwind の版が上がって分類が変わると、何も書き換えていないのに見た目が変わる。

条件でクラスを足すだけなら `cx`。畳まない。

```tsx
className={cx("mt-3 text-5xl", allDone && "text-slate-11")}
```

### `tv` は要るところにだけ

`tv` を通すのは、値で切り替わるもの（`variants` と `compoundVariants`）だけ。

`base` と `slots` は使わない。静的なクラスは JSX にそのまま書く——`base` に移すと、見た目を読むのに2箇所を行き来することになる。`slots` は1つの `tv` が複数要素の見た目を持つ形で、部品を分ける代わりにならない。

```tsx
const button = tv({
  variants: {
    variant: { primary: "bg-cyan-11 text-slate-1 hover:bg-cyan-12", /* … */ },
    size: { md: "h-11 px-4 text-sm", /* … */ },
  },
  defaultVariants: { variant: "secondary", size: "md" },
});

<Comp className={`inline-flex items-center rounded-lg ${button({ variant, size })}`} />
```

### 配色は Radix Colors の段の意味を守る

1=地、2=面、3〜5=部品、6〜8=境界、9〜10=塗り、11=低コントラストの字、12=高コントラストの字。

塗りは段 9 ではなく段 11。段 9 は明暗で同じ値になる色があり、字を載せると 4.5:1 に届かない。hover は段 12。`brightness()` フィルタで作らない——塗りと字の両方が明るくなり、字は 255 で頭打ちになるので比が下がる。

検査は `bun run check:contrast`。sRGB と display-p3 の両方で計算し、低いほうで判定する。`src/` で使っている段が検査対象に入っているかも見る。

## 部品

### 画面は組み立てだけ

状態機械はフックへ出す。収録は `~/lib/use-recorder`、入力の面は `~/components/input-setup`。画面に 19 個の `useState` が並んだら、切り出す合図。

### 描画に出ないものを state にしない

二重確定を避ける札や、React の外で回るループの生死は `useRef` で持つ。state にすると押すたびに描き直す。

### `Card` の見出しの段は入れ子の深さが決める

段を props で渡さない。渡すと、部品を移したときに数え直しを忘れて `h2` の中に `h2` が入る。`Card` は context で深さを数える。

### effect で state を追いかけない

「準備が済んだら畳む」は、済んだ出来事の側（コールバック）で畳む。state を見張る effect にすると、本人が開き直したものを勝手に閉じる。

例外は外部の仕組みとの同期（ルート遷移、`Channel` の受信、マウント時の焦点移動）。そこは effect の本来の用途。

## Rust との境界

### 型を手で書かない

`~/lib/bindings.gen.ts` が正本で、Rust のコマンド定義から生成する（`DEC-PLT-019`）。手で直さない——次の生成で消える。

```bash
KOERU_WRITE_BINDINGS=1 cargo test -p koeru-app --test bindings   # 作り直す
cargo test -p koeru-app --test bindings                          # 古くないか見る
```

`~/lib/ipc` はその上の薄い層で、持っているのは3つだけ。生成物の結果型を投げる形へ剥がすこと、位置引数で取り違えやすいものをオブジェクト引数に直すこと、Rust の識別子を日本語へ直すこと。

### 流し続けるものは Channel

`invoke` で引きに行かせない（`DEC-PLT-017`）。`invoke` は応答の順序を保証しないので、引きに行くと波形が巻き戻る。

### 小数は `Finite` を通す

specta は `f32` / `f64` を `number | null` に写す。JSON に NaN も無限も無く、serde はどちらも `null` にするので、これは正しい。標本数と固定レートから作る値のように有限だと分かっているものは、Rust 側で `Finite` を通して `number` にする。

## アクセシビリティ

- `<main>` は画面に1つ。`Card` に `title` を渡すと名前つきの `<section>` になり、領域移動で行き来できる
- 状態の変化は常設の `aria-live` へ入れる。文言と一緒に挿し込むと、支援技術が変化として拾えず読まれない
- 画面が変わったら見出しへ焦点を移す（`useScreenFocus`）。読み上げは `Announcer` が別に持つ——焦点で読ませようとすると二重に読まれる
- `aria-label` を可視テキストの上に置かない。名前が上書きされて、見えているものと読まれるものが食い違う
- 色だけで伝えない。数値と語も並べる

lint は `jsx-a11y` を有効にしてある。`vite.config.ts` の `lint.plugins` に載っていないプラグインの規則は黙って効かない。一度そうなった。

## 検証

```bash
bun run check          # 整形 + lint + 型（--fix つき）
bun run check:ci       # 直さずに見る + 試験（CI と同じ）
bun run test           # 名前・役割・値とフォーカス順序（`TR-PLT-25`）
bun run build          # ビルド + tsc + コントラスト + npm のライセンス
```

### 部品には story を書く

story が検査範囲を決める（`DEC-PLT-022`）。 `src/__tests__/stories.test.tsx` が
`composeStories` で全 story を組み立てて axe を当てるので、story を書けば
その部品が自動で対象へ入る。書かなければ一度も検査されない。

variant は全部出す。 `Button` の `primary` だけ出して `danger` を出さないと、
`danger` の配色は測られない。押せない状態（`opacity-45` が掛かる）も出す。

```bash
bun run storybook   # 立てて目で見る
bun run test        # 実ブラウザで axe と play を走らせる（CI と同じ）
```

実ブラウザで走らせる。 `color-contrast` は計算済みの色が要るので、擬似 DOM では
「判定不能」になり違反として上がらない。以前あった `check-contrast.ts`
（Radix の値を自前で計算する検査）は廃止した（`DEC-PLT-022`）。

段の網羅は `palette.stories.tsx` が持つ。 明暗を入れ子で並べて1つの story で測る。
story を分けない——vitest 統合は既定の globals で1回ずつ走らせるので、
`theme` を切り替えた story を別に置いても片方しか回らない。
段を使いはじめたら、ここへ足す。

`region` は切ってある。 部品1つの story には `<main>` が無いのが当たり前。

設定を `vitest.config.ts` という名前で置かない。 その名前だと `vp test` が
それを読み、擬似 DOM 側の試験を1つも拾わないまま緑になる。踏んだ。

### axe が見ない不変条件は `play` に書く

試験ファイルを別に置かない。 部品の性質は、その部品の story に付ける。

axe が見てくれるもの（`button-name` / `heading-order` / `tabindex` など）は
書かない。二重になるだけ。書くのは axe に規則が無いものだけ。

```tsx
export const 名前つき: Story = {
  args: { title: "マイク", children: "中身" },
  play: async ({ canvasElement }) => {
    const section = canvasElement.querySelector("section");
    await expect(section?.getAttribute("aria-labelledby")).not.toBeNull();
  },
};
```

`await` を落とさない。 `storybook/test` の `expect` は Promise を返す。
await しないと、違反があっても落ちないことがある。lint が
`no-floating-promises` で拾うので、警告を消さずに直す。

いま `play` が持っているのは3つ。`Card` の名前つき／名前なしで landmark に
なるか、入れ子で段が1つずつ下がるか、`<meter>` が値と範囲を持ち語も並ぶか
（`TR-PLT-28`、`TR-PLT-29`）。

npm の依存ライセンスは `check:licenses` が見る。 Rust 側の `cargo deny check` に
相当するもので、許可リストに無いものは通さない（`DEC-ALL-002`）。
新しいライセンスが混ざったら落ちる。通すなら理由を書いて足す。
