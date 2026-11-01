import type { ReactNode } from "react";

import { tv } from "~/lib/tv";

/**
 * 行の状態。 `RowTakesView.state` の語彙と同じ（`TR-RCL-18`）。
 *
 * bindings は `string` で渡すので、画面が絞ってから渡す。 契約が SDL の列挙へ
 * 移れば、そのまま型で届く（`DEC-PLT-035`）。
 */
export type RowStatus = "unrecorded" | "recorded" | "needs_retake" | "excluded";

/**
 * 曲の歌えるか。
 *
 * `SongView.singability` は `Complete` / `WithFallback` / `Unavailable` の3つで、
 * 「あと N 行」と「音域外」はどちらも `Unavailable` に畳まれて届く。 見分ける
 * 材料は画面が持つ（部品は推測しない）。
 */
export type SongStatus = "ready" | "fallback" | "remaining" | "out_of_range";

/**
 * 原音設定のエイリアス1つの状態。 `ReviewItemView.state` の語彙に `pinned` を足したもの。
 *
 * `pinned` は bindings では状態と別の欄（`pinned: string[]`）で、固定していても
 * `state` は残る。 どちらを見せるかは画面が決めて渡す。
 * `not_estimated`（まだ推定していない）は持たない——しるしを出さない状態。
 */
export type OriginStatus = "auto_confirmed" | "in_queue" | "blocked" | "pinned";

/**
 * 印の絵。 12px の枠に、線の太さ 1.5 で描く。
 *
 * 記号は `aria-hidden`。 意味は隣の字が持つので、読み上げに印の名前を重ねない。
 * 色を持たず、字と同じ色（`currentColor`）で描く。
 */
type GlyphName =
  | "ring"
  | "dot"
  | "triangle"
  | "minus"
  | "check"
  | "swap"
  | "half"
  | "slash"
  | "alert"
  | "pause"
  | "pin";

const GLYPHS: Record<GlyphName, ReactNode> = {
  ring: <circle cx="6" cy="6" r="4.5" />,
  dot: <circle cx="6" cy="6" r="5" fill="currentColor" stroke="none" />,
  triangle: <path d="M6 1 L11 10 H1 Z" strokeLinejoin="round" />,
  minus: <path d="M2 6 H10" />,
  check: <path d="M2 6.5 L5 9.5 L10 2.5" strokeLinecap="round" strokeLinejoin="round" />,
  swap: (
    <>
      <path d="M2 4.5 Q4 2.5 6 4.5 T10 4.5" strokeLinecap="round" />
      <path d="M2 8.5 Q4 6.5 6 8.5 T10 8.5" strokeLinecap="round" />
    </>
  ),
  half: (
    <>
      <circle cx="6" cy="6" r="4.5" />
      <path d="M6 1.5 A4.5 4.5 0 0 0 6 10.5 Z" fill="currentColor" stroke="none" />
    </>
  ),
  slash: (
    <>
      <circle cx="6" cy="6" r="4.5" />
      <path d="M2.8 9.2 L9.2 2.8" />
    </>
  ),
  alert: (
    <>
      <path d="M6 1.5 V7" strokeLinecap="round" />
      <circle cx="6" cy="10" r="0.9" fill="currentColor" stroke="none" />
    </>
  ),
  pause: <path d="M4 2 V10 M8 2 V10" strokeLinecap="round" />,
  pin: <path d="M4 1 H8 V5 L10 7 H2 L4 5 Z M6 7 V11" strokeWidth="1.3" strokeLinejoin="round" />,
};

const Glyph = ({ name }: { name: GlyphName }) => (
  <svg
    viewBox="0 0 12 12"
    aria-hidden="true"
    focusable="false"
    className="size-3 shrink-0"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
  >
    {GLYPHS[name]}
  </svg>
);

/*
 * 色相を持つのは3つだけ（`docs/reports/ux/direction.md`）。
 *
 * - 試唱できる: 声の色で塗る
 * - 要確認: attention で塗る
 * - 保留: rec の輪郭
 *
 * 残りは面の段と字の段、輪郭の線種（実線・破線）、塗りの有無で言う。 白黒で見ても、
 * 印と字は残る。
 *
 * 輪郭のあるものとないものを同じ高さにする。 輪郭の無いほうに `border-transparent` を
 * 置くのは、1px ぶん低くならないため。
 */
const row = tv({
  variants: {
    // 印だけの円は、長い一覧の行に並べるため。 字は読み上げに残す。
    compact: {
      true: "p-1.5",
      false: "gap-2 px-3 py-1",
    },
    status: {
      unrecorded: "border-line-strong text-2",
      recorded: "border-transparent bg-surface-2 text-1",
      // 輪郭を強くして、未収録（薄い輪郭）と見分ける。 色相は与えない。
      needs_retake: "border-slate-12 text-1",
      excluded: "border-dashed border-line-strong text-2",
    },
  },
});

