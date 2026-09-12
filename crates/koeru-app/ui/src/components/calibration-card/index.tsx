import { useMutation } from "@tanstack/react-query";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { api, errorMessage } from "~/lib/ipc";

/** 校正に使う発声の長さ（秒）。3〜5秒（`TR-REC-14`）。 */
const SECONDS = 4;

type CalibrationCardProps = {
  /** マイクを選べているか。 */
  ready: boolean;
  onStatus: (message: string) => void;
};

/**
 * 録るときの大きさを合わせる（`TR-REC-14`、`TR-REC-15`）。
 *
 * 関門にしない。 合わなくても録りはじめられる。
 * 3時間の収録の前に、レベル合わせで止められる方がよほど困る。
 *
 * ここだけは目標の範囲を持つ。 `Q-REC-004` で入力レベルの区分は外したが、
 * `TR-REC-14` の校正は「目標範囲へ寄せる」工程として定義されているので、
 * 範囲を示すのは判定ではなく手順。合ったかどうかも工程の結果として出す。
 *
 * 良し悪しは言わない（`TR-REC-16`）。 出すのは測った値と、次に何をすればよいか。
 */
export const CalibrationCard = ({ ready, onStatus }: CalibrationCardProps) => {
  /*
   * 押して初めて走るものは `useMutation`。
   *
   * 「走っている最中か」「結果」「失敗」を state で3つ持つと、
   * どれか1つを消し忘れる——成功したのに前の赤字が残る形が実際に出た。
   */
  const run = useMutation({
    mutationFn: () => api.calibrate(SECONDS),
    onMutate: () => onStatus(`${SECONDS} 秒間、いちばん高い音で声を出してください`),
    onSuccess: (c) => onStatus(c.settled ? "大きさが合いました" : "このまま進みます"),
  });

  // 前回と違うゲインで開いたか（`TR-REC-15`）。勝手に戻さない。
  //
  // 失敗は出さない。 これは補助で、読めなければ「差は無い」として進める。
  const drift = useMutation({ mutationFn: () => api.gainDrift() });

  const restore = useMutation({
    mutationFn: () => api.restoreSavedGain(),
    onSuccess: () => {
      drift.reset();
      onStatus("前回の大きさへ戻しました");
    },
  });

  const result = run.data ?? null;
  const error = run.error ?? restore.error;

  return (
    <Card title="録るときの大きさ">
      <p className="text-sm text-slate-12">
        いちばん高い音の全力発声を {SECONDS} 秒録って、初期値を合わせます。
        合わせなくても録りはじめられます。
      </p>

      <div className="flex flex-wrap gap-2">
        <Button
          variant="secondary"
          size="sm"
          onClick={() => run.mutate()}
          disabled={!ready || run.isPending}
        >
          {run.isPending ? `録っています（${SECONDS} 秒）` : "合わせる"}
        </Button>
        <Button variant="ghost" size="sm" onClick={() => drift.mutate()} disabled={!ready}>
          前回との差を見る
        </Button>
      </div>

      {result !== null && (
        <>
          <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 font-mono text-xs text-slate-11 tabular-nums">
            <dt>いちばん大きいところ</dt>
            <dd className="m-0 text-slate-12">
              {result.peak_dbfs === null ? "—" : `${result.peak_dbfs.toFixed(1)} dBFS`}
            </dd>
            <dt>マイクの入力量</dt>
            <dd className="m-0 text-slate-12">
              {result.gain === null ? "—" : `${Math.round(result.gain * 100)}%`}
            </dd>
          </dl>

          {/*
            ハードウェア以外では自動調整しない（`TR-REC-14`）。
            ソフトウェアのボリュームを上げても A/D の手前は変わらない。
          */}
          {result.control !== "Hardware" && (
            <p className="rounded-lg bg-slate-3 px-3 py-2 text-sm text-slate-12">
              このマイクの入力量は KOERU からは動かせません。
              {result.control === "Software" &&
                "音量つまみがソフトウェア側にあるので、上げても録れる音は変わりません。"}
              システム設定 → サウンド → 入力 で調整してください。
            </p>
          )}
        </>
      )}

      {drift.data != null && (
        <div className="flex flex-wrap items-center gap-2 rounded-lg bg-slate-3 px-3 py-2">
          <span className="text-sm text-slate-12">
            前回は <span className="font-mono tabular-nums">{Math.round(drift.data[0] * 100)}</span>
            %、いまは{" "}
            <span className="font-mono tabular-nums">{Math.round(drift.data[1] * 100)}</span>%
            です。
          </span>
          <Button size="sm" onClick={() => restore.mutate()} disabled={restore.isPending}>
            前回へ戻す
          </Button>
          <Button variant="ghost" size="sm" onClick={() => drift.reset()}>
            このまま
          </Button>
        </div>
      )}

      {error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(error)}
        </p>
      )}
    </Card>
  );
};
