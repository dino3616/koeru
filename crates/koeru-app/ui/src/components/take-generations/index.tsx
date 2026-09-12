import { Button } from "~/components/button";
import { Card } from "~/components/card";
import type { TakeSummaryView } from "~/lib/ipc";

type TakeGenerationsProps = {
  takes: readonly TakeSummaryView[];
  /** いま採用しているテイク。 */
  adoptedId: number | null;
  /** いま見ているテイク。 */
  shownId: number | null;
  onShow: (takeId: number) => void;
  onAdopt: (takeId: number) => void;
  onRetake: () => void;
  busy: boolean;
};

/**
 * 録った回（`TR-REC-21`、`TR-RCL-25`）。
 *
 * 録り直しは上書きではなく世代。 過去のものは非採用として残り、
 * いつでも採用を戻せる。
 *
 * **切り替えても被覆は変わらない**（`TR-RCL-25`）。変わるのは原音設定の値だけ。
 * だから壊れない操作で、恐れずいじれる——ここが「声の表情を選ぶ」ところになる
 * （`DEC-PLT-025`）。
 *
 * 見ることと採ることを分ける。 押して見るだけなら何も変わらない。
 * 採るのは別の的にする——聴き比べているつもりで採用が動くと、
 * 「壊れない操作」でなくなる。
 */
export const TakeGenerations = ({
  takes,
  adoptedId,
  shownId,
  onShow,
  onAdopt,
  onRetake,
  busy,
}: TakeGenerationsProps) => (
  <Card title="録った回">
    <ul className="flex flex-col gap-2">
      {takes.map((t) => {
        const adopted = t.take_id === adoptedId;
        const seconds = (t.duration_ms / 1000).toFixed(2);
        return (
          <li key={t.take_id} className="flex items-center gap-2">
            <button
              type="button"
              aria-pressed={t.take_id === shownId}
              onClick={() => onShow(t.take_id)}
              aria-label={
                t.invalid
                  ? `${t.generation} 回目、${seconds} 秒。音がとぎれているので使えません`
                  : `${t.generation} 回目、${seconds} 秒${adopted ? "。これを使っています" : ""}`
              }
              className={`flex min-w-0 flex-1 items-center gap-3 rounded-lg border p-3 text-left hover:bg-slate-4 ${
                t.take_id === shownId
                  ? "border-slate-7 bg-slate-4"
                  : "border-transparent bg-slate-3"
              }`}
            >
              <span className="flex min-w-0 flex-col">
                <span className="font-mono text-sm text-slate-12 tabular-nums">
                  {t.generation} 回目
                </span>
                <span className="font-mono text-xs text-slate-11 tabular-nums">{seconds} 秒</span>
              </span>
              {adopted && (
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
                  className="ml-auto text-slate-12"
                >
                  <path d="M3 8.5 6.5 12 13 4.5" />
                </svg>
              )}
              {t.invalid && <span className="ml-auto text-xs text-red-11">使えません</span>}
            </button>

            {!adopted && !t.invalid && (
              <Button
                variant="secondary"
                size="sm"
                onClick={() => onAdopt(t.take_id)}
                disabled={busy}
              >
                これを使う
              </Button>
            )}
          </li>
        );
      })}
    </ul>

    <hr className="h-px border-0 bg-slate-6" />

    <Button variant="secondary" size="sm" onClick={onRetake} disabled={busy}>
      もう一度録る
    </Button>
    <p className="text-xs text-slate-11">
      録り直しても、前のものは残ります。この行から取れる音は変わりません。
    </p>
  </Card>
);
