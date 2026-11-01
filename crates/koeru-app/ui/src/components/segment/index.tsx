import { type KeyboardEvent, useRef } from "react";

import { cx } from "~/lib/tv";

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
};

/**
 * 並んだ選択肢から1つ選ぶ。 方式、テイク、範囲の切り替えに使う。
 *
 * `radiogroup` として出す。 Tab ではグループに1回だけ入り、中は矢印キーで動く
 * （ロービングタブインデックス）。 動いた先がそのまま選ばれる。
 * 矢印は押せない項目を飛ばし、端で反対側へ回る。 Home / End は端の押せる項目。
 *
 * 選んでいるかは面の段（3 → 5）と字の太さで言う。 色相を持たない。
 *
 * `className` を受け取らない。 幅は中身が決める。
 */
export const Segment = <V extends string>({
  value,
  onValueChange,
  options,
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
    <div role="radiogroup" className="inline-flex gap-1 rounded-lg bg-slate-3 p-1" {...name}>
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
            className={cx(
              "inline-grid h-8 items-center rounded-md px-3 text-md disabled:opacity-45",
              checked ? "bg-slate-5 text-slate-12" : "text-slate-11 enabled:hover:text-slate-12",
            )}
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
