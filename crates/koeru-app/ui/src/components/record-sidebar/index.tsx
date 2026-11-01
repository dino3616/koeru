import { type KeyboardEvent, type ReactNode, useEffect, useId, useRef, useState } from "react";

import { Icon } from "~/components/icon";
import type { RowStatus } from "~/components/status-mark";
import { tv } from "~/lib/tv";

/** 曲の状態（`TR-RCL-19`）。 「あと N 行」の N は `rows` が持つ。 */
export type SidebarSongState =
  | { status: "ready" }
  | { status: "fallback" }
  | { status: "remaining"; rows: number }
  | { status: "out_of_range" };

/** 左の列の曲1つ。 `SongView` からの写しは `./from-views` が持つ。 */
export type SidebarSong = {
  id: string;
  title: string;
  /** 音域（「A3–D4」）。 曲が音高を持たなければ `null`。 */
  range: string | null;
  state: SidebarSongState;
  /** 要る単位のうち録ってある割合（0〜1）。 「あと N 行」の輪が描く。 */
  progress: number;
  /**
   * 状態の字の代わりに右端へ出す補助の字。 聴いているあいだの再生位置
   * （「0:12 / 1:48」）のように、曲の行へ足したいものを置く。
   */
  note?: string;
};

/** 左の列の行1つ。 `text` は空白で区切ったモーラ（`RowTakesView.text` と同じ）。 */
export type SidebarRow = {
  id: string;
  text: string;
  status: RowStatus;
};

type RecordSidebarProps = {
  songs: readonly SidebarSong[];
  rows: readonly SidebarRow[];
  /** 中央が扱っている行。 一覧に無ければどれも選ばれていない。 */
  selectedRowId: string | null;
  /** 行を選んだとき。 中央がその行になる。 */
  onSelectRow: (rowId: string) => void;
  /** 目標にしている曲。 無ければ `null`。 行は絞らず、その曲に要る行へ ♪ を付ける。 */
  selectedSongId: string | null;
  /** 目標の曲に要る行。 目標が無いあいだは空。 */
  songRowIds: ReadonlySet<string>;
  /** 曲を押したとき。 目標の曲をもう一度押すと `null`（目標を外す）。 */
  onSelectSong: (songId: string | null) => void;
  /** 曲の ▶ を押したとき。 鳴らすのは受け取った側で、ここは知らせるだけ。 */
  onPlaySong: (songId: string) => void;
};

const ROW_STATUS_LABEL: Record<RowStatus, string> = {
  unrecorded: "未収録",
  recorded: "収録済み",
  needs_retake: "要録り直し",
  excluded: "除外",
};

/*
 * 選んでいる行は、面と左の 3px の線で言う。 色だけに頼らない。 線は声の色の
 * 内側の影で、場所を取らないので選んだ瞬間に字がずれない。
 */
const entry = tv({
  base: "flex shrink-0 items-center rounded-md",
  variants: {
    selected: {
      true: "bg-surface-2",
      false: "hover:bg-surface-2",
    },
  },
});

const label = tv({
  variants: {
    // 録ったものは退く。 まだのものは薄くしない（`docs/reports/ux/direction.md`）。
    dim: { true: "text-2", false: "text-1" },
  },
});

const isSingable = ({ status }: SidebarSongState) => status === "ready" || status === "fallback";

type SectionHeaderProps = {
  id: string;
  title: string;
  /** 「93 / 131」。 `null` なら出さない。 */
  count?: string | null;
  /** 数の読み上げ。 見える字に前置きする。 */
  countLabel?: string;
};

/** 見出しの帯。 題は曲の題の頭（印の幅 22px の先）に揃える。 */
const SectionHeader = ({ id, title, count = null, countLabel = "" }: SectionHeaderProps) => (
  <div className="flex h-7 shrink-0 items-center gap-2 border-b border-line px-2.5">
    <h2 id={id} className="ml-5.5 text-xs font-semibold text-1">
      {title}
    </h2>
    {count !== null && (
      <p className="ml-auto font-mono text-xs text-2 tabular-nums">
        <span className="sr-only">{countLabel} </span>
        {count}
      </p>
    )}
  </div>
);

/** 輪の半径。 円周は `2πr`。 ボードは 22 の枠に r=9 で描く。 */
const RING_R = 9;
const RING_C = 2 * Math.PI * RING_R;

/** 「あと N 行」の曲の印。 声の色の弧が、要る単位のうち録った割合を描く。 */
const ProgressRing = ({ progress }: { progress: number }) => {
  const filled = Math.min(Math.max(progress, 0), 1) * RING_C;
  return (
    <svg
      viewBox="0 0 22 22"
      aria-hidden="true"
      focusable="false"
      className="size-6 shrink-0 -rotate-90"
      fill="none"
      strokeWidth="2"
    >
      <circle cx="11" cy="11" r={RING_R} className="stroke-slate-7" />
      {/* 声が無い音源では `text-voice` が解決せず、字の色に落ちる。 */}
      <circle
        cx="11"
        cy="11"
        r={RING_R}
        stroke="currentColor"
        strokeDasharray={`${filled} ${RING_C}`}
        strokeLinecap="round"
        className="text-voice"
      />
    </svg>
  );
};

