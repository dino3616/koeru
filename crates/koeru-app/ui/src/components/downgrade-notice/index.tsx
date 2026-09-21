import { useMutation, useQueryClient } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { api, errorMessage } from "~/lib/ipc";
import { methodLabel } from "~/lib/labels";
import { ledgerKey } from "~/lib/queries";

type Downgrade = {
  method: string;
  bytes: number;
};

type DowngradeNoticeProps = {
  downgrades: readonly Downgrade[];
};

/** バイトを「約 N MB」にする。単位を省かない。 */
const megabytes = (bytes: number) => `約 ${Math.max(1, Math.round(bytes / 1024 / 1024))} MB`;

/**
 * 下位方式への書き出し（`TR-PKG-24`, `TR-PKG-25`）。
 *
 * **容量を書き出し前に出す。** 「oto.ini 1ファイル分」ではない——
 * 独立した音源ルート・独立した ZIP になるので、WAV が複製されて
 * 元とほぼ同等の容量がもう1本できる。
 *
 * **素材の由来は出さない**（`DEC-RCL-015`）。 一度は「録った回 4 回 · 12 日の
 * あいだ」と出していたが、その数字から読めるのは「声が揃っていないかも
 * しれない」だけ。検知しないと言いながら判断材料を置いていたことになる。
 * 録った日時はテイクごとに出す（`TakeGenerations`）。
 *
 * 同じ ZIP には入れない（`TR-PKG-25`）。 接頭辞を付ければ単独音として
 * 使えなくなり、付けなければエイリアスが衝突する。両立しない。
 *
 * **押せる的を置く。** 「出せます」と書いてあるのに出す口が無いと、
 * 読んだ側は探し続けることになる。5値は対象方式の規約で作り直すので
 * （`TR-ALN-34`）、元の配り物とは別のファイルになる。
 */
export const DowngradeNotice = ({ downgrades }: DowngradeNoticeProps) => {
  const queryClient = useQueryClient();
  const run = useMutation({
    mutationFn: (method: string) => api.exportDowngrade(method),
    onSuccess: () => void queryClient.invalidateQueries({ queryKey: ledgerKey }),
  });
  const reveal = useMutation({ mutationFn: (seq: number) => api.revealRelease(seq) });

  if (downgrades.length === 0) return null;

  return (
    <div className="flex flex-col gap-3 rounded-lg border border-slate-7 bg-slate-3 p-3">
      <p className="text-sm font-semibold text-slate-12">別の作り方でも出せます</p>
      <ul className="flex flex-col gap-3">
        {downgrades.map((d) => (
          <li key={d.method} className="flex flex-col gap-1">
            <span className="text-sm text-slate-12">{methodLabel(d.method)}</span>
            <dl className="flex flex-col gap-1">
              <div className="flex items-baseline justify-between gap-3">
                <dt className="text-xs text-slate-11">増える容量</dt>
                <dd className="font-mono text-sm text-slate-12 tabular-nums">
                  {megabytes(d.bytes)}
                </dd>
              </div>
            </dl>
            <Button
              variant="secondary"
              size="sm"
              className="self-start"
              onClick={() => run.mutate(d.method)}
              disabled={run.isPending}
            >
              {run.isPending ? "つくっています" : `${methodLabel(d.method)}でつくる`}
            </Button>
          </li>
        ))}
      </ul>
      <p className="text-xs text-slate-11">
        別のファイルとして作ります。音は同じものを複製するので、そのぶん容量が増えます。
      </p>

      {run.data !== undefined && (
        <div className="flex flex-col gap-2 rounded-lg bg-slate-4 px-3 py-2">
          <p className="text-sm text-slate-12">
            できました。
            <span className="select-text font-mono"> {run.data.archive_name}</span> と、同じ中身の
            UAR を書き出しました。
          </p>
          <Button
            variant="secondary"
            size="sm"
            className="self-start"
            onClick={() => reveal.mutate(run.data.seq)}
          >
            置き場所を開く
          </Button>
        </div>
      )}

      {run.error !== null && (
        <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
          {errorMessage(run.error)}
        </p>
      )}

      {reveal.error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(reveal.error)}
        </p>
      )}
    </div>
  );
};
