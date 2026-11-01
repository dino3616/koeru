import { Link, useSearch } from "@tanstack/react-router";
import { useState } from "react";

import { Avatar } from "~/components/avatar";
import { Glyph, type GlyphName } from "~/components/app-shell/glyph";
import { groupByVoiceColor, methodName, type SubbankItem } from "~/components/app-shell/subbank";
import { voiceName as displayName } from "~/components/app-shell/voice-name";
import type { VoiceView } from "~/lib/ipc";
import { SCREEN_NAMES } from "~/lib/screen-names";
import { tv } from "~/lib/tv";

/** 開閉ボタン（`Toolbar`）が指す先。 窓に1つしか無いので固定の名前でよい。 */
export const SIDEBAR_ID = "app-sidebar";

/** 棚に並べる音源1つ。 */
export type ShelfVoice = {
  id: string;
  name: string;
  /** 声から作った色。 1つも録れていなければ `null`。 */
  color: VoiceView | null;
};

/**
 * 場の5つ。 並びはボードのとおり。 名前は経路から引く（`SCREEN_NAMES`）。
 *
 * 場の名前の横にショートカットのチップを出さない。
 */
const FIELDS = [
  { to: "/voicebank", glyph: "home" },
  { to: "/record", glyph: "mic" },
  { to: "/songs", glyph: "music" },
  { to: "/review", glyph: "wave" },
  { to: "/distribute", glyph: "package" },
] as const satisfies readonly { to: string; glyph: GlyphName }[];

type SidebarProps = {
  /** 64 のアイコン列に畳むか。 */
  collapsed: boolean;
  /** 棚に並べる音源。 `null` は読んでいるあいだ。 */
  voices: readonly ShelfVoice[] | null;
  /** 一覧を読めなかった。 */
  voicesFailed?: boolean | undefined;
  /** 選んでいる音源。 新しい音源や設定では無い。 */
  selectedId: string | undefined;
  /** 選んだ音源の枠。 `null` は読んでいるあいだ。 */
  subbanks: readonly SubbankItem[] | null;
  /** 枠を読めなかった。 */
  subbanksFailed?: boolean | undefined;
};

const sidebar = tv({
  variants: {
    collapsed: {
      true: "w-sidebar-collapsed",
      false: "w-sidebar",
    },
  },
});

const row = tv({
  variants: {
    collapsed: {
      true: "justify-center",
      false: "gap-2 px-3",
    },
  },
});

/*
 * 行の見た目。 選んでいるものは面（段 4）と太さと、左の細い線で言う。
 *
 * 線だけが声の色で、面は色を持たない。 声が1つも録れていない音源（と、音源を
 * 選ぶ前の画面）では `bg-voice` が宣言ごと無効になり、線は出ない——
 * そのとき選択は面と太さだけで伝わる。 「灰色の声」を線に塗らない。
 *
 * 高さは持たない。 行ごとに違う（棚と場は 40、枠は 36）。
 */
const ROW =
  "relative flex w-full items-center rounded-lg text-sm text-slate-12 hover:bg-slate-3 aria-[current]:bg-slate-4 aria-[current]:font-semibold aria-[current]:before:absolute aria-[current]:before:inset-y-2 aria-[current]:before:left-0 aria-[current]:before:w-1 aria-[current]:before:rounded-full aria-[current]:before:bg-voice";

const SECTION_LABEL = "px-3 pt-4 pb-1 text-xs font-semibold text-slate-11";

/**
 * 左の外枠。 音源の棚、選んだ音源の枠の木、場の5つ。
 *
 * 畳むと顔と場の絵だけの列になる。 字は読み上げのために残す（`sr-only`）——
 * 絵だけにすると、押せるものの名前が無くなる。 木は畳むと出さない。
 *
 * 木を開けるのは選んでいる音源だけ。 枠を読めるのは開いている音源だけで
 * （`TR-REC-30`）、棚の他の音源の木は引けない。
 *
 * `<main>` で包まない。 `<main>` と見出しは場の画面が持つ。 見出しも置かない
 * ——ここが先に来る DOM で `h2` から始まると、画面の `h1` より前に低い段が立つ。
 */
