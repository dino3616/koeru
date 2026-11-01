import {
  type ReactNode,
  createContext,
  useContext,
  useEffect,
  useId,
  useLayoutEffect,
  useState,
  useSyncExternalStore,
} from "react";

import { TERMS, type TermEntry, type TermId } from "~/lib/terms";

/*
 * 画面ごとの「どの用語を、どの `Term` が先に出したか」。
 *
 * 描画の中で数えない。 描画は捨てられたり2回走ったりするので、そこで
 * 「出した」と記録すると、画面に残らなかった描画が初出の席を取ったままになる。
 * 席を取るのは commit のあと（`useLayoutEffect`）で、commit の順は木の順なので、
 * 再描画や StrictMode の二重実行で初出が移らない。 外れたとき（unmount）だけ次の席へ移る。
 *
 * layout effect で登録するのは、paint の前に決まるようにするため。 `useEffect` だと、
 * 全部が初出の顔で一度描かれてから薄くなる。
 */
const createRegistry = () => {
  const owners = new Map<TermId, string[]>();
  const listeners = new Set<() => void>();
  const notify = () => {
    for (const listener of listeners) listener();
  };
  return {
    subscribe: (listener: () => void) => {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    /** まだ誰も席を取っていなければ、自分が初出になる。 */
    isFirst: (term: TermId, id: string) => {
      const ids = owners.get(term);
      return ids === undefined || ids.length === 0 || ids[0] === id;
    },
    register: (term: TermId, id: string) => {
      owners.set(term, [...(owners.get(term) ?? []), id]);
      notify();
      return () => {
        owners.set(
          term,
          (owners.get(term) ?? []).filter((other) => other !== id),
        );
        notify();
      };
    },
  };
};

type Registry = ReturnType<typeof createRegistry>;

const Scope = createContext<Registry | null>(null);

/**
 * 初出を数える範囲。 画面の根に1つ置く。
 *
 * 画面ごとに数えるのは、別の画面へ移ったら、読み手にとっては初めて見る場所だから。
 * 範囲の外に `Term` を置くと落とす——黙って毎回初出の顔にすると、薄くならないまま
 * 誰も気づかない。
 */
export const TermScope = ({ children }: { children: ReactNode }) => {
  const [registry] = useState(createRegistry);
  return <Scope.Provider value={registry}>{children}</Scope.Provider>;
};

/** ⓘ。 初出は塗り、2回目以降は線だけで細く描く。 どちらも字の段 11 以上で、見えなくはしない。 */
const Glyph = ({ first }: { first: boolean }) => (
  <svg
    viewBox="0 0 16 16"
    aria-hidden="true"
    focusable="false"
    className="size-4 shrink-0"
    fill="none"
    stroke="currentColor"
    strokeWidth={first ? 1.5 : 1}
  >
    <circle cx="8" cy="8" r="6.5" className={first ? "fill-slate-12" : ""} />
    <path
      d="M8 7.2 V11.2"
      strokeLinecap="round"
      className={first ? "stroke-slate-1" : ""}
      strokeWidth={first ? 1.75 : 1}
    />
    <circle
      cx="8"
      cy="4.9"
      r="0.9"
      stroke="none"
      className={first ? "fill-slate-1" : "fill-slate-11"}
    />
  </svg>
);

/**
 * 用語と、その隣の ⓘ。
 *
 * 説明は常に DOM にあり、`aria-describedby` で ⓘ に結ぶ。 出る・隠れるは見た目だけで、
 * 読み上げはホバーにもフォーカスにも依らず同じ文を受ける。 ホバーだけで出る仕組みにしない
 * （`TR-PLT-26`）。 ⓘ は `<button>` なので Tab で届き、Esc で畳める。
 *
 * 名前は「用語＋の説明」。 ⓘ の字は読ませず、見えている字の上に `aria-label` を置く形にもならない。
 *
 * 説明の面は影を持たない（`docs/reports/ux/direction.md`）。 輪郭は 1px の段 7。
 *
 * 押せる範囲は 24px 四方（`TR-PLT-31`）。
 */
export const Term = ({ term }: { term: TermId }) => {
  const registry = useContext(Scope);
  if (registry === null) {
    throw new Error("`Term` は `TermScope` の中に置く。画面の根に `TermScope` を足す");
  }
  return <TermBody term={term} registry={registry} />;
};

const TermBody = ({ term, registry }: { term: TermId; registry: Registry }) => {
  const id = useId();
  const popupId = useId();
  const [open, setOpen] = useState(false);
  const entry: TermEntry = TERMS[term];

  const first = useSyncExternalStore(
    registry.subscribe,
    () => registry.isFirst(term, id),
    () => true,
  );
  useLayoutEffect(() => registry.register(term, id), [registry, term, id]);

  // 開いているあいだだけ Esc を受ける。 閉じているのに握ると、外側のダイアログの Esc を奪う。
  // 見張るのは文書で、フォーカスの無いホバーだけの読み手にも畳む手段を残す。
  useEffect(() => {
    if (!open) return;
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.stopPropagation();
      setOpen(false);
    };
    document.addEventListener("keydown", onKeyDown, true);
    return () => document.removeEventListener("keydown", onKeyDown, true);
  }, [open]);

  return (
    // 操作を持たない。 ホバーの範囲を ⓘ と説明の両方にまたがせるためだけの外枠。
    <span
      role="presentation"
      className="relative inline-flex items-center gap-1"
      onPointerEnter={() => setOpen(true)}
      onPointerLeave={() => setOpen(false)}
    >
      <span>{entry.name}</span>
      <button
        type="button"
        aria-label={`${entry.name}の説明`}
        aria-describedby={popupId}
        data-first={first}
        className={`inline-flex h-6 w-6 items-center justify-center rounded-full ${
          first ? "text-slate-12" : "text-slate-11"
        }`}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
        onClick={() => setOpen(true)}
      >
        <Glyph first={first} />
      </button>
      <span
        id={popupId}
        role="tooltip"
        hidden={!open}
        className="absolute top-full left-0 z-10 w-72 pt-1 text-left"
      >
        {/* 外側の `pt-1` が ⓘ との隙間を埋める。 隙間があると、説明へ指を移す途中で閉じる。 */}
        <span className="block rounded-xl border border-slate-7 bg-slate-3 px-4 py-3">
          <span className="block text-sm font-semibold text-slate-12">{entry.name}</span>
          <span className="mt-1 block text-xs leading-relaxed text-slate-12">{entry.summary}</span>
          {entry.note !== undefined && (
            <span className="mt-2 block text-xs text-slate-11">{entry.note}</span>
          )}
        </span>
      </span>
    </span>
  );
};
