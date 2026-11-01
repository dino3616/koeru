import { useId, useState } from "react";

import { Button } from "~/components/button";
import { Field } from "~/components/field";
import { Segment } from "~/components/segment";

/** 切り替えの候補1つ。 `TakeSummaryView` から要るものだけ。 */
export type TakeChoice = {
  id: number;
  /** 何本目か（1 始まり）。 名前が無いときの表示に使う。 */
  generation: number;
  /** 本人が付けた名前。 */
  label: string | null;
  /** 取りこぼしで無効にしたテイク（`TR-REC-07`）。 採用には戻せない。 */
  invalid: boolean;
};

type TakeSwitchProps = {
  takes: readonly TakeChoice[];
  /** いま採用しているテイク。 無ければ最後のテイクを選んでいるものとして出す。 */
  adopted: number | null;
  /** 別のテイクを選んだとき。 採用を切り替えるのは受け取った側（`TR-REC-21`）。 */
  onAdopt: (takeId: number) => void;
  /** 名前を決めたとき。 空にしたら `null`（既定の「テイク N」へ戻す）。 */
  onRename: (takeId: number, label: string | null) => void;
  /** 録っているあいだなど、いま切り替えさせない。 */
  disabled?: boolean;
};

/** 名前が無ければ「テイク N」。 名前の既定は Rust ではなく画面が持つ（`TakeSummaryView.label`）。 */
export const takeName = (take: Pick<TakeChoice, "generation" | "label">): string =>
  take.label ?? `テイク ${take.generation}`;

/**
 * いまの行のテイクを切り替え、名前を変える。 骨格は S04 の右上（「テイク 1」「ちょい明め」）。
 *
 * 切り替えは採用の切り替え。 中央の波形と F0、合成に使う音が、選んだテイクになる。
 * 録り直しは上書きしないので（`TR-REC-21`）、前のテイクへいつでも戻せる。
 *
 * 名前を変える欄は押してから出す。 常に欄を置くと、録るたびに中央の上が字の入力で
 * 埋まる。 消すのと「＋」は口が無いので持たない。
 */
export const TakeSwitch = ({
  takes,
  adopted,
  onAdopt,
  onRename,
  disabled = false,
}: TakeSwitchProps) => {
  const fieldId = useId();
  /** 打っている名前。 欄を出していなければ `null`。 */
  const [draft, setDraft] = useState<string | null>(null);

  const current = takes.find((t) => t.id === adopted) ?? takes.at(-1);
  if (current === undefined) return null;

  return (
    <div className="flex flex-col items-end gap-2">
      <div className="flex items-center gap-2">
        <Segment
          aria-label="テイク"
          value={String(current.id)}
          onValueChange={(value) => {
            // 前のテイクの名前を打ちかけたまま、別のテイクへ当てない。
            setDraft(null);
            onAdopt(Number(value));
          }}
          options={takes.map((t) => ({
            value: String(t.id),
            label: takeName(t),
            disabled: disabled || t.invalid,
          }))}
        />
        <Button
          variant="ghost"
          size="sm"
          disabled={disabled || draft !== null}
          onClick={() => setDraft(current.label ?? "")}
        >
          名前を変える
        </Button>
      </div>
      {draft !== null && (
        <form
          className="flex items-end gap-2"
          onSubmit={(event) => {
            event.preventDefault();
            const label = draft.trim();
            onRename(current.id, label === "" ? null : label);
            setDraft(null);
          }}
        >
          <Field
            id={fieldId}
            label={`${takeName(current)} の新しい名前（空にすると「テイク ${current.generation}」）`}
            value={draft}
            onChange={(event) => setDraft(event.target.value)}
          />
          <Button type="submit" variant="secondary">
            決める
          </Button>
          <Button type="button" variant="ghost" onClick={() => setDraft(null)}>
            やめる
          </Button>
        </form>
      )}
    </div>
  );
};
