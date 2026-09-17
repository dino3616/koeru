import { Button } from "~/components/button";
import type { ReviewItemView } from "~/lib/ipc";
import { causeLabel } from "~/lib/labels";

type ReviewEntryProps = {
  /** 選んでいる音の確認待ち。済んでいれば `null`。 */
  item: ReviewItemView | null;
  /** まとめて確認へ切り替えたあとは、1件ずつ確定できない（`REQ-ALN-008`）。 */
  individual: boolean;
  /** この音を確認して確定させる。 */
  onConfirm: () => void;
  /** oto を直すのではなく録り直す（`REQ-ALN-009`, `TR-ALN-27`）。 */
  onRerecord: () => void;
  busy: boolean;
};

/**
 * 選んでいる音の確認（`TR-ALN-26`, `TR-ALN-27`）。
 *
 * 面を持たない（`DEC-PLT-024`）。 原音設定に独立した画面を与えず、
 * テイクを開いたらそこにある形にしてある。
 *
 * **主因は用語なしで言う**（`TR-ALN-26` (3)）。成分の名前（経路確信度・
 * 境界鋭さ）を出さない。数値と波形で見せるのは上級モードで、
 * それは `PROFILE-M6`（`Q-PLT-004` 待ち）。
 *
 * 確信度を点数として出さない。 良し悪しを教えない（`TR-SYN-20`、
 * `DEC-REC-008`）ので、出すのは「なぜ見てほしいか」だけにする。
 *
 * 直せない違反は確定させない。 `blocked` は検証で直しきれなかった状態で
 * （`TR-ALN-20`）、確認しても書き出せるようにならない。録り直しへ寄せる。
 */
export const ReviewEntry = ({
  item,
  individual,
  onConfirm,
  onRerecord,
  busy,
}: ReviewEntryProps) => {
  if (item === null) {
    return <p className="text-sm text-slate-11">この音は確認が済んでいます。</p>;
  }

  const blocked = item.state === "blocked";
  const cause = causeLabel(item.cause);

  return (
    <>
      <p className="text-sm text-slate-12">
        {blocked
          ? "切り出しが直せませんでした。録り直すと直ります。"
          : (cause ?? "念のため見ておいてください。")}
      </p>

      <div className="flex flex-wrap gap-2">
        {/*
          1件ずつ確定できるのは個別確認のときだけ（`REQ-ALN-008`）。
          切り替えたあとに出すと、Rust 側が断る的が残る。
        */}
        {individual && !blocked && (
          <Button type="button" variant="primary" onClick={onConfirm} disabled={busy}>
            これでよい
          </Button>
        )}
        <Button type="button" onClick={onRerecord} disabled={busy}>
          録り直す
        </Button>
      </div>
    </>
  );
};
