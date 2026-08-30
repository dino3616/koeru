import { useNavigate, useSearch } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";

import { LevelMeter } from "~/components/level-meter";
import { Button } from "~/components/ui/button";
import { Card, CardTitle } from "~/components/ui/card";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "~/components/ui/select";
import { Waveform } from "~/components/waveform";
import { cn } from "~/lib/cn";
import { api, type DeviceView, errorMessage, type ProgressView, type TakeView } from "~/lib/ipc";

/** 試唱の音高（MIDI）。C4 = 60。 */
const PREVIEW_PITCHES = [
  { midi: 55, label: "G3" },
  { midi: 60, label: "C4" },
  { midi: 64, label: "E4" },
  { midi: 67, label: "G4" },
  { midi: 72, label: "C5" },
];

/** 試唱の長さ（ミリ秒）。 */
const PREVIEW_LENGTH_MS = 800;

/**
 * 収録画面。**縦切りの本体。**
 *
 * 録る → 波形が出る → その場で歌わせて聴く、までをここで完結させる。
 * **パスを画面に出さない**（TR-PKG-45）。保存先も、ファイル名も見せない。
 */
export const RecordScreen = () => {
  const navigate = useNavigate();
  const { id } = useSearch({ from: "/record" });

  const [devices, setDevices] = useState<DeviceView[]>([]);
  const [deviceId, setDeviceId] = useState<string | undefined>(undefined);
  const [micMode, setMicMode] = useState<string | null>(null);
  const [level, setLevel] = useState(0);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  const [take, setTake] = useState<TakeView | null>(null);
  const [recording, setRecording] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  const startedAt = useRef<number | null>(null);
  const [elapsed, setElapsed] = useState(0);

  const fail = useCallback((e: unknown) => setError(errorMessage(e)), []);

  useEffect(() => {
    if (id === undefined) return;
    api.openProject(id).then(setProgress).catch(fail);
    api.listDevices().then(setDevices).catch(fail);
  }, [id, fail]);

  // 収録中の経過時間。**1秒ごとに読み上げへは流さない**（うるさい）。
  useEffect(() => {
    if (!recording) return;
    const t = window.setInterval(() => {
      if (startedAt.current !== null) {
        setElapsed(Math.floor((performance.now() - startedAt.current) / 1000));
      }
    }, 200);
    return () => window.clearInterval(t);
  }, [recording]);

  const chooseDevice = (next: string) => {
    setDeviceId(next);
    setError(null);
    setStatus("入力を確かめています");
    api
      .armDevice(next)
      .then((mode) => {
        setMicMode(mode);
        return api.probeInput(400);
      })
      .then((peak) => {
        setLevel(peak);
        setStatus(peak > 0.000_001 ? "入力が届いています" : "入力が届いていません");
      })
      .catch(fail);
  };

  const start = () => {
    setError(null);
    setTake(null);
    api
      .startTake()
      .then(() => {
        startedAt.current = performance.now();
        setElapsed(0);
        setRecording(true);
        setStatus("収録中");
      })
      .catch(fail);
  };

  const stop = () => {
    setRecording(false);
    api
      .finishTake()
      .then((t) => {
        setTake(t);
        setStatus(
          t.invalidated
            ? "取りこぼしがあったので、もう一度録ります"
            : t.has_oto
              ? "録れました。音高を選ぶと歌います"
              : "録れましたが、発声を見つけられませんでした",
        );
        return api.progress();
      })
      .then(setProgress)
      .catch(fail);
  };

  const sing = (midi: number) => {
    if (take === null) return;
    setError(null);
    api.preview(take.take_id, midi, PREVIEW_LENGTH_MS).catch(fail);
  };

  // **識別子が無いまま開かれることがある**（殻だけを先に出したときや、
  // 履歴から直接来たとき）。落とさず、戻る道を出す。
  if (id === undefined) {
    return (
      <main className="mx-auto flex h-full max-w-3xl flex-col items-center justify-center gap-4 p-8">
        <p className="text-text-dim">音源が選ばれていません。</p>
        <Button variant="primary" onClick={() => navigate({ to: "/" })}>
          一覧へ戻る
        </Button>
      </main>
    );
  }

  const ready = deviceId !== undefined;

  // **まだ読めていないことと、全部録れたことを混ぜない。**
  // 混ぜると、開いた直後に「全部録れました」と出る。
  const loaded = progress !== null;
  const allDone = loaded && progress.next_row_id === null;
  const pct =
    loaded && progress.required > 0 ? Math.round((progress.covered / progress.required) * 100) : 0;

  return (
    <main className="mx-auto flex h-full max-w-4xl flex-col gap-5 overflow-y-auto p-8">
      <header className="flex items-center justify-between gap-4">
        <div>
          <h1 className="text-xl font-semibold">収録</h1>
          {/* **分母に書き出し・公開・作者を含めない**（TR-PKG-35）。 */}
          <p className="mt-1 font-mono text-sm text-text-dim tabular-nums">
            {loaded ? `${progress.covered} / ${progress.required} 音（${pct}%）` : "読み込み中"}
          </p>
        </div>
        <Button variant="ghost" onClick={() => navigate({ to: "/" })}>
          一覧へ戻る
        </Button>
      </header>

      {/* **状態の変化を支援技術へ通知する**（TR-PLT-29）。 */}
      <p aria-live="polite" className="sr-only">
        {status}
      </p>

      <Card>
        <CardTitle>マイク</CardTitle>
        <div className="mt-3 flex flex-col gap-3">
          {/* **`exactOptionalPropertyTypes` なので、未選択は `value` を渡さない。** */}
          <Select
            {...(deviceId === undefined ? {} : { value: deviceId })}
            onValueChange={chooseDevice}
          >
            <SelectTrigger aria-label="入力デバイス">
              <SelectValue placeholder="入力デバイスを選ぶ" />
            </SelectTrigger>
            <SelectContent>
              {devices.map((d) => (
                <SelectItem key={d.id} value={d.id}>
                  {d.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>

          {ready && <LevelMeter peak={level} />}

          {micMode !== null && micMode !== "Standard" && (
            <p className="rounded-lg bg-surface-2 px-4 py-3 text-sm text-text-dim">
              OS 側の音声処理が入っています（{micMode}）。
              システム設定のマイクモードを「標準」にすると、録った音がそのまま残ります。
            </p>
          )}
        </div>
      </Card>

      <Card>
        <CardTitle>いま録るところ</CardTitle>
        {loaded ? (
          <p
            className={cn(
              "mt-3 select-text text-5xl font-semibold tracking-widest",
              allDone && "text-text-dim",
            )}
          >
            {progress.next_row_text ?? "全部録れました"}
          </p>
        ) : (
          <p className="mt-3 text-5xl font-semibold tracking-widest text-text-dim">…</p>
        )}

        <div className="mt-5 flex items-center gap-3">
          {recording ? (
            <Button variant="danger" size="lg" onClick={stop}>
              止める（{elapsed} 秒）
            </Button>
          ) : (
            <Button
              variant="primary"
              size="lg"
              onClick={start}
              disabled={!ready || !loaded || allDone}
            >
              録る
            </Button>
          )}
          {!ready && <span className="text-sm text-text-dim">先にマイクを選んでください</span>}
        </div>
      </Card>

      {take !== null && (
        <Card>
          <CardTitle>録れたもの</CardTitle>
          <div className="mt-3 flex flex-col gap-4">
            <Waveform peaks={take.thumbnail} peak={take.peak} durationMs={take.duration_ms} />

            {/*
              **取りこぼしたテイクは自動的に無効になる**（TR-REC-07）。
              勧めるのではなく、もう一度同じフレーズが出てくる。
            */}
            {take.invalidated && (
              <p role="alert" className="text-sm text-danger-text">
                取りこぼしが {take.discontinuities} 回ありました。
                このテイクは使わず、同じフレーズをもう一度録ります。
              </p>
            )}

            {/*
              **測った値を出すだけ。評価も警告もしない**（TR-REC-16）。
              「小さすぎます」「歪んでいます」は出さない。
            */}
            <dl className="grid grid-cols-2 gap-x-6 gap-y-1 font-mono text-xs text-text-dim tabular-nums sm:grid-cols-4">
              <div>
                <dt className="inline">ピーク </dt>
                <dd className="inline">
                  {take.peak_dbfs === null ? "—" : `${take.peak_dbfs.toFixed(1)} dBFS`}
                </dd>
              </div>
              <div>
                <dt className="inline">長さ </dt>
                <dd className="inline">{(take.duration_ms / 1000).toFixed(2)} 秒</dd>
              </div>
              <div>
                <dt className="inline">前の余白 </dt>
                <dd className="inline">{Math.round(take.leading_margin_ms)} ms</dd>
              </div>
              <div>
                <dt className="inline">後の余白 </dt>
                <dd className="inline">{Math.round(take.trailing_margin_ms)} ms</dd>
              </div>
            </dl>

            {take.has_oto ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm text-text-dim">歌わせる:</span>
                {PREVIEW_PITCHES.map((p) => (
                  <Button key={p.midi} onClick={() => sing(p.midi)}>
                    {p.label}
                  </Button>
                ))}
                <Button variant="ghost" onClick={() => api.stopPreview().catch(fail)}>
                  止める
                </Button>
              </div>
            ) : (
              <p className="text-sm text-text-dim">
                発声を見つけられませんでした。もう一度録ってみてください。
              </p>
            )}

            {take.confidence !== null && (
              <p className="font-mono text-xs text-text-dim tabular-nums">
                境界の確信度 {(take.confidence * 100).toFixed(0)}%
              </p>
            )}
          </div>
        </Card>
      )}

      {error !== null && (
        <p role="alert" className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text">
          {error}
        </p>
      )}
    </main>
  );
};
