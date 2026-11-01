import { type KeyboardEvent, type Ref, useRef } from "react";

import { cx, tv } from "~/lib/tv";

export type SegmentOption<V extends string> = {
  value: V;
  label: string;
  /** 押せない項目。録ったあとの方式のように、今は選べないものに使う。 */
  disabled?: boolean;
};

/*
 * 名前は必ず持たせる。 どちらも渡さない呼び方を型で塞ぐ。
 * 見える見出しがあるなら `aria-labelledby`、無いなら `aria-label`。
 */
type Name =
  | { "aria-label": string; "aria-labelledby"?: never }
  | { "aria-labelledby": string; "aria-label"?: never };

type SegmentProps<V extends string> = Name & {
  value: V;
  onValueChange: (value: V) => void;
  options: readonly SegmentOption<V>[];
  /** `sm` は収録画面の右上のテイク（S04）。 高さ 24px、12px の字。 */
  size?: "md" | "sm";
  /**
   * 選んでいる項目の右に、項目へ重ねる操作（⋮）の席を空ける。 操作そのものは
   * 群の外に置く——`radio` の中にも `radiogroup` の子にもボタンを置けない
   * （axe の `nested-interactive` / `aria-required-children`）。
   */
  reserveTrailing?: boolean;
  /** 群の要素。 重ねる操作の位置を、選んでいる項目から測るのに使う。 */
  groupRef?: Ref<HTMLDivElement>;
};

const group = tv({
  base: "inline-flex rounded-lg",
  variants: {
    size: {
      md: "gap-1 bg-slate-3 p-1",
      sm: "gap-0.5 bg-surface-2 p-0.5",
    },
  },
});

const item = tv({
  base: "inline-grid items-center rounded-md disabled:opacity-45",
  variants: {
    size: {
      md: "h-8 px-3 text-md",
      sm: "h-6 text-xs",
    },
    checked: {
      true: "",
      false: "enabled:hover:text-slate-12",
    },
    reserve: { true: "", false: "" },
  },
  // 余白の左右は1つの組み合わせだけが持つ。 `px` と `pr` を重ねると、勝つほうが並び順で決まる。
  compoundVariants: [
    { size: "md", checked: true, class: "bg-slate-5 text-slate-12" },
    { size: "md", checked: false, class: "text-slate-11" },
    { size: "sm", checked: true, class: "bg-surface-3 text-1" },
    { size: "sm", checked: false, class: "px-3 text-2" },
    { size: "sm", checked: true, reserve: false, class: "px-3" },
    { size: "sm", checked: true, reserve: true, class: "pr-8 pl-2.5" },
  ],
});

/**
 * 並んだ選択肢から1つ選ぶ。 方式、テイク、範囲の切り替えに使う。
 *
 * `radiogroup` として出す。 Tab ではグループに1回だけ入り、中は矢印キーで動く
 * （ロービングタブインデックス）。 動いた先がそのまま選ばれる。
 * 矢印は押せない項目を飛ばし、端で反対側へ回る。 Home / End は端の押せる項目。
 *
 * 選んでいるかは面の段と字の太さで言う。 色相を持たない。
 *
 * `className` を受け取らない。 幅は中身が決める。
 */
export const Segment = <V extends string>({
  value,
  onValueChange,
  options,
  size = "md",
  reserveTrailing = false,
  groupRef,
  ...name
}: SegmentProps<V>) => {
  const refs = useRef<(HTMLButtonElement | null)[]>([]);

  /*
   * Tab で入る先は1つだけ。 選んでいる項目が押せないとき（外から値を
   * 渡されたときに起きうる）は先頭の押せる項目にする。 どれも 0 にしないと、
   * グループごとキーボードから到達できなくなる。
   */
  const tabStop = options.some((o) => o.value === value && !o.disabled)
    ? value
    : options.find((o) => !o.disabled)?.value;

  const choose = (index: number) => {
    const option = options[index];
    if (!option || option.disabled) return;
    refs.current[index]?.focus();
    if (option.value !== value) onValueChange(option.value);
  };

  const step = (from: number, direction: 1 | -1) => {
    for (let k = 1; k <= options.length; k += 1) {
      const to = (((from + direction * k) % options.length) + options.length) % options.length;
      if (!options[to]?.disabled) return to;
    }
    return from;
  };

  const onKeyDown = (event: KeyboardEvent, index: number) => {
    let to: number | undefined;
    switch (event.key) {
      case "ArrowRight":
      case "ArrowDown":
        to = step(index, 1);
        break;
      case "ArrowLeft":
      case "ArrowUp":
        to = step(index, -1);
        break;
      case "Home":
        to = options.findIndex((o) => !o.disabled);
        break;
      case "End":
        to = options.findLastIndex((o) => !o.disabled);
        break;
      default:
        return;
    }
    event.preventDefault();
    if (to !== undefined && to >= 0) choose(to);
  };

  return (
    <div ref={groupRef} role="radiogroup" className={group({ size })} {...name}>
      {options.map((option, index) => {
        const checked = option.value === value;
        return (
          <button
            key={option.value}
            ref={(element) => {
              refs.current[index] = element;
            }}
            type="button"
            role="radio"
            aria-checked={checked}
            disabled={option.disabled}
            tabIndex={option.value === tabStop ? 0 : -1}
            onClick={() => choose(index)}
            onKeyDown={(event) => onKeyDown(event, index)}
            className={item({ size, checked, reserve: reserveTrailing })}
          >
            {/*
             * 太さを変えると幅が変わり、矢印で動くたびに隣が揺れる。
             * 太い字を見えない写しとして同じ格子に重ね、幅を太いほうで確保する。
             */}
            <span aria-hidden="true" className="invisible col-start-1 row-start-1 font-semibold">
              {option.label}
            </span>
            <span
              className={cx("col-start-1 row-start-1", checked ? "font-semibold" : "font-normal")}
            >
              {option.label}
            </span>
          </button>
        );
      })}
    </div>
  );
};
