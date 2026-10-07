// Runs a spark from the PROBE to its Receipt, then on to the EVID that is not written yet.
const RUN = 2400;

function ease(p: number): number {
  return p < 0.5 ? 2 * p * p : 1 - Math.pow(-2 * p + 2, 2) / 2;
}

function centre(el: HTMLElement): [number, number] {
  return [el.offsetLeft, el.offsetTop];
}

export default {
  motion: { "0": RUN },
  draw(slide, { index, t }) {
    const spark = slide.querySelector<HTMLElement>("[data-spark]");
    const probe = slide.querySelector<HTMLElement>('[data-kind][data-node="probe1"]');
    const receipt = slide.querySelector<HTMLElement>('[data-kind][data-node="receipt"]');
    const evid = slide.querySelector<HTMLElement>('[data-kind][data-node="evid3"]');
    if (!spark || !probe || !receipt || !evid) return;

    const p = index === 0 ? Math.min(1, Math.max(0, t / RUN)) : 1;
    const [from, to, q] = p < 0.5 ? [probe, receipt, ease(p * 2)] : [receipt, evid, ease(p * 2 - 1)];
    const [x0, y0] = centre(from);
    const [x1, y1] = centre(to);
    spark.style.left = `${x0 + (x1 - x0) * q}px`;
    spark.style.top = `${y0 + (y1 - y0) * q}px`;
    spark.style.opacity = String(p > 0 && p < 1 ? 1 : 0);
  },
} satisfies DekSlide;
