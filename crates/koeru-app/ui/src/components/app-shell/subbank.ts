import type { RecordingMethod, Subbank } from "~/lib/ipc";

/**
 * シェルが枠について知っておくこと。 識別子と、名乗り（声色・音階・方式）だけ。
 *
 * 進み具合は持たない。 サイドバーに出すのは「どの枠があるか」で、
 * 数が動くたびに木を描き直す理由が無い。
 */
export type SubbankItem = Pick<Subbank, "id" | "voiceColor" | "method"> & {
  /** 収録音階の英語音名。 MIDI 番号は出さない（`TR-REC-25`）。 */
  tone: string;
};

/**
 * 方式の名前。
 *
 * `~/lib/labels` の `methodLabel` は音源の作り方（小文字の識別子）を引くもので、
 * 枠の方式（`RecordingMethod`）とは語彙が違う。 同じ辞書に混ぜると、どちらかの
 * 識別子が「知らない作り方」に落ちる。
 */
const METHODS: Record<RecordingMethod, string> = {
  SINGLE: "単独音",
  SEQUENTIAL: "連続音",
  CVVC: "CVVC",
  MULTI_PITCH_SEQUENTIAL: "多音階の連続音",
};

export const methodName = (method: RecordingMethod): string => METHODS[method];

/** 「Normal A3 CVVC」。 声色が無い枠は音階と方式だけ。 */
export const subbankLabel = (subbank: SubbankItem): string =>
  [subbank.voiceColor, subbank.tone, methodName(subbank.method)]
    .filter((part) => part !== null && part !== "")
    .join(" ");

/** 声色が無い枠をまとめる札。 声色の空欄を空のまま見せない。 */
export const NO_VOICE_COLOR = "声色なし";

/**
 * 声色ごとにまとめる。 並びは枠の並び（録る順の既定）で、まとまりの順は最初に現れた順。
 *
 * 並べ替えない。 並びが録る順の既定（`TR-RCL-28`）なので、声色の名前順に
 * 直すと、本人が決めた順が木だけ違って見える。
 */
export const groupByVoiceColor = (
  subbanks: readonly SubbankItem[],
): { voiceColor: string; items: SubbankItem[] }[] => {
  const groups: { voiceColor: string; items: SubbankItem[] }[] = [];
  for (const item of subbanks) {
    const voiceColor =
      item.voiceColor === null || item.voiceColor === "" ? NO_VOICE_COLOR : item.voiceColor;
    const found = groups.find((g) => g.voiceColor === voiceColor);
    if (found) found.items.push(item);
    else groups.push({ voiceColor, items: [item] });
  }
  return groups;
};
