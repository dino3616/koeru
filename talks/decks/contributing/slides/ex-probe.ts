const RUN = 1600;

function ease(p: number): number {
  return p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2;
}

export default {
  motion: { "ex-probe-agent": RUN },
  draw(slide, { index, t }) {
    const spark = slide.querySelector<HTMLElement>("[data-spark]");
    const from = slide.querySelector<HTMLElement>('[data-kind][data-node="probe2"]');
    const to = slide.querySelector<HTMLElement>('[data-kind][data-node="req2"]');
    if (!spark || !from || !to) return;

    const p = index === 1 ? Math.min(1, Math.max(0, t / RUN)) : index > 1 ? 1 : 0;
    const q = ease(p);
    spark.style.left = `${from.offsetLeft + (to.offsetLeft - from.offsetLeft) * q}px`;
    spark.style.top = `${from.offsetTop + (to.offsetTop - from.offsetTop) * q}px`;
    spark.style.opacity = String(p > 0 && p < 1 ? 1 : 0);
  },
} satisfies DekSlide;
