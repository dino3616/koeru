import { type KeyboardEvent, type ReactNode, useRef, useState } from "react";

import { Button } from "~/components/button";
import { isBlackKey, toneName } from "~/lib/tones";
import { tv } from "~/lib/tv";

import { type ToneSpan, snapSpan } from "./geometry";

type LaneKeyboardProps = {
  /** レーンの名前（「Normal CVVC」など）。 鍵盤と送りのボタンの名前になる。 */
  label: string;
  /** 選べる音階の範囲。 両端を含む MIDI 番号。 外の鍵は描くが押せない。 */
  range: ToneSpan;
  /** 見えている窓。 両端を含む MIDI 番号。 両端は白鍵へ揃えて描く。 */
  view: ToneSpan;
  /** 左右のボタンで窓を送ったとき。 幅は変えず、1オクターブずつ動く。 */
  onViewChange: (view: ToneSpan) => void;
  /** 選んでいる音階（MIDI）。 */
  selected: readonly number[];
  /** 録り始めている音階。 下線を引く。 `selected` の部分集合。 */
  created?: readonly number[];
  /** 選べない音階。 同じ組がもうある。 */
  locked?: readonly number[];
  /** 鍵を押したとき。 足すか外すかは受け取った側が `selected` から決める。 */
  onToggle: (midi: number) => void;
  /**
   * 鍵の上に重ねる層。 鍵の位置は `keySpan` が同じ座標で返す。
   * 層は鍵盤と同じ大きさで、既定では押しを通す。 押せるものは置く側が戻す。
   */
  overlay?: ReactNode;
};

/*
 * 鍵の見た目。 白鍵と黒鍵、押せるかどうかで変わる。
 *
 * 選んだ鍵は塗りが段 11 で、字は段 1。 色相を持たせない（`docs/reports/ux/direction.md`）。
 * 黒鍵は段 1 の地に線で描く。 明るい面では白鍵より明るくなるが、形と位置で読める。
 */
const keyStyle = tv({
  variants: {
    black: { true: "", false: "" },
    state: {
      idle: "",
      selected: "border-slate-11 bg-slate-11 text-slate-1 hover:bg-slate-12",
      locked: "cursor-not-allowed opacity-45",
    },
  },
  compoundVariants: [
    { black: false, state: ["idle", "locked"], class: "bg-slate-3" },
    { black: false, state: "idle", class: "hover:bg-slate-4" },
    { black: true, state: ["idle", "locked"], class: "bg-slate-1" },
    { black: true, state: "idle", class: "hover:bg-slate-3" },
  ],
});

const ArrowIcon = ({ direction }: { direction: "left" | "right" }) => (
  <svg
    viewBox="0 0 16 16"
    className="size-4"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeLinecap="round"
    strokeLinejoin="round"
    aria-hidden="true"
  >
    <path d={direction === "left" ? "M10 3 5 8l5 5" : "M6 3l5 5-5 5"} />
  </svg>
);

/**
 * 横に長い鍵盤。 1行が1レーンで、鍵を押して音階を足す・外す。
 *
 * 音階の選び方の骨格は `docs/reports/ux/redesign-canvas/boards/P4c-PitchPicker.dc.html`。
 * 全レーンで鍵の位置がそろうので、同じ音階が縦に並ぶ。 そろえるのは置く側で、
 * 同じ `view` を全レーンへ渡す（窓は制御する——レーンごとに持つと、1本だけ送ると
 * 縦がずれる）。
 *
 * 描くのは DOM。 canvas にすると、鍵が支援技術に届かない。
 *
 * # 役割
 *
 * `toolbar` にして、鍵は中のボタンにする。 鍵盤は「1つの部品に Tab で1回入り、
 * 中は矢印で動く」もので、`toolbar` がちょうどその操作を約束している。
 * `listbox` も候補だが、選択の状態を `aria-selected` で持つ。 ここは足す・外すという
 * 操作で、名前が「A3 を足す」から「A3 を外す」へ変わる。 `aria-pressed` も置かない
 * ——名前が操作を言っているところに状態を重ねると、「A3 を外す、押されている」と二重に読まれる。
 * `grid` は行と列の2次元の移動を約束するが、ここは横1列。
 *
 * 焦点の置き方は roving tabindex。 Tab で入る鍵は1つだけ（選んだ鍵があればその先頭、
 * 無ければ窓の左端）で、あとは矢印と Home / End で動く。
 * 窓の外へは矢印で出ない。 送るのは左右のボタン。
 *
 * # 名前
 *
 * 見えている字は C と選んだ鍵だけ。 名前は読み上げ専用の字（`sr-only`）で作る。
 * `aria-label` にすると、見えている字を上書きしてしまう。 見える「A3」の続きに
 * 「を足す」が付く形なので、見えているものと読まれるものは食い違わない。
 *
 * 選べない鍵は `aria-disabled`。 `disabled` にすると焦点が止まらず、矢印で動くときに
 * 飛び越える。 どの音階がなぜ押せないのかを、読み上げで確かめられるようにする。
 *
 * 鍵の幅は 24 CSS ピクセル以上（`TR-PLT-31`）。 窓が広すぎて収まらなければ、
 * 鍵を縮めず溢れる。 窓の幅は置く側が決める。
 */
