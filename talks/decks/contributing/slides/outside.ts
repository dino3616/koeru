// Drifts the life chips in from the left edge, gathers them beside Signal, then sends the
// smallest chip into Signal and a spark once round the loop.
const DRIFT = 1800;
const GATHER = 1400;
const ENTER = 900;
const ROUND = 1900;
const ROW = 36;

function ease(p: number): number {
  return 1 - Math.pow(1 - p, 3);
}

function clamp(p: number): number {
  return Math.min(1, Math.max(0, p));
}

export default {
  motion: { "0": DRIFT, "outside-bring": GATHER, "outside-small": ENTER + ROUND },
  draw(slide, { index, t }) {
    const life = slide.querySelector<HTMLElement>(".life");
    const loop = slide.querySelector<HTMLElement>(".loop");
    const tiny = slide.querySelector<HTMLElement>("[data-tiny]");
    const spark = slide.querySelector<HTMLElement>("[data-spark]");
    if (!life || !loop || !tiny || !spark) return;

    const css = getComputedStyle(slide);
    const rx = parseFloat(css.getPropertyValue("--loop-rx"));
    const ry = parseFloat(css.getPropertyValue("--loop-ry"));
    const cx = loop.offsetLeft;
    const cy = loop.offsetTop;
    const signalY = cy - ry;
    const stackRight = cx - rx - 64;

    const drift = index === 0 ? ease(clamp(t / DRIFT)) : 1;
    const gather = index === 0 ? 0 : index === 1 ? ease(clamp(t / GATHER)) : 1;

    slide.querySelectorAll<HTMLElement>("[data-chip]").forEach((chip, i) => {
      const homeX = life.offsetLeft + chip.offsetLeft;
      const homeY = life.offsetTop + chip.offsetTop;
      const fromX = -(160 + 60 * i);
      const fromY = Math.sin(i * 1.7) * 30;
      const toX = stackRight - chip.offsetWidth - homeX;
      const toY = signalY - ROW * 0.5 + ROW * i - homeY;
      const x = (1 - drift) * fromX + gather * toX;
      const y = (1 - drift) * fromY + gather * toY;
      chip.style.translate = `${x}px ${y}px`;
      chip.style.opacity = String(drift);
    });

    const enter = index === 2 ? ease(clamp(t / ENTER)) : 0;
    const round = index === 2 ? clamp((t - ENTER) / ROUND) : 0;
    const tinyEndX = cx - tiny.offsetWidth - 54;
    const tinyEndY = signalY - tiny.offsetHeight / 2;
    tiny.style.translate = `${(1 - enter) * -120 + enter * tinyEndX}px ${tinyEndY - (1 - enter) * 40}px`;
    tiny.style.opacity = String(index === 2 ? Math.min(1, enter * 2) : 0);

    const a = round * Math.PI * 2;
    spark.style.left = `${cx + Math.sin(a) * rx}px`;
    spark.style.top = `${cy - Math.cos(a) * ry}px`;
    spark.style.opacity = String(round > 0 && round < 1 ? 1 : 0);
  },
} satisfies DekSlide;
