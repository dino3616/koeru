import { type ComponentProps, createContext, useContext, useId } from "react";

/**
 * 見出しの深さ。入れ子になった [`Card`] が自分の段を知るために持つ。
 *
 * 段を props で渡さないのは、渡す側が入れ子の深さを知っている必要があるため。
 * 「マイク」の中に置いた「入力レベル」は、どこから置かれたかで段が変わる。
 * 置いた側が数えると、部品を移したときに数え直しを忘れる。
 */
const Depth = createContext(2);

/** `h7` は無い。6 で止める（`TR-PLT-29`）。 */
const HEADINGS = ["h2", "h3", "h4", "h5", "h6"] as const;

/**
 * ひとまとまりの領域。
 *
 * `title` を渡すと `<section>` に名前が付いて landmark になり、
 * 支援技術の領域移動で行き来できる（`TR-PLT-29`）。
 * 名前の無い `<section>` は landmark にならないので、
 * 領域として扱ってほしいものには必ず `title` を渡す。
 *
 * 見出しの段は入れ子の深さから決まる。 画面の `h1` の下が `h2`、
 * その中の `Card` が `h3`。飛ばすと、見出しだけを辿る移動が壊れる。
 */
export const Card = ({
  title,
  children,
  ...props
}: Omit<ComponentProps<"section">, "title" | "className"> & { title?: string }) => {
  const id = useId();
  const depth = useContext(Depth);
  return (
    <Depth.Provider value={depth + 1}>
      <section
        className="rounded-xl border border-slate-6 bg-slate-2 p-5"
        {...(title === undefined ? {} : { "aria-labelledby": id })}
        {...props}
      >
        {title !== undefined && (
          <CardTitle id={id} depth={depth}>
            {title}
          </CardTitle>
        )}
        {children}
      </section>
    </Depth.Provider>
  );
};

export const CardTitle = ({
  depth = 2,
  ...props
}: Omit<ComponentProps<"h2">, "className"> & { depth?: number }) => {
  const Heading = HEADINGS[Math.min(depth, HEADINGS.length + 1) - 2] ?? "h6";
  return <Heading className="text-sm font-semibold tracking-wide text-slate-11" {...props} />;
};
