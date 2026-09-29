// 幹線の上を点が動き、点のいる瞬間から縦の線がすべての線路へ届く。
// 枝を一点に集めない——集めると、その点が中央駅に見える（5秒テストで2案とも読まれた）。
const START = 200;
const END = 1000;
// 止まったとき、縦の線がどの字も横切らない位置。字を動かしたら計算し直す。
const REST = 620;
const LINE_MS = 1200;
const EXIT_MS = 3600;

const ease = (p: number): number => (p < 0.5 ? 2 * p * p : 1 - (-2 * p + 2) ** 2 / 2);
const clamp = (p: number): number => Math.min(1, Math.max(0, p));

const beamAt = (p: number): number => {
  if (p < 0.7) {
    return START + (END - START) * ease(p / 0.7);
  }
  return END + (REST - END) * ease((p - 0.7) / 0.3);
};

export default {
  motion: { "trunk-line": LINE_MS, "trunk-exit": EXIT_MS },
  draw(slide, { index, step, t }) {
    const trunk = slide.querySelector<SVGPathElement>("[data-trunk]");
    const dot = slide.querySelector<SVGCircleElement>("[data-dot]");
    const beam = slide.querySelector<SVGLineElement>("[data-beam]");
    const ghosts = slide.querySelector<SVGGElement>("[data-ghosts]");

    let drawn = 0;
    if (step === "trunk-line") {
      drawn = ease(clamp(t / LINE_MS));
    } else if (index >= 2) {
      drawn = 1;
    }
    if (trunk) {
      trunk.style.strokeDashoffset = String(1 - drawn);
    }

    const sweeping = step === "trunk-exit";
    const shown = index >= 2;
    const x = sweeping ? beamAt(clamp(t / EXIT_MS)) : REST;

    for (const el of [dot, beam]) {
      if (el) {
        el.style.opacity = shown ? "1" : "0";
      }
    }
    dot?.setAttribute("cx", String(x));
    beam?.setAttribute("x1", String(x));
    beam?.setAttribute("x2", String(x));
    if (ghosts) {
      ghosts.style.opacity = shown && !(sweeping && t < EXIT_MS) ? "1" : "0";
    }
  },
} satisfies DekSlide;
