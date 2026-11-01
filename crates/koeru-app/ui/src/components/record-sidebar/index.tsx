import { type ComponentProps, type KeyboardEvent, useEffect, useId, useRef, useState } from "react";

import { RowMark, SongMark } from "~/components/status-mark";
import type { RowStatus } from "~/components/status-mark";
import { tv } from "~/lib/tv";

/** 曲の状態。 「あと N 行」の N は `remaining` が持つ。 */
export type SidebarSongState = ComponentProps<typeof SongMark>;

/** 左の列の曲1つ。 `SongView` からの写しは `./from-views` が持つ。 */
export type SidebarSong = {
  id: string;
  title: string;
  /** 音域（「A3–D4」）。 `SongView` に無いので、出せるときだけ渡す。 */
  range: string | null;
  state: SidebarSongState;
  /**
   * 題の右に出す補助の字。 聴いているあいだの再生位置（「0:12 / 1:48」）のように、
   * 状態とは別に曲の行へ足したいものを置く。
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
  /** 行を絞っている曲。 絞っていなければ `null`。 絞るのは行を渡す側で、ここは印を付けるだけ。 */
  selectedSongId: string | null;
  /** 曲を押したとき。 選んでいる曲をもう一度押すと `null`（絞りを外す）。 */
  onSelectSong: (songId: string | null) => void;
  /** 曲の ▶ を押したとき。 鳴らすのは受け取った側で、ここは知らせるだけ。 */
  onPlaySong: (songId: string) => void;
};

/*
 * 選んでいるものは、面と左の線で言う。 色だけに頼らない。
 * 線の色は声の色。 声が無い音源では `border-color` が解決せず、字の色に落ちる。
 *
 * 線は選んでいないものにも透明で置く。 置かないと、選んだ瞬間に字が 2px ずれる。
 */
