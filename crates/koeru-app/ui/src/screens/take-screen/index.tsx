import { useMutation, useQuery, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Suspense, useCallback, useState } from "react";

import { Breath } from "~/components/breath";
import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { TakeGenerations } from "~/components/take-generations";
import { TakeValues } from "~/components/take-values";
import { TakeWaveform } from "~/components/take-waveform";
import { api, errorMessage } from "~/lib/ipc";
import { ledgerKey, openProjectQuery, otosQuery, rowsWithTakesQuery } from "~/lib/queries";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * 歌わせる高さ（MIDI）。
 *
 * 「そのまま」を持たない。 素の「歌わせる」が C4 で鳴らすので、
 * **同じ行に同じことをする的が2つ並んでいた**（しかも隣に、別の意味の
 * 「そのまま聴く」がいた）。**踏んだ。** ここは C4 から離す高さだけを置く。
 */
const PITCHES = [
  { midi: 55, label: "低く" },
  { midi: 67, label: "高く" },
] as const;

/** 素の「歌わせる」で鳴らす高さ（MIDI）。C4。 */
const BASE_MIDI = 60;

/** 試唱の長さ（ミリ秒）。 */
const PREVIEW_LENGTH_MS = 800;

/**
 * 録った回（`DEC-PLT-024`）。
 *
 * 詳細の主語はテイク。 収録は行、原音設定は音、進捗も音と、3つの面が
 * 違う単位で同じ音源を語る。テイクを主語に置くと、**1回の録音から取れた音が
 * 波形1枚の上に並ぶ**——単位どうしの関係が同じ絵の中に入る。
 *
 * 2軸で動く。 左が録った回（テイク間）、波形の下が音（単位間）。
 *
 * 原音設定に独立した面を与えない。 ここを開けばそこにある——
 * 工程としての「原音設定をする」がナビゲーションの単位から消える。
 */
export const TakeScreen = () => {
  const navigate = useNavigate();
  const { id, row } = useSearch({ from: "/take" });

  if (id === undefined || row === undefined) {
    return (
      <main className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-3 p-8">
        <p className="text-sm text-slate-11">どの回を開くかが分かりませんでした。</p>
        <Button variant="primary" onClick={() => navigate({ to: "/" })}>
          声へ戻る
        </Button>
      </main>
    );
  }

  return (
    <Suspense
      fallback={
        <main
          role="status"
          className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-3 p-8 text-slate-11"
        >
          <Breath />
          <p className="text-sm text-slate-11">開いています</p>
        </main>
      }
    >
      <OpenTake id={id} rowId={row} />
    </Suspense>
  );
};

/**
 * 音源を開くところまで。
 *
 * **`open_project` と台帳の読みを束ねない。** `useSuspenseQueries` は並行に
 * 投げるので、`rows_with_takes` が開く前に着く——`app.no_project` で落ちるか、
 * **前に開いていた音源の行を、この音源の鍵で溜める。** 境界で分けておけば、
 * 順序が木の形として残る（`react-conventions` の「順に解かせたいものは
 * 部品を分けて境界を挟む」、音源の面は既にそうしている）。
 */
const OpenTake = ({ id, rowId }: { id: string; rowId: string }) => {
  useSuspenseQuery(openProjectQuery(id));
  return <TakeBody id={id} rowId={rowId} />;
};

