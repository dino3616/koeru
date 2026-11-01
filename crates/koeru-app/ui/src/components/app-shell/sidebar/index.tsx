import { Link, useSearch } from "@tanstack/react-router";
import { useState } from "react";

import { Avatar } from "~/components/avatar";
import {
  groupByVoiceColor,
  methodName,
  subbankLabel,
  type SubbankItem,
} from "~/components/app-shell/subbank";
import { voiceName as displayName } from "~/components/app-shell/voice-name";
import { Icon, type IconName } from "~/components/icon";
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
 * 線の太さは畳んだ列（S04）のもの。 開いた列（S06）は 1.6 で揃えて描く。
 *
 * 場の名前の横にショートカットのチップを出さない。
 */
const FIELDS = [
  { to: "/voicebank", icon: "home", stroke: 1.6 },
  { to: "/record", icon: "mic", stroke: 1.7 },
  { to: "/songs", icon: "music", stroke: 1.6 },
  { to: "/review", icon: "wave", stroke: 1.6 },
  { to: "/distribute", icon: "package", stroke: 1.6 },
] as const satisfies readonly { to: string; icon: IconName; stroke: number }[];

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

/*
 * 寸法は2枚のボードの写し（`DEC-PLT-062` の丸めだけ当てる）。
 * 畳んだ列は S04-Record、開いた列は S06-Voice。
 */
const nav = tv({
  variants: {
    collapsed: {
      true: "w-sidebar-collapsed items-center gap-2 pt-1 pb-2",
      false: "w-sidebar gap-1 px-3 pt-0.5 pb-3",
    },
  },
});

/** 畳んだ列の的。 40px の角丸で、いまの場は主ボタンと同じ塗り。 */
const RAIL_TARGET =
  "flex size-10 shrink-0 items-center justify-center rounded-lg text-2 hover:bg-surface-2 hover:text-1 aria-[current=page]:bg-primary aria-[current=page]:text-voice-ink";

/*
 * 棚の行。 選んでいる音源は `voice-wash` の面と太さで言う（S06）。
 *
 * 面を2枚重ねる。 声が1つも録れていない音源では `voice-wash` が宣言ごと無効になり、
 * 透明に落ちる。 下に `surface-3` を敷いておけば、そのときも選択は面で伝わる。
 * 「灰色の声」を混ぜて代わりにしない。
 */
const VOICE_ROW =
  "relative isolate flex h-9 w-full items-center gap-2 rounded-lg pr-2.5 pl-1 text-sm text-1 hover:bg-surface-2 aria-[current]:bg-surface-3 aria-[current]:font-semibold aria-[current]:before:absolute aria-[current]:before:inset-0 aria-[current]:before:-z-10 aria-[current]:before:rounded-lg aria-[current]:before:bg-voice-wash";

const SECTION_LABEL = "px-2.5 pb-1.5 text-xs font-semibold tracking-[0.08em] text-2";

/**
 * 左の外枠。 音源の棚、選んだ音源の枠の木、場の5つ。
 *
 * 畳むと、選んでいる音源の顔と場の絵だけの列になる（S04）。 音源を選んでいない
 * あいだだけ、顔の代わりに棚の顔を並べる——場が無いので、そうしないと列から
 * どこへも行けない。 字は読み上げのために残す（`sr-only` か `aria-label`）。
 *
 * 木を開けるのは選んでいる音源だけ。 枠を読めるのは開いている音源だけで
 * （`TR-REC-30`）、棚の他の音源の木は引けない。
 *
 * `<main>` で包まない。 `<main>` と見出しは場の画面が持つ。 見出しも置かない
 * ——ここが先に来る DOM で `h2` から始まると、画面の `h1` より前に低い段が立つ。
 */
export const Sidebar = (props: SidebarProps) => (
  <div
    id={SIDEBAR_ID}
    className={`flex h-full shrink-0 flex-col ${props.collapsed ? "w-sidebar-collapsed" : "w-sidebar"}`}
  >
    {/*
      ツールバーが載る分を空ける。 スクロールする領域の外に置く——中に入れると、
      スクロールした行がツールバーの下をくぐって押せなくなる。
    */}
    <div aria-hidden="true" className="h-toolbar shrink-0" />
    {props.collapsed ? <Rail {...props} /> : <Expanded {...props} />}
  </div>
);

/** 押しても何もしない。 開くのは検索とコマンド（#441）で、それまでは押せない形で置く。 */
const SEARCH_LABEL = "検索・コマンド";

