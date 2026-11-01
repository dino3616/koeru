import { Link, useNavigate, useRouter, useRouterState, useSearch } from "@tanstack/react-router";
import { useId } from "react";

import {
  useSidebarCollapsed,
  useSidebarToggle,
  useToolbarEndRef,
} from "~/components/app-shell/shell-state";
import { SIDEBAR_ID } from "~/components/app-shell/sidebar";
import { methodName, subbankLabel, type SubbankItem } from "~/components/app-shell/subbank";
import { Button } from "~/components/button";
import { Icon } from "~/components/icon";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "~/components/select";
import { SCREEN_NAMES } from "~/lib/screen-names";

type ToolbarProps = {
  /** 選んでいる音源。 新しい音源や設定では無い。 */
  voice: { id: string; name: string } | undefined;
  /** 選んだ音源の枠。 `null` は読んでいるあいだ。 */
  subbanks: readonly SubbankItem[] | null;
};

/**
 * 枠で扱う場。 パンくずに枠の選択が出る。
 *
 * 枠は検索引数 `subbank` で持つ（`/subbank`、`/record`、`/review`）。
 * 曲バンクと配布は音源の全体を扱うので、枠を選ばせない。
 */
const SUBBANK_SCOPED = ["/subbank", "/record", "/review"] as const;

/**
 * 場のページそのもの。 音源の名前や枠の選択が現在地になり、場の名前を重ねない。
 */
const IS_ITS_OWN_LEAF = new Set<string>(["/voicebank", "/subbank"]);

/**
 * 窓を動かす掴み。 Tauri は印を付けた要素そのものを押したときだけ窓を動かし、
 * 子へは継がない。 空きになる箱にはどれも付け、押せる部品には付けない。
 */
const DRAG = { "data-tauri-drag-region": true } as const;

/**
 * 上の外枠。 畳む、戻る・進む、パンくず、そして場の画面が置く右端。
 *
 * 窓の全幅に掛かる。 サイドバーの上にも載るので、畳んでも位置が動かない。
 * 見た目だけ全幅で、DOM はサイドバーの次——Tab は サイドバー → ツールバー →
 * 中身 の順に進む。
 *
 * macOS では信号ボタンがこの行に重なる（`tauri.macos.conf.json`）。 その幅だけ
 * 左を空ける（`traffic-lights`）。 窓の掴みもこの行が兼ねる（`DEC-PLT-062`）。
 *
 * 戻る・進むは履歴をたどる。 進む先があるかはブラウザの履歴から分からないので、
 * 進むは押せるままにする。 先が無ければ何も起きない。
 *
 * パンくずは窓が狭いと省略記号で縮む。 右端は縮まない——場の画面が置いたものが
 * 切れて操作できなくなるより、パンくずが切れるほうがよい（`DEC-PLT-029`）。
 */
