import { useId, useLayoutEffect, useRef, useState } from "react";

import { Button } from "~/components/button";
import { Field } from "~/components/field";
import { Icon } from "~/components/icon";
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
  /** ＋ を押したとき。 この行をカウントから録り直し、新しいテイクにする。 */
  onNew: () => void;
  /** 録っているあいだなど、いま切り替えさせない。 */
  disabled?: boolean;
  /** マイクが無いなど、いま録れない。 ＋ だけを止める。 */
  cannotRecord?: boolean;
};

/** 名前が無ければ「テイク N」。 名前の既定は Rust ではなく画面が持つ（`TakeSummaryView.label`）。 */
export const takeName = (take: Pick<TakeChoice, "generation" | "label">): string =>
  take.label ?? `テイク ${take.generation}`;

/** ⋮ の幅（20px）と、選んでいる項目の右の余白（2px）。 */
const KEBAB_PX = 20;
const KEBAB_INSET_PX = 2;

/**
 * いまの行のテイクを切り替え、名前を変える。 骨格は S04 の右上（「テイク 1」「ちょい弱め」と ＋）。
 *
 * 切り替えは採用の切り替え。 中央の波形と F0、合成に使う音が、選んだテイクになる。
 * 録り直しは上書きしないので（`TR-REC-21`）、前のテイクへいつでも戻せる。
 *
 * ⋮ は選んでいる項目に重ねて見せるが、DOM では群の外の兄弟に置く。 `radio` の中にも
 * `radiogroup` の子にもボタンを置けないため。 位置は選んでいる項目から測る。 押すと
 * 名前の欄が出る——テイクを消す口が無いので、メニューにする中身がこれしかない。
 *
 * 名前を変える欄は押してから出す。 常に欄を置くと、録るたびに中央の上が字の入力で
 * 埋まる。
 */
export const TakeSwitch = ({
  takes,
  adopted,
  onAdopt,
  onRename,
  onNew,
  disabled = false,
  cannotRecord = false,
}: TakeSwitchProps) => {
  const fieldId = useId();
  /** 打っている名前。 欄を出していなければ `null`。 */
  const [draft, setDraft] = useState<string | null>(null);
  const group = useRef<HTMLDivElement>(null);
  const kebab = useRef<HTMLSpanElement>(null);

  const current = takes.find((t) => t.id === adopted) ?? takes.at(-1);

  // 位置は DOM の実測でしか決まらない。 描く前に変数へ書き、state を経由しない。
  useLayoutEffect(() => {
    const box = group.current;
    const button = kebab.current;
    if (box === null || button === null) return;
    const place = () => {
      const checked = box.querySelector<HTMLElement>('[aria-checked="true"]');
      if (checked === null) return;
      const left = checked.offsetLeft + checked.offsetWidth - KEBAB_INSET_PX - KEBAB_PX;
      button.style.setProperty("--kebab", `${left}px`);
    };
    place();
    const watch = new ResizeObserver(place);
    watch.observe(box);
    return () => watch.disconnect();
  }, [current?.id, current?.label, takes.length]);

  if (current === undefined) return null;

  return (
    <div className="flex flex-col items-end gap-2">
      <div className="flex items-center gap-1.5">
        <div className="relative flex">
          <Segment
            aria-label="テイク"
            size="sm"
            reserveTrailing
            groupRef={group}
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
          <span ref={kebab} className="absolute top-1/2 left-(--kebab) flex -translate-y-1/2">
            <Button
              variant="ghost"
              size="icon-2xs"
              aria-label={`${takeName(current)} の名前を変える`}
              disabled={disabled || draft !== null}
              onClick={() => setDraft(current.label ?? "")}
            >
              <Icon name="kebab" size={14} />
            </Button>
          </span>
        </div>
        <Button
          variant="quiet"
          size="icon-xs"
          aria-label="新しいテイク"
          title="新しいテイク"
          disabled={disabled || cannotRecord}
          onClick={onNew}
        >
          <Icon name="plus" size={14} />
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
