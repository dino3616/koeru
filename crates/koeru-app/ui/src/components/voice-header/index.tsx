import { Button } from "~/components/button";
import { methodLabel } from "~/lib/labels";
import { useScreenFocus } from "~/lib/use-screen-focus";

/** 音源の面の切り替え。工程の動詞にしない（`DEC-PLT-024`）。 */
export const VOICE_TABS = [
  { id: "sound", label: "音" },
  { id: "songs", label: "曲" },
  { id: "package", label: "配り物" },
  { id: "settings", label: "設定" },
] as const;

export type VoiceTab = (typeof VOICE_TABS)[number]["id"];

type VoiceHeaderProps = {
  name: string;
  method: string | null;
  /** いま何で録っているか。1行だけ（`DEC-PLT-024`）。 */
  deviceName: string | null;
  tab: VoiceTab;
  onTab: (tab: VoiceTab) => void;
  onBack: () => void;
};

/**
 * 音源の面の帯。
 *
 * 面はオブジェクトで切る（`DEC-PLT-024`）。 「収録 / 整える / 手渡す」に
 * しない——工程で切ると、対象は同じでも「3つの段階を通過する」という
 * 読み方が残り、既存ツールの分断を製品の内側で作り直すことになる。
 *
 * 準備は設定の面にある。 ここに残すのは「いま何で録っているか」の1行だけ。
 * 一度で済むものを、毎回同じ大きさで置かない。
 */
export const VoiceHeader = ({ name, method, deviceName, tab, onTab, onBack }: VoiceHeaderProps) => {
  /*
   * 焦点はここで移す。
   *
   * ref を props で受け取らない。 見出しを持っているのはこの部品なので、
   * 外から ref を差すと「誰が焦点を持つか」が呼び出し側に散る。
   */
  const heading = useScreenFocus();

  return (
    <header className="flex h-16 flex-shrink-0 items-center gap-5 border-slate-6 border-b px-8">
      <Button variant="ghost" onClick={onBack}>
        <svg
          width="16"
          height="16"
          viewBox="0 0 16 16"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M10 3 5 8l5 5" />
        </svg>
        声
      </Button>

      <div className="flex items-baseline gap-3">
        <h1
          ref={heading}
          tabIndex={-1}
          className="select-text text-xl font-semibold text-slate-12 outline-none"
        >
          {name}
        </h1>
        <p className="text-xs text-slate-11">
          {methodLabel(method)}
          {deviceName !== null && ` · ${deviceName} で録っています`}
        </p>
      </div>

      <nav className="ml-auto flex gap-2" aria-label="音源">
        {VOICE_TABS.map((t) => (
          <button
            key={t.id}
            type="button"
            onClick={() => onTab(t.id)}
            {...(t.id === tab ? { "aria-current": "page" as const } : {})}
            className={`inline-flex h-11 items-center px-3 text-sm ${
              t.id === tab
                ? "font-semibold text-slate-12 shadow-[inset_0_-2px_0_var(--slate-12)]"
                : "text-slate-11 hover:text-slate-12"
            }`}
          >
            {t.label}
          </button>
        ))}
      </nav>
    </header>
  );
};