const Rail = ({ voices, voicesFailed, selectedId }: SidebarProps) => {
  const selected = voices?.find((v) => v.id === selectedId);

  return (
    <nav
      aria-label="音源と場"
      className={`flex min-h-0 flex-1 flex-col overflow-y-auto ${nav({ collapsed: true })}`}
    >
      {selected === undefined ? (
        <Shelf voices={voices} failed={voicesFailed === true} />
      ) : (
        <>
          <Link
            to="/voicebank"
            search={{ id: selected.id }}
            aria-label={displayName(selected.name)}
            inactiveProps={{ "aria-current": "true" }}
            className="mb-3 flex shrink-0 rounded-lg"
          >
            <span aria-hidden="true">
              <Avatar name={selected.name} size={36} color={selected.color} />
            </span>
          </Link>
          <ul aria-label={displayName(selected.name)} className="flex flex-col gap-2">
            {FIELDS.map((field) => (
              <li key={field.to}>
                <Link
                  to={field.to}
                  search={{ id: selected.id }}
                  aria-label={SCREEN_NAMES[field.to]}
                  className={RAIL_TARGET}
                >
                  <Icon name={field.icon} size={20} strokeWidth={field.stroke} />
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
      <button
        type="button"
        disabled
        aria-label={SEARCH_LABEL}
        className="mt-auto flex size-10 shrink-0 cursor-not-allowed items-center justify-center rounded-lg border border-line text-2"
      >
        <Icon name="search" size={18} />
      </button>
    </nav>
  );
};

/** 畳んだ列で、音源を選んでいないあいだだけ並べる棚。 */
const Shelf = ({ voices, failed }: { voices: readonly ShelfVoice[] | null; failed: boolean }) => {
  if (failed) {
    return (
      <p role="alert" className="sr-only">
        音源の一覧を読めませんでした
      </p>
    );
  }
  if (voices === null) {
    return (
      <p role="status" className="sr-only">
        音源を読んでいます
      </p>
    );
  }
  return (
    <ul aria-label="音源" className="flex flex-col gap-2">
      {voices.map((voice) => (
        <li key={voice.id}>
          <Link
            to="/voicebank"
            search={{ id: voice.id }}
            aria-label={displayName(voice.name)}
            className="flex rounded-lg"
          >
            <span aria-hidden="true">
              <Avatar name={voice.name} size={36} color={voice.color} />
            </span>
          </Link>
        </li>
      ))}
      <li>
        <Link to="/new" aria-label="新しい音源" className={RAIL_TARGET}>
          <Icon name="plus" size={18} strokeWidth={2} />
        </Link>
      </li>
    </ul>
  );
};

const Expanded = ({ voices, voicesFailed, selectedId, subbanks, subbanksFailed }: SidebarProps) => {
  const selected = voices?.find((v) => v.id === selectedId);
  // 本人が閉じた木は、別の音源へ移っても開き直さない。
  const [treeOpen, setTreeOpen] = useState(true);

  return (
    <nav
      aria-label="音源と場"
      className={`flex min-h-0 flex-1 flex-col overflow-y-auto ${nav({ collapsed: false })}`}
    >
      <button
        type="button"
        disabled
        className="flex h-8.5 w-full shrink-0 cursor-not-allowed items-center gap-2 rounded-lg border border-line px-2.5 text-md text-2"
      >
        <Icon name="search" size={15} />
        {SEARCH_LABEL}
      </button>

      <p id="shelf-label" className={`pt-3.5 ${SECTION_LABEL}`}>
        音源
      </p>
      {voicesFailed ? (
        <p role="alert" className="px-2.5 py-2 text-xs text-2">
          音源の一覧を読めませんでした
        </p>
      ) : voices === null ? (
        <p role="status" className="px-2.5 py-2 text-xs text-2">
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
                    className={VOICE_ROW}
                  >
                    {/*
                      開閉の印の席。 選んでいない音源の木は引けないので、閉じた印を絵として置くだけ。
                      選んでいる音源では、上に重ねた開閉のボタンがこの席に来る。
                    */}
                    <span aria-hidden="true" className="flex w-4 shrink-0 justify-center text-2">
                      {isSelected ? null : <Icon name="tree-closed" size={12} />}
                    </span>
                    {/* 名前は隣の字が言う。 顔の名前まで読むと、同じ名前が2度続く。 */}
                    <span aria-hidden="true">
                      <Avatar name={voice.name} size={24} color={voice.color} />
                    </span>
                    <span className="truncate">{displayName(voice.name)}</span>
                  </Link>
                  {isSelected ? (
                    <button
                      type="button"
                      aria-expanded={treeOpen}
                      aria-label="サブバンク"
                      onClick={() => setTreeOpen((open) => !open)}
                      // 的は 24 の幅で取る（`TR-PLT-31`）。 印は 16 の席の真ん中に来る。
                      className="absolute inset-y-0 left-0 flex w-6 items-center justify-center rounded-lg text-2 hover:text-1"
                    >
                      <Icon name={treeOpen ? "tree-open" : "tree-closed"} size={12} />
                    </button>
                  ) : null}
                </div>
                {isSelected && treeOpen ? (
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

      <Link
        to="/new"
        className="flex h-9 shrink-0 items-center gap-2.5 rounded-lg px-2.5 text-md text-2 hover:bg-surface-2 hover:text-1 aria-[current]:bg-surface-3 aria-[current]:font-semibold aria-[current]:text-1"
      >
        <Icon name="plus" size={14} strokeWidth={2} />
        新しい音源
      </Link>

      {selected === undefined ? null : (
        <>
          <p id="fields-label" className={`pt-4 ${SECTION_LABEL}`}>
            {displayName(selected.name)}
          </p>
          <ul aria-labelledby="fields-label" className="flex flex-col gap-1">
            {FIELDS.map((field) => (
              <li key={field.to}>
                <Link
                  to={field.to}
                  search={{ id: selected.id }}
                  className="flex h-9 items-center gap-2.5 rounded-lg px-2.5 text-sm text-1 hover:bg-surface-2 aria-[current=page]:bg-surface-3 aria-[current=page]:font-semibold"
                >
                  <Icon name={field.icon} size={17} strokeWidth={1.6} />
                  <span className="truncate">{SCREEN_NAMES[field.to]}</span>
                </Link>
              </li>
            ))}
          </ul>
        </>
      )}
    </nav>
  );
};

type SubbankTreeProps = {
  voiceId: string;
  voiceName: string;
  subbanks: readonly SubbankItem[] | null;
  failed: boolean;
};

const TREE_NOTE = "py-2 pr-2.5 pl-7.5 text-xs text-2";

/**
 * 選んだ音源の枠。 「声色 | 音階 | 方式」の3列（`TR-RCL-28`）。
 *
 * 声色ごとにまとめ、声色の名前はまとまりの最初の行にだけ書く（S06）。
 * まとまりの名前は読み上げに `ul` の名前で届ける。
 *
 * 枠は識別子で指す（`DEC-RCL-018`）。 声色・方式・音階の綴りを鍵にしない。
 * 付け替えると綴りは変わるが、識別子は変わらない。
 */
const SubbankTree = ({ voiceId, voiceName, subbanks, failed }: SubbankTreeProps) => {
  // 収録や原音設定にいるあいだは、その場で扱っている枠にも印を付ける。
  const { subbank: working } = useSearch({ strict: false });

  if (failed) {
    return (
      <p role="alert" className={TREE_NOTE}>
        サブバンクを読めませんでした
      </p>
    );
  }
  if (subbanks === null) {
    return (
      <p role="status" className={TREE_NOTE}>
        サブバンクを読んでいます
      </p>
    );
  }
  if (subbanks.length === 0) {
    return <p className={TREE_NOTE}>サブバンクはまだありません</p>;
  }

  return (
    <ul
      aria-label={`${displayName(voiceName)}のサブバンク`}
      className="mt-0.5 mb-1.5 flex flex-col gap-px"
    >
      {groupByVoiceColor(subbanks).map((group) => (
        <li key={group.voiceColor}>
          <ul aria-label={group.voiceColor} className="flex flex-col gap-px">
            {group.items.map((item, index) => (
              <li key={item.id}>
                <Link
                  to="/subbank"
                  search={{ id: voiceId, subbank: item.id }}
                  title={subbankLabel(item)}
                  {...(item.id === working ? { inactiveProps: { "aria-current": "true" } } : {})}
                  className="ml-5.5 flex h-7 items-center gap-2 rounded-md pr-2.5 pl-2 text-xs text-1 hover:bg-surface-2 aria-[current]:bg-surface-2 aria-[current]:font-semibold"
                >
                  {/* 名前は `ul` が言う。 見えている字を読み上げに重ねない。 */}
                  <span aria-hidden="true" className="min-w-0 flex-1 truncate">
                    {index === 0 ? group.voiceColor : null}
                  </span>
                  <span className="w-7 shrink-0 font-mono tabular-nums">{item.tone}</span>
                  <span className="w-10 shrink-0 truncate text-2">{methodName(item.method)}</span>
                </Link>
              </li>
            ))}
          </ul>
        </li>
      ))}
    </ul>
  );
};
