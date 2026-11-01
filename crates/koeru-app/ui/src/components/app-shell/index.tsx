import type { ReactNode } from "react";

import { ShellProvider, useSidebarCollapsed } from "~/components/app-shell/shell-state";
import { Sidebar, type ShelfVoice } from "~/components/app-shell/sidebar";
import type { SubbankItem } from "~/components/app-shell/subbank";
import { Toolbar } from "~/components/app-shell/toolbar";
import { voiceName } from "~/components/app-shell/voice-name";
import { voiceStyle } from "~/lib/voice-color";

/*
 * 場の画面がシェルへ頼む口。 実装は `shell-state`、ここから出す。
 * 場の画面は、この2つとシェルの部品だけを知っていればよい。
 */
export { ShellToolbarEnd, useSidebarCollapsedWhile } from "~/components/app-shell/shell-state";
export type { ShelfVoice } from "~/components/app-shell/sidebar";
export type { SubbankItem } from "~/components/app-shell/subbank";

type AppShellProps = {
  /** 棚に並べる音源。 `null` は読んでいるあいだ。 */
  voices: readonly ShelfVoice[] | null;
  voicesFailed?: boolean | undefined;
  /** 選んでいる音源。 新しい音源や設定では無い。 */
  selectedId: string | undefined;
  /** 選んだ音源の枠。 `null` は読んでいるあいだ。 */
  subbanks: readonly SubbankItem[] | null;
  /** 枠を読めなかった。 */
  subbanksFailed?: boolean | undefined;
  /** 場の画面。 */
  children: ReactNode;
};

/**
 * どの画面にも出る外枠。 左のサイドバー、上のツールバー、その下の中身。
 *
 * **`<main>` で包まない。** `<main>` と見出しは場の画面が持つ。 ここが持つのは
 * `<nav>`（サイドバー、パンくず）と `<header>`（ツールバー）。
 *
 * DOM の順は サイドバー → ツールバー → 中身で、Tab もこの順に進む。 ツールバーは
 * 絶対位置で全幅に載せるので、見た目の上下と DOM の前後は一致しない。
 * サイドバーを飛ばしたいときは領域移動（`nav` の landmark、`TR-PLT-29`）を使う。
 *
 * 選んだ音源の声の色を、ここで差す（`DEC-PLT-062`）。 祖先に差すので、サイドバーの
 * 選択の線だけでなく、中身の主ボタンもその色に染まる。 外枠の地（`voice-frame`）も
 * その色をわずかに混ぜる。 中身の面は `bg` のまま染めない。
 * 選んでいなければ打ち消す——外から受け継いだ色で、別の音源を塗らない。
 *
 * 窓の最小幅 960 で、サイドバーが 248、中身が残りの 712。 中身は `min-w-0` で
 * 縮められるようにしてある（`DEC-PLT-029`）。 縮められない固定幅を挟むと、
 * 縮み分を受ける先が無くなって、中身のどこかが潰れる。 3 列を置く画面は、
 * 中身の幅を見て自分で段を切る。
 */
export const AppShell = (props: AppShellProps) => (
  <ShellProvider>
    <Shell {...props} />
  </ShellProvider>
);

const Shell = ({
  voices,
  voicesFailed,
  selectedId,
  subbanks,
  subbanksFailed,
  children,
}: AppShellProps) => {
  const collapsed = useSidebarCollapsed();
  const selected = voices?.find((v) => v.id === selectedId);

  return (
    <div
      className="relative flex h-full bg-voice-frame text-1"
      /*
       * 変数を差すだけで、見た目の値は持たない。 別ファイルの関数なので、
       * 規則からは中身が読めず動的な塊に見える（`Avatar` と同じ）。
       */
      // oxlint-disable-next-line shadcn/no-inline-styles
      style={voiceStyle(selected?.color ?? null)}
    >
      <Sidebar
        collapsed={collapsed}
        voices={voices}
        voicesFailed={voicesFailed}
        selectedId={selectedId}
        subbanks={subbanks}
        subbanksFailed={subbanksFailed}
      />
      <Toolbar
        voice={
          selected === undefined ? undefined : { id: selected.id, name: voiceName(selected.name) }
        }
        subbanks={subbanks}
      />
      <div className="flex min-w-0 flex-1 flex-col pr-2 pb-2">
        {/* ツールバーが載る分。 サイドバーの上の空きと同じ高さ。 */}
        <div aria-hidden="true" className="h-toolbar shrink-0" />
        {/* 線は引かない。 外枠から浮かせるのは影の明るい縁（`--shadow-panel`）。 */}
        <div className="min-h-0 flex-1 overflow-hidden rounded-xl bg-bg shadow-panel">
          {children}
        </div>
      </div>
    </div>
  );
};
