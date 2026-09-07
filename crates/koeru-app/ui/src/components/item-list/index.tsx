import { useState } from "react";

import { Chip } from "~/components/chip";
import type { RowTakesView } from "~/lib/ipc";

/** 絞り込みの軸（`DEC-PLT-024`）。確認待ちは面ではなく、ここに畳む。 */
const FILTERS = [
  { id: "all", label: "すべて" },
  { id: "unrecorded", label: "まだ録っていない" },
  { id: "recorded", label: "録った" },
] as const;

type Filter = (typeof FILTERS)[number]["id"];

type ItemListProps = {
  rows: readonly RowTakesView[];
  /** 次に読む行。ここだけ印を付ける。 */
  nextRowId: string | null;
  /** 行を開く。テイクの面へ入る。 */
  onOpen: (rowId: string) => void;
};

const matches = (row: RowTakesView, filter: Filter) => {
  if (filter === "unrecorded") return row.takes.length === 0;
  if (filter === "recorded") return row.takes.length > 0;
  return true;
};

/**
 * 録るものの一覧（`DEC-PLT-024`）。
 *
 * 録る単位は行で、1音ではない（`TR-RCL-03`）。 「か き く け こ」の1行から
 * CV のエイリアスが5つ出るので、行ごとに「そこから何音取れるか」を書く。
 * 被覆は音で数え、行の消化率では数えない（`TR-RCL-19`）。
 *
 * 行 ID を出さない（`Q-REC-003`、`TR-REC-18`）。 読み上げにも入れない。
 * 行を指すのはテキストと並び順で、内部の識別子は経路の引数として持つだけ。
 *
 * まだ録っていない行を薄くしない（`docs/design/direction.md`）。
 * 欠けは不足ではなく「まだ」なので、灰色にも警告色にもしない。
 */
export const ItemList = ({ rows, nextRowId, onOpen }: ItemListProps) => {
  const [filter, setFilter] = useState<Filter>("all");
  const shown = rows.filter((r) => matches(r, filter));
  const recorded = rows.filter((r) => r.takes.length > 0).length;

  return (
    <>
      <div className="flex items-center justify-between gap-3">
        <p className="font-mono text-xs text-slate-11 tabular-nums">
          {recorded} / {rows.length} 行
        </p>
      </div>

      <div className="flex flex-wrap gap-2">
        {FILTERS.map((f) => (
          <Chip key={f.id} pressed={f.id === filter} onClick={() => setFilter(f.id)}>
            {f.label}
          </Chip>
        ))}
      </div>

      {shown.length === 0 ? (
        <p className="text-sm text-slate-11">この絞り込みに当たる行はありません。</p>
      ) : (
        <ul className="flex min-h-0 flex-1 flex-col gap-2 overflow-y-auto">
          {shown.map((row) => (
            <li key={row.row_id}>
              <button
                type="button"
                onClick={() => onOpen(row.row_id)}
                aria-label={
                  row.takes.length === 0
                    ? `${row.text}。まだ録っていません`
                    : `${row.text}。${row.takes.length} 回録りました`
                }
                className={`flex w-full flex-col gap-2 rounded-lg border p-3 text-left hover:bg-slate-4 ${
                  row.row_id === nextRowId
                    ? "border-slate-12 bg-slate-4"
                    : "border-transparent bg-slate-3"
                }`}
              >
                <span className="select-text text-sm text-slate-12">{row.text}</span>
                <span className="flex items-center justify-between gap-3">
                  <span className="font-mono text-xs text-slate-11 tabular-nums">
                    {row.units} 音
                  </span>
                  {row.takes.length > 0 && (
                    <span className="flex items-center gap-2 font-mono text-xs text-slate-12 tabular-nums">
                      <svg
                        width="16"
                        height="16"
                        viewBox="0 0 16 16"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="1.6"
                        strokeLinecap="round"
                        strokeLinejoin="round"
                        aria-hidden="true"
                      >
                        <path d="M3 8.5 6.5 12 13 4.5" />
                      </svg>
                      {row.takes.length} 回
                    </span>
                  )}
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </>
  );
};