export const Toolbar = ({ voice, subbanks }: ToolbarProps) => {
  const router = useRouter();
  const navigate = useNavigate();
  const pathname = useRouterState({ select: (s) => s.location.pathname });
  const historyIndex = useRouterState({ select: (s) => s.location.state.__TSR_index });
  const { subbank: chosen } = useSearch({ strict: false });
  const collapsed = useSidebarCollapsed();
  const toggle = useSidebarToggle();
  const endRef = useToolbarEndRef();
  const labelId = useId();
  const triggerId = useId();

  const scoped = SUBBANK_SCOPED.find((p) => p === pathname);
  const leaf = IS_ITS_OWN_LEAF.has(pathname) ? undefined : SCREEN_NAMES[pathname];
  const voiceIsCurrent = pathname === "/voicebank" || voice === undefined;
  const selected = subbanks?.find((s) => s.id === chosen);

  return (
    <header {...DRAG} className="absolute inset-x-0 top-0 flex h-toolbar items-center gap-2.5">
      <div
        {...DRAG}
        className="flex shrink-0 items-center gap-4 pl-5 traffic-lights:pl-traffic-lights"
      >
        <Button
          variant="quiet"
          size="icon-sm"
          aria-label="サイドバー"
          aria-expanded={!collapsed}
          aria-controls={SIDEBAR_ID}
          onClick={toggle}
        >
          <Icon name="sidebar" size={18} />
        </Button>
        <div role="group" aria-label="履歴" className="-ml-2 flex items-center gap-0.5">
          <Button
            variant="quiet"
            size="icon-sm"
            aria-label="戻る"
            disabled={historyIndex <= 0}
            onClick={() => router.history.back()}
          >
            <Icon name="back" size={16} />
          </Button>
          <Button
            variant="quiet"
            size="icon-sm"
            aria-label="進む"
            onClick={() => router.history.forward()}
          >
            <Icon name="forward" size={16} />
          </Button>
        </div>
      </div>

      <div
        {...DRAG}
        className="flex h-full min-w-0 flex-1 items-center gap-2.5 pr-7 pl-3.5 text-md text-2"
      >
        <nav {...DRAG} aria-label="いまの場所" className="flex h-full min-w-0 flex-1 items-center">
          <ol {...DRAG} className="flex h-full min-w-0 items-center gap-2.5">
            {voice === undefined ? null : (
              <li className="min-w-0 shrink">
                {voiceIsCurrent ? (
                  <span aria-current="page" className="block truncate text-1">
                    {voice.name}
                  </span>
                ) : (
                  <Link
                    to="/voicebank"
                    search={{ id: voice.id }}
                    className="block truncate text-2 hover:text-1"
                  >
                    {voice.name}
                  </Link>
                )}
              </li>
            )}
            {voice === undefined || scoped === undefined || subbanks === null ? null : (
              <>
                <li aria-hidden="true">›</li>
                <li className="shrink-0">
                  {/* 見えている字（選んだ枠）の前に、何の選択かを足す。 上書きすると見えるものと読まれるものが食い違う。 */}
                  <span id={labelId} className="sr-only">
                    サブバンク
                  </span>
                  <Select
                    // 選んでいない間も制御したまま持つ。 `undefined` から値へ移ると、Radix が非制御から制御へ移ったと警告する。
                    value={selected?.id ?? ""}
                    onValueChange={(next) =>
                      // 行や項目は元の枠のものなので持ち越さない。
                      void navigate({ to: scoped, search: { id: voice.id, subbank: next } })
                    }
                  >
                    <SelectTrigger
                      id={triggerId}
                      size="crumb"
                      aria-labelledby={`${labelId} ${triggerId}`}
                    >
                      <SelectValue placeholder="サブバンクを選ぶ">
                        {selected === undefined ? undefined : <SubbankName subbank={selected} />}
                      </SelectValue>
                    </SelectTrigger>
                    <SelectContent>
                      {subbanks.map((s) => (
                        <SelectItem key={s.id} value={s.id}>
                          {subbankLabel(s)}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </li>
              </>
            )}
            {leaf === undefined ? null : (
              <>
                {voice === undefined ? null : <li aria-hidden="true">›</li>}
                <li className="min-w-0 shrink">
                  <span aria-current="page" className="block truncate text-1">
                    {leaf}
                  </span>
                </li>
              </>
            )}
          </ol>
        </nav>

        <div {...DRAG} ref={endRef} className="flex shrink-0 items-center gap-2" />
      </div>
    </header>
  );
};

/**
 * 枠の名乗りを3つに分けて描く。 声色は字のまま、音階は桁の揃う書体、方式は1段落とす。
 *
 * 間は余白で空けるので、見える字をつなぐと「A3CVVC」と読まれる。 読み上げには
 * `subbankLabel` の並びを別に渡し、見える字は読ませない。
 */
const SubbankName = ({ subbank }: { subbank: SubbankItem }) => (
  <>
    <span className="sr-only">{subbankLabel(subbank)}</span>
    <span aria-hidden="true" className="inline-flex items-center whitespace-nowrap">
      {subbank.voiceColor === null || subbank.voiceColor === "" ? null : (
        <span className="mr-1.25 text-1">{subbank.voiceColor}</span>
      )}
      <span className="font-mono text-1 tabular-nums">{subbank.tone}</span>
      <span className="ml-1.5 text-2">{methodName(subbank.method)}</span>
    </span>
  </>
);
