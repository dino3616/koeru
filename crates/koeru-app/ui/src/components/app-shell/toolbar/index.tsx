import { Link, useNavigate, useRouter, useRouterState, useSearch } from "@tanstack/react-router";
import { useId } from "react";

import { Glyph } from "~/components/app-shell/glyph";
import {
  useSidebarCollapsed,
  useSidebarToggle,
  useToolbarEndRef,
} from "~/components/app-shell/shell-state";
import { SIDEBAR_ID } from "~/components/app-shell/sidebar";
import { subbankLabel, type SubbankItem } from "~/components/app-shell/subbank";
import { Button } from "~/components/button";
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
 * 上の外枠。 畳む、戻る・進む、パンくず、そして場の画面が置く右端。
 *
 * 窓の全幅に掛かる。 サイドバーの上にも載るので、畳んでも位置が動かない。
 * 見た目だけ全幅で、DOM はサイドバーの次——Tab は サイドバー → ツールバー →
 * 中身 の順に進む。
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
  const value = subbanks?.some((s) => s.id === chosen) ? chosen : undefined;

  return (
    <header className="absolute inset-x-0 top-0 flex h-toolbar items-center gap-1 px-3">
      <Button
        variant="ghost"
        size="icon"
        aria-label="サイドバー"
        aria-expanded={!collapsed}
        aria-controls={SIDEBAR_ID}
        onClick={toggle}
      >
        <Glyph name="sidebar" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label="戻る"
        disabled={historyIndex <= 0}
        onClick={() => router.history.back()}
      >
        <Glyph name="back" />
      </Button>
      <Button
        variant="ghost"
        size="icon"
        aria-label="進む"
        onClick={() => router.history.forward()}
      >
        <Glyph name="forward" />
      </Button>

      <nav aria-label="いまの場所" className="min-w-0 flex-1 pl-2">
        <ol className="flex items-center gap-2 text-sm">
          {voice === undefined ? null : (
            <li className="min-w-0 shrink">
              {voiceIsCurrent ? (
                <span aria-current="page" className="block truncate font-semibold text-slate-12">
                  {voice.name}
                </span>
              ) : (
                <Link
                  to="/voicebank"
                  search={{ id: voice.id }}
                  className="block truncate text-slate-11 hover:text-slate-12"
                >
                  {voice.name}
                </Link>
              )}
            </li>
          )}
          {voice === undefined || scoped === undefined || subbanks === null ? null : (
            <>
              <li aria-hidden="true" className="text-slate-11">
                ›
              </li>
              <li className="shrink-0">
                {/* 見えている字（選んだ枠）の前に、何の選択かを足す。 上書きすると見えるものと読まれるものが食い違う。 */}
                <span id={labelId} className="sr-only">
                  サブバンク
                </span>
                <Select
                  value={value}
                  onValueChange={(next) =>
                    // 行や項目は元の枠のものなので持ち越さない。
                    void navigate({ to: scoped, search: { id: voice.id, subbank: next } })
                  }
                >
                  <SelectTrigger
                    id={triggerId}
                    size="sm"
                    aria-labelledby={`${labelId} ${triggerId}`}
                  >
                    <SelectValue placeholder="サブバンクを選ぶ" />
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
              {voice === undefined ? null : (
                <li aria-hidden="true" className="text-slate-11">
                  ›
                </li>
              )}
              <li className="min-w-0 shrink">
                <span aria-current="page" className="block truncate font-semibold text-slate-12">
                  {leaf}
                </span>
              </li>
            </>
          )}
        </ol>
      </nav>

      <div ref={endRef} className="flex shrink-0 items-center gap-3" />
    </header>
  );
};
