import { useMutation, useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";
import { songStatusQuery } from "~/lib/queries";

/**
 * 歌える曲（`TR-RCL-17`、`TR-RCL-19`、`TR-SYN-20`）。
 *
 * 不足は「エイリアス名の一覧」ではなく「あと N 項目で『曲名』が歌える」で出す
 * （`TR-SYN-20`）。品質スコア、良し悪しの判定、他音源との比較、上達度は出さない。
 *
 * 曲は録り始めのとっかかりとして最も効く指標であって、唯一の指標ではない
 * （`TR-RCL-19`）。曲が1本も無くても、カバレッジで進捗は読める。
 */
export const SongList = () => {
  /*
   * 読みは `useSuspenseQuery`。
   *
   * 読み込み中は上の `Suspense`、失敗は `ErrorBoundary` が受ける。
   * 「まだ無い」と「失敗した」を自前で書き分けなくてよくなる——
   * 以前は成功時にエラーを消し忘れて、開いたあとも赤字が残っていた。
   */
  const { data: songs } = useSuspenseQuery(songStatusQuery());

  /**
   * 試唱。
   *
   * `variables` に押した曲の識別子が入るので、「いま用意している曲」を
   * 別に持たなくてよい。真偽値で持つと、1曲を歌わせたときに全行が
   * 「用意しています」になり、全部のボタンが disabled になってフォーカスも落ちる。
   *
   * 失敗しても一覧は消さない。 押した行の近くに1行出すだけ——
   * `Suspense` の外で受けるのは、これが読みではなく指示だから。
   */
  const sing = useMutation({ mutationFn: (id: string) => api.singSong(id) });

  /** 鳴っているものを止める。失敗しても言わない——止まっているのが望みなので。 */
  const stop = useMutation({ mutationFn: () => api.stopPreview() });
  const preparingId = sing.isPending ? sing.variables : null;

  const [pending, setPending] = useState(0);

  /*
   * 「録音終了 → 試唱ボタン活性化」の間に、無言の待ち時間を作らない（`TR-SYN-33`）。
   * 初回は前処理を含むので、中央値の目標が6秒ある。
   * 何も出ないまま6秒待たされると、壊れたと思われる。
   */
  useEffect(() => {
    let alive = true;
    let timer = 0;
    /**
     * 返ってきてから次を予約する。
     *
     * `setInterval` だと、1回が間隔より長くかかったときに問い合わせが重なる。
     * 待ち数がいちばん動くのはテイクの確定中で、そこがいちばん詰まる時間でもある。
     */
    const tick = () => {
      api
        .pendingWork()
        .then((n) => {
          if (alive) setPending(n);
        })
        .catch(() => {
          if (alive) setPending(0);
        })
        .finally(() => {
          if (alive) timer = window.setTimeout(tick, 400);
        });
    };
    tick();
    return () => {
      alive = false;
      window.clearTimeout(timer);
    };
  }, []);

  return (
    <Card title="歌える曲">
      {/*
        無言の待ち時間にしない（TR-SYN-33）。
        録り終えたあとの前処理が残っていることを出す。
      */}
      {/*
        領域は常に置き、中身だけを差し替える。 文言と一緒に挿し込むと、
        支援技術が変化として拾えず読まれない。空のときは何も描かない。
      */}
      <p aria-live="polite" aria-atomic="true" className="mt-2 text-sm text-slate-11">
        {pending > 0
          ? `録った音を整えています（残り ${pending} 件）。いま歌わせても鳴りますが、少し待ちます。`
          : ""}
      </p>

      {songs.length === 0 ? (
        <p className="mt-3 text-sm text-slate-11">
          曲が入っていません。UST を読み込むと、あと何項目で歌えるかが出ます。
        </p>
      ) : (
        <ul className="mt-3 flex flex-col gap-2">
          {songs.map((s) => (
            <li
              key={s.id}
              className="flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1 rounded-lg bg-slate-3 px-4 py-3"
            >
              <span className="font-medium">{s.title}</span>

              {s.missing_units === 0 ? (
                <span className="text-sm text-jade-12">いま歌えます</span>
              ) : s.singable ? (
                <span className="text-sm text-slate-11">
                  いま歌えます（あと {s.missing_units} 項目で本来の音に）
                </span>
              ) : (
                /*
                  エイリアス名の一覧ではなく、この形で出す（TR-SYN-20）。
                  行数と所要時間も添える（TR-RCL-17）。
                */
                <span className="font-mono text-sm text-slate-11 tabular-nums">
                  あと {s.missing_units} 項目（{s.missing_rows} 行・約{" "}
                  {Math.max(1, Math.round(s.seconds / 60))} 分）
                </span>
              )}

              {/*
               代替ありは、音のつながりが粗くなることを1行で説明する（TR-RCL-19）。
               */}
              {s.singability === "WithFallback" && (
                <p className="w-full text-xs text-slate-11">
                  録っていない単位を近いもので代えるので、音のつながりが粗くなります。
                </p>
              )}

              {/*
                途中まででも歌わせる（TR-SYN-18）。鳴らせないフレーズは落とす。
                落とした位置に無音・別音・代替音を挿さない。
              */}
              <div className="flex w-full items-center gap-3">
                {/*
                  押したボタン自身を disabled にしない。フォーカスが body へ落ちる。
                  `aria-busy` で状態を伝え、二重起動はハンドラ側で弾く。
                */}
                <Button
                  onClick={() => preparingId === null && sing.mutate(s.id)}
                  aria-busy={preparingId === s.id}
                  aria-label={`${s.title} を歌わせる`}
                >
                  {preparingId === s.id ? "用意しています" : "歌わせる"}
                </Button>
                <Button variant="ghost" onClick={() => stop.mutate()}>
                  止める
                </Button>
                {sing.data?.title === s.title && (
                  <span className="font-mono text-xs text-slate-11 tabular-nums">
                    {(sing.data.duration_ms / 1000).toFixed(1)} 秒
                    {sing.data.dropped_phrases > 0 &&
                      ` · ${sing.data.dropped_phrases} フレーズは飛ばしました`}
                  </span>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}

      {sing.error !== null && (
        <p role="alert" className="mt-3 text-sm text-red-11">
          {errorMessage(sing.error)}
        </p>
      )}
    </Card>
  );
};