const song = tv({
  variants: {
    status: {
      /*
       * 声の色は祖先の `voiceStyle` から届く（`lib/voice-color.ts`）。
       * 声が無い面では塗りが解決しないので、そういう面に置く画面が代わりの塗りを決める。
       */
      ready: "border-transparent bg-voice font-semibold text-voice-ink",
      fallback: "border-line-strong font-semibold text-1",
      remaining: "border-transparent bg-surface-3 font-semibold text-1",
      out_of_range: "border-transparent bg-surface-3 text-2",
    },
  },
});

const origin = tv({
  variants: {
    status: {
      auto_confirmed: "border-transparent bg-surface-2 text-1",
      /*
       * 段 11 の塗りに、同じ色相の段 1 の字。 `text-slate-1` にしない——
       * 明るい面で 4.49:1 に届かない（段 1 の白さが amber のほうがわずかに上）。
       */
      in_queue: "border-transparent bg-attention font-semibold text-amber-1",
      blocked: "border-rec text-1",
      pinned: "border-transparent bg-surface-2 text-1",
    },
  },
});

type Entry = { glyph: GlyphName; label: string };

const ROW: Record<RowStatus, Entry> = {
  unrecorded: { glyph: "ring", label: "未収録" },
  recorded: { glyph: "dot", label: "収録済み" },
  needs_retake: { glyph: "triangle", label: "要録り直し" },
  excluded: { glyph: "minus", label: "除外" },
};

const SONG: Record<SongStatus, Entry> = {
  ready: { glyph: "check", label: "試唱できる" },
  fallback: { glyph: "swap", label: "代替ありで試唱できる" },
  // 字は `SongMark` が行数から組む。
  remaining: { glyph: "half", label: "" },
  out_of_range: { glyph: "slash", label: "音域外" },
};

const ORIGIN: Record<OriginStatus, Entry> = {
  auto_confirmed: { glyph: "check", label: "自動確定" },
  in_queue: { glyph: "alert", label: "要確認" },
  blocked: { glyph: "pause", label: "保留：検証エラー" },
  pinned: { glyph: "pin", label: "手で固定" },
};

type RowMarkProps = {
  status: RowStatus;
  /**
   * 字を畳んで、印だけの円にする。 字は `sr-only` で残すので、読み上げは変わらない。
   * 幅の狭い列に数百行を並べる一覧のため（収録の左の列）。
   */
  compact?: boolean;
};

/**
 * 行の状態のしるし。 録音リストの行と、行の絞り込みの結果に置く（`TR-RCL-18`）。
 *
 * 角丸は全開（丸い札）。 曲と原音の札（角の小さい四角）と、形でも見分けがつく。
 */
export const RowMark = ({ status, compact = false }: RowMarkProps) => (
  <span
    className={`inline-flex items-center rounded-full border text-md ${row({ status, compact })}`}
  >
    <Glyph name={ROW[status].glyph} />
    {compact ? <span className="sr-only">{ROW[status].label}</span> : ROW[status].label}
  </span>
);

type SongMarkProps =
  | { status: "ready" | "fallback" | "out_of_range" }
  | {
      status: "remaining";
      /** あと何行録ると歌えるか（`TR-RCL-17`）。 */
      rows: number;
    };

/**
 * 曲の「歌えるか」のしるし（`TR-RCL-19`）。
 *
 * 欠けを不足として書かない（`docs/reports/ux/direction.md`）。 品質の良し悪しも言わない。
 * 「あと N 行」は事実だけで、「未達」とは言わない。
 */
export const SongMark = (props: SongMarkProps) => (
  <span
    className={`inline-flex items-center gap-2 rounded-md border px-3 py-1 text-md ${song({ status: props.status })}`}
  >
    <Glyph name={SONG[props.status].glyph} />
    {props.status === "remaining" ? `あと ${props.rows} 行` : SONG[props.status].label}
  </span>
);

type OriginMarkProps = { status: OriginStatus };

/**
 * 原音設定のエイリアスの状態のしるし（`TR-ALN-26`）。
 *
 * 要確認だけが塗られる。 確認を待っているのは人で、機械が済ませたものは静かにする。
 */
export const OriginMark = ({ status }: OriginMarkProps) => (
  <span
    className={`inline-flex items-center gap-2 rounded-md border px-3 py-1 text-md ${origin({ status })}`}
  >
    <Glyph name={ORIGIN[status].glyph} />
    {ORIGIN[status].label}
  </span>
);
