import { useNavigate, useSearch } from "@tanstack/react-router";
import { useCallback, useEffect, useState } from "react";

import { SongList } from "~/components/song-list";
import { TakeList } from "~/components/take-list";
import { TakeInspector } from "~/components/take-inspector";
import { Button } from "~/components/ui/button";
import { Card } from "~/components/ui/card";
import { Elapsed } from "~/components/elapsed";
import { Spinner } from "~/components/spinner";
import { InputSetup } from "~/components/input-setup";
import { cx } from "~/lib/tv";
import { useScreenFocus } from "~/lib/use-screen-focus";
import { useRecorder } from "~/lib/use-recorder";
import { api, errorMessage, type ProgressView } from "~/lib/ipc";

/** 試唱の基準音（MIDI）。C4。フォールバックもここを参照する。 */
const BASE_MIDI = 60;

/** 試唱の音高（MIDI）。C4 = 60。 */
const PREVIEW_PITCHES = [
  { midi: 55, label: "G3" },
  { midi: BASE_MIDI, label: "C4" },
  { midi: 64, label: "E4" },
  { midi: 67, label: "G4" },
  { midi: 72, label: "C5" },
] as const;

/** 試唱の長さ（ミリ秒）。 */
const PREVIEW_LENGTH_MS = 800;

/**
 * 収録画面。縦切りの本体。
 *
 * 録る → 波形が出る → その場で歌わせて聴く、までをここで完結させる。
 * パスを画面に出さない（`TR-PKG-45`）。保存先も、ファイル名も見せない。
 */
