import { useMutation, useQueryClient, useSuspenseQueries } from "@tanstack/react-query";
import { useNavigate, useSearch } from "@tanstack/react-router";
import { Suspense, useEffect, useRef, useState } from "react";

import { Breath } from "~/components/breath";
import { Button } from "~/components/button";
import { CalibrationCard } from "~/components/calibration-card";
import { Card } from "~/components/card";
import { CardSkeleton } from "~/components/card-skeleton";
import { DeviceCard } from "~/components/device-card";
import { ItemList } from "~/components/item-list";
import { LastTake } from "~/components/last-take";
import { LeakCard } from "~/components/leak-card";
import { NextPhrase } from "~/components/next-phrase";
import { PackagePanel } from "~/components/package-panel";
import { PendingWork } from "~/components/pending-work";
import { SongDetail } from "~/components/song-detail";
import { SongList } from "~/components/song-list";
import { VoiceHeader, type VoiceTab } from "~/components/voice-header";
import { VoicePortrait } from "~/components/voice-portrait";
import { VoiceSettings } from "~/components/voice-settings";
import { api, errorMessage, type ProgressView } from "~/lib/ipc";
import { PROBE_MS } from "~/lib/levels";
import {
  autoAdvanceQuery,
  chosenDeviceQuery,
  devicesQuery,
  ledgerKey,
  openProjectQuery,
  progressQuery,
  rowsWithTakesQuery,
  songStatusQuery,
  voiceStateQuery,
} from "~/lib/queries";
import { useRecorder } from "~/lib/use-recorder";

/** 回り込みの確認と音高提示に使う音（MIDI）。C4。 */
const GUIDE_MIDI = 60;

/**
 * 音源の面（`DEC-PLT-024`）。
 *
 * 中央に「育っていく声」を常駐させ、左右のパネルだけが面で入れ替わる
 * （`DEC-PLT-025`）。タブは工程の動詞にしない。
 *
 * パスを画面に出さない（`TR-PKG-45`）。保存先も、ファイル名も見せない。
 *
 * 開くまでと開いたあとを別の部品に分ける。 `open_project` を先に
 * 済ませないと、台帳を読む子が `app.no_project` を受ける。順番を
 * フックの並び順に頼らない——並べ替えても型は通り、`app.no_project` が
 * 出て初めて分かる。境界で分けておけば、順序が木の形として残る。
 */