const TakeBody = ({ id, rowId }: { id: string; rowId: string }) => {
  const navigate = useNavigate();
  const heading = useScreenFocus();
  const queryClient = useQueryClient();

  const { data: rows } = useSuspenseQuery(rowsWithTakesQuery(id));

  const row = rows.find((r) => r.row_id === rowId) ?? null;
  const [shownId, setShownId] = useState<number | null>(row?.adopted ?? null);
  const [selected, setSelected] = useState<string | null>(null);
  const [raw, setRaw] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const fail = useCallback((e: unknown) => setError(errorMessage(e)), []);

  const shown = row?.takes.find((t) => t.take_id === shownId) ?? row?.takes.at(-1) ?? null;

  /*
   * 重ねる目盛りは `useQuery` のまま。
   *
   * 取れなくても波形は読める。 中断させると、これを待つあいだ波形が消える。
   */
  const { data: otos = [] } = useQuery({
    ...otosQuery(shown?.take_id ?? 0),
    enabled: shown !== null,
  });

  const adopt = useMutation({
    mutationFn: (takeId: number) => api.adoptTake(rowId, takeId),
    onMutate: () => setError(null),
    onSuccess: () => queryClient.invalidateQueries({ queryKey: ledgerKey }),
    onError: fail,
  });

  const playRaw = useMutation({
    mutationFn: (takeId: number) => api.playTake(takeId),
    onMutate: () => setError(null),
    onError: fail,
  });

  const sing = useMutation({
    mutationFn: ({ takeId, midi }: { takeId: number; midi: number }) =>
      api.preview({ takeId, midi, lengthMs: PREVIEW_LENGTH_MS }),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 鳴っている音を止める。失敗しても言わない——止まっているのが望みなので。 */
  const stopPreview = useMutation({ mutationFn: () => api.stopPreview() });

  /** いま見ている音。選ぶ前は、この回から取れた先頭。 */
  const activeAlias = selected ?? otos[0]?.alias ?? null;

  const index = rows.findIndex((r) => r.row_id === rowId);
  const prev = index > 0 ? rows[index - 1] : undefined;
  const next = index >= 0 ? rows[index + 1] : undefined;
  const goto = (target: string) => {
    setShownId(null);
    setSelected(null);
    void navigate({ to: "/take", search: { id, row: target } });
  };

  if (row === null) {
    return (
      <main className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-3 p-8">
        <p className="text-sm text-slate-11">その行はこの声にありません。</p>
        <Button
          variant="primary"
          onClick={() => navigate({ to: "/voice", search: { id, tab: "sound" } })}
        >
          音へ戻る
        </Button>
      </main>
    );
  }

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <header className="flex h-16 flex-shrink-0 items-center gap-5 border-slate-6 border-b px-8">
        <Button
          variant="ghost"
          onClick={() => navigate({ to: "/voice", search: { id, tab: "sound" } })}
        >
          <svg
            width="16"
            height="16"
            viewBox="0 0 16 16"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M10 3 5 8l5 5" />
          </svg>
          音へ戻る
        </Button>

        <div className="flex items-baseline gap-3">
          <h1
            ref={heading}
            tabIndex={-1}
            className="select-text text-xl font-semibold text-slate-12 outline-none"
          >
            {row.text}
          </h1>
          <p className="font-mono text-xs text-slate-11 tabular-nums">
            {/*
              採用が無い行がある。 取りこぼしで無効になったテイクは
              `takes` に残るが採用されない（`TR-REC-07`）。番号を数えると
              **「1 回のうち 0 回目を使っています」**になっていた。
            */}
            {row.takes.length === 0
              ? "まだ録っていません"
              : row.adopted === null
                ? `${row.takes.length} 回録ったが、まだどれも使っていません`
                : `${row.takes.length} 回のうち ${row.takes.findIndex((t) => t.take_id === row.adopted) + 1} 回目を使っています`}
          </p>
        </div>

        <div className="ml-auto flex gap-2">
          {prev !== undefined && (
            <Button variant="ghost" size="sm" onClick={() => goto(prev.row_id)}>
              前の行
            </Button>
          )}
          {next !== undefined && (
            <Button variant="ghost" size="sm" onClick={() => goto(next.row_id)}>
              次の行
            </Button>
          )}
        </div>
      </header>

      <div className="flex min-h-0 flex-1 gap-8 p-8">
        <div className="w-72 flex-shrink-0 overflow-y-auto">
          <TakeGenerations
            takes={row.takes}
            adoptedId={row.adopted}
            shownId={shown?.take_id ?? null}
            onShow={(takeId) => {
              setShownId(takeId);
              setSelected(null);
            }}
            onAdopt={(takeId) => adopt.mutate(takeId)}
            /*
              録り直す行を運ぶ。 ただ戻るだけにしていたので、押した行が
              `progress.next_row_id` でないときは**何も起きなかった**
              ——戻った先の「録る」は次の行を録りはじめる。
            */
            onRetake={() => {
              void navigate({ to: "/voice", search: { id, tab: "sound", retake: rowId } });
            }}
            busy={adopt.isPending}
          />
        </div>

        <div className="flex min-h-0 flex-1 flex-col gap-5 overflow-y-auto">
          {shown === null ? (
            <Card title="この行">
              <p className="text-sm text-slate-12">まだ録っていません。音の面で録れます。</p>
            </Card>
          ) : (
            <Card title={`この 1 回から取れた ${otos.length} 音`}>
              <TakeWaveform
                // テイクが変わったら作り直す。描画の途中経過を持ち越さない。
                key={shown.take_id}
                takeId={shown.take_id}
                durationMs={shown.duration_ms}
                peak={shown.peak ?? 0}
                otos={otos}
                selected={activeAlias}
                onSelect={setSelected}
              />

              <hr className="h-px border-0 bg-slate-6" />

              <TakeValues
                otos={otos}
                selected={activeAlias}
                durationMs={shown.duration_ms}
                raw={raw}
                onRaw={setRaw}
              />

              <hr className="h-px border-0 bg-slate-6" />

              <div className="flex flex-wrap items-center gap-2">
                <Button
                  variant="primary"
                  onClick={() => sing.mutate({ takeId: shown.take_id, midi: BASE_MIDI })}
                  disabled={otos.length === 0}
                >
                  歌わせる
                </Button>
                <Button variant="secondary" onClick={() => playRaw.mutate(shown.take_id)}>
                  録った音を聴く
                </Button>
                <Button variant="ghost" onClick={() => stopPreview.mutate()}>
                  止める
                </Button>

                {/*
                  高さは押すたびに鳴るもので、状態ではない。 `Chip` にしない——
                  `aria-pressed` を持つと「いまこの高さが選ばれている」に見える。
                */}
                <span className="ml-auto flex items-center gap-2">
                  <span className="text-xs text-slate-11">別の高さで</span>
                  {PITCHES.map((p) => (
                    <Button
                      key={p.midi}
                      variant="secondary"
                      size="sm"
                      disabled={otos.length === 0}
                      aria-label={`${p.label}歌わせる`}
                      onClick={() => sing.mutate({ takeId: shown.take_id, midi: p.midi })}
                    >
                      {p.label}
                    </Button>
                  ))}
                </span>
              </div>
            </Card>
          )}

          {/*
            まだ決めていないところを、そのまま穴として置く。
            `Q-PLT-004` が閉じるまで、寄る／引くの段と、境界をつかんでいる
            あいだの描き直しは決めない（`DEC-PLT-024`）。
            実測前の推測を構造に焼き付けない。
          */}
          <div className="flex flex-col gap-2 rounded-lg border border-slate-11 border-dashed p-5">
            <p className="text-sm font-semibold text-slate-11">ここは、まだ決めていない</p>
            <p className="text-xs text-slate-11">
              寄る／引くの段と、境界をつかんでいるあいだの描き直し。実測が出るまで決めません。
            </p>
          </div>

          {error !== null && (
            <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
              {error}
            </p>
          )}
        </div>
      </div>
    </main>
  );
};
