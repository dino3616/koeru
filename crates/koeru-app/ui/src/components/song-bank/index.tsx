import { useMutation, useQueryClient, useSuspenseQuery } from "@tanstack/react-query";
import { useEffect, useId, useRef, useState } from "react";

import { Button } from "~/components/button";
import { Card } from "~/components/card";
import { Field } from "~/components/field";
import { api, errorMessage } from "~/lib/ipc";
import type { BankSongView } from "~/lib/ipc";
import { allSongsQuery, ledgerKey } from "~/lib/queries";

/**
 * 受け付ける大きさの上限（バイト）。
 *
 * UST も USTX も歌詞と音高の平文で、長い曲でも数百 KB に収まる。
 * これを超えるものは、まず曲ではない。
 */
const MAX_BYTES = 4 * 1024 * 1024;

type SongBankProps = {
  /** いま開いている音源。台帳を読む鍵に要る（`~/lib/queries`）。 */
  voiceId: string;
  /**
   * 取り込んだ曲を選ぶ。 渡さなければ何もしない。
   *
   * 取り込んだ直後に、その曲の「どこを歌うか」へ入れるため。
   * 一覧の下まで探しに行かせない。
   */
  onImported?: (songId: string) => void;
};

/**
 * 自分の曲バンク（`TR-RCL-12`）。
 *
 * > 主経路は本人が持ち込む UST / USTX
 *
 * 曲バンクを同梱しない。 何を目標にするかは本人が決めるので、
 * 取り込む口をここに置く。取り込んだ曲データは配布パッケージに含めない。
 *
 * **USTX は1トラックが1曲として入る。** ハモリを主旋律と同じノート列へ
 * 混ぜると、同じ拍に複数の歌詞が並び、範囲を選ぶ画面で「サビだけ」を指せない。
 *
 * 題はファイル名から採る。 `New Project.ustx` のまま並ぶと、
 * どれがどれか分からない。その場でも後からでも変えられる。
 *
 * 外した曲も並べる。 一覧から消すと、戻す的がどこにも無くなる。
 */
