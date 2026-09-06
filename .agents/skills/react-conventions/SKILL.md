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

検査は実ブラウザで走る axe（`bun run test`）。段の網羅は `src/styles/palette.story.tsx` が持ち、字と面の組み合わせ・境界とフォーカス・非テキストの比を、計算済みの色から測る。段を使いはじめたら、ここへ足す——載せていない組み合わせは一度も測られない。

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

### 読みは TanStack Query に載せる

`useEffect` と `useState` で書き下ろさない（`DEC-PLT-023`）。読みは `useSuspenseQuery`、押して初めて走るものは `useMutation`。待ちは `Suspense`、失敗は経路の受け口（`RouteError`）と `__root` の `ErrorBoundary` が受ける。

自前で書くと、部品ごとに「まだ無い」「取れた」「失敗した」を書き分けることになり、書き落としが出る——成功時にエラーを消し忘れて、一度失敗したあとは赤字が残ったままになっていた。

鍵と取得口は `~/lib/queries` に集める。散らすと、同じものを別の鍵で引いて二重に取りに行く。台帳から読むものは `ledgerKey` の下に置き、テイクが確定したら `invalidateQueries({ queryKey: ledgerKey })` でまとめて無効化する。版番号を鍵に混ぜない——変わるたびに別の鍵になってキャッシュが積み上がる。

関係の無いものを同じ部品で2つ読まない。同じ部品に `useSuspenseQuery` を並べると**直列**になる——1つ目が中断した時点で React は降りるので、2つ目のフックまで到達しない（`EVID-PLT-001` で実測）。並行に取るなら `useSuspenseQueries`。兄弟をそれぞれの `Suspense` で包む場合は並行に動く。

順に解かせたいものは、部品を分けて境界を挟む。並び順でも順序は保たれるが、並べ替えても型は通り、`app.no_project` が出て初めて分かる。境界で分ければ順序が木の形として残る。

失敗しても画面が成り立つものは `useQuery` のまま。波形に重ねる oto の目盛りは、取れなくても波形は読める。中断させると、これを待つあいだ波形が消える。

```tsx
const { data: rows } = useSuspenseQuery(rowsWithTakesQuery());   // 読み
const { data: otos = [] } = useQuery(otosQuery(takeId));         // 無くても成り立つ読み
const adopt = useMutation({ mutationFn: /* … */ });              // 押して走るもの
```

### 流し続けるものは Channel

`invoke` で引きに行かせない（`DEC-PLT-017`）。`invoke` は応答の順序を保証しないので、引きに行くと波形が巻き戻る。

待ち数のように繰り返し引くものも Query に載せない。返ってきてから次を予約する形を自分で書く——`refetchInterval` も `setInterval` も、1回が間隔より長くかかったときの振る舞いを自分で決められない。

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
bun run build          # ビルド + tsc + npm のライセンス
```

### すべての部品に story を書く

例外なし。 部品は1ディレクトリ1つで、`components/<name>/index.tsx` と
`components/<name>/<name>.story.tsx` が並ぶ。`check:stories` がその対応を見て、
無ければ落ちる。描かないものは `EXEMPT` へ理由つきで足す
（いまはルータの殻と経路の宣言だけ）。

story が検査範囲を決める（`DEC-PLT-022`）。 書き忘れた部品は axe に
一度も当たらないまま通るので、「検査が緑」と「検査した」が食い違う。

variant は全部出す。 `Button` の `primary` だけ出して `danger` を出さないと、
`danger` の配色は測られない。押せない状態（`opacity-45` が掛かる）も出す。

```bash
bun run storybook       # 立てて目で見る
bun run check:stories   # story の無い部品を探す
bun run test            # 実ブラウザで axe と play を走らせる
bun run check:ci        # 上の3つぶんをまとめて（CI と同じ）
```

### Rust の呼び出しはモックで差し替える

Storybook に Tauri は無い。 `~/lib/ipc` は `.storybook/main.ts` の別名で
`~/lib/ipc.mock.ts` へ向いていて、story が `mocked(api).progress` で
返り値を決める。

```tsx
beforeEach: () => {
  mocked(api.songStatus).mockResolvedValue([...]);
},
```

既定は「呼ばれたら待ち続ける」。 明示しないものは解決しないので、
読み込み中の見た目もそのまま story になる。

`Channel` も差し替えてある。 本物は `transformCallback` を呼ぶので、
Tauri の無いところで `new Channel()` すると即落ちる。

`sb.mock` は使わない。 対象のモジュールを変換して包むので、
`ipc.ts` の `export type … から` が値の再輸出として解決されて落ちる。

### ルータが要る部品は `withRouter` で包む

`useNavigate` / `useSearch` / `useRouterState` を使う部品は、ルータの外では
落ちる。`~/lib/story-router` が記憶上の履歴で最小のルータを組む。
本物の `routeTree` は使わない——`__root` から `theme.js` まで引き連れてくる。

実ブラウザで走らせる。 `color-contrast` は計算済みの色が要るので、擬似 DOM では
「判定不能」になり違反として上がらない。以前あった `check-contrast.ts`
（Radix の値を自前で計算する検査）は廃止した（`DEC-PLT-022`）。

段の網羅は `src/styles/palette.story.tsx` が持つ。 明暗を入れ子で並べて1つの story で測る。
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

書いてあるのは、たとえば `Card` の名前つき／名前なしで landmark になるか、
入れ子で段が1つずつ下がるか、`<meter>` が値と範囲を持ち語も並ぶか
（`TR-PLT-28`、`TR-PLT-29`）。

npm の依存ライセンスは `check:licenses` が見る。 Rust 側の `cargo deny check` に
相当するもので、許可リストに無いものは通さない（`DEC-ALL-002`）。
新しいライセンスが混ざったら落ちる。通すなら理由を書いて足す。
