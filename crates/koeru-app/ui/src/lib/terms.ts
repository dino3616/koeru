/*
 * 言い換えずに説明を添える用語（`docs/reports/ux/redesign-canvas/boards/P6-System.dc.html`
 * の「用語の説明（ⓘ）」）。
 *
 * 用語と文はここに1回だけ書く。 画面ごとに写すと、片方だけが直って
 * 同じ用語が場所によって別のことを言う。 出すのは `~/components/term-hint`。
 */

export type TermEntry = {
  /** 画面に出す名前。 言い換えない。 */
  readonly name: string;
  /** 何のことか。 */
  readonly summary: string;
  /** 出どころや単位など、補う一行。 無ければ出さない。 */
  readonly note?: string;
};

export const TERMS = {
  preutterance: {
    name: "先行発声",
    summary:
      "ノートの始まりより前に鳴らす長さ。子音を拍より先に出して、母音を拍に合わせるために使う。",
    note: "oto.ini の4番目の値 · 単位 ms",
  },
  oto_setting: {
    name: "原音設定",
    summary: "録った音のどこを使うかを決めること。oto.ini に書かれる",
  },
  alias: {
    name: "エイリアス",
    summary: "UST の歌詞から呼ばれる音の名前（例「a か」）",
  },
  cvvc: {
    name: "CVVC",
    summary: "「か」のような音と「a k」のようなつなぎ目を録る方式",
  },
  multi_tone: {
    name: "多音階",
    summary: "複数の高さで録り、歌う高さで使い分ける",
  },
  guide_bgm: {
    name: "ガイド BGM",
    summary: "テンポと音程の目安になる伴奏",
  },
} as const satisfies Record<string, TermEntry>;

export type TermId = keyof typeof TERMS;
