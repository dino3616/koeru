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
