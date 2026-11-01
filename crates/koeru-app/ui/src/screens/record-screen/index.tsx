import { useMutation, useQuery, useQueryClient, useSuspenseQueries } from "@tanstack/react-query";
import { useSearch } from "@tanstack/react-router";
import { Suspense, useState } from "react";

import { ShellToolbarEnd, useSidebarCollapsedWhile } from "~/components/app-shell";
import { Breath } from "~/components/breath";
import { RecordSidebar } from "~/components/record-sidebar";
import { rowItem, songItem } from "~/components/record-sidebar/from-views";
import { RecordStage, type StagePhase } from "~/components/record-stage";
import { RecordToolbar } from "~/components/record-toolbar";
import { TakeSwitch } from "~/components/take-switch";
import { api, errorMessage, type ProgressView, type RenameTakeInput } from "~/lib/ipc";
import { PROBE_MS } from "~/lib/levels";
import {
  autoAdvanceQuery,
  chosenDeviceQuery,
  devicesQuery,
  guideTempoQuery,
  ledgerKey,
  progressQuery,
  recordingOrderQuery,
  rowsWithTakesQuery,
  songPlanQuery,
  songStatusQuery,
  subbanksQuery,
  takePitchQuery,
} from "~/lib/queries";
import { useRecorder } from "~/lib/use-recorder";
import { useScreenFocus } from "~/lib/use-screen-focus";

/**
 * 収録の画面（S04）。 左に曲と行、中央に録る行と波形と F0、ツールバーの右に
 * カバレッジ・録る順・テンポ・マイク。
 *
 * 音源を開くのは外枠（`ProjectGate`）で、ここでは開かない（`TR-REC-30`）。
 * 開き直すと収録セッションが始め直しになる。
 *
 * 見出しは読み上げのためだけに置く。 どこにいるかはツールバーのパンくずが見せている。
 */
export const RecordScreen = () => {
  const heading = useScreenFocus();
  const { id, subbank, row } = useSearch({ strict: false });

  return (
    <main className="flex h-full min-h-0">
      <h1 ref={heading} tabIndex={-1} className="sr-only">
        収録
      </h1>
      {id === undefined ? (
        <p className="m-auto text-sm text-2">音源を選ぶと録れます。</p>
      ) : (
        <Suspense
          fallback={
            <p role="status" className="m-auto flex items-center gap-2 text-sm text-2">
              <Breath size="sm" />
              読んでいます
            </p>
          }
        >
          {/* 音源を替えたら、選んでいた行やマイクを持ち越さない。 */}
          <RecordBody key={id} id={id} subbank={subbank ?? null} initialRow={row ?? null} />
        </Suspense>
      )}
    </main>
  );
};

type RecordBodyProps = {
  id: string;
  /** 録る枠。 `null` なら全部の枠を通した録る順。 */
  subbank: string | null;
  /** 開いた直後に中央へ置く行。 原音設定の「録り直す」から来るときに要る。 */
  initialRow: string | null;
};