/** 音域の外の曲の印。 欠けた輪で「届かない」を描く。 */
const OutOfRangeRing = () => (
  <svg
    viewBox="0 0 22 22"
    aria-hidden="true"
    focusable="false"
    className="size-5.5 shrink-0 text-2"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    strokeDasharray="2.5 2.5"
  >
    <circle cx="11" cy="11" r={RING_R} />
  </svg>
);

/** 曲の行の右端の字。 色は状態の段に合わせる。 */
const songStatus = (state: SidebarSongState): ReactNode => {
  switch (state.status) {
    case "ready":
      return <span className="font-semibold text-voice">歌える</span>;
    case "fallback":
      return (
        <span className="font-semibold text-voice">
          歌える<span className="sr-only">（代わりの音を使う）</span>
        </span>
      );
    case "remaining":
      return (
        <span className="font-semibold text-1">
          あと <span className="font-mono tabular-nums">{state.rows}</span>行
        </span>
      );
    case "out_of_range":
      return <span className="text-2">音域外</span>;
  }
};

/** 行の左の印。 未収録は何も置かない——録っていないことは欠けではない。 */
const ROW_ICON: Record<RowStatus, ReactNode> = {
  unrecorded: null,
  recorded: <Icon name="check" size={13} />,
  needs_retake: <Icon name="retake" size={13} />,
  excluded: <Icon name="minus" size={13} />,
};

/**
 * 収録の画面の左の列。 上に曲（歌えるようになるまであと何行か）、下に行（録音リストの行と状態）。
 *
 * 寸法は `docs/reports/ux/redesign-canvas/boards/S04-Record.dc.html`。 値は props で受け、
 * 取りに行かない。 録る・止める、聴いている状態の表示は持たない。
 * キーボードのショートカットも持たない。 Tab と Enter / Space、矢印キーで動かせる。
 *
 * # 役割
 *
 * 曲は `listbox` にせず、ボタンの並びにする。 曲の行には「選ぶ」と「歌う」の2つの操作があり、
 * `option` の中に `button` は置けない（`option` の子は操作として届かない。 axe も
 * `nested-interactive` で落とす）。 `option` の外へ ▶ を出すと、`listbox` の子が `option` だけでは
 * なくなる。 だから行ごとに2つのボタンを並べ、Tab で順に届く形にした。 数は少ない。
 * 選ぶほうは `aria-pressed` のトグルで、目標を外す操作も同じボタンでできる。
 *
 * 行は `listbox`。 中に操作を持たず、選ぶだけの長い一覧なので、Tab で1回入り、中は
 * 矢印で動く形（roving tabindex）がちょうど合う。 数百行を Tab で通らせない。
 * 矢印は焦点だけを動かし、選ぶのは Enter / Space と押下（焦点に付いて選ぶと、
 * 矢印を押すたびに中央の行が替わる）。 Tab で入る先は選んでいる行で、無ければ先頭。
 *
 * 行の名前は中身から作る。 見える字（モーラ）と、読み上げだけの状態の字と ♪ の意味。
 *
 * # 送り
 *
 * 一覧は列の中だけで縦に送れる。 選んだ行が見えない位置にあれば、見える位置まで送る
 * （ページは送らない）。 下端は 85% から消えていき、続きがあることを言う。
 */
