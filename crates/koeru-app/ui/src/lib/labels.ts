/*
 * Rust の識別子を、画面に出す日本語へ直す。
 *
 * `~/lib/ipc` にも同じ役目のものがある（`micModeLabel`）。 あちらは
 * 境界の層に属するもので、ここは画面の語彙。 分けているのは、
 * ここが `docs/design/direction.md` の言葉の規律に従うため——
 * 内部表現の名前を出さない、評価しない、単位を省かない。
 */

/** 方式の名前。まだ単独音しか作れない（`PROFILE-M5` で増える）。 */
const METHODS: Record<string, string> = {
  single: "単独音",
  vcv: "連続音",
  cvvc: "CVVC",
};

/** 方式。読めなければ「作り方が分かりません」。 */
export const methodLabel = (method: string | null): string =>
  method === null ? "作り方が分かりません" : (METHODS[method] ?? "知らない作り方");

/**
 * 低確信度の主因（`TR-ALN-26` (3)）。
 *
 * 用語を出さない。 要件が「境界が曖昧 / 他のテイクと違う / 音が割れている 等」と
 * 例示しているとおり、成分の名前ではなく起きていることを書く。
 * 上級モードの数値表示は `PROFILE-M6`（`Q-PLT-004` 待ち）。
 */
const CAUSES: Record<string, string> = {
  "confidence.path": "どこで切るか決めきれませんでした",
  "confidence.sharpness": "音の変わり目がはっきりしません",
  "confidence.prior": "ほかの回と切り方が違います",
  "confidence.acoustic": "音が割れているか、小さすぎます",
};

/** 主因。無ければ `null`——「理由なし」と「まだ分からない」を分ける。 */
export const causeLabel = (cause: string | null): string | null =>
  cause === null ? null : (CAUSES[cause] ?? "理由を特定できませんでした");

/** 確認の進め方（`TR-ALN-25`）。 */
const REVIEW_MODES: Record<string, string> = {
  individual: "1件ずつ確認",
  batch: "まとめて確認",
  suggest_rerecord: "録り直しをすすめる",
};

/** 確認の進め方。読めなければ既定の言い方に倒す。 */
export const reviewModeLabel = (mode: string): string =>
  REVIEW_MODES[mode] ?? REVIEW_MODES["individual"] ?? mode;

/**
 * 5値の名前（`TR-ALN-30`）。
 *
 * `take-values` の言い換えと同じ語を使う。 同じ値が画面の場所によって
 * 別の名前で出ると、固定を解く的がどれを指しているのか分からなくなる。
 */
const SLOTS: Record<string, string> = {
  offset: "頭の余白",
  preutterance: "歌い出しの位置",
  overlap: "前の音との重なり",
  consonant: "伸ばさないところ",
  cutoff: "終わりの余白",
};

/** 5値の名前。 */
export const slotLabel = (slot: string): string => SLOTS[slot] ?? slot;