export const Sidebar = ({
  collapsed,
  voices,
  voicesFailed,
  selectedId,
  subbanks,
  subbanksFailed,
}: SidebarProps) => {
  const selected = voices?.find((v) => v.id === selectedId);
  // 本人が閉じた木は、別の音源へ移っても開き直さない。
  const [treeOpen, setTreeOpen] = useState(true);

  const text = collapsed ? "sr-only" : "truncate";

  return (
    <div id={SIDEBAR_ID} className={`flex h-full shrink-0 flex-col ${sidebar({ collapsed })}`}>
      {/*
        ツールバーが載る分を空ける。 スクロールする領域の外に置く——中に入れると、
        スクロールした行がツールバーの下をくぐって押せなくなる。
      */}
      <div aria-hidden="true" className="h-toolbar shrink-0" />
      <nav
        aria-label="音源と場"
        className="flex min-h-0 flex-1 flex-col gap-1 overflow-y-auto px-3 pb-3"
      >
        {/* 押しても何もしない。 開くのは検索とコマンド（#441）で、それまでは押せない形で置く。 */}
        <button
          type="button"
          disabled
          className={`flex h-10 w-full shrink-0 cursor-not-allowed items-center rounded-lg border border-line text-sm text-slate-11 disabled:opacity-45 ${row({ collapsed })}`}
        >
          <Glyph name="search" />
          <span className={text}>検索・コマンド</span>
        </button>

        {collapsed ? <hr className="my-1 border-t border-line" /> : null}
        <p id="shelf-label" className={collapsed ? "sr-only" : SECTION_LABEL}>
          音源
        </p>
        {voicesFailed ? (
          <p role="alert" className={collapsed ? "sr-only" : "px-3 py-2 text-xs text-slate-11"}>
            音源の一覧を読めませんでした
          </p>
        ) : voices === null ? (
          <p role="status" className={collapsed ? "sr-only" : "px-3 py-2 text-xs text-slate-11"}>
            音源を読んでいます
          </p>
        ) : (
          <ul aria-labelledby="shelf-label" className="flex flex-col gap-1">
            {voices.map((voice) => {
              const isSelected = voice.id === selectedId;
              return (
                <li key={voice.id}>
                  <div className="relative">
                    <Link
                      to="/voicebank"
                      search={{ id: voice.id }}
                      {...(isSelected ? { inactiveProps: { "aria-current": "true" } } : {})}
                      className={`${ROW} h-10 ${row({ collapsed })} ${isSelected && !collapsed ? "pr-10" : ""}`}
                    >
                      {/* 名前は隣の字が言う。 顔の名前まで読むと、同じ名前が2度続く。 */}
                      <span aria-hidden="true">
                        <Avatar name={voice.name} size={24} color={voice.color} />
                      </span>
                      <span className={text}>{displayName(voice.name)}</span>
                    </Link>
                    {isSelected && !collapsed ? (
                      <button
                        type="button"
                        aria-expanded={treeOpen}
                        aria-label="サブバンク"
                        onClick={() => setTreeOpen((open) => !open)}
                        className="absolute inset-y-0 right-0 flex w-10 items-center justify-center rounded-lg text-slate-11 hover:text-slate-12"
                      >
                        <Glyph name={treeOpen ? "chevron-down" : "chevron-right"} />
                      </button>
                    ) : null}
                  </div>
                  {isSelected && !collapsed && treeOpen ? (
                    <SubbankTree
                      voiceId={voice.id}
                      voiceName={voice.name}
                      subbanks={subbanks}
                      failed={subbanksFailed === true}
                    />
                  ) : null}
                </li>
              );
            })}
          </ul>
        )}

        <Link to="/new" className={`${ROW} h-10 ${row({ collapsed })}`}>
          <Glyph name="plus" />
          <span className={text}>新しい音源</span>
        </Link>

        {selected === undefined ? null : (
          <>
            {collapsed ? <hr className="my-1 border-t border-line" /> : null}
            <p id="fields-label" className={collapsed ? "sr-only" : SECTION_LABEL}>
              {displayName(selected.name)}
            </p>
            <ul aria-labelledby="fields-label" className="flex flex-col gap-1">
              {FIELDS.map((field) => (
                <li key={field.to}>
                  <Link
                    to={field.to}
                    search={{ id: selected.id }}
                    className={`${ROW} h-10 ${row({ collapsed })}`}
                  >
                    <Glyph name={field.glyph} />
                    <span className={text}>{SCREEN_NAMES[field.to]}</span>
                  </Link>
                </li>
              ))}
            </ul>
          </>
        )}
      </nav>
    </div>
  );
};

type SubbankTreeProps = {
  voiceId: string;
  voiceName: string;
  subbanks: readonly SubbankItem[] | null;
  failed: boolean;
};

/**
 * 選んだ音源の枠。 声色ごとに、収録音階と方式（`TR-RCL-28`）。
 *
 * 枠は識別子で指す（`DEC-RCL-018`）。 声色・方式・音階の綴りを鍵にしない。
 * 付け替えると綴りは変わるが、識別子は変わらない。
 */
const SubbankTree = ({ voiceId, voiceName, subbanks, failed }: SubbankTreeProps) => {
  // 収録や原音設定にいるあいだは、その場で扱っている枠にも印を付ける。
  const { subbank: working } = useSearch({ strict: false });

  if (failed) {
    return (
      <p role="alert" className="py-2 pr-3 pl-12 text-xs text-slate-11">
        サブバンクを読めませんでした
      </p>
    );
  }
  if (subbanks === null) {
    return (
      <p role="status" className="py-2 pr-3 pl-12 text-xs text-slate-11">
        サブバンクを読んでいます
      </p>
    );
  }
  if (subbanks.length === 0) {
    return <p className="py-2 pr-3 pl-12 text-xs text-slate-11">サブバンクはまだありません</p>;
  }

  return (
    <ul
      aria-label={`${displayName(voiceName)}のサブバンク`}
      className="flex flex-col pt-1 pb-2 pl-9"
    >
      {groupByVoiceColor(subbanks).map((group) => (
        <li key={group.voiceColor}>
          {/* 名前は下の `ul` が持つ。 見えている字を読み上げに重ねない。 */}
          <p aria-hidden="true" className="px-3 pt-2 pb-1 text-xs text-slate-11">
            {group.voiceColor}
          </p>
          <ul aria-label={group.voiceColor} className="flex flex-col">
            {group.items.map((item) => (
              <li key={item.id}>
                <Link
                  to="/subbank"
                  search={{ id: voiceId, subbank: item.id }}
                  {...(item.id === working ? { inactiveProps: { "aria-current": "true" } } : {})}
                  className={`${ROW} ${row({ collapsed: false })} h-9 text-md`}
                >
                  <span className="w-8 shrink-0 font-mono">{item.tone}</span>
                  <span className="truncate text-slate-11">{methodName(item.method)}</span>
                </Link>
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
};
