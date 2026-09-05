import { useState } from "react";

import { Button } from "~/components/ui/button";
import { Card } from "~/components/ui/card";
import { type CalibrationView, api, errorMessage } from "~/lib/ipc";

/** 校正に使う発声の長さ（秒）。3〜5秒（`TR-REC-14`）。 */
const SECONDS = 4;

type CalibrationCardProps = {
  /** マイクを選んでいるか。 */
  ready: boolean;
  onStatus: (message: string) => void;
};

/**
 * 入力レベルの校正（`TR-REC-14`、`TR-REC-15`）。
 *
 * 関門にしない。 収束しなくても収録に進める。
 * 3時間の収録の前に、レベル合わせで止められる方がよほど困る。
 *
 * 「小さすぎます」「歪んでいます」は出さない（`TR-REC-16`）。
 * 出すのは測った値と、次に何をすればよいかだけ。
 */
export const CalibrationCard = ({ ready, onStatus }: CalibrationCardProps) => {
  const [running, setRunning] = useState(false);
  const [result, setResult] = useState<CalibrationView | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [drift, setDrift] = useState<[number, number] | null>(null);

  const run = () => {
    setRunning(true);
    setError(null);
    onStatus(`${SECONDS} 秒間、いちばん高い音で声を出してください`);
    api
      .calibrate(SECONDS)
      .then((c) => {
        setResult(c);
        onStatus(c.settled ? "レベルが合いました" : "レベルはこのまま進みます");
      })
      .catch((e: unknown) => setError(errorMessage(e)))
      .finally(() => setRunning(false));
  };

  // 前回と違うゲインで開いたか（`TR-REC-15`）。勝手に戻さない。
  const checkDrift = () => {
    api
      .gainDrift()
      .then(setDrift)
      .catch(() => setDrift(null));
  };

  return (
    <Card title="入力レベル">
      <div className="mt-3 flex flex-col gap-3">
        <p className="text-sm text-slate-11">
          いちばん高い音の全力発声を {SECONDS} 秒録って、初期値を合わせます。
          <br />
          合わなくても収録には進めます。
        </p>

        <div className="flex items-center gap-3">
          <Button variant="secondary" onClick={run} disabled={!ready || running}>
            {running ? `録っています（${SECONDS} 秒）` : "レベルを合わせる"}
          </Button>
          <Button variant="ghost" onClick={checkDrift} disabled={!ready}>
            前回との差を見る
          </Button>
        </div>

        {result !== null && (
          <div className="flex flex-col gap-2">
            <dl className="flex flex-wrap gap-x-6 font-mono text-xs text-slate-11 tabular-nums">
              <div>
                <dt className="inline">ピーク </dt>
                <dd className="inline">
                  {result.peak_dbfs === null ? "—" : `${result.peak_dbfs.toFixed(1)} dBFS`}
                </dd>
              </div>
              <div>
                <dt className="inline">ゲイン </dt>
                <dd className="inline">
                  {result.gain === null ? "—" : `${Math.round(result.gain * 100)}%`}
                </dd>
              </div>
            </dl>

            {/*
              ハードウェア以外では自動調整しない（TR-REC-14）。
              ソフトウェアのボリュームを上げても A/D の手前は変わらない。
            */}
            {result.control !== "Hardware" && (
              <p className="rounded-lg bg-slate-3 px-4 py-3 text-sm text-slate-11">
                このマイクのゲインは KOERU からは動かせません。
                {result.control === "Software"
                  ? "音量つまみがソフトウェア側にあるため、上げても録れる音の質は変わりません。"
                  : ""}
                <br />
                システム設定 → サウンド → 入力 で調整してください。
              </p>
            )}
          </div>
        )}

        {drift !== null && (
          <div className="flex flex-wrap items-center gap-3 rounded-lg bg-slate-3 px-4 py-3 text-sm">
            <span className="text-slate-11">
              前回は {Math.round(drift[0] * 100)}%、いまは {Math.round(drift[1] * 100)}% です。
            </span>
            <Button
              onClick={() => {
                api
                  .restoreSavedGain()
                  .then(() => {
                    setDrift(null);
                    onStatus("前回のレベルへ戻しました");
                  })
                  .catch((e: unknown) => setError(errorMessage(e)));
              }}
            >
              前回へ戻す
            </Button>
            <Button variant="ghost" onClick={() => setDrift(null)}>
              このまま
            </Button>
          </div>
        )}

        {error !== null && (
          <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
            {error}
          </p>
        )}
      </div>
    </Card>
  );
};
