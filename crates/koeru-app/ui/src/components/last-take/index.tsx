import { useQuery } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { TakeWaveform } from "~/components/take-waveform";
import type { TakeView } from "~/lib/ipc";
import { otosQuery } from "~/lib/queries";

type LastTakeProps = {
  take: TakeView;
  /** その行の読み上げ文字列。行 ID は出さない（`TR-REC-18`）。 */
  rowText: string;
  /** この行から取れた音の数。 */
  units: number;
  /** 収録中は録り直しを出さない。 */
  busy: boolean;
  onListen: () => void;
  onRetake: () => void;
  onOpen: () => void;
};

/**
 * いま録れたもの。
 *
 * 測った値をそのまま出す（`TR-REC-16`、`DEC-REC-008`）。
 * 「小さすぎます」「歪んでいます」「うまくなりました」を書かない。
 *
 * 単位を省かない（`docs/design/direction.md`）。 数字だけを置かない。
 *
 * 取りこぼしは事実として出す（`TR-REC-07`）。 勧めるのではなく、
 * もう一度同じ行が出てくることを書く。
 *
 * 波形は録った回の面と同じものを描く。 サムネイル（512 バケットのピーク）で
 * 済ませない——**録れた直後にいちばん見たいのは、声が入っているかと、
 * 自動で決めた切り出しがどこに来たか**で、粗い包絡ではどちらも読めない。
 */
export const LastTake = ({
  take,
  rowText,
  units,
  busy,
  onListen,
  onRetake,
  onOpen,
}: LastTakeProps) => {
  /*
   * 自動で決めた切り出し（`TR-ALN-33`）。
   *
   * `useSuspenseQuery` にしない。 波形に重ねる目盛りで、取れなくても
   * 波形は読める。中断させると、これを待つあいだ波形が消える。
   */
  const { data: otos = [] } = useQuery(otosQuery(take.take_id));

  return (
    <Card title={busy ? "ひとつ前に録れたもの" : "いま録れたもの"}>
      <p className="select-text text-sm text-slate-12">{rowText}</p>

      <TakeWaveform
        // テイクが変わったら作り直す。描画の途中経過を持ち越さない。
        key={take.take_id}
        takeId={take.take_id}
        durationMs={take.duration_ms}
        peak={take.peak}
        otos={otos}
        selected={null}
        height="sm"
      />

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 font-mono text-xs text-slate-11 tabular-nums">
        <dt>長さ</dt>
        <dd className="m-0 select-text text-slate-12">{(take.duration_ms / 1000).toFixed(2)} 秒</dd>
        <dt>いちばん大きいところ</dt>
        <dd className="m-0 select-text text-slate-12">{Math.round(take.peak * 100)}%</dd>
        <dt>この行から取れた音</dt>
        <dd className="m-0 select-text text-slate-12">{units} 音</dd>
        <dt>前の余白</dt>
        <dd className="m-0 select-text text-slate-12">
          {(take.leading_margin_ms / 1000).toFixed(2)} 秒
        </dd>
        <dt>後の余白</dt>
        <dd className="m-0 select-text text-slate-12">
          {(take.trailing_margin_ms / 1000).toFixed(2)} 秒
        </dd>
      </dl>

      {take.invalidated && (
        <p role="alert" className="text-sm text-red-11">
          音が {take.discontinuities} 回とぎれました。この回は使わず、同じ行がもう一度出てきます。
        </p>
      )}

      {!take.has_oto && !take.invalidated && (
        <p className="text-sm text-slate-11">
          声を見つけられませんでした。もう一度録ると、この行から音が取れます。
        </p>
      )}

      <div className="flex flex-wrap gap-2">
        <Button variant="secondary" size="sm" onClick={onListen} disabled={busy}>
          聴く
        </Button>
        <Button variant="secondary" size="sm" onClick={onRetake} disabled={busy}>
          録り直す
        </Button>
        <Button variant="ghost" size="sm" onClick={onOpen}>
          くわしく
        </Button>
      </div>
    </Card>
  );
};
