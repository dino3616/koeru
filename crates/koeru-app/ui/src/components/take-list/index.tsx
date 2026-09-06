import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";
import { ledgerKey, rowsWithTakesQuery } from "~/lib/queries";

type TakeListProps = {
  /** 収録中は録り直しを出さない。 */
  busy: boolean;
  /** 録り直しを始める。 */
  onRetake: (rowId: string) => void;
  /** そのテイクをそのまま鳴らす（`TR-REC-43`）。合成を通さない。 */
  onPlay: (takeId: number) => void;
};

/**
 * 録れた行と、その行に積んだテイク（`TR-REC-21`、`TR-RCL-25`）。
 *
 * 録り直しは上書きではなく世代。過去のテイクは非採用として残り、
 * いつでも採用を戻せる。採用を切り替えてもカバレッジは変わらない
 * （行が生む単位は行が持っていて、テイクに依らない）。変わるのは原音設定の値だけ。
 *
 * 一覧が無いと、一度録った行を二度と選べない。 `next_row` は
 * 未収録しか返さないので、ここが録り直しの唯一の入口になる。
 */
export const TakeList = ({ busy, onRetake, onPlay }: TakeListProps) => {
  /*
   * 読みは `useSuspenseQuery`。
   *
   * 読み込み中は上の `Suspense` が受け、失敗は `ErrorBoundary` が受ける。
   * だから `rows` は必ず在り、失敗の state も要らない——
   * 「まだ無い」と「失敗した」を部品ごとに書き分けなくてよくなる。
   */
  const { data: rows } = useSuspenseQuery(rowsWithTakesQuery());
  const queryClient = useQueryClient();

  /*
   * 採用の切り替えは指示。 問い合わせではないので `useMutation`。
   * 済んだら台帳を無効化して、一覧も進み具合も取り直させる。
   */
  const adopt = useMutation({
    mutationFn: ({ rowId, takeId }: { rowId: string; takeId: number }) =>
      api.adoptTake(rowId, takeId),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });

  const recorded = rows.filter((r) => r.takes.length > 0);

  return (
    <Card title="録れたもの一覧">
      {/* 採用の切り替えだけは、ここで失敗を出す。読みの失敗は境界が受ける。 */}
      {adopt.error !== null && (
        <p role="alert" className="mt-3 text-sm text-red-11">
          {errorMessage(adopt.error)}
        </p>
      )}

      {recorded.length === 0 ? (
        <p className="mt-3 text-sm text-slate-11">まだ1つも録れていません。</p>
      ) : (
        <ul className="mt-3 flex flex-col gap-2">
          {recorded.map((r) => (
            <li
              key={r.row_id}
              className="flex flex-wrap items-center gap-x-3 gap-y-2 border-slate-6 border-b py-2 last:border-b-0"
            >
              <span className="w-14 shrink-0 font-mono text-sm text-slate-11">{r.row_id}</span>
              <span className="min-w-40 grow text-base">{r.text}</span>

              {/* 世代を並べる。 採用中が分かるようにする（`TR-RCL-25`）。 */}
              <span className="flex flex-wrap items-center gap-1">
                {r.takes.map((t) => {
                  const adopted = r.adopted === t.take_id;
                  const seconds = (t.duration_ms / 1000).toFixed(1);
                  return (
                    <Button
                      key={t.take_id}
                      size="md"
                      variant={adopted ? "primary" : "ghost"}
                      // 使えないから押せない、と、いま採用中だから押しても変わらない、を
                      // 同じ disabled に潰さない。採用中は押せるままにして状態で伝える。
                      disabled={busy || t.invalid}
                      aria-pressed={adopted}
                      aria-label={
                        t.invalid
                          ? `${r.row_id} の ${t.generation} 本目、${seconds} 秒。取りこぼしがあるので使えません`
                          : `${r.row_id} の ${t.generation} 本目、${seconds} 秒${adopted ? "。採用中" : "を採用する"}`
                      }
                      onClick={() => {
                        if (!adopted) adopt.mutate({ rowId: r.row_id, takeId: t.take_id });
                      }}
                    >
                      <span aria-hidden="true">
                        {t.generation}
                        {t.invalid ? "✕" : ""}
                      </span>
                    </Button>
                  );
                })}
              </span>

              <Button
                size="md"
                variant="secondary"
                disabled={busy || r.adopted === null}
                aria-label={`${r.row_id}「${r.text}」の採用テイクを聴く`}
                onClick={() => r.adopted !== null && onPlay(r.adopted)}
              >
                聴く
              </Button>

              <Button
                size="md"
                variant="secondary"
                disabled={busy}
                aria-label={`${r.row_id}「${r.text}」を録り直す`}
                onClick={() => onRetake(r.row_id)}
              >
                録り直す
              </Button>
            </li>
          ))}
        </ul>
      )}

      {recorded.length > 0 && (
        <p className="mt-3 text-sm text-slate-11">
          数字がテイクです。押すとその世代に切り替わります。「聴く」は採用中のものを鳴らします。
          <br />
          切り替えても歌える曲は変わりません。変わるのは原音設定の値だけです。
        </p>
      )}
    </Card>
  );
};