const RecordBody = ({ id, subbank, initialRow }: RecordBodyProps) => {
  const queryClient = useQueryClient();

  /*
   * まとめて並行に取る。 `useSuspenseQuery` を並べると直列になる（`EVID-PLT-001`）。
   */
  const [
    { data: progress },
    { data: allRows },
    { data: songs },
    { data: subbanks },
    { data: order },
    { data: devices },
    { data: chosen },
    { data: tempo },
    { data: advanceMs },
  ] = useSuspenseQueries({
    queries: [
      progressQuery(id),
      rowsWithTakesQuery(id),
      songStatusQuery(id),
      subbanksQuery(id),
      recordingOrderQuery(id, subbank),
      devicesQuery(),
      chosenDeviceQuery(id),
      guideTempoQuery(id),
      autoAdvanceQuery(),
    ],
  });

  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  /**
   * 本人が選んだ行。 `null` なら次に録る行を追う。
   *
   * 追っているあいだは、確定するたびに中央が次の行へ進む。 選んだ行は、
   * 録り直しても動かない——録ったものをその場で確かめられる。
   */
  const [chosenRow, setChosenRow] = useState<string | null>(initialRow);
  /** 目標にしている曲。 */
  const [songId, setSongId] = useState<string | null>(null);
  /**
   * 選んでいるマイク（`TR-REC-03`）。 初期値は Rust から取る。 画面だけで持つと、
   * 別の場から戻るたびに選択が消える。
   */
  const [deviceId, setDeviceId] = useState<string | undefined>(chosen.id ?? undefined);

  const fail = (e: unknown) => setError(errorMessage(e));

  /**
   * 録る前にマイクを開き直す（`TR-REC-03`）。 手順は音源の面と同じ。
   *
   * 開いただけでは録れない。 `probe_input` が通るまで `start_take` を受け付けない
   * （`REQ-REC-106`）ので、開いたら確かめる。
   */
  const ensureArmed = async () => {
    const now = await api.chosenDevice();
    if (now.armed) return;
    // 画面で選んでいるほうを優先する。 Rust が覚えているのは最後に開けたマイク。
    const want = deviceId ?? now.id;
    if (want === null || want === undefined) {
      throw new Error("マイクを選ぶと録れます。");
    }
    await api.armDevice(want);
    const peak = await api.probeInput(PROBE_MS);
    if (peak <= 0) {
      throw new Error("マイクから音が届いていません。設定で確かめてください。");
    }
  };

  /**
   * 確定したら、進み具合を書いてから台帳を無効化する。 無効化だけだと、
   * 取り直しが返るまで数が一拍遅れて動く。
   */
  const onSettled = ({ progress: p }: { progress: ProgressView }) => {
    queryClient.setQueryData(progressQuery(id).queryKey, p);
    void queryClient.invalidateQueries({ queryKey: ledgerKey });
  };

  const recorder = useRecorder({
    advanceMs,
    onSettled,
    onStatus: setStatus,
    onError: fail,
    onRetry: () => setError(null),
    ensureArmed,
    initiallyRecording: chosen.recording,
    subbank,
    leadInMs: tempo.leadInMs,
    beatMs: tempo.beatMs,
  });

  const phase: StagePhase = recorder.settling
    ? "settling"
    : recorder.recording
      ? "recording"
      : recorder.count !== null
        ? "countdown"
        : "idle";
  const busy = phase !== "idle";
  // 録っているあいだはサイドバーを畳み、中央を広げる。 止めたら本人の開閉へ戻る。
  useSidebarCollapsedWhile(busy);

  /** 枠の行だけ。 並びは台帳のまま。 */
  const rows = subbank === null ? allRows : allRows.filter((r) => r.subbank === subbank);
  /*
   * 目標の曲に要る行（`TR-RCL-17`）。 その曲を歌うために、あと録る行。 一覧は絞らず、
   * 印を付ける——並びと位置が変わらないので、録っている場所を見失わない（S04）。
   *
   * 読めなくても一覧は成り立つので `useQuery`。 読んでいるあいだは印を付けない。
   */
  const plan = useQuery({ ...songPlanQuery(id, songId ?? ""), enabled: songId !== null });
  const songRowIds = new Set(songId === null ? [] : (plan.data?.rows.map((p) => p.row_id) ?? []));
  const listed = rows;

  /*
   * 次に録る行は Rust が決める。 枠を指していれば、その枠の録る順の先頭。
   * 指していなければ `progress` の行。 ここで2つ目の規則を書かない。
   */
  const nextRowId = subbank === null ? progress.next_row_id : (order.row_ids[0] ?? null);
  const center =
    rows.find((r) => r.row_id === chosenRow) ?? rows.find((r) => r.row_id === nextRowId) ?? null;
  const at = center === null ? -1 : listed.findIndex((r) => r.row_id === center.row_id);
  const previous = at > 0 ? (listed[at - 1] ?? null) : null;
  const adopted = center?.takes.find((t) => t.take_id === center.adopted) ?? null;

  const pitch = useQuery({
    ...takePitchQuery(id, adopted?.take_id ?? -1),
    enabled: adopted !== null,
  });

  const invalidate = () => queryClient.invalidateQueries({ queryKey: ledgerKey });

  /** 採用を切り替える（`TR-REC-21`）。 前のテイクへいつでも戻せる。 */
  const adopt = useMutation({
    mutationFn: ({ rowId, takeId }: { rowId: string; takeId: number }) =>
      api.adoptTake(rowId, takeId),
    onMutate: () => setError(null),
    onError: fail,
    onSuccess: invalidate,
  });

  const rename = useMutation({
    mutationFn: (input: RenameTakeInput) => api.renameTake(input),
    onMutate: () => setError(null),
    onError: fail,
    onSuccess: ({ outcome }) => {
      if (outcome.__typename === "TakeRenamed") void invalidate();
      else setError(outcome.message);
    },
  });

  /** 録る順を切り替える（`TR-SYN-19` の (b)）。可逆。 */
  const switchOrder = useMutation({
    mutationFn: (mode: string) => api.setRecordingOrder(mode),
    onMutate: () => setError(null),
    onError: fail,
    onSuccess: invalidate,
  });

  /** テンポを変える（`DEC-REC-012`）。 返ってきた値をそのまま入れる。 */
  const changeTempo = useMutation({
    mutationFn: (bpm: number) => api.setGuideTempo({ bpm }),
    onMutate: () => setError(null),
    onError: fail,
    onSuccess: ({ outcome }) => {
      if (outcome.__typename === "GuideTempoSet") {
        queryClient.setQueryData(guideTempoQuery(id).queryKey, outcome.tempo);
      } else {
        setError(outcome.message);
      }
    },
  });

  /** マイクを替える。 開いて、届いているかを確かめるところまで（`TR-REC-03`）。 */
  const switchDevice = useMutation({
    mutationFn: async (next: string) => {
      await api.armDevice(next);
      const peak = await api.probeInput(PROBE_MS);
      if (peak <= 0) throw new Error("マイクから音が届いていません。");
    },
    onMutate: (next) => {
      setDeviceId(next);
      setError(null);
    },
    onError: fail,
  });

  const play = useMutation({
    mutationFn: (takeId: number) => api.playTake(takeId),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 曲を歌わせる（`TR-SYN-18`）。 */
  const sing = useMutation({
    mutationFn: (song: string) => api.singSong(song),
    onMutate: () => setError(null),
    onError: fail,
  });

  /**
   * 中央の行を録る。 次に録る行なら、選ぶのを Rust に任せる（`start_take`）。
   * 別の行なら、その行を指して録る（`start_retake`）。
   */
  const record = () => {
    if (center === null) return;
    if (center.row_id === nextRowId) recorder.start();
    else recorder.retake(center.row_id);
  };

  const box = subbank === null ? undefined : subbanks.find((s) => s.id === subbank);
  const coverage =
    box === undefined
      ? { done: progress.covered, total: progress.required }
      : { done: box.progress.recordedRows, total: box.progress.rows };

  return (
    <>
      <ShellToolbarEnd>
        <RecordToolbar
          coverage={coverage}
          mode={order.mode}
          onModeChange={(mode) => switchOrder.mutate(mode)}
          tempo={tempo}
          onTempoChange={(bpm) => changeTempo.mutate(bpm)}
          devices={devices}
          deviceId={deviceId}
          onDeviceChange={(next) => switchDevice.mutate(next)}
          locked={busy}
          pending={switchOrder.isPending || changeTempo.isPending || switchDevice.isPending}
        />
      </ShellToolbarEnd>

      {/* 状態の変化を支援技術へ通知する（`TR-PLT-29`）。 */}
      <p aria-live="polite" aria-atomic="true" className="sr-only">
        {status}
      </p>

      <RecordSidebar
        songs={songs.map(songItem)}
        rows={listed.map(rowItem)}
        selectedRowId={center?.row_id ?? null}
        onSelectRow={setChosenRow}
        selectedSongId={songId}
        songRowIds={songRowIds}
        onSelectSong={setSongId}
        onPlaySong={(song) => sing.mutate(song)}
      />

      <div className="flex min-h-0 min-w-0 flex-1 flex-col gap-4 overflow-y-auto px-10 py-6">
        <RecordStage
          rowText={center?.text ?? null}
          phase={phase}
          count={recorder.count}
          beatMs={tempo.beatMs}
          take={
            adopted === null
              ? null
              : { id: adopted.take_id, durationMs: adopted.duration_ms, peak: adopted.peak ?? 0 }
          }
          pitch={adopted === null ? undefined : pitch.data}
          ready={deviceId !== undefined}
          takes={
            center !== null && (
              <TakeSwitch
                takes={center.takes.map((t) => ({
                  id: t.take_id,
                  generation: t.generation,
                  label: t.label,
                  invalid: t.invalid,
                }))}
                adopted={center.adopted}
                disabled={busy || adopt.isPending || rename.isPending}
                onAdopt={(takeId) => adopt.mutate({ rowId: center.row_id, takeId })}
                onRename={(take, label) => rename.mutate({ take, label })}
              />
            )
          }
          canGoBack={previous !== null}
          onRecord={record}
          onStop={recorder.stop}
          onCancel={recorder.cancel}
          onBack={() => {
            if (previous !== null) setChosenRow(previous.row_id);
          }}
          onPlay={() => {
            if (adopted !== null) play.mutate(adopted.take_id);
          }}
        />
        {error !== null && (
          <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
            {error}
          </p>
        )}
      </div>
    </>
  );
};
