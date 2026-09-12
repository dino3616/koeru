import { useSuspenseQuery } from "@tanstack/react-query";
import { useId, useState } from "react";

import { Button } from "~/components/button";
import { Field } from "~/components/field";
import { methodPresetsQuery } from "~/lib/queries";

type NewVoiceProps = {
  /** 名前を決めて作る。作ったら開く。 */
  onCreate: (displayName: string) => void;
  /** 作っている最中か。 */
  creating: boolean;
  onClose: () => void;
};

/** 秒を「約 N 分」にする。単位を省かない（`docs/design/direction.md`）。 */
const minutes = (seconds: number) => `約 ${Math.max(1, Math.round(seconds / 60))} 分`;

/**
 * 新しく作るところ。
 *
 * 一覧の中に置く（`DEC-PLT-024`）。 名前を打つところと作り方を選ぶところを
 * 兼ねる。環の席は空いたまま——まだ声がないので、そこに座るものが無い。
 *
 * 名前で止まらせない（`UC-F01-02`）。 仮の名前で通過でき、あとから変えられる
 * （`DEC-PKG-007`）ので、ここで判断を求めていない。
 *
 * 作れない作り方を灰色で並べない。 グレーアウトは未完成を失敗として描く形
 * （`docs/design/direction.md`）。**いま作れるものだけを出し、無いものは
 * 1行で「まだ無い」と書く。**
 */
export const NewVoice = ({ onCreate, creating, onClose }: NewVoiceProps) => {
  const nameId = useId();
  const [name, setName] = useState("");
  const { data: presets } = useSuspenseQuery(methodPresetsQuery());

  const submit = () => {
    const trimmed = name.trim();
    if (trimmed === "" || creating) return;
    onCreate(trimmed);
  };

  return (
    <div className="flex w-full max-w-2xl flex-col gap-5">
      <Field
        id={nameId}
        label="名前"
        hint="あとから変えられます。絵文字や記号も使えます。"
        value={name}
        onChange={(e) => setName(e.target.value)}
        onKeyDown={(e) => {
          // 変換確定の Enter で送信しない。 IME で変換しているあいだも
          // `keydown` は `Enter` で飛ぶので、見ないと変換途中の名前で作ってしまう。
          if (e.key === "Enter" && !e.nativeEvent.isComposing) submit();
        }}
        placeholder="ミナ"
      />

      <div className="flex flex-col gap-3">
        <p className="text-sm font-semibold text-slate-11">どこまで作るか</p>
        <ul className="flex flex-col gap-3">
          {presets.map((p) => (
            <li
              key={p.id}
              className="flex flex-col gap-2 rounded-lg border border-slate-7 bg-slate-3 p-3"
            >
              <span className="text-sm font-semibold text-slate-12">{p.label}</span>
              <span className="text-xs text-slate-11">{p.summary}</span>
              <dl className="flex flex-col gap-2">
                <div className="flex items-baseline justify-between gap-3">
                  <dt className="text-xs text-slate-11">かかる時間</dt>
                  <dd className="font-mono text-sm text-slate-12 tabular-nums">
                    {minutes(p.seconds)}
                  </dd>
                </div>
                <div className="flex items-baseline justify-between gap-3">
                  <dt className="text-xs text-slate-11">読む行</dt>
                  <dd className="font-mono text-sm text-slate-12 tabular-nums">
                    {p.rows} 行 · {p.units} 音
                  </dd>
                </div>
                <div className="flex items-baseline justify-between gap-3">
                  <dt className="text-xs text-slate-11">録る回数</dt>
                  <dd className="font-mono text-sm text-slate-12 tabular-nums">{p.passes} 回</dd>
                </div>
                <div className="flex items-baseline justify-between gap-3">
                  <dt className="text-xs text-slate-11">読み上げ</dt>
                  <dd className="text-sm text-slate-12">{p.reading}</dd>
                </div>
              </dl>
              <span className="text-xs text-slate-11">{p.reach}</span>
            </li>
          ))}
        </ul>
        <p className="text-xs text-slate-11">
          いま選べる作り方はこれだけです。読んでいる途中から歌えます。
        </p>
      </div>

      <div className="flex gap-2">
        <Button variant="primary" onClick={submit} disabled={name.trim() === "" || creating}>
          {creating ? "作っています" : "作る"}
        </Button>
        <Button variant="ghost" onClick={onClose}>
          やめる
        </Button>
      </div>
    </div>
  );
};