export const SongBank = ({ voiceId, onImported }: SongBankProps) => {
  const inputId = useId();
  const queryClient = useQueryClient();
  const { data: songs } = useSuspenseQuery(allSongsQuery(voiceId));
  const [editing, setEditing] = useState<string | null>(null);

  const invalidate = () => void queryClient.invalidateQueries({ queryKey: ledgerKey });

  const bring = useMutation({
    mutationFn: async (file: File) => {
      if (file.size > MAX_BYTES) {
        throw new Error("ファイルが大きすぎます。4 MB までにしてください。");
      }
      const bytes = Array.from(new Uint8Array(await file.arrayBuffer()));
      return api.importSongs(bytes, file.name);
    },
    onSuccess: (imported) => {
      invalidate();
      // USTX は1トラックが1曲。 先頭（＝先に書かれていたトラック）を選ぶ。
      const first = imported[0];
      if (first !== undefined) onImported?.(first.id);
    },
  });

  const rename = useMutation({
    mutationFn: ({ id, title }: { id: string; title: string }) => api.renameSong(id, title),
    onSuccess: () => {
      setEditing(null);
      invalidate();
    },
  });

  const shelve = useMutation({
    mutationFn: ({ id, inBank }: { id: string; inBank: boolean }) => api.setSongInBank(id, inBank),
    onSuccess: invalidate,
  });

  const failure = bring.error ?? rename.error ?? shelve.error;

  return (
    <Card title="曲を持ち込む">
      <label className="text-xs text-slate-11" htmlFor={inputId}>
        UST / USTX のファイル
      </label>
      <input
        id={inputId}
        type="file"
        accept=".ust,.ustx"
        disabled={bring.isPending}
        onChange={(e) => {
          const file = e.target.files?.[0];
          // 同じファイルを選び直せるようにする。値を残すと、2回目の change が飛ばない。
          e.target.value = "";
          if (file !== undefined) bring.mutate(file);
        }}
        className="min-w-0 select-text text-sm text-slate-12 file:mr-3 file:h-9 file:rounded-lg file:border file:border-slate-7 file:bg-slate-3 file:px-3 file:text-sm file:text-slate-12"
      />
      <p className="text-xs text-slate-11">
        歌詞が仮名で書かれたものを読めます。取り込んだ曲は配り物に入りません。
      </p>

      {/*
        取り込んだ数を言う（`TR-SYN-33` と同じ理由）。押したあとを無言にしない。
        USTX はトラックの数だけ増えるので、1ファイルでも1曲とは限らない。
      */}
      {bring.data !== undefined && (
        <p role="status" className="text-xs text-slate-11">
          <span className="font-mono tabular-nums">{bring.data.length}</span> 曲を取り込みました。
        </p>
      )}

      {failure !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(failure)}
        </p>
      )}

      <hr className="h-px border-0 bg-slate-6" />

      {/*
        「外す」が何をするかを1行で言う。 工程の名前（曲バンク）では書かない
        ——画面の言葉は「歌いたい曲から」で揃えてある（`RecordingOrder`）。
      */}
      <p className="text-xs text-slate-11">
        外した曲は「あと何音で歌えるか」の数えから抜けます。曲そのものは消えません。
      </p>

      {songs.length === 0 ? (
        <p className="text-sm text-slate-11">まだ1曲もありません。</p>
      ) : (
        <ul className="flex flex-col gap-3">
          {songs.map((song) =>
            song.id === editing ? (
              <li key={song.id}>
                <Rename
                  song={song}
                  busy={rename.isPending}
                  onSubmit={(title) => rename.mutate({ id: song.id, title })}
                  onCancel={() => setEditing(null)}
                />
              </li>
            ) : (
              <li key={song.id} className="flex items-center justify-between gap-3">
                <span className="flex min-w-0 flex-col">
                  <span className="text-sm text-slate-12">{song.title}</span>
                  {/*
                    出典と許諾（`TR-RCL-12` (f)）。配り物に入るかどうかがここで読める。

                    外してあることを字で出す。 的の名前（「戻す」）だけで言うと、
                    どちらの状態にいるのかを押す前に読めない。
                  */}
                  <span className="flex flex-wrap gap-x-2 text-xs text-slate-11">
                    <span className="tabular-nums">{song.notes} 拍</span>
                    <span>{song.license}</span>
                    {!song.in_bank && <span>外してあります</span>}
                  </span>
                </span>
                <span className="flex shrink-0 items-center gap-1">
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={`${song.title} の題を変える`}
                    onClick={() => setEditing(song.id)}
                  >
                    題を変える
                  </Button>
                  {/*
                    外したものも一覧に残す（`TR-RCL-12` の「本人が外せる」）。
                    消してしまうと、間違えて外した曲を戻せない。
                  */}
                  <Button
                    size="sm"
                    variant="ghost"
                    aria-label={
                      song.in_bank ? `${song.title} を目標から外す` : `${song.title} を目標に戻す`
                    }
                    disabled={shelve.isPending}
                    onClick={() => shelve.mutate({ id: song.id, inBank: !song.in_bank })}
                  >
                    {song.in_bank ? "外す" : "戻す"}
                  </Button>
                </span>
              </li>
            ),
          )}
        </ul>
      )}
    </Card>
  );
};

type RenameProps = {
  song: BankSongView;
  busy: boolean;
  onSubmit: (title: string) => void;
  onCancel: () => void;
};

/** 題を打ち直す行。 開いているのは常に1行だけ。 */
const Rename = ({ song, busy, onSubmit, onCancel }: RenameProps) => {
  const fieldId = useId();
  const field = useRef<HTMLInputElement>(null);
  const [draft, setDraft] = useState(song.title);
  const trimmed = draft.trim();
  const changed = trimmed !== "" && trimmed !== song.title;

  /*
    開いたら欄へ焦点を移す。 押した的（「題を変える」）はこの行と
    入れ替わって消えるので、移さないと焦点が body へ落ちる。

    `autoFocus` にしない。 属性で書くと、読み込み時にも焦点を奪う形と
    区別が付かない（`jsx-a11y/no-autofocus`）。ここは押した結果として動く。
  */
  useEffect(() => {
    field.current?.focus();
  }, []);

  return (
    <div className="flex flex-col gap-2">
      <Field
        id={fieldId}
        ref={field}
        label="題"
        hint="一覧と試唱に、この題で並びます。"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          // 変換確定の Enter で送信しない。 IME の変換中も `keydown` は
          // `Enter` で飛ぶので、見ないと変換途中の題で保存してしまう。
          if (e.key === "Enter" && !e.nativeEvent.isComposing && changed) onSubmit(trimmed);
          if (e.key === "Escape") onCancel();
        }}
      />
      <span className="flex items-center gap-2">
        <Button
          size="sm"
          variant="secondary"
          disabled={!changed || busy}
          onClick={() => onSubmit(trimmed)}
        >
          {busy ? "変えています" : "変える"}
        </Button>
        <Button size="sm" variant="ghost" onClick={onCancel}>
          やめる
        </Button>
      </span>
    </div>
  );
};
