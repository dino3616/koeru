import { Button } from "~/components/button";
import type { SongView } from "~/lib/ipc";

type SongListProps = {
  songs: readonly SongView[];
  /** いま声を用意している曲。押してから鳴るまでを無言にしない（`TR-SYN-33`）。 */
  preparingId: string | null;
  onSing: (id: string) => void;
  /** 曲を選ぶ。曲の面では詳細が右に出る。渡さなければ選べない。 */
  onSelect?: (id: string) => void;
  selectedId?: string | null;
};

/**
 * 曲ごとの状態（`TR-RCL-17`、`TR-RCL-19`、`TR-SYN-20`）。
 *
 * 不足はエイリアス名の一覧ではなく「あと N 音」で出す（`TR-SYN-20`）。
 * 品質スコア、良し悪しの判定、他音源との比較、上達度は出さない。
 *
 * 欠けを不足として書かない（`docs/design/direction.md`）。
 * 「未達」ではなく「◯行を録ると歌えます」。
 *
 * 代替ありは、音のつながりが粗くなることを1行で説明する（`TR-RCL-19`）。
 */
export const SongList = ({
  songs,
  preparingId,
  onSing,
  onSelect,
  selectedId = null,
}: SongListProps) => {
  if (songs.length === 0) {
    return (
      <p className="text-sm text-slate-11">
        曲がありません。曲を持ち込むと、あと何音で歌えるかが出ます。
      </p>
    );
  }

  return (
    <ul className="flex flex-col gap-3">
      {songs.map((s) => {
        const line = s.singable
          ? s.missing_units === 0
            ? "いま歌えます"
            : `近い音で置き換えて、いま歌えます（本来の音まであと ${s.missing_units} 音）`
          : // 単位を1つの節に混ぜない。 録るのは行、そこから取れるのが音。
            `あと ${s.missing_rows} 行（${s.missing_units} 音）を録ると歌えます`;

        const body = (
          <>
            <span className="text-sm text-slate-12">{s.title}</span>
            <span className="text-xs text-slate-11">{line}</span>
          </>
        );

        return (
          <li key={s.id} className="flex items-center justify-between gap-3">
            {onSelect === undefined ? (
              <span className="flex min-w-0 flex-col gap-2">{body}</span>
            ) : (
              <button
                type="button"
                aria-pressed={s.id === selectedId}
                onClick={() => onSelect(s.id)}
                className={`flex min-w-0 flex-1 flex-col gap-2 rounded-lg border p-3 text-left hover:bg-slate-4 ${
                  s.id === selectedId
                    ? "border-slate-7 bg-slate-4"
                    : "border-transparent bg-slate-3"
                }`}
              >
                {body}
              </button>
            )}

            {s.singable && (
              /*
                押したボタン自身を disabled にしない。 フォーカスが body へ落ちる。
                `aria-busy` で状態を伝え、二重起動は呼び出し側が弾く。
              */
              <Button
                size="sm"
                variant="secondary"
                aria-busy={preparingId === s.id}
                aria-label={`${s.title} を歌わせる`}
                onClick={() => preparingId === null && onSing(s.id)}
              >
                {preparingId === s.id ? "用意しています" : "歌わせる"}
              </Button>
            )}
          </li>
        );
      })}
    </ul>
  );
};