export const RecordScreen = () => {
  const navigate = useNavigate();
  const heading = useScreenFocus();
  const { id } = useSearch({ from: "/record" });

  const [deviceId, setDeviceId] = useState<string | undefined>(undefined);
  const [progress, setProgress] = useState<ProgressView | null>(null);
  /**
   * 台帳が変わるたびに増やす。
   *
   * カバレッジでは代用できない。 採用テイクを切り替えても、
   * 録り直しても、カバレッジは変わらない（`TR-RCL-25`）。
   * それを鍵にすると、一覧が更新されない。
   */
  const [revision, setRevision] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  /** 回り込みの確認結果。音高提示を鳴らしてよいかを決める（`TR-REC-24`）。 */
  const [leaking, setLeaking] = useState<boolean | null>(null);

  const fail = useCallback((e: unknown) => setError(errorMessage(e)), []);

  /** 確定したら、進み具合と一覧を同時に進める。片方だけ動くと数が合わない。 */
  const onSettled = useCallback(({ progress: p }: { progress: ProgressView }) => {
    setProgress(p);
    setRevision((n) => n + 1);
  }, []);

  const {
    take,
    recording,
    settling,
    continuous,
    advanceMs,
    start,
    stop,
    retake,
    runContinuous,
    pauseContinuous,
  } = useRecorder({ onSettled, onStatus: setStatus, onError: fail });

  /** デバイスを選べているか。選ぶまでは録らせない。 */
  const ready = deviceId !== undefined;

  useEffect(() => {
    if (id === undefined) return;
    api.openProject(id).then(setProgress).catch(fail);
  }, [id, fail]);

  /** 録れたものをそのまま鳴らす（`TR-REC-43`）。 */
  const playRaw = (takeId: number) => {
    setError(null);
    api.playTake(takeId).catch(fail);
  };

  const sing = (midi: number) => {
    if (take === null) return;
    setError(null);
    api.preview({ takeId: take.take_id, midi, lengthMs: PREVIEW_LENGTH_MS }).catch(fail);
  };

  // 識別子が無いまま開かれることがある（殻だけを先に出したときや、
  // 履歴から直接来たとき）。落とさず、戻る道を出す。
  if (id === undefined) {
    return (
      <main className="mx-auto flex h-full max-w-3xl flex-col items-center justify-center gap-4 p-8">
        <p className="text-slate-11">音源が選ばれていません。</p>
        <Button variant="primary" onClick={() => navigate({ to: "/" })}>
          一覧へ戻る
        </Button>
      </main>
    );
  }

  // まだ読めていないことと、全部録れたことを混ぜない。
  // 混ぜると、開いた直後に「全部録れました」と出る。
  const loaded = progress !== null;
  const allDone = loaded && progress.next_row_id === null;
  const pct =
    loaded && progress.required > 0 ? Math.round((progress.covered / progress.required) * 100) : 0;

  return (
    <main className="mx-auto flex h-full max-w-4xl flex-col gap-5 overflow-y-auto p-8">
      <header className="flex items-center justify-between gap-4">
        <div>
          <h1 ref={heading} tabIndex={-1} className="text-xl font-semibold outline-none">
            収録
          </h1>
          {/* 分母に書き出し・公開・作者を含めない（`TR-PKG-35`）。 */}
          {/*
            カバレッジと「いま歌える曲の数」を常時両方出す。どちらかを隠さない
            （TR-RCL-19）。カバレッジは単位の被覆率で、行の消化率ではない——
            行数は本人の作業量、単位の被覆は音源の到達度で、意味が違う。
          */}
          <p className="mt-1 font-mono text-sm text-slate-11 tabular-nums">
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

      {/* 状態の変化を支援技術へ通知する（`TR-PLT-29`）。 */}
      <p aria-live="polite" aria-atomic="true" className="sr-only">
        {status}
      </p>

      <InputSetup
        deviceId={deviceId}
        onDeviceChange={(next) => {
          setDeviceId(next);
          setError(null);
        }}
        guideMidi={PREVIEW_PITCHES[1]?.midi ?? BASE_MIDI}
        onStatus={setStatus}
        onError={fail}
        onLeakChecked={setLeaking}
      />

      <Card title="いま録るところ">
        {loaded ? (
          <p
            className={cx(
              "mt-3 select-text text-5xl font-semibold tracking-widest",
              allDone && "text-slate-11",
            )}
          >
            {progress.next_row_text ?? "全部録れました"}
          </p>
        ) : (
          <p className="mt-3 text-5xl font-semibold tracking-widest text-slate-11">…</p>
        )}

        <div className="mt-5 flex flex-wrap items-center gap-3">
          {recording ? (
            <Button variant="danger" size="lg" onClick={stop} aria-label="止める">
              {/*
                経過秒は名前に入れない。入れるとフォーカス中の要素の
                accessible name が毎秒書き換わり、読み上げが追えなくなる。
              */}
              止める
              <Elapsed />
            </Button>
          ) : settling ? (
            /*
              確定の間（`finish_take`）。解析とアライメントを含むので数秒かかる。
              押せる的を出さない。 ここで「録る」を出すと、確定の途中で
              次を始めさせてしまう。
            */
            <Button variant="primary" size="lg" disabled>
              <Spinner />
              確かめています
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
            連続収録（TR-REC-20）。1フレーズ {advanceMs}ms の固定長で進む。
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

          {/* 音高提示は回り込みが無いときだけ（`TR-REC-24`）。 */}
          {leaking === false && (
            <Button
              variant="ghost"
              onClick={() => {
                api.playPitch(PREVIEW_PITCHES[1]?.midi ?? BASE_MIDI).catch(fail);
              }}
              disabled={recording || continuous}
            >
              音高を聞く
            </Button>
          )}

          {!ready && <span className="text-sm text-slate-11">先にマイクを選んでください</span>}
        </div>

        {continuous ? (
          <p className="mt-3 text-sm text-slate-11">
            1フレーズ {(advanceMs / 1000).toFixed(1)} 秒で自動的に次へ進みます。
            やめたフレーズは未収録のまま残ります。
          </p>
        ) : (
          <p className="mt-3 text-sm text-slate-11">
            {recording
              ? "言い終えたら「止める」を押してください。押した 0.5 秒あとまで録ります。"
              : "「録る」は止めるまで録り続けます。押した 0.5 秒前から録れています。"}
          </p>
        )}
      </Card>

      {take !== null && (
        <Card title={recording || continuous ? "ひとつ前に録れたもの" : "録れたもの"}>
          <div className="mt-3 flex flex-col gap-4">
            {/*
              アプリが所有する単一の描画面へ直接描く（TR-PLT-04）。
              可視域のみ計算し、可視域のみ描く。
            */}
            <TakeInspector
              // テイクが変わったら作り直す。範囲や描画の途中経過を持ち越さない。
              key={take.take_id}
              takeId={take.take_id}
              durationMs={take.duration_ms}
              peak={take.peak}
            />

            {/*
              取りこぼしたテイクは自動的に無効になる（TR-REC-07）。
              勧めるのではなく、もう一度同じフレーズが出てくる。
            */}
            {take.invalidated && (
              <p role="alert" className="text-sm text-red-11">
                取りこぼしが {take.discontinuities} 回ありました。
                このテイクは使わず、同じフレーズをもう一度録ります。
              </p>
            )}

            {/*
              測った値を出すだけ。評価も警告もしない（TR-REC-16）。
              「小さすぎます」「歪んでいます」は出さない。
            */}
            <dl className="grid select-text grid-cols-2 gap-x-6 gap-y-1 font-mono text-xs text-slate-11 tabular-nums sm:grid-cols-4">
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

            {/*
              そのまま聴く（TR-REC-43）。試唱は代わりにならない——
              試唱は oto で切り出して目標音高へ寄せた音なので、
              素材が無音なのか合成が失敗したのかを区別できない。
            */}
            <div className="flex flex-wrap items-center gap-2">
              <span className="text-sm text-slate-11">録れた音:</span>
              <Button variant="secondary" onClick={() => playRaw(take.take_id)}>
                そのまま聴く
              </Button>
              <Button variant="ghost" onClick={() => api.stopPreview().catch(fail)}>
                止める
              </Button>
            </div>

            {take.has_oto ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm text-slate-11">歌わせる:</span>
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
              <p className="text-sm text-slate-11">
                発声を見つけられませんでした。もう一度録ってみてください。
              </p>
            )}

            {take.confidence !== null && (
              <p className="font-mono text-xs text-slate-11 tabular-nums">
                境界の確信度 {(take.confidence * 100).toFixed(0)}%
              </p>
            )}
          </div>
        </Card>
      )}

      {/*
        プロジェクトが開くまで、台帳を読む子を出さない。
        React は Effect を子から先に流すので、出しておくと `open_project` より先に
        問い合わせて `app.no_project` を受ける。`revision` は最初のテイクまで
        増えないので、そのエラーはそれまで消えない。
      */}
      {loaded && (
        <>
          <TakeList
            revision={revision}
            busy={recording || continuous}
            onRetake={retake}
            onPlay={playRaw}
          />

          <SongList revision={revision} />
        </>
      )}

      {error !== null && (
        <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
          {error}
        </p>
      )}
    </main>
  );
};
