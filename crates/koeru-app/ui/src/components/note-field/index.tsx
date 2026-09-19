import type { ComponentProps } from "react";

type NoteFieldProps = Omit<ComponentProps<"textarea">, "className"> & {
  /** 見えている名札。`aria-label` にしない——見える字と読まれる字を揃える。 */
  label: string;
  /** 名札の下に置く一行。何を打てばよいかを書く。 */
  hint?: string;
  /** 行数。既定は3行。 */
  lines?: 3 | 6 | 10;
};

/** 行数はクラスで固定する。任意値を使わない（`vite.config.ts` の検査）。 */
const HEIGHTS = { 3: "h-20", 6: "h-36", 10: "h-56" } as const;

/**
 * 何行か打つところ。
 *
 * [`Field`](../field) の複数行版。 別の部品にしているのは、`<input>` と
 * `<textarea>` で受け取れる props が違うため——1つにまとめると、
 * どちらにも無い属性が型の上で通ってしまう。
 *
 * 打った字は選べる。 `globals.css` が `user-select: none` を敷いているので、
 * 打つところだけ個別に戻す。
 */
export const NoteField = ({ label, hint, id, lines = 3, ...props }: NoteFieldProps) => (
  <div className="flex flex-col gap-2">
    <label className="text-xs text-slate-11" htmlFor={id}>
      {label}
    </label>
    <textarea
      id={id}
      className={`w-full select-text rounded-lg border border-slate-7 bg-slate-3 p-3 text-sm text-slate-12 placeholder:text-slate-11 ${HEIGHTS[lines]}`}
      {...props}
    />
    {hint !== undefined && <p className="text-xs text-slate-11">{hint}</p>}
  </div>
);
