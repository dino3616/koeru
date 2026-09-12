import { useCallback, useEffect, useRef, useState } from "react";

import { api, type ProgressView, type TakeView } from "~/lib/ipc";

/** テイクが1つ確定したときに呼び側へ渡すもの。 */
type Settled = {
  take: TakeView;
  /** 確定を反映したあとの進み具合。呼び側が読み直さずに済むよう、ここで取る。 */
  progress: ProgressView;
};

type RecorderOptions = {
  /**
   * 連続収録の1フレーズの長さ（`TR-REC-20`）。
   *
   * ここでは読まない。 読むと、呼び側が既に中断している問い合わせの
   * うしろに並んでしまう——同じ描画の中で `useSuspenseQuery` を2つ通すと、
   * 1つ目で中断した時点で2つ目のフックまで降りないので、順に取りに行く
   * （`EVID-PLT-001` で実測）。呼び側が `useSuspenseQueries` で束ねて渡す。
   */
  advanceMs: number;
  /** テイクが確定した。台帳が変わっているので、一覧は作り直す。 */
  onSettled: (s: Settled) => void;
  /** 本人へ出す1行。画面の読み上げ領域へそのまま渡る。 */
  onStatus: (message: string) => void;
  onError: (cause: unknown) => void;
  /**
   * 前の失敗を消す。
   *
   * 録り直すときに呼ぶ。 呼ばないと、成功しても前の赤い文言が残り、
   * 直ったのに直っていないように見える。画面が持っているので、呼んで知らせる。
   */
  onRetry: () => void;
  /**
   * 録る前にマイクを開き直す（`TR-REC-03`）。
   *
   * 選択の持ち主は Rust 側（`chosen_device`）だが、**起動し直したあとは
   * 選ばれているだけでストリームが開いていない。** 開くのを設定の面まで
   * 引っ張らないために、最初のテイクの手前で開く。
   *
   * 開いてから始める。 開くのを待たずに `start_take` を呼ぶと、
   * ストリームの無い状態への収録要求になる。
   */
  ensureArmed: () => Promise<void>;
  /**
   * マウントした時点で Rust が収録中か（`chosen_device`）。
   *
   * **画面の state だけで始めない。** 収録中に別の面へ移ると、このフックは
   * 作り直されて「録っていない」から始まる。Rust は録り続けているので、
   * 「止める」が出ないまま次の収録が `app.already_recording` で断られる
   * ——**止めることも録ることもできなくなる。踏んだ。**
   */
  initiallyRecording: boolean;
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
 *
 * # ここの `useCallback` は消せない
 *
 * **このファイルだけ React Compiler が素通りする。** 原因は `try` / `finally`
 * で、これがあるとコンパイラはその関数を含むフック全体を丸ごと諦める。
 * `try` / `catch` なら通るので、避けているのは `finally` のほう。
 *
 * **何も言わずに諦める。** 診断も警告も出ないので、`useCallback` を外すと
 * 「コンパイラが見てくれる」つもりのまま、実際には毎回作り直される関数が
 * `useEffect` の依存に載る。他の 37 ファイルは通っているので、
 * ここだけ手で置いてあるのが正しい。
 *
 * 確かめ方。 `oxc-transform-react` の `transformSync` に
 * `{ reactCompiler: {} }` で通し、出力に `_c(` が現れるかを見る。
 * このファイルは0個、他は1関数につき1個出る。
 */
export const useRecorder = ({
  advanceMs,
  onSettled,
  onStatus,
  onError,
  onRetry,
  ensureArmed,
  initiallyRecording,
}: RecorderOptions) => {
  const [take, setTake] = useState<TakeView | null>(null);
  const [recording, setRecording] = useState(initiallyRecording);
  const [continuous, setContinuous] = useState(false);
  /**
   * テイクを確定させている最中か。
   *
   * `finish_take` は解析とアライメントを含むので数秒かかる。 押してから
   * 何も変わらないと、壊れたと思われる（`TR-SYN-33` と同じ理由）。
   * `recording` を下ろしてから結果が返るまでの間を、これで埋める。
   */
  const [settling, setSettling] = useState(false);
  /**
   * 確定させている最中か。React の外から読むので ref でも持つ。
   *
   * state と二重に持つ。 描画には state が要るが、連続収録のループは
   * 描画とは別の寿命で回るので、閉じ込めた古い state を読んでしまう。
   * 札の側を ref にすると、ループが「いまの」値を読める。
   */
  const settlingRef = useRef(false);

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
      //
      // 確定の途中も弾く（`TR-REC-42`）。 `finish_take` は解析とアライメントを
      // 含むので数秒かかり、そのあいだ `arming` は下りている。**押せる的を
      // 出さないだけでは足りない**——`続けて録る` は `settling` を見ていなかった。
      if (arming.current || settlingRef.current) return null;
      arming.current = true;
      try {
        // 選ばれているマイクを開き直す（`TR-REC-03`）。開いていれば何もしない。
        await ensureArmed();
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
    [onStatus, ensureArmed],
  );

  /** テイクを確定させて、呼び側へ渡す。 */
  const settle = useCallback(async () => {
    setRecording(false);
    setSettling(true);
    settlingRef.current = true;
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
      settlingRef.current = false;
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
    onRetry();
    beginTake(() => api.startTake()).catch(onError);
  }, [beginTake, onError, onRetry]);

  /**
   * 行を指定して録り直す（`TR-REC-21`、`TR-RCL-25`、`TR-ALN-27`）。
   *
   * 単発の収録として扱う（`TR-REC-42`）。自動で次へ送らない。
   */
  const retake = useCallback(
    (rowId: string) => {
      onRetry();
      beginTake(() => api.startRetake(rowId))
        .then(() => onStatus(`${rowId} を録り直しています。終わったら「止める」`))
        .catch(onError);
    },
    [beginTake, onError, onStatus, onRetry],
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
        /*
         * 始められなかったら、そこで抜ける。
         *
         * 回し続けない。 `recordOnce` が `null` を返すのは
         * 「開こうとしている最中」か「確定の途中」で、どちらも次の周でも
         * 同じままのことがある——**空回りのループになる。**
         */
        if ((await recordOnce(advanceMs)) === null) break;
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
   * 自分のものだと思って走る。
   *
   * 途中のテイクを確定させない。 固定長の途中で止めたぶんは切れた発声なので、
   * 確定させると台帳へ積まれ、進み具合がその部分的なテイクで進む——
   * 「止めたフレーズは未収録のまま残る」という約束と食い違う。
   *
   * 開いたままのストリームは、次に録りはじめるときの `start_take` が畳む。
   * Rust 側に「捨てる」口は無いので、ここで確定させないことが唯一の手当て。
   */
  const pauseContinuous = useCallback(() => {
    continuing.current = false;
    // 番号を進めて、待っている自動終了を自分のものでなくする（`TR-REC-42`）。
    takeSeq.current += 1;
  }, []);

  return {
    take,
    recording,
    settling,
    continuous,
    start,
    stop,
    retake,
    runContinuous,
    pauseContinuous,
  };
};