export const LaneKeyboard = ({
  label,
  range,
  view,
  onViewChange,
  selected,
  created = [],
  locked = [],
  onToggle,
  overlay,
}: LaneKeyboardProps) => {
  const toolbar = useRef<HTMLDivElement>(null);
  const [active, setActive] = useState<number | null>(null);

  const [from, to] = snapSpan(view);
  const [rangeFrom, rangeTo] = snapSpan(range);
  const chosen = new Set(selected);
  const made = new Set(created);
  const fixed = new Set(locked);
  const isLocked = (midi: number) => fixed.has(midi) || midi < range[0] || midi > range[1];

  const midis = Array.from({ length: to - from + 1 }, (_, i) => from + i);
  // 黒鍵は、すぐ上の白鍵の左端に置く。 並びが音の高さの順になる。
  const whites = midis
    .filter((m) => !isBlackKey(m))
    .map((m) => ({ white: m, black: isBlackKey(m - 1) && m - 1 >= from ? m - 1 : null }));

  // 印が窓の外なら使わない。 窓を送ったあとに、見えない鍵へ Tab が入ってしまう。
  const entry =
    active !== null && active >= from && active <= to
      ? active
      : (midis.find((m) => chosen.has(m)) ?? from);

  const focusKey = (midi: number) => {
    setActive(midi);
    toolbar.current?.querySelector<HTMLButtonElement>(`[data-midi="${midi}"]`)?.focus();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLButtonElement>, midi: number) => {
    const targets: Record<string, number> = {
      ArrowLeft: Math.max(from, midi - 1),
      ArrowRight: Math.min(to, midi + 1),
      Home: from,
      End: to,
    };
    const next = targets[event.key];
    if (next === undefined) {
      return;
    }
    event.preventDefault();
    focusKey(next);
  };

  const shift = (direction: -1 | 1) => {
    const width = to - from;
    const start = Math.min(
      Math.max(from + direction * 12, rangeFrom),
      Math.max(rangeFrom, rangeTo - width),
    );
    onViewChange([start, start + width]);
  };

  const renderKey = (midi: number) => {
    const black = isBlackKey(midi);
    const lockedKey = isLocked(midi);
    const on = chosen.has(midi);
    const name = toneName(midi);
    const action = lockedKey ? "（選べない）" : on ? " を外す" : " を足す";
    const madeAlready = made.has(midi);
    return (
      <button
        type="button"
        key={midi}
        data-midi={midi}
        tabIndex={midi === entry ? 0 : -1}
        aria-disabled={lockedKey || undefined}
        onFocus={() => setActive(midi)}
        onKeyDown={(event) => onKeyDown(event, midi)}
        onClick={() => {
          if (!lockedKey) {
            onToggle(midi);
          }
        }}
        className={`flex items-end justify-center border border-slate-7 font-mono text-xs tabular-nums focus-visible:z-20 ${
          black
            ? "absolute top-0 left-0 z-10 h-3/5 w-7/10 -translate-x-1/2 rounded-b-md pb-1"
            : "relative h-full w-full rounded-b-md pb-2"
        } ${keyStyle({ black, state: lockedKey ? "locked" : on ? "selected" : "idle" })}`}
      >
        {(midi % 12 === 0 || on || madeAlready) && <span aria-hidden="true">{name}</span>}
        <span className="sr-only">
          {name}
          {action}
          {madeAlready && "（作成済み）"}
        </span>
        {/* 下線。 録り始めている音階。 名前には「作成済み」で出ている。 */}
        {madeAlready && (
          <span
            aria-hidden="true"
            className="absolute inset-x-1.5 bottom-0.5 h-0.5 rounded-full bg-current"
          />
        )}
      </button>
    );
  };

  return (
    <div className="flex items-center gap-2">
      <Button
        size="icon"
        variant="ghost"
        aria-label={`${label} の鍵盤を左へ送る`}
        disabled={from <= rangeFrom}
        onClick={() => shift(-1)}
      >
        <ArrowIcon direction="left" />
      </Button>
      <div className="relative min-w-0 flex-1">
        <div
          ref={toolbar}
          role="toolbar"
          aria-label={`${label} の鍵盤`}
          className="flex h-14 gap-px"
        >
          {whites.map(({ white, black }) => (
            <div key={white} className="relative min-w-6 flex-1">
              {black !== null && renderKey(black)}
              {renderKey(white)}
            </div>
          ))}
        </div>
        {overlay !== undefined && (
          <div className="pointer-events-none absolute inset-0 z-30">{overlay}</div>
        )}
      </div>
      <Button
        size="icon"
        variant="ghost"
        aria-label={`${label} の鍵盤を右へ送る`}
        disabled={to >= rangeTo}
        onClick={() => shift(1)}
      >
        <ArrowIcon direction="right" />
      </Button>
    </div>
  );
};
