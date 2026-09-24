import { Button } from "~/components/button";

type RecordingOrderProps = {
  /** いまのモード。`SongBankFirst` か `CoverageEfficiency`。 */
  mode: string;
  /** 残り所要時間（秒、`TR-RCL-09`）。固定の見積もり。 */
  remainingSeconds: number;
  /** 残りの行数（`TR-RCL-09`）。多音階では音高の本数を掛けた数。 */
  remainingRows: number;
  /** 曲バンクに曲があるか。無ければ切り替える先が無い。 */
  hasSongs: boolean;
  onChange: (mode: string) => void;
  /** 切り替えている最中か。 */
  switching: boolean;
};

/** モードごとの言葉。工程の名前ではなく、何が起きるかで書く。 */
const LABEL = {
  SongBankFirst: {
    name: "歌いたい曲から",
    why: "選んだ曲に要る音を先に録ります。早く1曲歌えます。",
  },
  CoverageEfficiency: {
    name: "少ない行数で",
    why: "似た音をまとめて録ります。全体が早く埋まります。",
  },
} as const;

/** 知らないモード名は曲バンク優先として読む。 */
const labelOf = (mode: string) =>
  mode === "CoverageEfficiency" ? LABEL.CoverageEfficiency : LABEL.SongBankFirst;

/**
 * 録る順（`TR-SYN-19`）。
 *
 * **どちらでいるかを常に表示する。** モードが2つあるのに表示が無いと、
 * 「次に何を録るか」が理由もなく変わったように見える。
 *
 * 切り替えは可逆（`TR-SYN-19` の (b)）。 被覆効率から曲バンク優先へも戻せる。
 *
 * リストそのものは変わらない。 変わるのは並びだけで、録音リストの正準順も
 * カバレッジ台帳の行集合も動かない。そう1行で書いておく——
 * 順序が変わると「録り直しになる」と読まれる。
 */
/** 秒を読める長さにする。単位を省かない（`docs/reports/ux/direction.md`）。 */
const remaining = (seconds: number) => {
  if (seconds < 60) return "あと少し";
  const m = Math.round(seconds / 60);
  if (m < 60) return `約 ${m} 分`;
  return `約 ${Math.round(m / 60)} 時間`;
};

export const RecordingOrder = ({
  mode,
  remainingSeconds,
  remainingRows,
  hasSongs,
  onChange,
  switching,
}: RecordingOrderProps) => {
  const current = labelOf(mode);
  const other = mode === "SongBankFirst" ? "CoverageEfficiency" : "SongBankFirst";
  const next = labelOf(other);

  return (
    <div className="flex flex-col gap-2 rounded-lg border border-slate-7 bg-slate-3 p-3">
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-xs text-slate-11">録る順</span>
        <span className="text-sm font-semibold text-slate-12">{current.name}</span>
      </div>
      {/*
        残りを時間と件数の両方で出す（`TR-RCL-09`、`DEC-RCL-013`）。

        **時間だけにしない。** 時間は固定の見積もりで、桁が合っているだけ
        （`DEC-RCL-008`）。数だけ出すと精度を騙ることになる。
        行数は数え上げなので正確で、本人が自分のペースを当てはめられる。
      */}
      <div className="flex items-baseline justify-between gap-3">
        <span className="text-xs text-slate-11">残り</span>
        <span className="font-mono text-sm text-slate-12 tabular-nums">
          {remainingRows} 行 · {remaining(remainingSeconds)}
        </span>
      </div>
      <p className="text-xs text-slate-11">時間は目安です。行数は数えた数そのものです。</p>
      <p className="text-xs text-slate-11">{current.why}</p>
      {/* 曲が無ければ曲バンク優先へ戻す先が無い。 押せる的として出さない。 */}
      {(hasSongs || mode === "SongBankFirst") && (
        <Button
          variant="ghost"
          size="sm"
          onClick={() => onChange(other)}
          disabled={switching}
          className="self-start"
        >
          {next.name}に変える
        </Button>
      )}
      <p className="text-xs text-slate-11">並びが変わるだけです。録った音はそのまま残ります。</p>
    </div>
  );
};
