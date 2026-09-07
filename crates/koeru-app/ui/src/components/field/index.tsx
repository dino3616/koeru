import type { ComponentProps } from "react";

type FieldProps = Omit<ComponentProps<"input">, "className" | "type"> & {
  /** 見えている名札。`aria-label` にしない——見える字と読まれる字を揃える。 */
  label: string;
  /** 名札の下に置く一行。何を打てばよいかを書く。 */
  hint?: string;
};

/**
 * 文字を打つところ。
 *
 * 名札を必ず持つ。 `placeholder` は名前にならない——打ちはじめた瞬間に
 * 消えるので、あとから「これは何の欄か」を確かめられない（`TR-PLT-29`）。
 *
 * 打った字は選べる。 `globals.css` が `user-select: none` を敷いているので、
 * 打つところと読ませる文章だけ個別に戻す。
 */
export const Field = ({ label, hint, id, ...props }: FieldProps) => (
  <div className="flex flex-col gap-2">
    <label className="text-xs text-slate-11" htmlFor={id}>
      {label}
    </label>
    <input
      id={id}
      type="text"
      autoComplete="off"
      className="h-11 w-full select-text rounded-lg border border-slate-7 bg-slate-3 px-3 text-sm text-slate-12 placeholder:text-slate-11"
      {...props}
    />
    {hint !== undefined && <p className="text-xs text-slate-11">{hint}</p>}
  </div>
);
