import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "~/components/button";
import { Field } from "~/components/field";
import { api, errorMessage } from "~/lib/ipc";
import { methodLabel } from "~/lib/labels";
import { ledgerKey } from "~/lib/queries";

type VoiceSettingsProps = {
  id: string;
  name: string;
  method: string | null;
  /** 録音リストの行の数。 */
  rows: number;
  /** 名前が変わった。呼び側が見出しを描き替える。 */
  onRenamed: (name: string) => void;
};

/**
 * この声の設定（`DEC-PLT-024`、`DEC-PKG-007`）。
 *
 * 名前は必須で、あとから変えられる。 表示名は完成の条件なので
 * （`TR-PKG-34`）、未設定のまま被覆だけ満ちる経路を残さない。一方で
 * 名前で止まらせない（`UC-F01-02`）ので、仮の名前で通過できるようにしてある。
 *
 * 空にはできない。 完成の条件を後から崩すことになる（`DEC-PKG-007`）。
 * 弾くのは Rust 側で、ここは押させないだけ。
 *
 * 作り方は変わらない。 最初に選んだまま。変える経路は `PROFILE-M5` で入る。
 *
 * 枠を持たない。 置く側の領域がそのまま枠になる（`docs/design/direction.md` の
 * 部品の粒度）。ここで `Card` を返すと、領域の枠の中にもう1枚枠が出る。
 */
export const VoiceSettings = ({ id, name, method, rows, onRenamed }: VoiceSettingsProps) => {
  const nameId = useId();
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState(name);

  const rename = useMutation({
    mutationFn: (next: string) => api.renameProject(id, next),
    onSuccess: (_, next) => {
      onRenamed(next);
      /*
        台帳の鍵ごと無効化する。 一覧だけ無効化していたので、
        `voice_state` が持つ表示名は溜まったまま残っていた——
        **音源を開き直すと、画面の名前だけ古いほうへ戻る**
        （`VoiceBody` は初期値としてそれを読む）。名前は manifest にあり、
        `voice_state` も一覧も同じところから読む。
      */
      void queryClient.invalidateQueries({ queryKey: ledgerKey });
    },
  });

  const trimmed = draft.trim();
  const changed = trimmed !== "" && trimmed !== name;

  return (
    <>
      <Field
        id={nameId}
        label="名前"
        hint="空にはできません。配るときにも、この名前が付きます。"
        value={draft}
        onChange={(e) => setDraft(e.target.value)}
        onKeyDown={(e) => {
          // 変換確定の Enter で送信しない。 IME の変換中も `keydown` は
          // `Enter` で飛ぶので、見ないと変換途中の名前で保存してしまう。
          if (e.key === "Enter" && !e.nativeEvent.isComposing && changed) rename.mutate(trimmed);
        }}
      />
      <Button
        variant="secondary"
        size="sm"
        onClick={() => rename.mutate(trimmed)}
        disabled={!changed || rename.isPending}
      >
        {rename.isPending ? "変えています" : "名前を変える"}
      </Button>

      {rename.error !== null && (
        <p role="alert" className="text-sm text-red-11">
          {errorMessage(rename.error)}
        </p>
      )}

      <hr className="h-px border-0 bg-slate-6" />

      <dl className="grid grid-cols-[auto_1fr] gap-x-3 gap-y-2 text-xs text-slate-11">
        <dt>作り方</dt>
        <dd className="m-0 font-mono text-slate-12 tabular-nums">
          {methodLabel(method)} · {rows} 行
        </dd>
      </dl>
      <p className="text-xs text-slate-11">作り方は最初に選んだまま変わりません。</p>
    </>
  );
};
