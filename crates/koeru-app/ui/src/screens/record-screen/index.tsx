import {
  useMutation,
  useQueryClient,
  useSuspenseQueries,
  useSuspenseQuery,
} from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Suspense, useCallback, useState } from "react";

import { SongList } from "~/components/song-list";
import { CardSkeleton } from "~/components/card-skeleton";
import { TakeList } from "~/components/take-list";
import { TakeInspector } from "~/components/take-inspector";
import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { Elapsed } from "~/components/elapsed";
import { Spinner } from "~/components/spinner";
import { InputSetup } from "~/components/input-setup";
import { cx } from "~/lib/tv";
import { useScreenFocus } from "~/lib/use-screen-focus";
import { useRecorder } from "~/lib/use-recorder";
import { api, errorMessage, type ProgressView } from "~/lib/ipc";
import { autoAdvanceQuery, ledgerKey, openProjectQuery, progressQuery } from "~/lib/queries";

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
 *
 * 開くまでと開いたあとを別の部品に分ける。 `open_project` を先に
 * 済ませないと、台帳を読む子が `app.no_project` を受ける。順番を
 * フックの並び順に頼らない——並べ替えても型は通り、`app.no_project` が
 * 出て初めて分かる。境界で分けておけば、順序が木の形として残る。
 */
export const RecordScreen = () => {
  const navigate = useNavigate();
  const { id } = useSearch({ from: "/record" });

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

  return (
    <Suspense
      fallback={
        <main
          role="status"
          className="mx-auto flex h-full max-w-3xl flex-col items-center justify-center gap-3 p-8"
        >
          <Spinner />
          <p className="text-slate-11">読み込み中</p>
        </main>
      }
    >
      <RecordSession id={id} />
    </Suspense>
  );
};

/**
 * プロジェクトを開くところまで。
 *
 * 子を返すだけの層に見えるが、中断が解けるまで子は描かれない——React は
 * 親が中断した時点で降りるのをやめる。これが「開いてから読む」の保証になっている。
 *
 * 1フレーズの長さはここで一緒に取る。 開くのとは関係が無いので、
 * `useSuspenseQueries` で束ねて並行に投げる。`useSuspenseQuery` を2つ並べると
 * 順に取りに行く（`EVID-PLT-001` で実測）。
 */
const RecordSession = ({ id }: { id: string }) => {
  const [, advance] = useSuspenseQueries({
    queries: [openProjectQuery(id), autoAdvanceQuery()],
  });
  return <RecordBody advanceMs={advance.data} />;
};

