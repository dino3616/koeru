import { useCallback, useEffect, useRef, useState } from "react";

import { api, type ProgressView, type TakeView } from "~/lib/ipc";

/** テイクが1つ確定したときに呼び側へ渡すもの。 */
type Settled = {
  take: TakeView;
  /** 確定を反映したあとの進み具合。呼び側が読み直さずに済むよう、ここで取る。 */
  progress: ProgressView;
};

type RecorderOptions = {
  /** テイクが確定した。台帳が変わっているので、一覧は作り直す。 */
  onSettled: (s: Settled) => void;
  /** 本人へ出す1行。画面の読み上げ領域へそのまま渡る。 */
  onStatus: (message: string) => void;
  onError: (cause: unknown) => void;
};

/**
 * 収録の状態機械。
 *
 * 画面から切り出してあるのは、ここが「押した順」ではなく
 * 「どのテイクの話か」で動くため。 二重確定を避ける札（`TR-REC-42`）と
 * 連続収録のループ（`TR-REC-20`）は、描画とは別の寿命で回っている。
 *
 * 状態を1つに畳まないのは、`recording` が描画に要るのに対して
 * `takeSeq` と `arming` は描画に出ないから。 出ないものを state にすると、
 * 押すたびに描き直すことになる。
 */
export const useRecorder = ({ onSettled, onStatus, onError }: RecorderOptions) => {
  const [take, setTake] = useState<TakeView | null>(null);
  const [recording, setRecording] = useState(false);
  const [continuous, setContinuous] = useState(false);
  /**
   * テイクを確定させている最中か。
   *
   * `finish_take` は解析とアライメントを含むので数秒かかる。 押してから
   * 何も変わらないと、壊れたと思われる（`TR-SYN-33` と同じ理由）。
   * `recording` を下ろしてから結果が返るまでの間を、これで埋める。
   */
  const [settling, setSettling] = useState(false);
  const [advanceMs, setAdvanceMs] = useState(3000);

  /**
   * いま録っているテイクの番号。
   *
   * 自動終了と手動終了が同時に走らないための札（`TR-REC-42`）。
   * 止めるたびに進めるので、待っている自動終了は自分の番号でなくなる。
   */
  const takeSeq = useRef(0);
  /** 収録を開こうとしている最中か。二重に開かせない。 */
  const arming = useRef(false);
  /** 連続収録が回っているか。React の外から読むので ref で持つ。 */
  const continuing = useRef(false);

  useEffect(() => {
    api.autoAdvanceMs().then(setAdvanceMs).catch(onError);
  }, [onError]);

  /*
   * 画面を離れたら連続収録を止める。
   *
   * ループは React の外で回るので、これが無いと一覧へ戻ったあとも録り続け、
   * 本人が喋っていないテイクが台帳に積まれる。
   */
  useEffect(
    () => () => {
      continuing.current = false;
    },
    [],
  );

  /**
   * テイクを1つ始める。
   *
   * 番号を1つ進めて返す。 待っている自動終了が、
   * 自分の番号でなくなったら確定させない——二重に確定させない（`TR-REC-42`）。
   */
  const beginTake = useCallback(
    async (starter: () => Promise<string>) => {
      // `await starter()` の間は `recording` がまだ false なので、ボタンが押せる
      // ままになる。ここで弾く。開けなかったときは `null` を返す——
      // 番号を返すと、呼び出し側が「自分が開いたテイク」と取り違えて確定させにいく。
      if (arming.current) return null;
      arming.current = true;
      try {
        // 直前のテイクを消さない。 録音の途中でも自分の声を聴けることが中核なので、
        // 次を録り始めた瞬間に前のものが画面から消える形にしない。
        // 確定したら `settle` が差し替える。
        await starter();
        takeSeq.current += 1;
        setRecording(true);
        onStatus("収録中。終わったら「止める」");
        return takeSeq.current;
      } finally {
        // 失敗しても必ず下ろす。下ろさないと二度と録れなくなる。
        arming.current = false;
      }
    },
    [onStatus],
  );

  /** テイクを確定させて、呼び側へ渡す。 */
  const settle = useCallback(async () => {
    setRecording(false);
    setSettling(true);
    onStatus("録った音を確かめています");
    try {
      const t = await api.finishTake();
      setTake(t);
      onStatus(
        t.invalidated
          ? "取りこぼしがあったので、もう一度録ります"
          : t.has_oto
            ? "録れました。音高を選ぶと歌います"
            : "録れましたが、発声を見つけられませんでした",
      );
      onSettled({ take: t, progress: await api.progress() });
      return t;
    } finally {
      // 失敗しても必ず下ろす。下ろさないと、止めるボタンが戻らない。
      setSettling(false);
    }
  }, [onSettled, onStatus]);

  /**
   * 単発の収録（`TR-REC-42`）。本人が止めるまで録る。
   *
   * `TR-REC-20` の固定長は連続収録の自動送りの条件であって、
   * 単発の終了条件ではない。発話の長さは項目で倍以上違う——
   * 「あ い う え お」と「ん」を同じ長さで切る理由が無い。
   */
  const start = useCallback(() => {
    beginTake(() => api.startTake()).catch(onError);
  }, [beginTake, onError]);

  /**
   * 行を指定して録り直す（`TR-REC-21`、`TR-RCL-25`、`TR-ALN-27`）。
   *
   * 単発の収録として扱う（`TR-REC-42`）。自動で次へ送らない。
   */
  const retake = useCallback(
    (rowId: string) => {
      beginTake(() => api.startRetake(rowId))
        .then(() => onStatus(`${rowId} を録り直しています。終わったら「止める」`))
        .catch(onError);
    },
    [beginTake, onError, onStatus],
  );

  /**
   * 止める。
   *
   * 番号を進めてから確定させる。 進めておかないと、
   * 連続収録で待っている自動終了が、確定済みのテイクをもう一度確定させにいく
   * ——収録していない状態への確定要求になってエラーが出る（`TR-REC-42`）。
   */
  const stop = useCallback(() => {
    takeSeq.current += 1;
    settle().catch(onError);
  }, [settle, onError]);

  /**
   * 止められる待ち。
   *
   * 一息に眠らない。 `continuing` が false になったら、そこで返す。
   * 一息に眠ると、やめても最大 `advanceMs` ぶん録り続けることになる。
   */
  const sleepWhileRunning = useCallback(async (ms: number) => {
    const step = 50;
    for (let left = ms; left > 0 && continuing.current; left -= step) {
      await new Promise((r) => setTimeout(r, Math.min(step, left)));
    }
  }, []);

  const recordOnce = useCallback(
    async (holdMs: number) => {
      const mine = await beginTake(() => api.startTake());
      // 開けなかった（既に開こうとしていた）なら、確定させにいかない。
      if (mine === null) return null;
      await sleepWhileRunning(holdMs);
      // 本人が先に止めたなら、ここでは確定させない。
      if (takeSeq.current !== mine) return null;
      return settle();
    },
    [beginTake, settle, sleepWhileRunning],
  );

  /**
   * 連続収録（`TR-REC-20`）。
   *
   * 止めたフレーズは未収録のまま残る。 途中で抜けても、続きから再開できる。
   * フレーズの間もストリームは止めないので、プリロールは保たれる（`TR-REC-19`）。
   */
  const runContinuous = useCallback(async () => {
    continuing.current = true;
    setContinuous(true);
    try {
      while (continuing.current) {
        const p = await api.progress();
        if (!continuing.current || p.next_row_id === null) break;
        await recordOnce(advanceMs);
        // フレーズ間の間。声を出し終える時間を残す。
        await sleepWhileRunning(400);
      }
    } catch (e) {
      onError(e);
    } finally {
      continuing.current = false;
      setContinuous(false);
      onStatus("連続収録を止めました");
    }
  }, [advanceMs, recordOnce, sleepWhileRunning, onError, onStatus]);

  /**
   * 連続収録をやめる。
   *
   * 番号を進めてから止める。 進めておかないと、待っている確定が
   * 自分のものだと思って走る。開いたままのテイクは `settle` が畳む。
   */
  const pauseContinuous = useCallback(() => {
    continuing.current = false;
    takeSeq.current += 1;
    if (recording) settle().catch(onError);
  }, [recording, settle, onError]);

  return {
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
  };
};
