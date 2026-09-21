import { useSuspenseQuery } from "@tanstack/react-query";
import { Suspense, useId, useState } from "react";

import { Button } from "~/components/button";
import { Field } from "~/components/field";
import { TonePicker } from "~/components/tone-picker";
import { DEFAULT_TONE_MIDI } from "~/lib/tones";
import { methodPresetsQuery } from "~/lib/queries";
import { cx } from "~/lib/tv";

type NewVoiceProps = {
  /** 名前・作り方・収録音高を決めて作る。作ったら開く。 */
  onCreate: (displayName: string, presetId: string, tones: number[]) => void;
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
 *
 * 作り方は選ばせる（`TR-RCL-11`）。 出すのは所要時間の代表値1個、到達点、
 * 想定回数、読み上げの難しさ。差が5分未満のものは Rust 側で畳んであるので、
 * ここに並んだものは互いに意味のある差を持つ。
 *
 * **収録音高は作り方とは別に選ぶ**（`TR-RCL-01`）。 本数が所要時間に比例するので、
 * 音高の欄を先に置き、作り方の所要時間はその本数で計算し直す。
 *
 * ラジオで組む。 「どれか1つ」を選ぶ形が役割として伝わり、
 * 矢印キーで行き来できる。
 */
export const NewVoice = ({ onCreate, creating, onClose }: NewVoiceProps) => {
  const nameId = useId();
  const groupId = useId();
  const [name, setName] = useState("");
  const [tones, setTones] = useState<number[]>([DEFAULT_TONE_MIDI]);
  const { data: presets } = useSuspenseQuery(methodPresetsQuery(tones.length));
  // 先頭は所要時間がいちばん短いもの（Rust 側が並べている）。
  // 初めての人が最初に触るので、いちばん軽いところに置く。
  const [presetId, setPresetId] = useState(presets[0]?.id ?? "");

  const submit = () => {
    const trimmed = name.trim();
    if (trimmed === "" || creating || presetId === "" || tones.length === 0) return;
    onCreate(trimmed, presetId, tones);
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

      <fieldset className="flex flex-col gap-3">
        <legend className="text-sm font-semibold text-slate-11">音の高さ</legend>
        <Suspense fallback={<p className="text-xs text-slate-11">読み込んでいます</p>}>
          <TonePicker tones={tones} onChange={setTones} />
        </Suspense>
      </fieldset>

      <fieldset className="flex flex-col gap-3">
        <legend className="text-sm font-semibold text-slate-11" id={groupId}>
          どこまで作るか
        </legend>
        <ul className="flex flex-col gap-3">
          {presets.map((p) => (
            <li key={p.id}>
              <label
                className={cx(
                  "flex cursor-pointer flex-col gap-2 rounded-lg border bg-slate-3 p-3",
                  // 選択中は面と境界を1段濃くする。 色相では伝えない——
                  // ラジオの状態は `checked` が持っていて、色は補助（`TR-PLT-28`）。
                  p.id === presetId
                    ? "border-slate-9 bg-slate-4"
                    : "border-slate-7 hover:border-slate-8",
                )}
              >
                <span className="flex items-center gap-2">
                  <input
                    type="radio"
                    name="method-preset"
                    value={p.id}
                    checked={p.id === presetId}
                    onChange={() => setPresetId(p.id)}
                    className="h-4 w-4 accent-slate-11"
                  />
                  <span className="text-sm font-semibold text-slate-12">{p.label}</span>
                </span>
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
              </label>
            </li>
          ))}
        </ul>
        <p className="text-xs text-slate-11">
          あとから作り方は変えられません。読んでいる途中から歌えます。
        </p>
      </fieldset>

      <div className="flex gap-2">
        <Button
          variant="primary"
          onClick={submit}
          disabled={name.trim() === "" || creating || presetId === "" || tones.length === 0}
        >
          {creating ? "作っています" : "作る"}
        </Button>
        <Button variant="ghost" onClick={onClose}>
          やめる
        </Button>
      </div>
    </div>
  );
};