/** 開いたあとの収録画面。 */
const RecordBody = ({ advanceMs }: { advanceMs: number }) => {
  const navigate = useNavigate();
  const heading = useScreenFocus();
  const queryClient = useQueryClient();

  const { data: progress } = useSuspenseQuery(progressQuery());

  const [deviceId, setDeviceId] = useState<string | undefined>(undefined);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  /** 回り込みの確認結果。音高提示を鳴らしてよいかを決める（`TR-REC-24`）。 */
  const [leaking, setLeaking] = useState<boolean | null>(null);

  const fail = useCallback((e: unknown) => setError(errorMessage(e)), []);

  /**
   * 確定したら、進み具合と一覧を同時に進める。片方だけ動くと数が合わない。
   *
   * 進み具合は確定が返した値をそのまま書く。 無効化だけにすると、
   * 取り直しが返るまで数字が一拍遅れて動く——連続収録では数秒ごとに起きる。
   * 書いたうえで台帳全体を無効化し、一覧を取り直させる。進み具合も
   * 同じ鍵の下なので取り直されるが、返るのは今書いたのと同じ値になる。
   *
   * カバレッジでは代用できない。 採用テイクを切り替えても録り直しても、
   * カバレッジは変わらない（`TR-RCL-25`）。
   */
  const onSettled = useCallback(
    ({ progress: p }: { progress: ProgressView }) => {
      queryClient.setQueryData(progressQuery().queryKey, p);
      void queryClient.invalidateQueries({ queryKey: ledgerKey });
    },
    [queryClient],
  );

  const {
    take,
    recording,
    settling,
    continuous,
    start,
    stop,
    retake,
    runContinuous,
    pauseContinuous,
  } = useRecorder({
    advanceMs,
    onSettled,
    onStatus: setStatus,
    onError: fail,
    // 録り直すときに前の失敗を消す。残すと、直ったのに直っていないように見える。
    onRetry: useCallback(() => setError(null), []),
  });

  /** デバイスを選べているか。選ぶまでは録らせない。 */
  const ready = deviceId !== undefined;

  /*
   * 鳴らす操作はどれも `useMutation`。
   *
   * `.catch(fail)` を手で書くのと同じことをしているように見えるが、
   * 前の失敗を消すのが `onMutate` に寄るので、書き忘れる場所が無くなる。
   * 押した順に走るだけのもので、取り直しも重複排除も要らない。
   */
  const playRaw = useMutation({
    mutationFn: (takeId: number) => api.playTake(takeId),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 録れたものを、目標の音高で歌わせる。 */
  const sing = useMutation({
    mutationFn: ({ takeId, midi }: { takeId: number; midi: number }) =>
      api.preview({ takeId, midi, lengthMs: PREVIEW_LENGTH_MS }),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 音高提示（`TR-REC-24`）。回り込みが無いときだけ出す。 */
  const playPitch = useMutation({
    mutationFn: (midi: number) => api.playPitch(midi),
    onMutate: () => setError(null),
    onError: fail,
  });

  const stopPreview = useMutation({ mutationFn: () => api.stopPreview(), onError: fail });

  const allDone = progress.next_row_id === null;
  const pct = progress.required > 0 ? Math.round((progress.covered / progress.required) * 100) : 0;

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
            {progress.covered} / {progress.required} 音（{pct}%）
            {progress.songs_in_bank > 0 && (
              <>
                {" · "}
                {progress.singable_songs} / {progress.songs_in_bank} 曲が歌える
              </>
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

      <Suspense fallback={<CardSkeleton title="マイク" />}>
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
      </Suspense>

      <Card title="いま録るところ">
        <p
          className={cx(
            "mt-3 select-text text-5xl font-semibold tracking-widest",
            allDone && "text-slate-11",
          )}
        >
          {progress.next_row_text ?? "全部録れました"}
        </p>

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
              disabled={!ready || allDone || continuous}
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
              disabled={!ready || allDone || recording}
            >
              続けて録る
            </Button>
          )}

          {/* 音高提示は回り込みが無いときだけ（`TR-REC-24`）。 */}
          {leaking === false && (
            <Button
              variant="ghost"
              onClick={() => playPitch.mutate(PREVIEW_PITCHES[1]?.midi ?? BASE_MIDI)}
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
              <Button variant="secondary" onClick={() => playRaw.mutate(take.take_id)}>
                そのまま聴く
              </Button>
              <Button variant="ghost" onClick={() => stopPreview.mutate()}>
                止める
              </Button>
            </div>

            {take.has_oto ? (
              <div className="flex flex-wrap items-center gap-2">
                <span className="text-sm text-slate-11">歌わせる:</span>
                {PREVIEW_PITCHES.map((p) => (
                  <Button
                    key={p.midi}
                    onClick={() => sing.mutate({ takeId: take.take_id, midi: p.midi })}
                  >
                    {p.label}
                  </Button>
                ))}
                <Button variant="ghost" onClick={() => stopPreview.mutate()}>
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
        外枠を先に出す。 中身の取得を待たせない
        （`async-suspense-boundaries`）。失敗は上の `ErrorBoundary` が受ける。
      */}
      <Suspense fallback={<CardSkeleton title="録れたもの一覧" />}>
        <TakeList
          busy={recording || continuous}
          onRetake={retake}
          onPlay={(takeId) => playRaw.mutate(takeId)}
        />
      </Suspense>

      <Suspense fallback={<CardSkeleton title="歌える曲" />}>
        <SongList />
      </Suspense>

      {error !== null && (
        <p role="alert" className="rounded-lg bg-red-3 px-4 py-3 text-sm text-red-11">
          {error}
        </p>
      )}
    </main>
  );
};