const entry = tv({
  variants: {
    selected: {
      true: "border-voice bg-surface-3",
      false: "border-transparent hover:bg-surface-2",
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
  /** 「3 / 4」。 `null` なら出さない。 */
  count: string | null;
  /** 数の読み上げ。 見える字に前置きする。 */
  countLabel: string;
};

const SectionHeader = ({ id, title, count, countLabel }: SectionHeaderProps) => (
  <div className="flex h-7 shrink-0 items-center justify-between gap-2 border-b border-line px-2.5">
    <h2 id={id} className="text-xs font-semibold text-1">
      {title}
    </h2>
    {count !== null && (
      <p className="font-mono text-xs text-2 tabular-nums">
        <span className="sr-only">{countLabel} </span>
        {count}
      </p>
    )}
  </div>
);

const PlayIcon = () => (
  <svg viewBox="0 0 10 10" aria-hidden="true" focusable="false" className="size-2.5">
    <path d="M2.5 1.2 L8.6 5 L2.5 8.8 Z" fill="currentColor" />
  </svg>
);

/**
 * 収録の画面の左の列。 上に曲（歌えるようになるまであと何行か）、下に行（録音リストの行と状態）。
 *
 * 骨格は `docs/reports/ux/redesign-canvas/boards/S04-Record.dc.html`。 値は props で受け、
 * 取りに行かない。 録る・止める、聴いている状態の表示は持たない。
 * キーボードのショートカットも持たない。 Tab と Enter / Space、矢印キーで動かせる。
 *
 * # 役割
 *
 * 曲は `listbox` にせず、ボタンの並びにする。 曲の行には「選ぶ」と「歌う」の2つの操作があり、
 * `option` の中に `button` は置けない（`option` の子は操作として届かない。 axe も
 * `nested-interactive` で落とす）。 `option` の外へ ▶ を出すと、`listbox` の子が `option` だけでは
 * なくなる。 だから行ごとに2つのボタンを並べ、Tab で順に届く形にした。 数は少ない。
 * 選ぶほうは `aria-pressed` のトグルで、絞りを外す操作も同じボタンでできる。
 *
 * 行は `listbox`。 中に操作を持たず、選ぶだけの長い一覧なので、Tab で1回入り、中は
 * 矢印で動く形（roving tabindex）がちょうど合う。 数百行を Tab で通らせない。
 * 矢印は焦点だけを動かし、選ぶのは Enter / Space と押下（焦点に付いて選ぶと、
 * 矢印を押すたびに中央の行が替わる）。 Tab で入る先は選んでいる行で、無ければ先頭。
 *
 * 行の名前は中身から作る。 見える字（モーラ）と、`RowMark` の読み上げの字（状態）。
 *
 * # 送り
 *
 * 一覧は列の中だけで縦に送れる。 選んだ行が見えない位置にあれば、見える位置まで送る
 * （ページは送らない）。 絞りで並びが変わったときも、選んだ行を追う。
 */
export const RecordSidebar = ({
  songs,
  rows,
  selectedRowId,
  onSelectRow,
  selectedSongId,
  onSelectSong,
  onPlaySong,
}: RecordSidebarProps) => {
  const songsId = useId();
  const rowsId = useId();
  const listbox = useRef<HTMLDivElement>(null);
  // 焦点が一覧の中にあるあいだだけ持つ。 出たら選んでいる行へ戻す。
  const [focusId, setFocusId] = useState<string | null>(null);

  const singable = songs.filter((s) => isSingable(s.state)).length;
  const recorded = rows.filter((r) => r.status === "recorded").length;
  const has = (id: string | null) => id !== null && rows.some((r) => r.id === id);
  const entryId = has(focusId) ? focusId : has(selectedRowId) ? selectedRowId : rows[0]?.id;

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
    <div className="flex h-full min-h-0 w-65 shrink-0 flex-col gap-3">
      <section aria-labelledby={songsId} className="flex shrink-0 flex-col">
        <SectionHeader
          id={songsId}
          title="曲"
          count={songs.length === 0 ? null : `${singable} / ${songs.length}`}
          countLabel="歌える曲"
        />
        {songs.length === 0 ? (
          <p className="px-2.5 pt-2 text-md text-2">曲はまだありません。</p>
        ) : (
          <ul className="flex max-h-[40vh] flex-col gap-0.5 overflow-y-auto p-1">
            {songs.map((song) => {
              const selected = song.id === selectedSongId;
              return (
                <li
                  key={song.id}
                  className={`flex items-start gap-2 rounded-lg border-l-2 py-2 pr-2 pl-2 ${entry({ selected })}`}
                >
                  {/* 歌えない曲にも同じ幅を空ける。 題の頭が揃う。 */}
                  {isSingable(song.state) ? (
                    <button
                      type="button"
                      aria-label={`${song.title} を歌う`}
                      onClick={() => onPlaySong(song.id)}
                      className="flex size-7 shrink-0 items-center justify-center rounded-full bg-primary text-voice-ink hover:bg-primary-hover active:bg-primary-active"
                    >
                      <PlayIcon />
                    </button>
                  ) : (
                    <span aria-hidden="true" className="size-7 shrink-0" />
                  )}
                  <button
                    type="button"
                    aria-pressed={selected}
                    onClick={() => onSelectSong(selected ? null : song.id)}
                    className="flex min-w-0 flex-1 flex-col gap-1 text-left"
                  >
                    <span className="flex items-baseline justify-between gap-2">
                      <span
                        className={`truncate text-sm font-semibold ${label({ dim: song.state.status === "out_of_range" })}`}
                      >
                        {song.title}
                      </span>
                      {song.note !== undefined && (
                        <span className="shrink-0 font-mono text-xs text-2 tabular-nums">
                          {song.note}
                        </span>
                      )}
                    </span>
                    {/* 代替ありの札は長い。 並べて収まらなければ、音域の下へ折る。 */}
                    <span className="flex flex-wrap items-center gap-x-2 gap-y-1">
                      {song.range !== null && (
                        <span className="font-mono text-xs text-2">{song.range}</span>
                      )}
                      <SongMark {...song.state} />
                    </span>
                  </button>
                </li>
              );
            })}
          </ul>
        )}
      </section>

      <section aria-labelledby={rowsId} className="flex min-h-0 flex-1 flex-col">
        <SectionHeader
          id={rowsId}
          title="行"
          count={rows.length === 0 ? null : `${recorded} / ${rows.length}`}
          countLabel="録った行"
        />
        {rows.length === 0 ? (
          <p className="px-2.5 pt-2 text-md text-2">行はまだありません。</p>
        ) : (
          <div
            ref={listbox}
            role="listbox"
            aria-labelledby={rowsId}
            onBlur={(event) => {
              if (!event.currentTarget.contains(event.relatedTarget)) setFocusId(null);
            }}
            // `relative` は `offsetTop` の基準。 `p-1` は焦点の輪が端で切れないため。
            className="relative flex min-h-0 flex-1 flex-col overflow-y-auto p-1"
          >
            {rows.map((row) => {
              const selected = row.id === selectedRowId;
              return (
                <div
                  key={row.id}
                  role="option"
                  aria-selected={selected}
                  tabIndex={row.id === entryId ? 0 : -1}
                  onClick={() => onSelectRow(row.id)}
                  onFocus={() => setFocusId(row.id)}
                  onKeyDown={(event) => onKeyDown(event, row.id)}
                  className={`flex h-8.5 shrink-0 items-center gap-2 rounded-lg border-l-2 pr-2 pl-2 ${entry({ selected })}`}
                >
                  <span
                    className={`min-w-0 flex-1 truncate text-md ${label({ dim: row.status === "recorded" })}`}
                  >
                    {row.text}
                  </span>
                  <RowMark status={row.status} compact />
                </div>
              );
            })}
          </div>
        )}
      </section>
    </div>
  );
};