export const RecordSidebar = ({
  songs,
  rows,
  selectedRowId,
  onSelectRow,
  selectedSongId,
  songRowIds,
  onSelectSong,
  onPlaySong,
}: RecordSidebarProps) => {
  const songsId = useId();
  const rowsId = useId();
  const listbox = useRef<HTMLDivElement>(null);
  // 焦点が一覧の中にあるあいだだけ持つ。 出たら選んでいる行へ戻す。
  const [focusId, setFocusId] = useState<string | null>(null);

  const recorded = rows.filter((r) => r.status === "recorded").length;
  const has = (id: string | null) => id !== null && rows.some((r) => r.id === id);
  const entryId = has(focusId) ? focusId : has(selectedRowId) ? selectedRowId : rows[0]?.id;
  const target = songs.find((s) => s.id === selectedSongId);

  // 選んだ行が窓の外なら、一覧の中だけを送る。 `scrollIntoView` は祖先まで送る。
  useEffect(() => {
    const box = listbox.current;
    const chosen = box?.querySelector<HTMLElement>('[aria-selected="true"]');
    if (box === null || box === undefined || chosen === null || chosen === undefined) return;
    const top = chosen.offsetTop;
    const bottom = top + chosen.offsetHeight;
    if (top < box.scrollTop) box.scrollTop = top;
    else if (bottom > box.scrollTop + box.clientHeight) box.scrollTop = bottom - box.clientHeight;
  }, [selectedRowId, rows.length]);

  const onKeyDown = (event: KeyboardEvent<HTMLElement>, rowId: string) => {
    const here = event.currentTarget;
    const next: Record<string, Element | null> = {
      ArrowDown: here.nextElementSibling,
      ArrowUp: here.previousElementSibling,
      Home: here.parentElement?.firstElementChild ?? null,
      End: here.parentElement?.lastElementChild ?? null,
    };
    if (event.key === "Enter" || event.key === " ") {
      // Space は既定だと一覧を送ってしまう。
      event.preventDefault();
      onSelectRow(rowId);
      return;
    }
    if (event.key in next) {
      event.preventDefault();
      (next[event.key] as HTMLElement | null)?.focus();
    }
  };

  return (
    <div className="flex h-full min-h-0 w-65 shrink-0 flex-col gap-2.5 border-r border-line px-2.5 py-3.5">
      <section aria-labelledby={songsId} className="flex shrink-0 flex-col">
        <SectionHeader id={songsId} title="曲" />
        {songs.length === 0 ? (
          <p className="px-2.5 pt-2 text-md text-2">曲はまだありません。</p>
        ) : (
          <ul className="flex max-h-[40vh] flex-col gap-0.5 overflow-y-auto pt-1.5">
            {songs.map((song) => {
              const selected = song.id === selectedSongId;
              const out = song.state.status === "out_of_range";
              return (
                <li key={song.id} className={`h-13 gap-2.5 px-3 ${entry({ selected })}`}>
                  {isSingable(song.state) ? (
                    <button
                      type="button"
                      aria-label={`${song.title} を歌う`}
                      onClick={() => onPlaySong(song.id)}
                      className="flex size-5.5 shrink-0 items-center justify-center rounded-full bg-primary text-voice-ink hover:bg-primary-hover active:bg-primary-active"
                    >
                      <Icon name="play-small" size={10} />
                    </button>
                  ) : (
                    <span className="flex size-5.5 shrink-0 items-center justify-center">
                      {out ? <OutOfRangeRing /> : <ProgressRing progress={song.progress} />}
                    </span>
                  )}
                  <button
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onSelectSong(selected ? null : song.id)}
                    className="flex h-full min-w-0 flex-1 items-center gap-2.5 text-left"
                  >
                    <span className="flex min-w-0 flex-1 flex-col gap-0.5">
                      <span className={`truncate text-base font-semibold ${label({ dim: out })}`}>
                        {song.title}
                      </span>
                      {song.range !== null && (
                        <span className="font-mono text-xs text-2">{song.range}</span>
                      )}
                    </span>
                    <span className="shrink-0 text-md">
                      {song.note === undefined ? (
                        songStatus(song.state)
                      ) : (
                        <span className="font-mono text-xs text-2 tabular-nums">{song.note}</span>
                      )}
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-labelledby={rowsId} className="flex min-h-0 flex-1 flex-col gap-2.5">
        <SectionHeader
          id={rowsId}
          title="行"
          count={rows.length === 0 ? null : `${recorded} / ${rows.length}`}
          countLabel="録った行"
        />
        {rows.length === 0 ? (
          <p className="px-2.5 text-md text-2">行はまだありません。</p>
        ) : (
          <div
            ref={listbox}
            role="listbox"
            aria-labelledby={rowsId}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget)) setFocusId(null);
            }}
            // `relative` は `offsetTop` の基準。
            className="relative flex min-h-0 flex-1 flex-col overflow-y-auto mask-b-from-85%"
          >
            {rows.map((row) => {
              const selected = row.id === selectedRowId;
              const needed = target !== undefined && songRowIds.has(row.id);
              return (
                <div
                  key={row.id}
                  role="option"
                  aria-selected={selected}
                  tabIndex={row.id === entryId ? 0 : -1}
                  onClick={() => onSelectRow(row.id)}
                  onFocus={() => setFocusId(row.id)}
                  onKeyDown={(event) => onKeyDown(event, row.id)}
                  // 枠の外に輪を出すと、送りの窓で切れる。 内側に引く。
                  className={`h-8.5 gap-2 px-2.5 focus-visible:-outline-offset-2 ${entry({ selected })} ${selected ? "shadow-voice-edge" : ""}`}
                >
                  <span aria-hidden="true" className="flex w-3.5 shrink-0 text-2">
                    {ROW_ICON[row.status]}
                  </span>
                  <span
                    className={`min-w-0 truncate text-md ${label({ dim: row.status === "recorded" })}`}
                  >
                    {row.text}
                  </span>
                  <span className="sr-only">{ROW_STATUS_LABEL[row.status]}</span>
                  {needed && (
                    <span title={`${target.title} に要る行`} className="ml-auto flex text-2">
                      <Icon name="music" size={13} />
                      <span className="sr-only">{target.title} に要る行</span>
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
};
