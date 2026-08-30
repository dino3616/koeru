import { useNavigate, useSearch } from "@tanstack/react-router";
import { useCallback, useEffect, useRef, useState } from "react";

import { CalibrationCard } from "~/components/calibration-card";
import { LeakCard } from "~/components/leak-card";
import { SongList } from "~/components/song-list";
import { TakeInspector } from "~/components/take-inspector";
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
import { cn } from "~/lib/cn";
import {
  api,
  type DeviceView,
  errorMessage,
  type ProgressView,
  type SpaceView,
  type TakeView,
} from "~/lib/ipc";

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
  const [space, setSpace] = useState<SpaceView | null>(null);
  const [status, setStatus] = useState("");
  // 連続収録（TR-REC-20）。**発話の検出結果を条件にしない。固定長で進む。**
  const [continuous, setContinuous] = useState(false);
  const [advanceMs, setAdvanceMs] = useState(3000);
  const continuing = useRef(false);
  const [leaking, setLeaking] = useState<boolean | null>(null);
  const startedAt = useRef<number | null>(null);
  const [elapsed, setElapsed] = useState(0);

  const fail = useCallback((e: unknown) => setError(errorMessage(e)), []);

  useEffect(() => {
    if (id === undefined) return;
    api.openProject(id).then(setProgress).catch(fail);
    api.listDevices().then(setDevices).catch(fail);
    api.autoAdvanceMs().then(setAdvanceMs).catch(fail);
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
        return api.estimateSpace();
      })
      .then(setSpace)
      .catch(fail);
  };

  /** 1フレーズ録って確定させる。**連続収録もここを繰り返す。** */
  const recordOnce = async (holdMs: number) => {
    setTake(null);
    await api.startTake();
    startedAt.current = performance.now();
    setElapsed(0);
    setRecording(true);
    setStatus("収録中");

    await new Promise((r) => setTimeout(r, holdMs));

    setRecording(false);
    const t = await api.finishTake();
    setTake(t);
    setStatus(
      t.invalidated
        ? "取りこぼしがあったので、もう一度録ります"
        : t.has_oto
          ? "録れました。音高を選ぶと歌います"
          : "録れましたが、発声を見つけられませんでした",
    );
    setProgress(await api.progress());
    return t;
  };

  const start = () => {
    setError(null);
    recordOnce(advanceMs).catch(fail);
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

  /**
   * 連続収録（TR-REC-20）。
   *
   * **止めたフレーズは未収録のまま残る。** 途中で抜けても、続きから再開できる。
   * フレーズの間もストリームは止めないので、プリロールは保たれる（TR-REC-19）。
   */
  const runContinuous = async () => {
    continuing.current = true;
    setContinuous(true);
    try {
      while (continuing.current) {
        const p = await api.progress();
        if (p.next_row_id === null) break;
        await recordOnce(advanceMs);
        // フレーズ間の間。**声を出し終える時間を残す。**
        await new Promise((r) => setTimeout(r, 400));
      }
    } catch (e) {
      fail(e);
    } finally {
      continuing.current = false;
      setContinuous(false);
      setStatus("連続収録を止めました");
    }
  };

  const pauseContinuous = () => {
    continuing.current = false;
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
          {/*
            **カバレッジと「いま歌える曲の数」を常時両方出す。どちらかを隠さない**
            （TR-RCL-19）。カバレッジは単位の被覆率で、行の消化率ではない——
            行数は本人の作業量、単位の被覆は音源の到達度で、意味が違う。
          */}
          <p className="mt-1 font-mono text-sm text-text-dim tabular-nums">
            {loaded ? (
              <>
                {progress.covered} / {progress.required} 音（{pct}%）
                {progress.songs_in_bank > 0 && (
                  <>
                    {" · "}
                    {progress.singable_songs} / {progress.songs_in_bank} 曲が歌える
                  </>
                )}
              </>
            ) : (
              "読み込み中"
            )}
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

          {/*
            **残量が足りないときは「その残量で何件録れるか」を出す**（TR-REC-41）。
            「足りません」だけでは、何を削れば足りるのか分からない。
          */}
          {space !== null && !space.sufficient && (
            <p
              role="alert"
              className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text"
            >
              保存先の残量では、残り {space.remaining_rows} 件のうち {space.rows_that_fit}{" "}
              件までしか録れません。
            </p>
          )}

          {micMode !== null && micMode !== "Standard" && (
            <p className="rounded-lg bg-surface-2 px-4 py-3 text-sm text-text-dim">
              OS 側の音声処理が入っています（{micMode}）。
              システム設定のマイクモードを「標準」にすると、録った音がそのまま残ります。
            </p>
          )}
        </div>
      </Card>

      <CalibrationCard ready={ready} onStatus={setStatus} />

      <LeakCard
        ready={ready}
        midi={PREVIEW_PITCHES[1]?.midi ?? 60}
        onStatus={setStatus}
        onChecked={setLeaking}
      />

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

        <div className="mt-5 flex flex-wrap items-center gap-3">
          {recording ? (
            <Button variant="danger" size="lg" onClick={stop}>
              止める（{elapsed} 秒）
            </Button>
          ) : (
            <Button
              variant="primary"
              size="lg"
              onClick={start}
              disabled={!ready || !loaded || allDone || continuous}
            >
              録る
            </Button>
          )}

          {/*
            **連続収録**（TR-REC-20）。1フレーズ {advanceMs}ms の固定長で進む。
            発話の検出結果を条件にしない。
          */}
          {continuous ? (
            <Button variant="secondary" size="lg" onClick={pauseContinuous}>
              続けて録るのをやめる
            </Button>
          ) : (
            <Button
              size="lg"
              onClick={() => {
                setError(null);
                void runContinuous();
              }}
              disabled={!ready || !loaded || allDone || recording}
            >
              続けて録る
            </Button>
          )}

          {/* **音高提示は回り込みが無いときだけ**（TR-REC-24）。 */}
          {leaking === false && (
            <Button
              variant="ghost"
              onClick={() => {
                api.playPitch(PREVIEW_PITCHES[1]?.midi ?? 60).catch(fail);
              }}
              disabled={recording || continuous}
            >
              音高を聞く
            </Button>
          )}

          {!ready && <span className="text-sm text-text-dim">先にマイクを選んでください</span>}
        </div>

        {continuous && (
          <p className="mt-3 text-sm text-text-dim">
            1フレーズ {(advanceMs / 1000).toFixed(1)} 秒で自動的に次へ進みます。
            やめたフレーズは未収録のまま残ります。
          </p>
        )}
      </Card>

      {take !== null && (
        <Card>
          <CardTitle>録れたもの</CardTitle>
          <div className="mt-3 flex flex-col gap-4">
            {/*
              **アプリが所有する単一の描画面へ直接描く**（TR-PLT-04）。
              可視域のみ計算し、可視域のみ描く。
            */}
            <TakeInspector takeId={take.take_id} durationMs={take.duration_ms} peak={take.peak} />

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

      <SongList revision={progress?.covered ?? 0} />

      {error !== null && (
        <p role="alert" className="rounded-lg bg-danger-surface px-4 py-3 text-sm text-danger-text">
          {error}
        </p>
      )}
    </main>
  );
};
