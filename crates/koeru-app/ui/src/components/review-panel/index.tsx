import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";
import { reviewModeLabel } from "~/lib/labels";
import { ledgerKey, reviewSummaryQuery } from "~/lib/queries";

type ReviewPanelProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
};

/** 秒を「何分何秒」へ。上限も見積もりも分の単位で語られる（`DEC-ALN-003`）。 */
const duration = (seconds: number): string => {
  const m = Math.floor(seconds / 60);
  const s = seconds % 60;
  if (m === 0) return `${s} 秒`;
  return s === 0 ? `${m} 分` : `${m} 分 ${s} 秒`;
};

/**
 * 確認の進み具合と、原音設定の書き出し（`TR-ALN-25`, `TR-ALN-28`, `TR-ALN-21`）。
 *
 * 件数ではなく時間で言う。 上限は合計所要時間で切っていて（`DEC-ALN-003`）、
 * 何件あるかより「あとどれだけ見ることになるか」のほうが決め手になる。
 * 件数も並べるのは、量だけで伝えないため。
 *
 * **上限を超えるまで、まとめて確認は出さない。** 個別確認をやめられるのは
 * 超えたときだけで（`INV-ALN-004`）、先に出すと押せない的が常駐する。
 *
 * 品質の良し悪しを言わない（`TR-SYN-20`、`DEC-REC-008`）。 確認待ちが
 * 多いことを失敗として描かない——「まだ見ていないものがこれだけある」と書く。
 */
export const ReviewPanel = ({ voiceId }: ReviewPanelProps) => {
  const { data: summary } = useSuspenseQuery(reviewSummaryQuery(voiceId));
  const queryClient = useQueryClient();
  const [error, setError] = useState<string | null>(null);
  const [exported, setExported] = useState<string | null>(null);

  /*
    書き出してよいかは Rust に訊く。 件数から組み立て直さない——関門の条件は
    `ReviewQueue::may_export` が持っていて（`INV-ALN-003`）、同じ規則をここにも
    書くと片方だけが古くなる。**実際にずれた**: `pending` は未推定を数えないので、
    録り直しに回した音が残っていても「確認は済みました」と出して的を押させ、
    押すと必ず断られていた。
  */
  const done = summary.pending === 0 && summary.blocked === 0 && summary.missing === 0;

  const after = () => queryClient.invalidateQueries({ queryKey: ledgerKey });
  const fail = (e: unknown) => setError(errorMessage(e));

  const switchMode = useMutation({
    mutationFn: (mode: "batch" | "suggest_rerecord") => api.switchReviewMode(mode),
    onMutate: () => setError(null),
    onSuccess: after,
    onError: fail,
  });

  const confirmAll = useMutation({
    mutationFn: () => api.confirmAllEntries(),
    onMutate: () => setError(null),
    onSuccess: after,
    onError: fail,
  });

  const exportOtos = useMutation({
    mutationFn: () => api.exportOtos(),
    onMutate: () => {
      setError(null);
      setExported(null);
    },
    onSuccess: (path) => {
      setExported(path);
      return after();
    },
    onError: fail,
  });

  const busy = switchMode.isPending || confirmAll.isPending || exportOtos.isPending;

  return (
    <Card title="見ておく音">
      <p aria-live="polite" className="text-sm text-slate-12">
        {done
          ? "確認の済んでいない音はありません。"
          : `まだ見ていない音が ${summary.pending} 件、見るのにおよそ ${duration(
              summary.estimated_seconds,
            )}。`}
      </p>

      <dl className="grid grid-cols-2 gap-3">
        <div className="flex flex-col gap-1">
          <dt className="text-xs text-slate-11">進め方</dt>
          <dd className="m-0 text-sm text-slate-12">{reviewModeLabel(summary.mode)}</dd>
        </div>
        <div className="flex flex-col gap-1">
          <dt className="text-xs text-slate-11">1度に見る上限</dt>
          <dd className="m-0 font-mono text-sm text-slate-12 tabular-nums">
            {duration(summary.budget_seconds)}
          </dd>
        </div>
      </dl>

      {summary.blocked > 0 && (
        // 字に色相を与えない。 印としての amber は図形側が持つ（`package-panel`）。
        <p className="text-sm text-slate-12">
          切り出しが直せなかった音が {summary.blocked} 件あります。録り直すと直ります。
        </p>
      )}

      {summary.missing > 0 && (
        <p className="text-sm text-slate-12">
          発声が見つからなかった行が {summary.missing} 件あります。録り直すまで書き出せません。
        </p>
      )}

      {summary.unestimated > 0 && (
        <p className="text-sm text-slate-12">
          録り直しを待っている音が {summary.unestimated} 件あります。
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        {/*
          超えたときにだけ出す。 `INV-ALN-004` が「個別確認をやめるのは
          上限を超えたときだけ」と定めていて、Rust 側も超えていなければ断る。
        */}
        {summary.exceeds_budget && summary.mode === "individual" && (
          <>
            <Button type="button" onClick={() => switchMode.mutate("batch")} disabled={busy}>
              まとめて確認する
            </Button>
            <Button
              type="button"
              onClick={() => switchMode.mutate("suggest_rerecord")}
              disabled={busy}
            >
              録り直しをすすめてもらう
            </Button>
          </>
        )}

        {summary.mode === "batch" && (
          <Button type="button" onClick={() => confirmAll.mutate()} disabled={busy}>
            残りをまとめて確認する
          </Button>
        )}

        <Button
          type="button"
          variant="primary"
          onClick={() => exportOtos.mutate()}
          // 関門の答えをそのまま使う（`INV-ALN-003`）。
          disabled={busy || !summary.may_export}
        >
          {summary.exported ? "書き出し済み" : "原音設定を書き出す"}
        </Button>
      </div>

      {/*
        飛ばせる方式では、飛ばす経路を必ず出す（`TR-ALN-28`）。
        単独音は到達水準が保留なので、ここには出ない。
      */}
      {summary.allows_skipping && (
        <p className="text-xs text-slate-11">
          この作り方では確認が空になりません。まとめて引き受けて先へ進めます。
        </p>
      )}

      <p aria-live="polite" className="text-sm text-slate-11">
        {error ?? (exported === null ? "" : "書き出しました。")}
      </p>
    </Card>
  );
};