export const VoiceScreen = () => {
  const navigate = useNavigate();
  const { id, tab, retake: retakeRow } = useSearch({ from: "/voice" });

  // 識別子が無いまま開かれることがある（殻だけを先に出したときや、
  // 履歴から直接来たとき）。落とさず、戻る道を出す。
  if (id === undefined) {
    return (
      <main className="mx-auto flex h-full max-w-2xl flex-col items-center justify-center gap-3 p-8">
        <p className="text-sm text-slate-11">どの声を開くかが分かりませんでした。</p>
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
      <OpenVoice id={id} tab={tab} retakeRow={retakeRow} />
    </Suspense>
  );
};

/**
 * 音源を開くところまで。
 *
 * 子を返すだけの層に見えるが、中断が解けるまで子は描かれない——React は
 * 親が中断した時点で降りるのをやめる。これが「開いてから読む」の保証になっている。
 *
 * 1フレーズの長さはここで一緒に取る。 開くのとは関係が無いので、
 * `useSuspenseQueries` で束ねて並行に投げる。`useSuspenseQuery` を2つ並べると
 * 順に取りに行く（`EVID-PLT-001` で実測）。
 */
const OpenVoice = ({
  id,
  tab,
  retakeRow,
}: {
  id: string;
  tab: VoiceTab;
  retakeRow: string | undefined;
}) => {
  const [, advance] = useSuspenseQueries({
    queries: [openProjectQuery(id), autoAdvanceQuery()],
  });
  return <VoiceBody id={id} tab={tab} advanceMs={advance.data} retakeRow={retakeRow} />;
};

/** 開いたあとの音源の面。 */
const VoiceBody = ({
  id,
  tab,
  advanceMs,
  retakeRow,
}: {
  id: string;
  tab: VoiceTab;
  advanceMs: number;
  retakeRow: string | undefined;
}) => {
  const navigate = useNavigate();
  const queryClient = useQueryClient();

  /*
   * まとめて並行に取る。
   *
   * `useSuspenseQuery` を並べない。 同じ部品に並べると**直列**になる——
   * 1つ目が中断した時点で React は降りるので、2つ目のフックまで到達しない
   * （`EVID-PLT-001` で実測）。
   */
  const [
    { data: progress },
    { data: voice },
    { data: rows },
    { data: songs },
    { data: devices },
    { data: chosen },
  ] = useSuspenseQueries({
    queries: [
      progressQuery(id),
      voiceStateQuery(id),
      rowsWithTakesQuery(id),
      songStatusQuery(id),
      devicesQuery(),
      chosenDeviceQuery(id),
    ],
  });

  /**
   * 選ばれているマイク（`TR-REC-03`）。
   *
   * 初期値を Rust から取る。 ここだけの state にしていたので、
   * **テイクの面へ入って戻るだけで選択が消え、設定の面へ行き直すことになっていた。**
   * 準備を設定の面へ移した理由（`DEC-PLT-024`）が、それでは成り立たない。**踏んだ。**
   */
  const [deviceId, setDeviceId] = useState<string | undefined>(chosen.id ?? undefined);
  /** ストリームが開いているか。開くのが要る操作は、開くまで押させない。 */
  const [armed, setArmed] = useState(chosen.armed);
  const [error, setError] = useState<string | null>(null);
  const [status, setStatus] = useState("");
  /** 回り込みの確認結果。音高提示を鳴らしてよいかを決める（`TR-REC-24`）。 */
  const [leaking, setLeaking] = useState<boolean | null>(null);
  /** 曲の面で選んでいる曲。 */
  const [songId, setSongId] = useState<string | null>(null);
  /** 名前。改名で動く（`DEC-PKG-007`）。 */
  const [name, setName] = useState(voice.display_name);
  /** 確定した直後だけ環を伸ばす（`DEC-PLT-025`）。 */
  const [grown, setGrown] = useState(0);

  const fail = (e: unknown) => setError(errorMessage(e));

  /**
   * 録る前にマイクを開き直す（`TR-REC-03`）。
   *
   * 開いているかを Rust に訊く。 画面側で覚えない——覚えると、
   * 設定の面で開き直したことや、収録中の抜き差し（`TR-REC-04`）で
   * 閉じたことに追従できない。テイク1本は数秒かかる操作なので、
   * その手前の1往復は見えない。
   */
  const ensureArmed = async () => {
    const now = await api.chosenDevice();
    if (now.armed) return;
    /*
     * 画面で選んでいるほうを優先する。
     *
     * Rust が覚えているのは最後に開けたマイク。 新しいマイクを開こうとして
     * 失敗すると、画面は新しいほうを出したまま Rust は古いほうを指す——
     * **そのまま開き直すと、別のマイクで録れてしまう。**
     */
    const want = deviceId ?? now.id;
    if (want === null || want === undefined) {
      throw new Error("設定でマイクを選ぶと録れます。");
    }
    await api.armDevice(want);
    /*
     * 開いただけでは録れない。 `Session` は「届いているか未確認」のままで、
     * `probe_input` が通るまで `start_take` を受け付けない（`REQ-REC-106`）。
     * **開くだけにしていたので、選択を戻した最初の1本が必ず失敗していた。**
     * 設定の面の手順（開く → 確かめる）と同じものを、ここでも通す。
     */
    const peak = await api.probeInput(PROBE_MS);
    if (peak <= 0) {
      throw new Error("マイクから音が届いていません。設定で確かめてください。");
    }
    setArmed(true);
  };

  /**
   * 確定したら、進み具合と一覧を同時に進める。片方だけ動くと数が合わない。
   *
   * 進み具合は確定が返した値をそのまま書く。 無効化だけにすると、
   * 取り直しが返るまで数字が一拍遅れて動く——連続収録では数秒ごとに起きる。
   * 書いたうえで台帳全体を無効化し、環と一覧を取り直させる。
   *
   * カバレッジでは代用できない。 採用テイクを切り替えても録り直しても、
   * カバレッジは変わらない（`TR-RCL-25`）。
   */
  const onSettled = ({ progress: p }: { progress: ProgressView }) => {
    queryClient.setQueryData(progressQuery(id).queryKey, p);
    void queryClient.invalidateQueries({ queryKey: ledgerKey });
    setGrown((n) => n + 1);
  };

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
    onRetry: () => setError(null),
    ensureArmed,
    initiallyRecording: chosen.recording,
  });

  /**
   * 経路で運ばれてきた録り直しを、一度だけ走らせる。
   *
   * ルート遷移との同期なので effect で受ける（`react-conventions` の例外）。
   * 札は `useRef`——描画に出ないので state にしない。
   *
   * 引数を先に落とす。 残したままだと、テイクの面へ入って戻るたびに
   * また録りはじめる。
   */
  const retakeStarted = useRef(false);
  useEffect(() => {
    if (retakeRow === undefined || retakeStarted.current) return;
    retakeStarted.current = true;
    void navigate({ to: "/voice", search: { id, tab }, replace: true });
    retake(retakeRow);
  }, [retakeRow, retake, navigate, id, tab]);

  /*
   * 鳴らす操作はどれも `useMutation`。
   *
   * `.catch(fail)` を手で書くのと同じことをしているように見えるが、
   * 前の失敗を消すのが `onMutate` に寄るので、書き忘れる場所が無くなる。
   */
  const playRaw = useMutation({
    mutationFn: (takeId: number) => api.playTake(takeId),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 音の高さを鳴らす（`TR-REC-24` の音高提示）。回り込みが無いときだけ出す。 */
  const playPitch = useMutation({
    mutationFn: (midi: number) => api.playPitch(midi),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** 鳴っている音を止める。失敗しても言わない——止まっているのが望みなので。 */
  const stopPreview = useMutation({ mutationFn: () => api.stopPreview() });

  /** 曲を歌わせる（`TR-SYN-18`）。先頭フレーズができた時点で鳴りはじめる。 */
  const sing = useMutation({
    mutationFn: (songId: string) => api.singSong(songId),
    onMutate: () => setError(null),
    onError: fail,
  });

  /** いま何で録っているか。帯に出すのはこの1行だけ（`DEC-PLT-024`）。 */
  const deviceName =
    deviceId === undefined ? null : (devices.find((d) => d.id === deviceId)?.name ?? null);

  const nextRow = rows.find((r) => r.row_id === progress.next_row_id) ?? null;
  const takeRow = take === null ? null : (rows.find((r) => r.row_id === take.row_id) ?? null);
  /*
   * 曲の面で見ている曲。
   *
   * 既定は「まだ歌えない曲のうち先頭」。 曲の面は次に何を録るかを決める場所なので、
   * 全部録れている曲を開いても、そこに読むものが無い。
   * `song_status` は手が届く順に返す（`TR-RCL-17`）ので、先頭が歌える曲になる。
   */
  const selectedSong =
    songs.find((s) => s.id === songId) ??
    songs.find((s) => s.missing_units > 0) ??
    songs[0] ??
    null;
  /**
   * 中央の「聴く」で鳴らす曲。
   *
   * 歌えるものを先に見るが、無ければ先頭を渡す。 `singable` が偽なのは
   * 「フォールバックでも解決できない音符がある」だけで、`TR-SYN-18` は
   * **鳴らせないフレーズを除いた短縮版として鳴らす**と定めている。
   * ここで弾くと、被覆が満ちるまで中央の的が死ぬ——`DEC-PLT-025` の
   * 「いつでも押せて、そのときは鳴らないことが返事になる」と食い違う。
   * 短すぎるものは Rust が `synth.too_short` で断る。
   */
  const listenable = songs.find((s) => s.singable) ?? songs[0] ?? null;

  // どの面から開いたかを運ぶ。戻るときにそこへ返す。
  const openTake = (rowId: string) =>
    void navigate({ to: "/take", search: { id, row: rowId, from: tab } });

  return (
    <main className="flex h-full flex-col overflow-hidden">
      <VoiceHeader
        name={name}
        method={voice.method}
        deviceName={deviceName}
        tab={tab}
        onTab={(next) => void navigate({ to: "/voice", search: { id, tab: next } })}
        onBack={() => void navigate({ to: "/" })}
      />

      {/* 状態の変化を支援技術へ通知する（`TR-PLT-29`）。 */}
      <p aria-live="polite" aria-atomic="true" className="sr-only">
        {status}
      </p>

      {/*
        3つの領域の幅の約束。

        **どの列も潰れない下限を持たせる。** 中央を `flex-shrink-0` の固定幅に
        していたので、縮み分を右がぜんぶ引き受けていた——右の中身は字なので
        min-content がいくらでも小さくなる。**宣言している最小の窓（960px、
        `tauri.conf.json`）で右が 52px になり、「歌える曲」が1文字ずつ
        縦に改行されていた。踏んだ。**

        1200px で段を切る。 3列に要るのは
        左 288 + 中央 460 + 右 288 + 余白 128 = 1164px なので、
        それを下回ったら中央と右を縦に積む。**隠さない**——狭いだけで
        読めるものが消えると、窓の大きさで機能が変わる。

        並び順は DOM と見た目で違える。 見た目は左→中央→右のまま、
        DOM は中央→右→左にして `order` で戻す。**Tab は DOM 順に進む**ので、
        こうしないと主操作へ届くまでに収録項目 29 行を通ることになる（実測で
        14 回押してもまだ一覧の中だった）。読む順としても、まず手を動かす場所、
        次にその結果、最後に一覧を見に行く形になる。
      */}
      <div className="flex min-h-0 flex-1 gap-8 p-8">
        {/* ── 中央と右。狭いと縦に積む ── */}
        <div className="order-2 flex min-h-0 min-w-0 flex-1 gap-8 max-[1200px]:flex-col max-[1200px]:overflow-y-auto">
          {/* ── 中央。面が変わっても、ここは動かない ── */}
          {/*
            段を詰める（0.75rem）。 常設の読み上げ領域が2つ挟まっていて、
            中身が空でも `gap` は1つぶん取る。1.25rem のままだと、
            「次に読む」の末尾の1行が下端から出た。
          */}
          <div className="flex min-h-0 min-w-0 max-w-115 flex-1 flex-col items-center gap-3 overflow-y-auto max-[1200px]:max-w-none max-[1200px]:flex-none max-[1200px]:overflow-visible">
            <VoicePortrait
              state={voice}
              preparing={sing.isPending}
              onStop={() => stopPreview.mutate()}
              grow={grown > 0}
              onListen={() => {
                if (listenable === null) {
                  setStatus("曲がありません");
                  return;
                }
                sing.mutate(listenable.id);
              }}
            />

            <PendingWork />

            {tab === "sound" && (
              <NextPhrase
                text={progress.next_row_text}
                units={nextRow?.units ?? 0}
                recording={recording}
                settling={settling}
                continuous={continuous}
                ready={deviceId !== undefined}
                advanceMs={advanceMs}
                onStart={start}
                onStop={stop}
                onContinuous={() => {
                  setError(null);
                  void runContinuous();
                }}
                onPause={pauseContinuous}
              />
            )}

            {/*
              被覆が満ちることと完成を、同じものとして書かない。

              `TR-PKG-34` の完成は3条件で、被覆はそのうちの1つ。
              `DEC-PKG-007` は「被覆が満ちた瞬間には必ず完成している」形を
              目指しているが、**原音設定の検証はまだ通していない**
              （`progress` は `all_oto_validated` に `false` を渡している）ので、
              いまは全部録れても `AwaitingOto` で止まる。
              **満ちれば完成、と書くと約束が先走る。**
            */}
            {tab === "package" && (
              <p className="max-w-80 text-center text-xs text-slate-11">
                <span className="font-mono tabular-nums">{voice.required}</span>{" "}
                音すべてを録るのが、完成までの最後の一歩です。
              </p>
            )}
          </div>

          {/* ── 右 ── */}
          {/*
            縦に積むときは `flex-none` にする。 `flex-1` は
            `flex-basis: 0%` を置くので、縦の並びでは**高さの基準が 0** になる。
            縮まないようにしただけでは伸びる余地も無く、**高さ 0 の箱から
            中身が溢れて、環と「次に読む」が重なった。踏んだ。**
          */}
          <div className="flex min-h-0 min-w-72 flex-1 flex-col gap-5 overflow-y-auto max-[1200px]:min-w-0 max-[1200px]:flex-none max-[1200px]:overflow-visible">
            {tab === "sound" && (
              <>
                {take !== null && takeRow !== null && (
                  <LastTake
                    take={take}
                    rowText={takeRow.text}
                    units={takeRow.units}
                    busy={recording || continuous}
                    onListen={() => playRaw.mutate(take.take_id)}
                    onRetake={() => retake(take.row_id)}
                    onOpen={() => openTake(take.row_id)}
                  />
                )}
                <Card title="歌える曲">
                  <SongList
                    songs={songs}
                    preparingId={sing.isPending ? sing.variables : null}
                    onSing={(sid) => sing.mutate(sid)}
                  />
                </Card>
              </>
            )}

            {tab === "songs" && selectedSong !== null && (
              <Suspense fallback={<CardSkeleton title={selectedSong.title} />}>
                <SongDetail
                  voiceId={id}
                  song={selectedSong}
                  onRecordFrom={(rowId) => {
                    void navigate({ to: "/voice", search: { id, tab: "sound" } });
                    retake(rowId);
                  }}
                />
              </Suspense>
            )}

            {tab === "package" && (
              <Suspense fallback={<CardSkeleton title="書き出す前に見ること" />}>
                <PackagePanel voiceId={id} rows={rows} onOpenRow={openTake} />
              </Suspense>
            )}

            {tab === "settings" && (
              <>
                <Suspense fallback={<CardSkeleton title="録るときの音" />}>
                  <DeviceCard
                    deviceId={deviceId}
                    armed={armed}
                    onDeviceChange={(next) => {
                      setDeviceId(next);
                      setError(null);
                    }}
                    onArmed={() => setArmed(true)}
                    onStatus={setStatus}
                  />
                </Suspense>
                {/*
                  開いているかで判断する。 選ばれているだけでは足りない——
                  どちらも Rust 側でストリームを要求する（起動し直した直後は、
                  選択が戻っていても開いていない）。
                */}
                <CalibrationCard ready={armed} onStatus={setStatus} />
                <LeakCard
                  ready={armed}
                  midi={GUIDE_MIDI}
                  onStatus={setStatus}
                  onChecked={setLeaking}
                />
                {leaking === false && (
                  <Card title="音の高さ">
                    <p className="text-sm text-slate-12">録りながら音の高さを聞けます。</p>
                    <Button
                      variant="secondary"
                      size="sm"
                      onClick={() => playPitch.mutate(GUIDE_MIDI)}
                    >
                      鳴らす
                    </Button>
                  </Card>
                )}
              </>
            )}

            {error !== null && (
              <p role="alert" className="rounded-lg bg-red-3 px-3 py-2 text-sm text-red-11">
                {error}
              </p>
            )}
          </div>
        </div>

        {/* ── 左 ── */}
        <section
          /*
            中身のぶんだけ高くする。 列の全高まで伸ばしていたので、
            中身が上端だけの面（配り物・設定）で、下 2/3 が空の枠になっていた。
          */
          className="order-1 flex w-72 min-h-0 max-h-full shrink-0 flex-col gap-3 self-start rounded-xl border border-slate-7 bg-slate-2 p-5"
          aria-labelledby="left"
        >
          <h2 id="left" className="text-sm font-semibold text-slate-11">
            {tab === "sound"
              ? "録るもの"
              : tab === "songs"
                ? "歌わせたい曲"
                : tab === "package"
                  ? "配り物"
                  : "この声の設定"}
          </h2>

          {tab === "sound" && (
            <ItemList rows={rows} nextRowId={progress.next_row_id} onOpen={openTake} />
          )}
          {tab === "songs" && (
            <SongList
              songs={songs}
              preparingId={sing.isPending ? sing.variables : null}
              onSing={(sid) => sing.mutate(sid)}
              onSelect={setSongId}
              selectedId={selectedSong?.id ?? null}
            />
          )}
          {tab === "package" && (
            <>
              <p className="text-sm text-slate-12">
                配り物をつくらなくても、この声は完成にできます。
              </p>
              <p className="text-xs text-slate-11">
                書き出しはまだできません。いまは、書き出す前に見えることだけを出します。
              </p>
            </>
          )}
          {tab === "settings" && (
            <VoiceSettings
              id={id}
              name={name}
              method={voice.method}
              rows={voice.rows}
              onRenamed={setName}
            />
          )}
        </section>
      </div>
    </main>
  );
};
